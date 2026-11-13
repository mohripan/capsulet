//! Typed graphs, agent definitions, and agent runs.

use super::super::*;

#[async_trait::async_trait]
impl AgentStore for FakeStore {
    async fn upsert_graph(
        &self,
        graph: &GraphDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        self.claim_resource("graphs", graph.id().as_str(), tenant_id, project_id)?;
        let mut graphs = self.graphs.lock().map_err(|error| error.to_string())?;
        graphs.retain(|existing| existing.id() != graph.id());
        graphs.push(graph.clone());
        Ok(())
    }

    async fn list_graphs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<GraphDefinition>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        let graphs = self
            .graphs
            .lock()
            .map_err(|error| error.to_string())?
            .clone();
        Ok(graphs
            .into_iter()
            .filter(|graph| self.owns("graphs", graph.id().as_str(), tenant_id, project_id))
            .take(limit)
            .collect())
    }

    async fn find_graph(
        &self,
        id: &GraphId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<GraphDefinition>, Self::Error> {
        if !self.owns("graphs", id.as_str(), tenant_id, project_id) {
            return Ok(None);
        }
        Ok(self
            .graphs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|graph| graph.id() == id)
            .cloned())
    }

    async fn upsert_agent(
        &self,
        agent: &AgentDefinition,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        if !self.owns("graphs", agent.graph().id().as_str(), tenant_id, project_id) {
            return Err("scope mismatch".to_string());
        }
        self.claim_resource("agents", agent.id().as_str(), tenant_id, project_id)?;
        let mut agents = self.agents.lock().map_err(|error| error.to_string())?;
        agents.retain(|existing| existing.id() != agent.id());
        agents.push(agent.clone());
        Ok(())
    }

    async fn list_agents(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<AgentDefinition>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        let agents = self
            .agents
            .lock()
            .map_err(|error| error.to_string())?
            .clone();
        Ok(agents
            .into_iter()
            .filter(|agent| self.owns("agents", agent.id().as_str(), tenant_id, project_id))
            .take(limit)
            .collect())
    }

    async fn find_agent(
        &self,
        id: &AgentId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<AgentDefinition>, Self::Error> {
        if !self.owns("agents", id.as_str(), tenant_id, project_id) {
            return Ok(None);
        }
        Ok(self
            .agents
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|agent| agent.id() == id)
            .cloned())
    }

    async fn start_agent_run(
        &self,
        start: &capsulet_postgres::AgentRunStart<'_>,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<(), Self::Error> {
        let run = start.run;
        if !self.owns("agents", run.agent_id.as_str(), tenant_id, project_id) {
            return Err("scope mismatch".to_string());
        }
        let mut runs = self.agent_runs.lock().map_err(|error| error.to_string())?;
        if runs.iter().any(|existing| existing.id == run.id) {
            return Err(
                if self.owns("agent_runs", run.id.as_str(), tenant_id, project_id) {
                    "already exists".to_string()
                } else {
                    "scope mismatch".to_string()
                },
            );
        }
        self.claim_resource("agent_runs", run.id.as_str(), tenant_id, project_id)?;
        runs.push(run.clone());
        Ok(())
    }

    async fn list_agent_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<AgentRunRecord>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        let runs = self
            .agent_runs
            .lock()
            .map_err(|error| error.to_string())?
            .clone();
        Ok(runs
            .into_iter()
            .filter(|run| self.owns("agent_runs", run.id.as_str(), tenant_id, project_id))
            .take(limit)
            .collect())
    }

    async fn find_agent_run(
        &self,
        id: &AgentRunId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<AgentRunRecord>, Self::Error> {
        if !self.owns("agent_runs", id.as_str(), tenant_id, project_id) {
            return Ok(None);
        }
        Ok(self
            .agent_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|run| run.id == *id)
            .cloned())
    }
}
