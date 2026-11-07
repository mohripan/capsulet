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
    /// Saves or replaces an agent definition and its backing typed graph, in the
    /// default project.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_agent(&self, agent: &AgentDefinition) -> Result<(), PostgresStoreError> {
        self.upsert_graph(agent.graph()).await?;
        self.upsert_agent_in_project(agent, "default", "default")
            .await
    }

    /// Saves or replaces an agent definition owned by one project.
    ///
    /// The graph it binds must already belong to the same project, and stays
    /// locked until the write commits, so an agent can never end up bound to a
    /// graph its project does not own. The graph itself is not written: an agent
    /// carries a copy, and saving one must not revert whatever the graph's own
    /// endpoint last stored.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError::ScopeMismatch`] when the graph or the agent
    /// id belongs to another project, and [`PostgresStoreError`] when persistence
    /// fails.
    pub async fn upsert_agent_in_project(
        &self,
        agent: &AgentDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), PostgresStoreError> {
        let mut tx = self.pool.begin().await?;
        let graph_owned = sqlx::query_scalar::<_, i32>(
            r"
            SELECT 1 FROM graph_definitions
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            FOR SHARE
            ",
        )
        .bind(agent.graph().id().as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&mut *tx)
        .await?;
        if graph_owned.is_none() {
            return Err(PostgresStoreError::ScopeMismatch);
        }

        let written = sqlx::query(
            r"
            INSERT INTO agent_definitions (
                id, name, graph_id, budget_max_steps, budget_max_tokens,
                budget_max_seconds, budget_max_cost_micros, tenant_id, project_id, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                graph_id = EXCLUDED.graph_id,
                budget_max_steps = EXCLUDED.budget_max_steps,
                budget_max_tokens = EXCLUDED.budget_max_tokens,
                budget_max_seconds = EXCLUDED.budget_max_seconds,
                budget_max_cost_micros = EXCLUDED.budget_max_cost_micros,
                updated_at = now()
            WHERE agent_definitions.tenant_id = EXCLUDED.tenant_id
              AND agent_definitions.project_id = EXCLUDED.project_id
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
        .bind(tenant_id)
        .bind(project_id)
        .execute(&mut *tx)
        .await?;
        if written.rows_affected() == 0 {
            return Err(PostgresStoreError::ScopeMismatch);
        }

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

    /// Finds one agent definition inside a project.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn find_agent_in_project(
        &self,
        id: &AgentId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<AgentDefinition>, PostgresStoreError> {
        let owned = sqlx::query_scalar::<_, i32>(
            "SELECT 1 FROM agent_definitions WHERE id = $1 AND tenant_id = $2 AND project_id = $3",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        if owned.is_none() {
            return Ok(None);
        }
        self.find_agent(id).await
    }

    /// Lists a project's agents by most recent update, scoping before the limit.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn list_agents_in_project(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<AgentDefinition>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id
            FROM agent_definitions
            WHERE tenant_id = $1 AND project_id = $2
            ORDER BY updated_at DESC, id ASC
            LIMIT $3
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut agents = Vec::with_capacity(rows.len());
        for row in rows {
            let agent_id = AgentId::new(row.try_get::<String, _>("id")?)
                .map_err(PostgresStoreError::InvalidPersistedValue)?;
            if let Some(agent) = self.find_agent(&agent_id).await? {
                agents.push(agent);
            }
        }
        Ok(agents)
    }

    /// Creates a queued agent run owned by one project.
    ///
    /// Insert only. An id that already exists is a conflict rather than an
    /// overwrite, because overwriting would reset a run that may be under way.
    /// The agent must belong to the same project, and stays locked until the
    /// write commits.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError::ScopeMismatch`] when the agent or the run id
    /// belongs to another project, [`PostgresStoreError::AlreadyExists`] when
    /// this project already has a run with this id, and [`PostgresStoreError`]
    /// when persistence fails.
    pub async fn create_agent_run_in_project(
        &self,
        run: &AgentRunRecord,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), PostgresStoreError> {
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

        let state_version = i64::try_from(run.state_version)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;
        let written = sqlx::query(
            r"
            INSERT INTO agent_runs (
                id, agent_id, status, state_version, state_json, tenant_id, project_id, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, now())
            ON CONFLICT (id) DO NOTHING
            ",
        )
        .bind(run.id.as_str())
        .bind(run.agent_id.as_str())
        .bind(run.status.to_string())
        .bind(state_version)
        .bind(run.state_json.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .execute(&mut *tx)
        .await?;
        if written.rows_affected() == 0 {
            let same_project = sqlx::query_scalar::<_, i32>(
                "SELECT 1 FROM agent_runs WHERE id = $1 AND tenant_id = $2 AND project_id = $3",
            )
            .bind(run.id.as_str())
            .bind(tenant_id)
            .bind(project_id)
            .fetch_optional(&mut *tx)
            .await?;
            return Err(if same_project.is_some() {
                PostgresStoreError::AlreadyExists(run.id.as_str().to_string())
            } else {
                PostgresStoreError::ScopeMismatch
            });
        }

        sqlx::query(
            r"
            INSERT INTO agent_state_snapshots (agent_run_id, version, state_json)
            VALUES ($1, $2, $3)
            ",
        )
        .bind(run.id.as_str())
        .bind(state_version)
        .bind(run.state_json.as_str())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Finds one agent run inside a project.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn find_agent_run_in_project(
        &self,
        id: &AgentRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<AgentRunRecord>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, agent_id, status, state_version, state_json
            FROM agent_runs
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_agent_run).transpose()
    }

    /// Lists a project's agent runs by most recent update, scoping before the limit.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn list_agent_runs_in_project(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<AgentRunRecord>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, agent_id, status, state_version, state_json
            FROM agent_runs
            WHERE tenant_id = $1 AND project_id = $2
            ORDER BY updated_at DESC, id ASC
            LIMIT $3
            ",
        )
        .bind(tenant_id)
        .bind(project_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_agent_run).collect()
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
        row_to_agent_run(&row).map(Some)
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
        rows.iter().map(row_to_agent_run).collect()
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

fn row_to_agent_run(row: &sqlx::postgres::PgRow) -> Result<AgentRunRecord, PostgresStoreError> {
    Ok(AgentRunRecord {
        id: AgentRunId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        agent_id: AgentId::new(row.try_get::<String, _>("agent_id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        status: parse_agent_run_status(&row.try_get::<String, _>("status")?)?,
        state_version: u64::try_from(row.try_get::<i64, _>("state_version")?)
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        state_json: row.try_get::<String, _>("state_json")?,
    })
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
