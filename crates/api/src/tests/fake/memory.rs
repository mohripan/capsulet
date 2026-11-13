//! Memory records.

use super::super::*;

#[async_trait::async_trait]
impl MemoryStore for FakeStore {
    async fn upsert_memory_source(&self, source: &Source) -> Result<(), Self::Error> {
        let mut sources = self
            .memory_sources
            .lock()
            .map_err(|error| error.to_string())?;
        sources.retain(|existing| existing.id() != source.id());
        sources.push(source.clone());
        Ok(())
    }

    async fn list_memory_sources(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Source>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_sources
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|source| {
                source.scope().tenant_id() == tenant_id && source.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_source(&self, id: &SourceId) -> Result<Option<Source>, Self::Error> {
        Ok(self
            .memory_sources
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|source| source.id() == id)
            .cloned())
    }

    async fn upsert_memory_evidence(&self, evidence: &Evidence) -> Result<(), Self::Error> {
        let mut records = self
            .memory_evidence
            .lock()
            .map_err(|error| error.to_string())?;
        records.retain(|existing| existing.id() != evidence.id());
        records.push(evidence.clone());
        Ok(())
    }

    async fn list_memory_evidence(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Evidence>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_evidence
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|evidence| {
                evidence.scope().tenant_id() == tenant_id
                    && evidence.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_evidence(&self, id: &EvidenceId) -> Result<Option<Evidence>, Self::Error> {
        Ok(self
            .memory_evidence
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|evidence| evidence.id() == id)
            .cloned())
    }

    async fn upsert_memory_entity(&self, entity: &Entity) -> Result<(), Self::Error> {
        let mut entities = self
            .memory_entities
            .lock()
            .map_err(|error| error.to_string())?;
        entities.retain(|existing| existing.id() != entity.id());
        entities.push(entity.clone());
        Ok(())
    }

    async fn list_memory_entities(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Entity>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_entities
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

    async fn find_memory_entity(&self, id: &EntityId) -> Result<Option<Entity>, Self::Error> {
        Ok(self
            .memory_entities
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|entity| entity.id() == id)
            .cloned())
    }

    async fn upsert_memory_claim(&self, claim: &Claim) -> Result<(), Self::Error> {
        let mut claims = self
            .memory_claims
            .lock()
            .map_err(|error| error.to_string())?;
        claims.retain(|existing| existing.id() != claim.id());
        claims.push(claim.clone());
        Ok(())
    }

    async fn list_memory_claims(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Claim>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_claims
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|claim| {
                claim.scope().tenant_id() == tenant_id && claim.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_claim(&self, id: &ClaimId) -> Result<Option<Claim>, Self::Error> {
        Ok(self
            .memory_claims
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|claim| claim.id() == id)
            .cloned())
    }

    async fn upsert_memory_claim_conflict(
        &self,
        conflict: &ClaimConflict,
    ) -> Result<(), Self::Error> {
        let mut conflicts = self
            .memory_claim_conflicts
            .lock()
            .map_err(|error| error.to_string())?;
        conflicts.retain(|existing| existing.id() != conflict.id());
        conflicts.push(conflict.clone());
        Ok(())
    }

    async fn list_memory_claim_conflicts(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<ClaimConflict>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_claim_conflicts
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|conflict| {
                conflict.scope().tenant_id() == tenant_id
                    && conflict.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_claim_conflict(
        &self,
        id: &ClaimConflictId,
    ) -> Result<Option<ClaimConflict>, Self::Error> {
        Ok(self
            .memory_claim_conflicts
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|conflict| conflict.id() == id)
            .cloned())
    }

    async fn upsert_memory_event(&self, event: &Event) -> Result<(), Self::Error> {
        let mut events = self
            .memory_events
            .lock()
            .map_err(|error| error.to_string())?;
        events.retain(|existing| existing.id() != event.id());
        events.push(event.clone());
        Ok(())
    }

    async fn list_memory_events(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Event>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_events
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|event| {
                event.scope().tenant_id() == tenant_id && event.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_event(&self, id: &EventId) -> Result<Option<Event>, Self::Error> {
        Ok(self
            .memory_events
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|event| event.id() == id)
            .cloned())
    }

    async fn upsert_memory_relationship(
        &self,
        relationship: &Relationship,
    ) -> Result<(), Self::Error> {
        let mut relationships = self
            .memory_relationships
            .lock()
            .map_err(|error| error.to_string())?;
        relationships.retain(|existing| existing.id() != relationship.id());
        relationships.push(relationship.clone());
        Ok(())
    }

    async fn list_memory_relationships(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Relationship>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_relationships
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|relationship| {
                relationship.scope().tenant_id() == tenant_id
                    && relationship.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_relationship(
        &self,
        id: &RelationshipId,
    ) -> Result<Option<Relationship>, Self::Error> {
        Ok(self
            .memory_relationships
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|relationship| relationship.id() == id)
            .cloned())
    }

    async fn upsert_memory_contract(&self, contract: &MemoryContract) -> Result<(), Self::Error> {
        let mut contracts = self
            .memory_contracts
            .lock()
            .map_err(|error| error.to_string())?;
        contracts.retain(|existing| existing.id() != contract.id());
        contracts.push(contract.clone());
        Ok(())
    }

    async fn list_memory_contracts(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<MemoryContract>, Self::Error> {
        let limit = usize::try_from(limit).map_err(|error| error.to_string())?;
        Ok(self
            .memory_contracts
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|contract| {
                contract.scope().tenant_id() == tenant_id
                    && contract.scope().project_id() == project_id
            })
            .take(limit)
            .cloned()
            .collect())
    }

    async fn find_memory_contract(
        &self,
        id: &MemoryContractId,
    ) -> Result<Option<MemoryContract>, Self::Error> {
        Ok(self
            .memory_contracts
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|contract| contract.id() == id)
            .cloned())
    }
}
