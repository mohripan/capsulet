use async_trait::async_trait;
use capsulet_application::{
    AgentRepository, AgentRunRecord, AgentRuntimeRepository, AgentTraceRecord,
};
use capsulet_core::{
    AgentBudget, AgentDefinition, AgentId, AgentRunId, AgentRunStatus, AgentTerminationPolicy,
    TerminationCondition,
};
use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

impl PostgresStore {
    /// Saves or replaces an agent definition and its backing typed graph.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_agent(&self, agent: &AgentDefinition) -> Result<(), PostgresStoreError> {
        self.upsert_graph(agent.graph()).await?;
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r"
            INSERT INTO agent_definitions (
                id, name, graph_id, budget_max_steps, budget_max_tokens,
                budget_max_seconds, budget_max_cost_micros, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, now())
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                graph_id = EXCLUDED.graph_id,
                budget_max_steps = EXCLUDED.budget_max_steps,
                budget_max_tokens = EXCLUDED.budget_max_tokens,
                budget_max_seconds = EXCLUDED.budget_max_seconds,
                budget_max_cost_micros = EXCLUDED.budget_max_cost_micros,
                updated_at = now()
            ",
        )
        .bind(agent.id().as_str())
        .bind(agent.name())
        .bind(agent.graph().id().as_str())
        .bind(
            i32::try_from(agent.budget().max_steps())
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        )
        .bind(
            i64::try_from(agent.budget().max_tokens())
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        )
        .bind(
            i64::try_from(agent.budget().max_seconds())
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        )
        .bind(
            i64::try_from(agent.budget().max_cost_micros())
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM agent_termination_conditions WHERE agent_id = $1")
            .bind(agent.id().as_str())
            .execute(&mut *tx)
            .await?;
        for (position, condition) in agent.termination_policy().conditions().iter().enumerate() {
            sqlx::query(
                r"
                INSERT INTO agent_termination_conditions (agent_id, position, condition)
                VALUES ($1, $2, $3)
                ",
            )
            .bind(agent.id().as_str())
            .bind(
                i32::try_from(position).map_err(|error| {
                    PostgresStoreError::InvalidPersistedValue(error.to_string())
                })?,
            )
            .bind(condition.to_string())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Finds one agent definition.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn find_agent(
        &self,
        id: &AgentId,
    ) -> Result<Option<AgentDefinition>, PostgresStoreError> {
        let Some(row) = sqlx::query(
            r"
            SELECT id, name, graph_id, budget_max_steps, budget_max_tokens,
                   budget_max_seconds, budget_max_cost_micros
            FROM agent_definitions
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let graph_id = capsulet_core::GraphId::new(row.try_get::<String, _>("graph_id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?;
        let Some(graph) = self.find_graph(&graph_id).await? else {
            return Err(PostgresStoreError::InvalidPersistedValue(format!(
                "agent {id} references missing graph {graph_id}"
            )));
        };
        Ok(Some(AgentDefinition::new(
            AgentId::new(row.try_get::<String, _>("id")?)
                .map_err(PostgresStoreError::InvalidPersistedValue)?,
            row.try_get::<String, _>("name")?,
            graph,
            Some(AgentBudget::new(
                u32::try_from(row.try_get::<i32, _>("budget_max_steps")?).map_err(|error| {
                    PostgresStoreError::InvalidPersistedValue(error.to_string())
                })?,
                u64::try_from(row.try_get::<i64, _>("budget_max_tokens")?).map_err(|error| {
                    PostgresStoreError::InvalidPersistedValue(error.to_string())
                })?,
                u64::try_from(row.try_get::<i64, _>("budget_max_seconds")?).map_err(|error| {
                    PostgresStoreError::InvalidPersistedValue(error.to_string())
                })?,
                u64::try_from(row.try_get::<i64, _>("budget_max_cost_micros")?).map_err(
                    |error| PostgresStoreError::InvalidPersistedValue(error.to_string()),
                )?,
            )?),
            Some(self.load_termination_policy(id).await?),
        )?))
    }

    /// Lists agent definitions by most recent update.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn list_agents(
        &self,
        limit: i64,
    ) -> Result<Vec<AgentDefinition>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id
            FROM agent_definitions
            ORDER BY updated_at DESC, id ASC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut agents = Vec::with_capacity(rows.len());
        for row in rows {
            let agent_id = AgentId::new(row.try_get::<String, _>("id")?)
                .map_err(PostgresStoreError::InvalidPersistedValue)?;
            let Some(agent) = self.find_agent(&agent_id).await? else {
                continue;
            };
            agents.push(agent);
        }
        Ok(agents)
    }

    /// Saves or replaces an agent run and its current state snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_agent_run(&self, run: &AgentRunRecord) -> Result<(), PostgresStoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r"
            INSERT INTO agent_runs (id, agent_id, status, state_version, state_json, updated_at)
            VALUES ($1, $2, $3, $4, $5, now())
            ON CONFLICT (id) DO UPDATE SET
                agent_id = EXCLUDED.agent_id,
                status = EXCLUDED.status,
                state_version = EXCLUDED.state_version,
                state_json = EXCLUDED.state_json,
                updated_at = now()
            ",
        )
        .bind(run.id.as_str())
        .bind(run.agent_id.as_str())
        .bind(run.status.to_string())
        .bind(
            i64::try_from(run.state_version)
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        )
        .bind(run.state_json.as_str())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r"
            INSERT INTO agent_state_snapshots (agent_run_id, version, state_json)
            VALUES ($1, $2, $3)
            ON CONFLICT (agent_run_id, version) DO UPDATE SET
                state_json = EXCLUDED.state_json
            ",
        )
        .bind(run.id.as_str())
        .bind(
            i64::try_from(run.state_version)
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        )
        .bind(run.state_json.as_str())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Finds one agent run.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn find_agent_run(
        &self,
        id: &AgentRunId,
    ) -> Result<Option<AgentRunRecord>, PostgresStoreError> {
        let Some(row) = sqlx::query(
            r"
            SELECT id, agent_id, status, state_version, state_json
            FROM agent_runs
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(AgentRunRecord {
            id: AgentRunId::new(row.try_get::<String, _>("id")?)
                .map_err(PostgresStoreError::InvalidPersistedValue)?,
            agent_id: AgentId::new(row.try_get::<String, _>("agent_id")?)
                .map_err(PostgresStoreError::InvalidPersistedValue)?,
            status: parse_agent_run_status(&row.try_get::<String, _>("status")?)?,
            state_version: u64::try_from(row.try_get::<i64, _>("state_version")?)
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
            state_json: row.try_get::<String, _>("state_json")?,
        }))
    }

    /// Lists agent runs by most recent update.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn list_agent_runs(
        &self,
        limit: i64,
    ) -> Result<Vec<AgentRunRecord>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, agent_id, status, state_version, state_json
            FROM agent_runs
            ORDER BY updated_at DESC, id ASC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(AgentRunRecord {
                    id: AgentRunId::new(row.try_get::<String, _>("id")?)
                        .map_err(PostgresStoreError::InvalidPersistedValue)?,
                    agent_id: AgentId::new(row.try_get::<String, _>("agent_id")?)
                        .map_err(PostgresStoreError::InvalidPersistedValue)?,
                    status: parse_agent_run_status(&row.try_get::<String, _>("status")?)?,
                    state_version: u64::try_from(row.try_get::<i64, _>("state_version")?).map_err(
                        |error| PostgresStoreError::InvalidPersistedValue(error.to_string()),
                    )?,
                    state_json: row.try_get::<String, _>("state_json")?,
                })
            })
            .collect()
    }

    /// Appends or replaces an agent trace event at a run-local sequence.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn append_agent_trace_event(
        &self,
        event: &AgentTraceRecord,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO agent_trace_events (agent_run_id, sequence, event_type, payload_json)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (agent_run_id, sequence) DO UPDATE SET
                event_type = EXCLUDED.event_type,
                payload_json = EXCLUDED.payload_json
            ",
        )
        .bind(event.run_id.as_str())
        .bind(
            i64::try_from(event.sequence)
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        )
        .bind(event.event_type.as_str())
        .bind(event.payload_json.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Lists trace events for one agent run in sequence order.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn list_agent_trace_events(
        &self,
        run_id: &AgentRunId,
    ) -> Result<Vec<AgentTraceRecord>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT agent_run_id, sequence, event_type, payload_json
            FROM agent_trace_events
            WHERE agent_run_id = $1
            ORDER BY sequence ASC
            ",
        )
        .bind(run_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(AgentTraceRecord {
                    run_id: AgentRunId::new(row.try_get::<String, _>("agent_run_id")?)
                        .map_err(PostgresStoreError::InvalidPersistedValue)?,
                    sequence: u64::try_from(row.try_get::<i64, _>("sequence")?).map_err(
                        |error| PostgresStoreError::InvalidPersistedValue(error.to_string()),
                    )?,
                    event_type: row.try_get::<String, _>("event_type")?,
                    payload_json: row.try_get::<String, _>("payload_json")?,
                })
            })
            .collect()
    }

    async fn load_termination_policy(
        &self,
        agent_id: &AgentId,
    ) -> Result<AgentTerminationPolicy, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT condition
            FROM agent_termination_conditions
            WHERE agent_id = $1
            ORDER BY position ASC
            ",
        )
        .bind(agent_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        Ok(AgentTerminationPolicy::new(
            rows.iter()
                .map(|row| parse_termination_condition(&row.try_get::<String, _>("condition")?))
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
}

#[async_trait]
impl AgentRepository for PostgresStore {
    type Error = PostgresStoreError;

    async fn save_agent(&self, agent: &AgentDefinition) -> Result<(), Self::Error> {
        self.upsert_agent(agent).await
    }

    async fn save_run(&self, run: &AgentRunRecord) -> Result<(), Self::Error> {
        self.upsert_agent_run(run).await
    }
}

#[async_trait]
impl AgentRuntimeRepository for PostgresStore {
    type Error = PostgresStoreError;

    async fn save_agent_run(&self, run: &AgentRunRecord) -> Result<(), Self::Error> {
        self.upsert_agent_run(run).await
    }

    async fn append_trace_event(&self, event: &AgentTraceRecord) -> Result<(), Self::Error> {
        self.append_agent_trace_event(event).await
    }
}

fn parse_agent_run_status(value: &str) -> Result<AgentRunStatus, PostgresStoreError> {
    match value {
        "queued" => Ok(AgentRunStatus::Queued),
        "running" => Ok(AgentRunStatus::Running),
        "succeeded" => Ok(AgentRunStatus::Succeeded),
        "failed" => Ok(AgentRunStatus::Failed),
        "cancelled" => Ok(AgentRunStatus::Cancelled),
        "stopped" => Ok(AgentRunStatus::Stopped),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown agent run status {value}"
        ))),
    }
}

fn parse_termination_condition(value: &str) -> Result<TerminationCondition, PostgresStoreError> {
    match value {
        "validator_pass" => Ok(TerminationCondition::ValidatorPass),
        "safety_failure" => Ok(TerminationCondition::SafetyFailure),
        "no_progress" => Ok(TerminationCondition::NoProgress),
        "human_escalation" => Ok(TerminationCondition::HumanEscalation),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown termination condition {value}"
        ))),
    }
}
