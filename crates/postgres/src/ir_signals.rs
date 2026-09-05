//! The signal inbox.
//!
//! Everything that arrives from outside a run lands here first: a webhook, a
//! person's decision, a scheduler saying a timer elapsed. None of it goes
//! straight into the run's log, because only the lease holder writes the log
//! and an API request is not the lease holder. Writing here instead means an
//! event that arrives while no worker is running is not lost, and an event that
//! arrives while one is running does not race it.
//!
//! The worker reads the pending signals, asks [`capsulet_runtime::wait::resume`]
//! whether each one actually matches what the run is waiting for, and records
//! the answer. A signal that matched carries the log position of the resumption
//! it caused; one that did not carries the reason. Either way it is consumed
//! once, which is what makes a delivered-twice webhook resume a run once.

use std::collections::BTreeSet;

use capsulet_ir::id::Identifier;
use capsulet_runtime::wait::{Signal, SignalKind};
use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

/// A signal as it is stored, with the row id needed to consume it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveredSignal {
    pub id: i64,
    pub run_id: String,
    pub signal: Signal,
}

/// What happened to a signal when a worker looked at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalOutcome {
    /// It matched, and the run resumed at the given log position.
    Resumed { position: u64 },
    /// It did not match what the run was waiting for.
    Refused,
}

impl PostgresStore {
    /// Delivers a signal to a run.
    ///
    /// Recording the authorities the deliverer held is deliberate: whether
    /// somebody could open a gate is a fact about the moment they tried, and
    /// resolving it again later would answer a different question.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the run does not exist or the write
    /// fails.
    pub async fn deliver_ir_run_signal(
        &self,
        tenant_id: &str,
        project_id: &str,
        run_id: &str,
        signal: &Signal,
    ) -> Result<i64, PostgresStoreError> {
        let (kind, subject) = match &signal.kind {
            SignalKind::Event { name } => ("event", Some(name.as_str().to_string())),
            SignalKind::HumanDecision { obligation } => {
                ("human_decision", Some(obligation.as_str().to_string()))
            }
            SignalKind::TimerElapsed => ("timer_elapsed", None),
        };
        let authorities: Vec<&str> = signal
            .authorities
            .iter()
            .map(capsulet_ir::id::Identifier::as_str)
            .collect();
        let authorities = serde_json::to_value(authorities)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;

        let id = sqlx::query_scalar::<_, i64>(
            r"
            INSERT INTO ir_run_signals (
                tenant_id, project_id, run_id, kind, subject, delivered_by, authorities
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb)
            RETURNING id
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(run_id)
        .bind(kind)
        .bind(subject)
        .bind(signal.by.as_str())
        .bind(authorities)
        .fetch_one(&self.pool)
        .await?;

        Ok(id)
    }

    /// The signals a run has not answered yet, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails or a stored row does
    /// not parse, which would mean it was written by an incompatible build.
    pub async fn pending_ir_run_signals(
        &self,
        tenant_id: &str,
        project_id: &str,
        run_id: &str,
    ) -> Result<Vec<DeliveredSignal>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, run_id, kind, subject, delivered_by, authorities
            FROM ir_run_signals
            WHERE tenant_id = $1 AND project_id = $2 AND run_id = $3
              AND consumed_at IS NULL
            ORDER BY id
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_signal).collect()
    }

    /// Marks a signal answered, once.
    ///
    /// Returns whether this call was the one that consumed it. A `false` means
    /// somebody got there first, which is the answer a worker needs in order
    /// not to resume a run twice for one webhook.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the write fails.
    pub async fn consume_ir_run_signal(
        &self,
        id: i64,
        outcome: SignalOutcome,
    ) -> Result<bool, PostgresStoreError> {
        let (label, position) = match outcome {
            SignalOutcome::Resumed { position } => (
                "resumed",
                Some(
                    i64::try_from(position)
                        .map_err(|_| PostgresStoreError::Overflow("log position"))?,
                ),
            ),
            SignalOutcome::Refused => ("refused", None),
        };

        // `consumed_at IS NULL` in the predicate rather than relying on the
        // trigger, so a second caller gets `false` instead of an error: losing
        // the race is ordinary, not exceptional.
        let result = sqlx::query(
            r"
            UPDATE ir_run_signals
            SET consumed_at = now(), consumed_at_position = $2, outcome = $3
            WHERE id = $1 AND consumed_at IS NULL
            ",
        )
        .bind(id)
        .bind(position)
        .bind(label)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }
}

fn row_to_signal(row: &sqlx::postgres::PgRow) -> Result<DeliveredSignal, PostgresStoreError> {
    let kind: String = row.get("kind");
    let subject: Option<String> = row.get("subject");
    let delivered_by: String = row.get("delivered_by");
    let authorities: serde_json::Value = row.get("authorities");

    let parse = |value: &str| {
        Identifier::parse(value)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))
    };
    let named = |what: &str| {
        subject.as_deref().ok_or_else(|| {
            PostgresStoreError::InvalidPersistedValue(format!("a {what} signal names no subject"))
        })
    };

    let kind = match kind.as_str() {
        "event" => SignalKind::Event {
            name: parse(named("event")?)?,
        },
        "human_decision" => SignalKind::HumanDecision {
            obligation: parse(named("human decision")?)?,
        },
        "timer_elapsed" => SignalKind::TimerElapsed,
        other => {
            return Err(PostgresStoreError::InvalidPersistedValue(format!(
                "unknown signal kind `{other}`"
            )));
        }
    };

    let authorities: Vec<String> = serde_json::from_value(authorities)
        .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;
    let authorities: BTreeSet<Identifier> = authorities
        .iter()
        .map(|value| parse(value))
        .collect::<Result<_, _>>()?;

    Ok(DeliveredSignal {
        id: row.get("id"),
        run_id: row.get("run_id"),
        signal: Signal {
            kind,
            by: parse(&delivered_by)?,
            authorities,
        },
    })
}
