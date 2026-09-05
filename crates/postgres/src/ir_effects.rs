//! Reading the effect ledger.
//!
//! There are no writes here. Claims reach the ledger only by appending an
//! event to a run, which is what keeps the ledger unable to say something the
//! log does not. What this module adds is the query the log cannot answer
//! cheaply: which protected effects are in flight right now, across every run,
//! and how long have they been there.
//!
//! That question is the one an operator asks after an incident, and it is the
//! reason the ledger exists at all.

use capsulet_runtime::Epoch;
use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

/// One attempt at one effect, as the ledger has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrEffectClaim {
    pub tenant_id: String,
    pub project_id: String,
    pub run_id: String,
    pub node: String,
    pub effect: String,
    pub attempt: u32,
    /// The key handed to the far side, where the effect declared one.
    pub idempotency_key: Option<String>,
    /// `claimed` means nobody knows yet.
    pub outcome: String,
    pub receipt: Option<String>,
    /// How many times this attempt was claimed. More than one means a keyed
    /// effect was retried under the key it had already used.
    pub claim_count: u32,
    pub epoch: Epoch,
    /// Where the most recent claim sits in the run's log.
    pub position: u64,
}

impl PostgresStore {
    /// Every effect nobody can yet account for, oldest first.
    ///
    /// `tenant` narrows the answer to one tenant; `None` asks about the whole
    /// installation.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails.
    pub async fn list_outstanding_ir_effects(
        &self,
        tenant: Option<&str>,
        limit: i64,
    ) -> Result<Vec<IrEffectClaim>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT tenant_id, project_id, run_id, node_id, effect_id, attempt,
                   idempotency_key, outcome, receipt, claim_count, epoch, position
            FROM ir_effect_claims
            WHERE outcome = 'claimed'
              AND ($1::text IS NULL OR tenant_id = $1)
            ORDER BY claimed_at, run_id, position
            LIMIT $2
            ",
        )
        .bind(tenant)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_claim).collect()
    }

    /// Every claim one run has made, in the order it made them.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails.
    pub async fn ir_effect_claims_for_run(
        &self,
        tenant_id: &str,
        project_id: &str,
        run_id: &str,
    ) -> Result<Vec<IrEffectClaim>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT tenant_id, project_id, run_id, node_id, effect_id, attempt,
                   idempotency_key, outcome, receipt, claim_count, epoch, position
            FROM ir_effect_claims
            WHERE tenant_id = $1 AND project_id = $2 AND run_id = $3
            ORDER BY position
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(row_to_claim).collect()
    }
}

fn row_to_claim(row: &sqlx::postgres::PgRow) -> Result<IrEffectClaim, PostgresStoreError> {
    let attempt: i32 = row.get("attempt");
    let claim_count: i32 = row.get("claim_count");
    let epoch: i64 = row.get("epoch");
    let position: i64 = row.get("position");

    Ok(IrEffectClaim {
        tenant_id: row.get("tenant_id"),
        project_id: row.get("project_id"),
        run_id: row.get("run_id"),
        node: row.get("node_id"),
        effect: row.get("effect_id"),
        attempt: u32::try_from(attempt)
            .map_err(|_| PostgresStoreError::Overflow("effect attempt"))?,
        idempotency_key: row.get("idempotency_key"),
        outcome: row.get("outcome"),
        receipt: row.get("receipt"),
        claim_count: u32::try_from(claim_count)
            .map_err(|_| PostgresStoreError::Overflow("effect claim count"))?,
        epoch: Epoch(u64::try_from(epoch).map_err(|_| PostgresStoreError::Overflow("run epoch"))?),
        position: u64::try_from(position)
            .map_err(|_| PostgresStoreError::Overflow("log position"))?,
    })
}
