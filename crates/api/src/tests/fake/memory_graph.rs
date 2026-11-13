//! The memory graph.

use super::super::*;

#[async_trait::async_trait]
impl MemoryGraphStore for FakeStore {
    async fn upsert_memory_subgraph(&self, subgraph: &MemorySubgraph) -> Result<(), Self::Error> {
        if let Some(parent) = subgraph.parent_subgraph_id() {
            self.require_subgraph_in_scope(parent, subgraph.scope())?;
        }
        let mut subgraphs = self
            .memory_subgraphs
            .lock()
            .map_err(|error| error.to_string())?;
        subgraphs.retain(|existing| existing.id() != subgraph.id());
        subgraphs.push(subgraph.clone());
        Ok(())
    }

    async fn list_memory_subgraphs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<MemorySubgraph>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_subgraphs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|subgraph| {
                subgraph.scope().tenant_id() == tenant_id
                    && subgraph.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_subgraph(
        &self,
        id: &capsulet_core::MemorySubgraphId,
    ) -> Result<Option<MemorySubgraph>, Self::Error> {
        Ok(self
            .memory_subgraphs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|subgraph| subgraph.id() == id)
            .cloned())
    }

    async fn upsert_memory_subgraph_member(
        &self,
        member: &MemorySubgraphMember,
    ) -> Result<(), Self::Error> {
        self.require_subgraph_in_scope(member.subgraph_id(), member.scope())?;
        let mut members = self
            .memory_subgraph_members
            .lock()
            .map_err(|error| error.to_string())?;
        members.retain(|existing| existing.id() != member.id());
        members.push(member.clone());
        Ok(())
    }

    async fn upsert_memory_canonical_entity(
        &self,
        entity: &CanonicalEntity,
    ) -> Result<(), Self::Error> {
        let mut entities = self
            .memory_canonical_entities
            .lock()
            .map_err(|error| error.to_string())?;
        entities.retain(|existing| existing.id() != entity.id());
        entities.push(entity.clone());
        Ok(())
    }

    async fn list_memory_canonical_entities(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<CanonicalEntity>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_canonical_entities
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|entity| {
                entity.scope().tenant_id() == tenant_id && entity.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn upsert_memory_entity_resolution(
        &self,
        resolution: &EntityResolution,
    ) -> Result<(), Self::Error> {
        self.require_subgraph_in_scope(resolution.subgraph_id(), resolution.scope())?;
        let mut resolutions = self
            .memory_entity_resolutions
            .lock()
            .map_err(|error| error.to_string())?;
        resolutions.retain(|existing| existing.id() != resolution.id());
        resolutions.push(resolution.clone());
        Ok(())
    }

    async fn list_memory_entity_resolutions(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<EntityResolution>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_entity_resolutions
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|resolution| {
                resolution.scope().tenant_id() == tenant_id
                    && resolution.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_entity_resolution(
        &self,
        id: &EntityResolutionId,
    ) -> Result<Option<EntityResolution>, Self::Error> {
        Ok(self
            .memory_entity_resolutions
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|resolution| resolution.id() == id)
            .cloned())
    }

    async fn upsert_memory_subgraph_edge(&self, edge: &SubgraphEdge) -> Result<(), Self::Error> {
        self.require_subgraph_in_scope(edge.from_subgraph_id(), edge.scope())?;
        self.require_subgraph_in_scope(edge.to_subgraph_id(), edge.scope())?;
        let mut edges = self
            .memory_subgraph_edges
            .lock()
            .map_err(|error| error.to_string())?;
        edges.retain(|existing| existing.id() != edge.id());
        edges.push(edge.clone());
        Ok(())
    }

    async fn upsert_memory_summary_trace(&self, trace: &SummaryTrace) -> Result<(), Self::Error> {
        self.require_subgraph_in_scope(trace.subgraph_id(), trace.scope())?;
        let mut traces = self
            .memory_summary_traces
            .lock()
            .map_err(|error| error.to_string())?;
        traces.retain(|existing| existing.id() != trace.id());
        traces.push(trace.clone());
        Ok(())
    }

    async fn list_memory_summary_traces(
        &self,
        subgraph_id: &capsulet_core::MemorySubgraphId,
        summary_claim_id: &ClaimId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<SummaryTrace>, Self::Error> {
        Ok(self
            .memory_summary_traces
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|trace| {
                trace.subgraph_id() == subgraph_id
                    && trace.summary_claim_id() == summary_claim_id
                    && trace.scope().tenant_id() == tenant_id
                    && trace.scope().project_id() == project_id
            })
            .cloned()
            .collect())
    }

    async fn upsert_memory_entity_graph_attachment(
        &self,
        attachment: &EntityGraphAttachment,
    ) -> Result<(), Self::Error> {
        self.require_subgraph_in_scope(attachment.subgraph_id(), attachment.scope())?;
        let mut attachments = self
            .memory_entity_graph_attachments
            .lock()
            .map_err(|error| error.to_string())?;
        attachments.retain(|existing| existing.id() != attachment.id());
        attachments.push(attachment.clone());
        Ok(())
    }
}
