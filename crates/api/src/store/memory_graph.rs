//! The memory graph: subgraphs and their members and edges, canonical entities, entity resolutions,
//! summary traces and graph attachments.

use async_trait::async_trait;
use capsulet_core::{
    CanonicalEntity, ClaimId, EntityGraphAttachment, EntityResolution, EntityResolutionId,
    MemorySubgraph, MemorySubgraphId, MemorySubgraphMember, SubgraphEdge, SummaryTrace,
};
use capsulet_postgres::PostgresStore;

use super::StoreBase;

/// The memory graph: subgraphs and their members and edges, canonical entities, entity resolutions,
/// summary traces and graph attachments.
#[async_trait]
pub trait MemoryGraphStore: StoreBase {
    /// A lookup within one project: another project's record is not found.
    async fn find_memory_subgraph_in_project(
        &self,
        id: &MemorySubgraphId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<MemorySubgraph>, Self::Error> {
        Ok(self.find_memory_subgraph(id).await?.filter(|found| {
            found.scope().tenant_id() == tenant_id && found.scope().project_id() == project_id
        }))
    }

    /// A lookup within one project: another project's record is not found.
    async fn find_memory_entity_resolution_in_project(
        &self,
        id: &EntityResolutionId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<EntityResolution>, Self::Error> {
        Ok(self
            .find_memory_entity_resolution(id)
            .await?
            .filter(|found| {
                found.scope().tenant_id() == tenant_id && found.scope().project_id() == project_id
            }))
    }

    /// Saves a subgraph. Like every write below that names a subgraph, it refuses a
    /// subgraph outside the record's own project with an error
    /// [`Self::is_scope_mismatch`] recognises.
    async fn upsert_memory_subgraph(&self, subgraph: &MemorySubgraph) -> Result<(), Self::Error>;

    async fn list_memory_subgraphs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<MemorySubgraph>, Self::Error>;

    async fn find_memory_subgraph(
        &self,
        id: &MemorySubgraphId,
    ) -> Result<Option<MemorySubgraph>, Self::Error>;

    async fn upsert_memory_subgraph_member(
        &self,
        member: &MemorySubgraphMember,
    ) -> Result<(), Self::Error>;

    async fn upsert_memory_canonical_entity(
        &self,
        entity: &CanonicalEntity,
    ) -> Result<(), Self::Error>;

    async fn list_memory_canonical_entities(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<CanonicalEntity>, Self::Error>;

    async fn upsert_memory_entity_resolution(
        &self,
        resolution: &EntityResolution,
    ) -> Result<(), Self::Error>;

    async fn list_memory_entity_resolutions(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<EntityResolution>, Self::Error>;

    async fn find_memory_entity_resolution(
        &self,
        id: &EntityResolutionId,
    ) -> Result<Option<EntityResolution>, Self::Error>;

    async fn upsert_memory_subgraph_edge(&self, edge: &SubgraphEdge) -> Result<(), Self::Error>;

    async fn upsert_memory_summary_trace(&self, trace: &SummaryTrace) -> Result<(), Self::Error>;

    /// The project's own traces for a summary; another project's trace never counts.
    async fn list_memory_summary_traces(
        &self,
        subgraph_id: &MemorySubgraphId,
        summary_claim_id: &ClaimId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<SummaryTrace>, Self::Error>;

    async fn upsert_memory_entity_graph_attachment(
        &self,
        attachment: &EntityGraphAttachment,
    ) -> Result<(), Self::Error>;
}

#[async_trait]
impl MemoryGraphStore for PostgresStore {
    async fn find_memory_subgraph_in_project(
        &self,
        id: &MemorySubgraphId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<MemorySubgraph>, Self::Error> {
        PostgresStore::find_memory_subgraph_in_project(self, id, tenant_id, project_id).await
    }

    async fn find_memory_entity_resolution_in_project(
        &self,
        id: &EntityResolutionId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<EntityResolution>, Self::Error> {
        PostgresStore::find_memory_entity_resolution_in_project(self, id, tenant_id, project_id)
            .await
    }

    async fn upsert_memory_subgraph(&self, subgraph: &MemorySubgraph) -> Result<(), Self::Error> {
        self.upsert_memory_subgraph(subgraph).await
    }

    async fn list_memory_subgraphs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<MemorySubgraph>, Self::Error> {
        self.list_memory_subgraphs(tenant_id, project_id, limit)
            .await
    }

    async fn find_memory_subgraph(
        &self,
        id: &MemorySubgraphId,
    ) -> Result<Option<MemorySubgraph>, Self::Error> {
        self.find_memory_subgraph(id).await
    }

    async fn upsert_memory_subgraph_member(
        &self,
        member: &MemorySubgraphMember,
    ) -> Result<(), Self::Error> {
        self.upsert_memory_subgraph_member(member).await
    }

    async fn upsert_memory_canonical_entity(
        &self,
        entity: &CanonicalEntity,
    ) -> Result<(), Self::Error> {
        self.upsert_memory_canonical_entity(entity).await
    }

    async fn list_memory_canonical_entities(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<CanonicalEntity>, Self::Error> {
        self.list_memory_canonical_entities(tenant_id, project_id, limit)
            .await
    }

    async fn upsert_memory_entity_resolution(
        &self,
        resolution: &EntityResolution,
    ) -> Result<(), Self::Error> {
        self.upsert_memory_entity_resolution(resolution).await
    }

    async fn list_memory_entity_resolutions(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<EntityResolution>, Self::Error> {
        self.list_memory_entity_resolutions(tenant_id, project_id, limit)
            .await
    }

    async fn find_memory_entity_resolution(
        &self,
        id: &EntityResolutionId,
    ) -> Result<Option<EntityResolution>, Self::Error> {
        self.find_memory_entity_resolution(id).await
    }

    async fn upsert_memory_subgraph_edge(&self, edge: &SubgraphEdge) -> Result<(), Self::Error> {
        self.upsert_memory_subgraph_edge(edge).await
    }

    async fn upsert_memory_summary_trace(&self, trace: &SummaryTrace) -> Result<(), Self::Error> {
        self.upsert_memory_summary_trace(trace).await
    }

    async fn list_memory_summary_traces(
        &self,
        subgraph_id: &MemorySubgraphId,
        summary_claim_id: &ClaimId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<SummaryTrace>, Self::Error> {
        self.list_memory_summary_traces(subgraph_id, summary_claim_id, tenant_id, project_id)
            .await
    }

    async fn upsert_memory_entity_graph_attachment(
        &self,
        attachment: &EntityGraphAttachment,
    ) -> Result<(), Self::Error> {
        self.upsert_memory_entity_graph_attachment(attachment).await
    }
}
