//! Agent runs on the durable path.
//!
//! An agent run is the product record; the IR run behind it is what executes. This
//! module is where the two meet, and it has two jobs.
//!
//! Starting one writes everything in a single transaction: the compiled definition,
//! the IR run and its admission, the inputs it was given, and the agent run that
//! links them. There is no window in which a queued run exists without the inputs it
//! needs, or an agent run points at nothing.
//!
//! Reading one answers from the log. The status column on `agent_runs` is what the
//! row said when it was created; where the run actually is comes from folding its
//! events, so there is only one answer to the question.

use std::collections::BTreeMap;

use capsulet_application::{AgentRunExecution, AgentRunRecord, AgentRunUsage};
use capsulet_core::{AgentRunId, AgentRunStatus, JobRun};
use capsulet_ir::admission::AdmissionRecord;
use capsulet_ir::agent::AGENT_LEAVE;
use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::definition::Definition;
use capsulet_ir::digest::Digest;
use capsulet_ir::id::Identifier;
use capsulet_ir::loop_region::FailureKind;
use capsulet_runtime::{RunEvent, RunFailure, RunState, RunStatus};
use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

/// The most a single stored value may hold, matching the table's constraint.
pub const MAX_IR_RUN_VALUE_BYTES: usize = 16 * 1024 * 1024;

/// Everything starting an agent run writes.
#[derive(Debug, Clone, Copy)]
pub struct AgentRunStart<'a> {
    pub run: &'a AgentRunRecord,
    pub definition: &'a Definition,
    pub admission: &'a AdmissionRecord,
    /// Each input value by name, encoded as JSON text.
    pub inputs: &'a BTreeMap<String, String>,
    pub at: RecordedTime,
}

impl PostgresStore {
    /// Stores a value under its content digest and returns the digest.
    ///
    /// Idempotent: the same bytes in the same project are the same row. Scoped to the
    /// project even though the key is a digest, so holding a digest is never a way to
    /// read what another project stored.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError::InvalidPersistedValue`] when the value is larger
    /// than [`MAX_IR_RUN_VALUE_BYTES`], and [`PostgresStoreError`] when the write fails.
    pub async fn put_ir_run_value(
        &self,
        tenant_id: &str,
        project_id: &str,
        content: &str,
    ) -> Result<Digest, PostgresStoreError> {
        let digest = checked_digest(content)?;
        insert_value(&self.pool, tenant_id, project_id, &digest, content).await?;
        Ok(digest)
    }

    /// Reads a value by digest, within one project.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the query fails.
    pub async fn get_ir_run_value(
        &self,
        tenant_id: &str,
        project_id: &str,
        digest: &Digest,
    ) -> Result<Option<String>, PostgresStoreError> {
        Ok(sqlx::query_scalar::<_, String>(
            r"
            SELECT content FROM ir_run_values
            WHERE tenant_id = $1 AND project_id = $2 AND digest = $3
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(digest.to_string())
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Starts an agent run: the definition, the IR run, its inputs, and the link
    /// between them, all or nothing.
    ///
    /// The IR run shares the agent run's id. A run id is unique per project on both
    /// sides, so neither ever needs a lookup to find the other.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError::ScopeMismatch`] when the agent or the run id
    /// belongs to another project, [`PostgresStoreError::AlreadyExists`] when this
    /// project already has a run with this id, and [`PostgresStoreError`] when the
    /// write fails. Nothing is left behind by any of those.
    pub async fn start_agent_run_in_project(
        &self,
        start: &AgentRunStart<'_>,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), PostgresStoreError> {
        let run = start.run;
        let run_id = run.id.as_str();
        let mut tx = self.pool.begin().await?;

        let agent_owned = sqlx::query_scalar::<_, i32>(
            r"
            SELECT 1 FROM agent_definitions
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            FOR SHARE
            ",
        )
        .bind(run.agent_id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&mut *tx)
        .await?;
        if agent_owned.is_none() {
            return Err(PostgresStoreError::ScopeMismatch);
        }

        let digest = crate::ir_definitions::insert_definition_version(
            &mut tx,
            tenant_id,
            project_id,
            start.definition,
            start.admission,
        )
        .await?;

        let mut bound = BTreeMap::new();
        for (name, content) in start.inputs {
            let value_digest = checked_digest(content)?;
            insert_value(&mut *tx, tenant_id, project_id, &value_digest, content).await?;
            bound.insert(name.clone(), value_digest);
        }

        // Claimed before anything else names it, so losing this race leaves nothing
        // to clean up and the answer to "who has this id" comes from one place.
        claim_run_id(&mut tx, run, &digest, tenant_id, project_id).await?;

        crate::ir_runs::insert_run_and_admission(
            &mut tx,
            tenant_id,
            project_id,
            run_id,
            &digest,
            start.definition.assurance,
            start.at,
        )
        .await?;
        crate::ir_runs::insert_event_unfenced(
            &mut tx,
            tenant_id,
            project_id,
            run_id,
            &RunEvent::InputsBound { inputs: bound },
            start.at,
        )
        .await?;

        sqlx::query(
            r"
            UPDATE agent_runs SET ir_run_id = $1
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(run_id)
        .bind(tenant_id)
        .bind(project_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r"
            INSERT INTO agent_state_snapshots (agent_run_id, version, state_json)
            VALUES ($1, $2, $3)
            ",
        )
        .bind(run_id)
        .bind(to_version(run.state_version)?)
        .bind(run.state_json.as_str())
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Creates a job run owned by a project, unless it already exists.
    ///
    /// Returns whether this call created it. An effect's retry derives the same id,
    /// so the second call finds the run the first one made instead of starting
    /// another. The job definition must belong to the project.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError::ScopeMismatch`] when the job definition, or a run
    /// with this id, belongs to another project.
    pub async fn create_job_run_if_absent_in_project(
        &self,
        run: &JobRun,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<bool, PostgresStoreError> {
        let mut tx = self.pool.begin().await?;
        let owned = sqlx::query_scalar::<_, i32>(
            r"
            SELECT 1 FROM job_definitions
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            FOR SHARE
            ",
        )
        .bind(run.job_definition_id().as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&mut *tx)
        .await?;
        if owned.is_none() {
            return Err(PostgresStoreError::ScopeMismatch);
        }

        let created = sqlx::query(
            r"
            INSERT INTO job_runs (
                id, job_definition_id, status, execution_pool, input, attempt_count,
                tenant_id, project_id, updated_at
            )
            VALUES ($1, $2, $3, $4, $5::jsonb, $6, $7, $8, now())
            ON CONFLICT (id) DO NOTHING
            ",
        )
        .bind(run.id().as_str())
        .bind(run.job_definition_id().as_str())
        .bind(run.status().to_string())
        .bind(run.execution_pool().as_str())
        .bind(run.input_json())
        .bind(i32::try_from(run.attempt_count()).map_err(|_| PostgresStoreError::AttemptOverflow)?)
        .bind(tenant_id)
        .bind(project_id)
        .execute(&mut *tx)
        .await?;

        if created.rows_affected() == 0 {
            let same_project = sqlx::query_scalar::<_, i32>(
                "SELECT 1 FROM job_runs WHERE id = $1 AND tenant_id = $2 AND project_id = $3",
            )
            .bind(run.id().as_str())
            .bind(tenant_id)
            .bind(project_id)
            .fetch_optional(&mut *tx)
            .await?;
            if same_project.is_none() {
                return Err(PostgresStoreError::ScopeMismatch);
            }
            tx.commit().await?;
            return Ok(false);
        }
        tx.commit().await?;
        Ok(true)
    }

    /// Finds an agent run with where it actually is, read from its log.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails, or the log does not fold.
    pub async fn find_agent_run_with_progress(
        &self,
        id: &AgentRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<AgentRunRecord>, PostgresStoreError> {
        let Some(run) = self
            .find_agent_run_in_project(id, tenant_id, project_id)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(self.with_progress(run, tenant_id, project_id).await?))
    }

    /// Lists a project's agent runs, with where each one actually is.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails, or a log does not fold.
    pub async fn list_agent_runs_with_progress(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<AgentRunRecord>, PostgresStoreError> {
        let runs = self
            .list_agent_runs_in_project(tenant_id, project_id, limit)
            .await?;
        let mut projected = Vec::with_capacity(runs.len());
        for run in runs {
            projected.push(self.with_progress(run, tenant_id, project_id).await?);
        }
        Ok(projected)
    }

    /// Replaces what a row remembers with what the log says.
    ///
    /// A run from before agent runs executed has no log to read, so it reports what
    /// it always reported.
    async fn with_progress(
        &self,
        mut run: AgentRunRecord,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<AgentRunRecord, PostgresStoreError> {
        let link = sqlx::query(
            r"
            SELECT ir_run_id, definition_digest FROM agent_runs
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(run.id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_one(&self.pool)
        .await?;
        let ir_run_id: Option<String> = link.try_get("ir_run_id")?;
        let definition_digest: Option<String> = link.try_get("definition_digest")?;
        let (Some(ir_run_id), Some(definition_digest)) = (ir_run_id, definition_digest) else {
            return Ok(run);
        };

        let events = self
            .load_ir_run_events(tenant_id, project_id, &ir_run_id)
            .await?;
        let state = RunState::fold(&events)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;

        run.status = status_of(&state);
        run.state_version = u64::try_from(
            events
                .iter()
                .filter(|recorded| matches!(recorded.event, RunEvent::NodeFinished { .. }))
                .count(),
        )
        .unwrap_or(u64::MAX);
        run.state_json = self
            .final_state(&run.state_json, &state, tenant_id, project_id)
            .await?;

        let spent = state.spent();
        run.execution = Some(AgentRunExecution {
            ir_run_id,
            definition_digest,
            detail: state.failure().map(describe_failure),
            waiting_on: state.waiting_on().map(describe_wait),
            usage: AgentRunUsage {
                tokens: spent.tokens,
                cost_micro_units: spent.cost_micro_units,
                effects: spent.effects,
                unmeasured: spent.unmeasured,
            },
        });
        Ok(run)
    }

    /// The state a run started with, overlaid with what its loop left behind.
    async fn final_state(
        &self,
        initial: &str,
        state: &RunState,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<String, PostgresStoreError> {
        let Ok(leave) = Identifier::parse(AGENT_LEAVE) else {
            return Ok(initial.to_string());
        };
        // The last iteration's values when one finished; the one before it while the
        // next is still running, because the exit node has been reset by then.
        let mut written = state.outputs_of(&leave);
        if written.is_empty() {
            written = state.previous_outputs_of(&leave);
        }
        if written.is_empty() {
            return Ok(initial.to_string());
        }

        let mut merged: serde_json::Value = serde_json::from_str(initial)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;
        let Some(fields) = merged.as_object_mut() else {
            return Ok(initial.to_string());
        };
        for (field, digest) in written {
            let Some(content) = self
                .get_ir_run_value(tenant_id, project_id, &digest)
                .await?
            else {
                continue;
            };
            let value = serde_json::from_str(&content)
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;
            fields.insert(field, value);
        }
        serde_json::to_string(&merged)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))
    }
}

fn to_version(version: u64) -> Result<i64, PostgresStoreError> {
    i64::try_from(version)
        .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))
}

/// Writes the agent run's row, unless its id is taken.
async fn claim_run_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run: &AgentRunRecord,
    digest: &Digest,
    tenant_id: &str,
    project_id: &str,
) -> Result<(), PostgresStoreError> {
    let run_id = run.id.as_str();
    let claimed = sqlx::query(
        r"
        INSERT INTO agent_runs (
            id, agent_id, status, state_version, state_json, tenant_id, project_id,
            definition_digest, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now())
        ON CONFLICT (id) DO NOTHING
        ",
    )
    .bind(run_id)
    .bind(run.agent_id.as_str())
    .bind(run.status.to_string())
    .bind(to_version(run.state_version)?)
    .bind(run.state_json.as_str())
    .bind(tenant_id)
    .bind(project_id)
    .bind(digest.to_string())
    .execute(&mut **tx)
    .await?;
    if claimed.rows_affected() > 0 {
        return Ok(());
    }

    let same_project = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM agent_runs WHERE id = $1 AND tenant_id = $2 AND project_id = $3",
    )
    .bind(run_id)
    .bind(tenant_id)
    .bind(project_id)
    .fetch_optional(&mut **tx)
    .await?;
    Err(if same_project.is_some() {
        PostgresStoreError::AlreadyExists(run_id.to_string())
    } else {
        PostgresStoreError::ScopeMismatch
    })
}

fn checked_digest(content: &str) -> Result<Digest, PostgresStoreError> {
    if content.len() > MAX_IR_RUN_VALUE_BYTES {
        return Err(PostgresStoreError::InvalidPersistedValue(format!(
            "a value of {} bytes is larger than the {MAX_IR_RUN_VALUE_BYTES} a run may store",
            content.len()
        )));
    }
    Ok(Digest::of(content.as_bytes()))
}

async fn insert_value<'e, E>(
    executor: E,
    tenant_id: &str,
    project_id: &str,
    digest: &Digest,
    content: &str,
) -> Result<(), PostgresStoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query(
        r"
        INSERT INTO ir_run_values (tenant_id, project_id, digest, content, size_bytes)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (tenant_id, project_id, digest) DO NOTHING
        ",
    )
    .bind(tenant_id)
    .bind(project_id)
    .bind(digest.to_string())
    .bind(content)
    .bind(i64::try_from(content.len()).unwrap_or(i64::MAX))
    .execute(executor)
    .await?;
    Ok(())
}

/// Where a run is, in the terms an agent run reports.
///
/// `stopped` is any end that is not the work finishing and not the work breaking: a
/// budget, a loop that stopped making progress, a safety failure, or a person saying
/// no. None of those is a success, and calling them failures would blame the work for
/// a limit somebody set.
fn status_of(state: &RunState) -> AgentRunStatus {
    match state.status() {
        RunStatus::Queued => AgentRunStatus::Queued,
        RunStatus::Running => AgentRunStatus::Running,
        RunStatus::Waiting => AgentRunStatus::Waiting,
        RunStatus::Completed => AgentRunStatus::Succeeded,
        RunStatus::Cancelled => AgentRunStatus::Cancelled,
        RunStatus::Failed => match state.failure() {
            Some(
                RunFailure::BudgetExhausted { .. }
                | RunFailure::LoopStopped { .. }
                | RunFailure::GateRejected { .. }
                | RunFailure::Node {
                    failure: FailureKind::UnsafeEffect | FailureKind::NonProgress,
                    ..
                },
            ) => AgentRunStatus::Stopped,
            _ => AgentRunStatus::Failed,
        },
    }
}

fn describe_failure(failure: &RunFailure) -> String {
    match failure {
        RunFailure::Node {
            node,
            failure,
            detail,
        } => format!("node `{node}` failed with {}: {detail}", failure.as_str()),
        RunFailure::EffectUncertain { node, effect } => format!(
            "nobody can say whether effect `{effect}` on `{node}` happened, so the run stopped \
             rather than guess"
        ),
        RunFailure::BudgetExhausted { resource } => format!("the {resource} budget ran out"),
        RunFailure::ControlMissing { node, port, .. } => {
            format!("`{node}` reported nothing on `{port}`, which the loop has to read")
        }
        RunFailure::LoopStopped { reason, .. } => match reason {
            capsulet_ir::loop_region::StopReason::BudgetExhausted { budget } => {
                format!("the {} budget ran out", budget.as_str())
            }
            other => format!("the loop stopped: {}", other.as_str()),
        },
        RunFailure::GateRejected { node, by, .. } => {
            format!("`{by}` refused at the gate on `{node}`")
        }
    }
}

fn describe_wait(wait: &capsulet_runtime::Wait) -> String {
    match wait {
        capsulet_runtime::Wait::Timer { until } => {
            format!("a timer due at {}", until.epoch_millis())
        }
        capsulet_runtime::Wait::Event { name } => format!("the event `{name}`"),
        capsulet_runtime::Wait::HumanGate { node, obligation } => {
            format!("a decision from `{obligation}` on `{node}`")
        }
    }
}

#[cfg(test)]
mod tests {
    use capsulet_ir::correctness::evidence::RecordedTime;
    use capsulet_ir::definition::AssuranceMode;
    use capsulet_ir::loop_region::{BudgetKind, StopReason};
    use capsulet_runtime::{Epoch, RecordedEvent};

    use super::*;

    fn id(value: &str) -> Identifier {
        Identifier::parse(value).expect("a legal identifier")
    }

    fn folded(ending: RunEvent) -> RunState {
        let events: Vec<RecordedEvent> = [
            RunEvent::Admitted {
                definition: Digest::of(b"definition"),
                mode: AssuranceMode::Observe,
            },
            RunEvent::Started { by: id("worker") },
            ending,
        ]
        .into_iter()
        .enumerate()
        .map(|(position, event)| RecordedEvent {
            position: position as u64,
            epoch: Epoch(1),
            at: RecordedTime(0),
            event,
        })
        .collect();
        RunState::fold(&events).expect("the log folds")
    }

    fn failed(reason: RunFailure) -> AgentRunStatus {
        status_of(&folded(RunEvent::Failed { reason }))
    }

    #[test]
    fn a_budget_stop_is_neither_a_success_nor_a_failure_of_the_work() {
        assert_eq!(
            failed(RunFailure::LoopStopped {
                region: id("agent-loop"),
                reason: StopReason::BudgetExhausted {
                    budget: BudgetKind::Tokens,
                },
            }),
            AgentRunStatus::Stopped
        );
        assert_eq!(
            failed(RunFailure::BudgetExhausted {
                resource: "wall time".to_string(),
            }),
            AgentRunStatus::Stopped
        );
    }

    #[test]
    fn a_safety_failure_and_a_refusal_stop_a_run() {
        assert_eq!(
            failed(RunFailure::Node {
                node: id("check"),
                failure: FailureKind::UnsafeEffect,
                detail: "no".to_string(),
            }),
            AgentRunStatus::Stopped
        );
        assert_eq!(
            failed(RunFailure::GateRejected {
                node: id("check"),
                obligation: id("human-reviewer"),
                by: id("reviewer"),
            }),
            AgentRunStatus::Stopped
        );
    }

    #[test]
    fn an_ordinary_node_failure_is_a_failure() {
        assert_eq!(
            failed(RunFailure::Node {
                node: id("model"),
                failure: FailureKind::VerifierUnavailable,
                detail: "no model is configured".to_string(),
            }),
            AgentRunStatus::Failed
        );
    }

    #[test]
    fn completing_is_succeeding_and_says_nothing_about_being_right() {
        assert_eq!(
            status_of(&folded(RunEvent::Completed {
                outputs: BTreeMap::new(),
            })),
            AgentRunStatus::Succeeded
        );
    }
}
