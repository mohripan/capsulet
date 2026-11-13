//! Memory records: sources and their bytes, evidence, entities, claims and their conflicts, events,
//! relationships and contracts.

use async_trait::async_trait;
use capsulet_core::{
    Claim, ClaimConflict, ClaimConflictId, ClaimId, Entity, EntityId, Event, EventId, Evidence,
    EvidenceId, MemoryContract, MemoryContractId, Relationship, RelationshipId, Source,
    SourceContent, SourceId,
};
use capsulet_postgres::PostgresStore;

use super::StoreBase;

/// Memory records: sources and their bytes, evidence, entities, claims and their conflicts, events,
/// relationships and contracts.
#[async_trait]
pub trait MemoryStore: StoreBase {
    async fn upsert_memory_source(&self, source: &Source) -> Result<(), Self::Error>;

    async fn list_memory_sources(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Source>, Self::Error>;

    /// A lookup within one project: another project's record is not found.
    async fn find_memory_source_in_project(
        &self,
        id: &SourceId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Source>, Self::Error> {
        Ok(self.find_memory_source(id).await?.filter(|found| {
            found.scope().tenant_id() == tenant_id && found.scope().project_id() == project_id
        }))
    }

    /// A lookup within one project: another project's record is not found.
    async fn find_memory_evidence_in_project(
        &self,
        id: &EvidenceId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Evidence>, Self::Error> {
        Ok(self.find_memory_evidence(id).await?.filter(|found| {
            found.scope().tenant_id() == tenant_id && found.scope().project_id() == project_id
        }))
    }

    /// A lookup within one project: another project's record is not found.
    async fn find_memory_entity_in_project(
        &self,
        id: &EntityId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Entity>, Self::Error> {
        Ok(self.find_memory_entity(id).await?.filter(|found| {
            found.scope().tenant_id() == tenant_id && found.scope().project_id() == project_id
        }))
    }

    /// A lookup within one project: another project's record is not found.
    async fn find_memory_claim_in_project(
        &self,
        id: &ClaimId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Claim>, Self::Error> {
        Ok(self.find_memory_claim(id).await?.filter(|found| {
            found.scope().tenant_id() == tenant_id && found.scope().project_id() == project_id
        }))
    }

    /// A lookup within one project: another project's record is not found.
    async fn find_memory_claim_conflict_in_project(
        &self,
        id: &ClaimConflictId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<ClaimConflict>, Self::Error> {
        Ok(self.find_memory_claim_conflict(id).await?.filter(|found| {
            found.scope().tenant_id() == tenant_id && found.scope().project_id() == project_id
        }))
    }

    /// A lookup within one project: another project's record is not found.
    async fn find_memory_event_in_project(
        &self,
        id: &EventId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Event>, Self::Error> {
        Ok(self.find_memory_event(id).await?.filter(|found| {
            found.scope().tenant_id() == tenant_id && found.scope().project_id() == project_id
        }))
    }

    /// A lookup within one project: another project's record is not found.
    async fn find_memory_relationship_in_project(
        &self,
        id: &RelationshipId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Relationship>, Self::Error> {
        Ok(self.find_memory_relationship(id).await?.filter(|found| {
            found.scope().tenant_id() == tenant_id && found.scope().project_id() == project_id
        }))
    }

    /// A lookup within one project: another project's record is not found.
    async fn find_memory_contract_in_project(
        &self,
        id: &MemoryContractId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<MemoryContract>, Self::Error> {
        Ok(self.find_memory_contract(id).await?.filter(|found| {
            found.scope().tenant_id() == tenant_id && found.scope().project_id() == project_id
        }))
    }

    async fn find_memory_source(&self, id: &SourceId) -> Result<Option<Source>, Self::Error>;

    async fn upsert_memory_evidence(&self, evidence: &Evidence) -> Result<(), Self::Error>;

    async fn list_memory_evidence(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Evidence>, Self::Error>;

    async fn find_memory_evidence(&self, id: &EvidenceId) -> Result<Option<Evidence>, Self::Error>;

    async fn insert_memory_source_content(
        &self,
        _content: &SourceContent,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn find_memory_source_content(
        &self,
        _source_id: &SourceId,
        _content_hash: &str,
    ) -> Result<Option<SourceContent>, Self::Error> {
        Ok(None)
    }

    async fn upsert_memory_entity(&self, entity: &Entity) -> Result<(), Self::Error>;

    async fn list_memory_entities(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Entity>, Self::Error>;

    async fn find_memory_entity(&self, id: &EntityId) -> Result<Option<Entity>, Self::Error>;

    async fn upsert_memory_claim(&self, claim: &Claim) -> Result<(), Self::Error>;

    async fn list_memory_claims(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Claim>, Self::Error>;

    async fn find_memory_claim(&self, id: &ClaimId) -> Result<Option<Claim>, Self::Error>;

    async fn upsert_memory_claim_conflict(
        &self,
        conflict: &ClaimConflict,
    ) -> Result<(), Self::Error>;

    async fn list_memory_claim_conflicts(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<ClaimConflict>, Self::Error>;

    async fn find_memory_claim_conflict(
        &self,
        id: &ClaimConflictId,
    ) -> Result<Option<ClaimConflict>, Self::Error>;

    async fn upsert_memory_event(&self, event: &Event) -> Result<(), Self::Error>;

    async fn list_memory_events(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Event>, Self::Error>;

    async fn find_memory_event(&self, id: &EventId) -> Result<Option<Event>, Self::Error>;

    async fn upsert_memory_relationship(
        &self,
        relationship: &Relationship,
    ) -> Result<(), Self::Error>;

    async fn list_memory_relationships(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Relationship>, Self::Error>;

    async fn find_memory_relationship(
        &self,
        id: &RelationshipId,
    ) -> Result<Option<Relationship>, Self::Error>;

    async fn upsert_memory_contract(&self, contract: &MemoryContract) -> Result<(), Self::Error>;

    /// A project's contracts, scoped before the limit.
    async fn list_memory_contracts(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<MemoryContract>, Self::Error>;

    async fn find_memory_contract(
        &self,
        id: &MemoryContractId,
    ) -> Result<Option<MemoryContract>, Self::Error>;
}

#[async_trait]
impl MemoryStore for PostgresStore {
    async fn upsert_memory_source(&self, source: &Source) -> Result<(), Self::Error> {
        self.upsert_memory_source(source).await
    }

    async fn list_memory_sources(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Source>, Self::Error> {
        self.list_memory_sources(tenant_id, project_id, limit).await
    }

    async fn find_memory_source_in_project(
        &self,
        id: &SourceId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Source>, Self::Error> {
        PostgresStore::find_memory_source_in_project(self, id, tenant_id, project_id).await
    }

    async fn find_memory_evidence_in_project(
        &self,
        id: &EvidenceId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Evidence>, Self::Error> {
        PostgresStore::find_memory_evidence_in_project(self, id, tenant_id, project_id).await
    }

    async fn find_memory_entity_in_project(
        &self,
        id: &EntityId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Entity>, Self::Error> {
        PostgresStore::find_memory_entity_in_project(self, id, tenant_id, project_id).await
    }

    async fn find_memory_claim_in_project(
        &self,
        id: &ClaimId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Claim>, Self::Error> {
        PostgresStore::find_memory_claim_in_project(self, id, tenant_id, project_id).await
    }

    async fn find_memory_claim_conflict_in_project(
        &self,
        id: &ClaimConflictId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<ClaimConflict>, Self::Error> {
        PostgresStore::find_memory_claim_conflict_in_project(self, id, tenant_id, project_id).await
    }

    async fn find_memory_event_in_project(
        &self,
        id: &EventId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Event>, Self::Error> {
        PostgresStore::find_memory_event_in_project(self, id, tenant_id, project_id).await
    }

    async fn find_memory_relationship_in_project(
        &self,
        id: &RelationshipId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Relationship>, Self::Error> {
        PostgresStore::find_memory_relationship_in_project(self, id, tenant_id, project_id).await
    }

    async fn find_memory_contract_in_project(
        &self,
        id: &MemoryContractId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<MemoryContract>, Self::Error> {
        PostgresStore::find_memory_contract_in_project(self, id, tenant_id, project_id).await
    }

    async fn find_memory_source(&self, id: &SourceId) -> Result<Option<Source>, Self::Error> {
        self.find_memory_source(id).await
    }

    async fn insert_memory_source_content(
        &self,
        content: &SourceContent,
    ) -> Result<(), Self::Error> {
        self.insert_memory_source_content(content).await
    }

    async fn find_memory_source_content(
        &self,
        source_id: &SourceId,
        content_hash: &str,
    ) -> Result<Option<SourceContent>, Self::Error> {
        self.find_memory_source_content(source_id, content_hash)
            .await
    }

    async fn upsert_memory_evidence(&self, evidence: &Evidence) -> Result<(), Self::Error> {
        self.upsert_memory_evidence(evidence).await
    }

    async fn list_memory_evidence(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Evidence>, Self::Error> {
        self.list_memory_evidence(tenant_id, project_id, limit)
            .await
    }

    async fn find_memory_evidence(&self, id: &EvidenceId) -> Result<Option<Evidence>, Self::Error> {
        self.find_memory_evidence(id).await
    }

    async fn upsert_memory_entity(&self, entity: &Entity) -> Result<(), Self::Error> {
        self.upsert_memory_entity(entity).await
    }

    async fn list_memory_entities(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Entity>, Self::Error> {
        self.list_memory_entities(tenant_id, project_id, limit)
            .await
    }

    async fn find_memory_entity(&self, id: &EntityId) -> Result<Option<Entity>, Self::Error> {
        self.find_memory_entity(id).await
    }

    async fn upsert_memory_claim(&self, claim: &Claim) -> Result<(), Self::Error> {
        self.upsert_memory_claim(claim).await
    }

    async fn list_memory_claims(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Claim>, Self::Error> {
        self.list_memory_claims(tenant_id, project_id, limit).await
    }

    async fn find_memory_claim(&self, id: &ClaimId) -> Result<Option<Claim>, Self::Error> {
        self.find_memory_claim(id).await
    }

    async fn upsert_memory_claim_conflict(
        &self,
        conflict: &ClaimConflict,
    ) -> Result<(), Self::Error> {
        self.upsert_memory_claim_conflict(conflict).await
    }

    async fn list_memory_claim_conflicts(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<ClaimConflict>, Self::Error> {
        self.list_memory_claim_conflicts(tenant_id, project_id, limit)
            .await
    }

    async fn find_memory_claim_conflict(
        &self,
        id: &ClaimConflictId,
    ) -> Result<Option<ClaimConflict>, Self::Error> {
        self.find_memory_claim_conflict(id).await
    }

    async fn upsert_memory_event(&self, event: &Event) -> Result<(), Self::Error> {
        self.upsert_memory_event(event).await
    }

    async fn list_memory_events(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Event>, Self::Error> {
        self.list_memory_events(tenant_id, project_id, limit).await
    }

    async fn find_memory_event(&self, id: &EventId) -> Result<Option<Event>, Self::Error> {
        self.find_memory_event(id).await
    }

    async fn upsert_memory_relationship(
        &self,
        relationship: &Relationship,
    ) -> Result<(), Self::Error> {
        self.upsert_memory_relationship(relationship).await
    }

    async fn list_memory_relationships(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Relationship>, Self::Error> {
        self.list_memory_relationships(tenant_id, project_id, limit)
            .await
    }

    async fn find_memory_relationship(
        &self,
        id: &RelationshipId,
    ) -> Result<Option<Relationship>, Self::Error> {
        self.find_memory_relationship(id).await
    }

    async fn upsert_memory_contract(&self, contract: &MemoryContract) -> Result<(), Self::Error> {
        self.upsert_memory_contract(contract).await
    }

    async fn list_memory_contracts(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<MemoryContract>, Self::Error> {
        self.list_memory_contracts(tenant_id, project_id, limit)
            .await
    }

    async fn find_memory_contract(
        &self,
        id: &MemoryContractId,
    ) -> Result<Option<MemoryContract>, Self::Error> {
        self.find_memory_contract(id).await
    }
}
