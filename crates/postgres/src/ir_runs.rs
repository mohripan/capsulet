//! Durable storage for runs of verified-computation IR definitions.
//!
//! The log is the truth. A run's status, its progress through the graph, what
//! its loops have spent, and which effects are outstanding are a fold over
//! `ir_run_events` — never a column somebody updated. This module therefore
//! offers exactly two writes: create a run, and append one event to it. There
//! is deliberately no `set_status`, because a second way to answer "where is
//! this run" is a second answer that can disagree with the first, and the
//! disagreement always surfaces during recovery.
//!
//! Appending takes the next position under the run row's lock, so two workers
//! racing on the same run cannot both write position *n*: one of them takes
//! *n*, the other takes *n+1*, and the log stays gapless either way.
//!
//! Every append names the fencing epoch it was authorised under. A worker whose
//! lease was reclaimed while it was paused will name the old epoch, the write
//! matches no row, and it finds out it is no longer the owner instead of
//! corrupting a run somebody else is now advancing.

use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::definition::AssuranceMode;
use capsulet_ir::digest::Digest;
use capsulet_runtime::{Epoch, RecordedEvent, RunEvent, RunState};
use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

/// A run as it is stored.
///
/// `status` and `next_position` are projections of the log kept for querying;
/// the events remain the authority, and a test asserts the two agree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrRunRecord {
    pub tenant_id: String,
    pub project_id: String,
    pub id: String,
    pub definition_digest: String,
    pub status: String,
    pub epoch: Epoch,
    pub lease_owner: Option<String>,
    pub next_position: u64,
}

impl PostgresStore {
    /// Creates a run and writes its admission event.
    ///
    /// Both happen in one transaction. A run row without its admission event
    /// would be a run whose log does not fold, and there is no reason to let
    /// that state exist even briefly.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the run id is already taken, the
    /// definition version is not stored, or the write fails.
    pub async fn create_ir_run(
        &self,
        tenant_id: &str,
        project_id: &str,
        run_id: &str,
        definition_digest: &Digest,
        mode: AssuranceMode,
        at: RecordedTime,
    ) -> Result<IrRunRecord, PostgresStoreError> {
        let admitted = RunEvent::Admitted {
            definition: *definition_digest,
            mode,
        };
        let payload = serde_json::to_value(&admitted)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;

        let mut transaction = self.pool.begin().await?;

        sqlx::query(
            r"
            INSERT INTO ir_runs (tenant_id, project_id, id, definition_digest)
            VALUES ($1, $2, $3, $4)
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(run_id)
        .bind(definition_digest.to_string())
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            r"
            WITH claimed AS (
                UPDATE ir_runs
                SET next_position = next_position + 1
                WHERE tenant_id = $1 AND project_id = $2 AND id = $3
                RETURNING next_position - 1 AS position
            )
            INSERT INTO ir_run_events (
                tenant_id, project_id, run_id, position, kind, payload, epoch, recorded_at
            )
            SELECT $1, $2, $3, claimed.position, $4, $5::jsonb, 0, $6
            FROM claimed
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(run_id)
        .bind(admitted.as_str())
        .bind(payload)
        .bind(at.epoch_millis())
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;

        self.get_ir_run(tenant_id, project_id, run_id)
            .await?
            .ok_or_else(|| PostgresStoreError::RunNotFound(run_id.to_string()))
    }

    /// Appends one event, taking the next position atomically.
    ///
    /// The whole append is a single statement. The `UPDATE` locks the run row
    /// and hands back the position it just claimed, and the `INSERT` uses it;
    /// a concurrent append blocks on that lock rather than guessing.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError::EpochSuperseded`] when the run has since
    /// been leased by somebody else, [`PostgresStoreError::RunNotFound`] when
    /// there is no such run, and [`PostgresStoreError`] when the write fails.
    pub async fn append_ir_run_event(
        &self,
        tenant_id: &str,
        project_id: &str,
        run_id: &str,
        epoch: Epoch,
        event: &RunEvent,
        at: RecordedTime,
    ) -> Result<RecordedEvent, PostgresStoreError> {
        let payload = serde_json::to_value(event)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;
        let epoch_value =
            i64::try_from(epoch.0).map_err(|_| PostgresStoreError::Overflow("run epoch"))?;

        let position = sqlx::query_scalar::<_, i64>(
            r"
            WITH claimed AS (
                UPDATE ir_runs
                SET next_position = next_position + 1
                WHERE tenant_id = $1 AND project_id = $2 AND id = $3 AND epoch = $4
                RETURNING next_position - 1 AS position
            )
            INSERT INTO ir_run_events (
                tenant_id, project_id, run_id, position, kind, payload, epoch, recorded_at
            )
            SELECT $1, $2, $3, claimed.position, $5, $6::jsonb, $4, $7
            FROM claimed
            RETURNING position
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(run_id)
        .bind(epoch_value)
        .bind(event.as_str())
        .bind(payload)
        .bind(at.epoch_millis())
        .fetch_optional(&self.pool)
        .await?;

        // No row means the `WHERE` matched nothing. Which of the two reasons it
        // was matters to the caller: a superseded epoch means stop working, a
        // missing run means the caller is confused about what exists.
        let Some(position) = position else {
            return Err(
                match self.get_ir_run(tenant_id, project_id, run_id).await? {
                    Some(run) => PostgresStoreError::EpochSuperseded {
                        run: run_id.to_string(),
                        attempted: epoch.0,
                        current: run.epoch.0,
                    },
                    None => PostgresStoreError::RunNotFound(run_id.to_string()),
                },
            );
        };

        Ok(RecordedEvent {
            position: u64::try_from(position)
                .map_err(|_| PostgresStoreError::Overflow("log position"))?,
            epoch,
            at,
            event: event.clone(),
        })
    }

    /// Reads a run's events in the order they were written.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails or a stored payload
    /// does not parse, which would mean the row was written by an incompatible
    /// build.
    pub async fn load_ir_run_events(
        &self,
        tenant_id: &str,
        project_id: &str,
        run_id: &str,
    ) -> Result<Vec<RecordedEvent>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT position, epoch, recorded_at, payload
            FROM ir_run_events
            WHERE tenant_id = $1 AND project_id = $2 AND run_id = $3
            ORDER BY position
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_event).collect()
    }

    /// Folds a run's log into its state.
    ///
    /// This is the only way this crate answers "where is the run", and it is
    /// the same fold a worker performs in memory, so recovery and normal
    /// operation cannot reach different conclusions.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the events cannot be read, or
    /// [`PostgresStoreError::InvalidPersistedValue`] when they do not fold —
    /// which would mean the stored history is not one any run could produce.
    pub async fn load_ir_run_state(
        &self,
        tenant_id: &str,
        project_id: &str,
        run_id: &str,
    ) -> Result<RunState, PostgresStoreError> {
        let events = self
            .load_ir_run_events(tenant_id, project_id, run_id)
            .await?;
        RunState::fold(&events)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))
    }

    /// Reads one run.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails.
    pub async fn get_ir_run(
        &self,
        tenant_id: &str,
        project_id: &str,
        run_id: &str,
    ) -> Result<Option<IrRunRecord>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT tenant_id, project_id, id, definition_digest, status, epoch,
                   lease_owner, next_position
            FROM ir_runs
            WHERE tenant_id = $1 AND project_id = $2 AND id = $3
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(row_to_run).transpose()
    }

}

fn row_to_run(row: &sqlx::postgres::PgRow) -> Result<IrRunRecord, PostgresStoreError> {
    let epoch: i64 = row.get("epoch");
    let next_position: i64 = row.get("next_position");
    Ok(IrRunRecord {
        tenant_id: row.get("tenant_id"),
        project_id: row.get("project_id"),
        id: row.get("id"),
        definition_digest: row.get("definition_digest"),
        status: row.get("status"),
        epoch: Epoch(u64::try_from(epoch).map_err(|_| PostgresStoreError::Overflow("run epoch"))?),
        lease_owner: row.get("lease_owner"),
        next_position: u64::try_from(next_position)
            .map_err(|_| PostgresStoreError::Overflow("log position"))?,
    })
}

fn row_to_event(row: &sqlx::postgres::PgRow) -> Result<RecordedEvent, PostgresStoreError> {
    let position: i64 = row.get("position");
    let epoch: i64 = row.get("epoch");
    let payload: serde_json::Value = row.get("payload");
    let event: RunEvent = serde_json::from_value(payload)
        .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;

    Ok(RecordedEvent {
        position: u64::try_from(position)
            .map_err(|_| PostgresStoreError::Overflow("log position"))?,
        epoch: Epoch(u64::try_from(epoch).map_err(|_| PostgresStoreError::Overflow("run epoch"))?),
        at: RecordedTime(row.get("recorded_at")),
        event,
    })
}
