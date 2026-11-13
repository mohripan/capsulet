//! Typed graphs, agent definitions, and agent runs.

use async_trait::async_trait;
use capsulet_application::AgentRunRecord;
use capsulet_core::{AgentDefinition, AgentId, AgentRunId, GraphDefinition, GraphId};
use capsulet_postgres::PostgresStore;

use super::StoreBase;

/// Typed graphs, agent definitions, and agent runs.
#[async_trait]
pub trait AgentStore: StoreBase {
    async fn upsert_graph(
        &self,
        graph: &GraphDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error>;

    async fn list_graphs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<GraphDefinition>, Self::Error>;

    async fn find_graph(
        &self,
        id: &GraphId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<GraphDefinition>, Self::Error>;

    async fn upsert_agent(
        &self,
        agent: &AgentDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error>;

    async fn list_agents(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<AgentDefinition>, Self::Error>;

    async fn find_agent(
        &self,
        id: &AgentId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<AgentDefinition>, Self::Error>;

    /// Starts an agent run: its definition, its IR run, its inputs, all or nothing.
    async fn start_agent_run(
        &self,
        start: &capsulet_postgres::AgentRunStart<'_>,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error>;

    /// Delivers something from outside to a run's inbox, for the worker to answer.
    async fn deliver_ir_run_signal(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _run_id: &str,
        _signal: &capsulet_runtime::Signal,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Erases what a finished agent run was given and produced. `None` when the
    /// project has no such run.
    async fn erase_agent_run_payloads(
        &self,
        _id: &AgentRunId,
        _tenant_id: &str,
        _project_id: &str,
        _by: &str,
    ) -> Result<Option<capsulet_postgres::PayloadErasure>, Self::Error> {
        Ok(None)
    }

    async fn list_agent_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<AgentRunRecord>, Self::Error>;

    async fn find_agent_run(
        &self,
        id: &AgentRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<AgentRunRecord>, Self::Error>;
}

#[async_trait]
impl AgentStore for PostgresStore {
    async fn upsert_graph(
        &self,
        graph: &GraphDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        self.upsert_graph_in_project(graph, tenant_id, project_id)
            .await
    }

    async fn list_graphs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<GraphDefinition>, Self::Error> {
        self.list_graphs_in_project(tenant_id, project_id, limit)
            .await
    }

    async fn find_graph(
        &self,
        id: &GraphId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<GraphDefinition>, Self::Error> {
        self.find_graph_in_project(id, tenant_id, project_id).await
    }

    async fn upsert_agent(
        &self,
        agent: &AgentDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        self.upsert_agent_in_project(agent, tenant_id, project_id)
            .await
    }

    async fn list_agents(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<AgentDefinition>, Self::Error> {
        self.list_agents_in_project(tenant_id, project_id, limit)
            .await
    }

    async fn find_agent(
        &self,
        id: &AgentId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<AgentDefinition>, Self::Error> {
        self.find_agent_in_project(id, tenant_id, project_id).await
    }

    async fn start_agent_run(
        &self,
        start: &capsulet_postgres::AgentRunStart<'_>,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        self.start_agent_run_in_project(start, tenant_id, project_id)
            .await
    }

    async fn deliver_ir_run_signal(
        &self,
        tenant_id: &str,
        project_id: &str,
        run_id: &str,
        signal: &capsulet_runtime::Signal,
    ) -> Result<(), Self::Error> {
        PostgresStore::deliver_ir_run_signal(self, tenant_id, project_id, run_id, signal)
            .await
            .map(|_| ())
    }

    async fn erase_agent_run_payloads(
        &self,
        id: &AgentRunId,
        tenant_id: &str,
        project_id: &str,
        by: &str,
    ) -> Result<Option<capsulet_postgres::PayloadErasure>, Self::Error> {
        self.erase_agent_run_payloads_in_project(id, tenant_id, project_id, by)
            .await
    }

    async fn list_agent_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<AgentRunRecord>, Self::Error> {
        self.list_agent_runs_with_progress(tenant_id, project_id, limit)
            .await
    }

    async fn find_agent_run(
        &self,
        id: &AgentRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<AgentRunRecord>, Self::Error> {
        self.find_agent_run_with_progress(id, tenant_id, project_id)
            .await
    }
}
