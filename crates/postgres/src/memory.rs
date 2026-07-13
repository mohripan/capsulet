#![expect(
    clippy::missing_errors_doc,
    reason = "memory persistence methods all return PostgresStoreError for SQL and domain conversion failures"
)]

use capsulet_core::{
    Authority, CanonicalEntity, CanonicalEntityId, Claim, ClaimConflict, ClaimConflictId,
    ClaimConflictStatus, ClaimId, ClaimStatus, Confidence, Entity, EntityGraphAttachment,
    EntityGraphAttachmentType, EntityId, EntityResolution, EntityResolutionId,
    EntityResolutionStatus, Event, EventId, Evidence, EvidenceId, MemoryContract, MemoryContractId,
    MemoryMemberId, MemoryMemberKind, MemoryScope, MemorySubgraph, MemorySubgraphId,
    MemorySubgraphMember, MemorySubgraphMemberRole, MemorySubgraphOwner, MemorySubgraphOwnerKind,
    MemorySubgraphPermissions, MemorySubgraphStatus, Relationship, RelationshipId, Source,
    SourceId, SubgraphEdge, SummaryTrace, SummaryTraceId,
};
use serde_json::Value;
use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

impl PostgresStore {
    pub async fn upsert_memory_source(&self, source: &Source) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_sources (id, tenant_id, project_id, kind, uri, title, authority, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                kind = EXCLUDED.kind,
                uri = EXCLUDED.uri,
                title = EXCLUDED.title,
                authority = EXCLUDED.authority,
                updated_at = now()
            ",
        )
        .bind(source.id().as_str())
        .bind(source.scope().tenant_id())
        .bind(source.scope().project_id())
        .bind(source.kind())
        .bind(source.uri())
        .bind(source.title())
        .bind(source.authority().to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_memory_sources(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Source>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, kind, uri, title, authority
            FROM memory_sources
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
        rows.iter().map(row_to_source).collect()
    }

    pub async fn find_memory_source(
        &self,
        id: &SourceId,
    ) -> Result<Option<Source>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, kind, uri, title, authority
            FROM memory_sources
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_source).transpose()
    }

    pub async fn upsert_memory_evidence(
        &self,
        evidence: &Evidence,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_evidence (id, tenant_id, project_id, source_id, locator, excerpt, observed_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                source_id = EXCLUDED.source_id,
                locator = EXCLUDED.locator,
                excerpt = EXCLUDED.excerpt,
                observed_at = EXCLUDED.observed_at,
                updated_at = now()
            ",
        )
        .bind(evidence.id().as_str())
        .bind(evidence.scope().tenant_id())
        .bind(evidence.scope().project_id())
        .bind(evidence.source_id().as_str())
        .bind(evidence.locator())
        .bind(evidence.excerpt())
        .bind(evidence.observed_at())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_memory_evidence(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Evidence>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, source_id, locator, excerpt, observed_at
            FROM memory_evidence
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
        rows.iter().map(row_to_evidence).collect()
    }

    pub async fn find_memory_evidence(
        &self,
        id: &EvidenceId,
    ) -> Result<Option<Evidence>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, source_id, locator, excerpt, observed_at
            FROM memory_evidence
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_evidence).transpose()
    }

    pub async fn upsert_memory_entity(&self, entity: &Entity) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_entities (id, tenant_id, project_id, entity_type, name, aliases, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                entity_type = EXCLUDED.entity_type,
                name = EXCLUDED.name,
                aliases = EXCLUDED.aliases,
                updated_at = now()
            ",
        )
        .bind(entity.id().as_str())
        .bind(entity.scope().tenant_id())
        .bind(entity.scope().project_id())
        .bind(entity.entity_type())
        .bind(entity.name())
        .bind(entity.aliases())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_memory_entities(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Entity>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, entity_type, name, aliases
            FROM memory_entities
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
        rows.iter().map(row_to_entity).collect()
    }

    pub async fn find_memory_entity(
        &self,
        id: &EntityId,
    ) -> Result<Option<Entity>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, entity_type, name, aliases
            FROM memory_entities
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_entity).transpose()
    }

    pub async fn upsert_memory_claim(&self, claim: &Claim) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_claims (
                id, tenant_id, project_id, subject_id, predicate, object, evidence_ids,
                confidence, authority, status, observed_at, valid_from, valid_until, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                subject_id = EXCLUDED.subject_id,
                predicate = EXCLUDED.predicate,
                object = EXCLUDED.object,
                evidence_ids = EXCLUDED.evidence_ids,
                confidence = EXCLUDED.confidence,
                authority = EXCLUDED.authority,
                status = EXCLUDED.status,
                observed_at = EXCLUDED.observed_at,
                valid_from = EXCLUDED.valid_from,
                valid_until = EXCLUDED.valid_until,
                updated_at = now()
            ",
        )
        .bind(claim.id().as_str())
        .bind(claim.scope().tenant_id())
        .bind(claim.scope().project_id())
        .bind(claim.subject_id().as_str())
        .bind(claim.predicate())
        .bind(claim.object())
        .bind(id_strings(claim.evidence_ids()))
        .bind(claim.confidence().value())
        .bind(claim.authority().to_string())
        .bind(claim.status().to_string())
        .bind(claim.observed_at())
        .bind(claim.valid_from())
        .bind(claim.valid_until())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_memory_claims(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Claim>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, subject_id, predicate, object, evidence_ids,
                   confidence, authority, status, observed_at, valid_from, valid_until
            FROM memory_claims
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
        rows.iter().map(row_to_claim).collect()
    }

    pub async fn find_memory_claim(
        &self,
        id: &ClaimId,
    ) -> Result<Option<Claim>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, subject_id, predicate, object, evidence_ids,
                   confidence, authority, status, observed_at, valid_from, valid_until
            FROM memory_claims
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_claim).transpose()
    }

    pub async fn upsert_memory_claim_conflict(
        &self,
        conflict: &ClaimConflict,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_claim_conflicts (
                id, tenant_id, project_id, subject_id, canonical_entity_id, predicate,
                claim_ids, status, reason, preferred_claim_id, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                subject_id = EXCLUDED.subject_id,
                canonical_entity_id = EXCLUDED.canonical_entity_id,
                predicate = EXCLUDED.predicate,
                claim_ids = EXCLUDED.claim_ids,
                status = EXCLUDED.status,
                reason = EXCLUDED.reason,
                preferred_claim_id = EXCLUDED.preferred_claim_id,
                updated_at = now()
            ",
        )
        .bind(conflict.id().as_str())
        .bind(conflict.scope().tenant_id())
        .bind(conflict.scope().project_id())
        .bind(conflict.subject_id().as_str())
        .bind(
            conflict
                .canonical_entity_id()
                .map(capsulet_core::CanonicalEntityId::as_str),
        )
        .bind(conflict.predicate())
        .bind(id_strings(conflict.claim_ids()))
        .bind(conflict.status().to_string())
        .bind(conflict.reason())
        .bind(
            conflict
                .preferred_claim_id()
                .map(capsulet_core::ClaimId::as_str),
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_memory_claim_conflicts(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<ClaimConflict>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, subject_id, canonical_entity_id, predicate,
                   claim_ids, status, reason, preferred_claim_id
            FROM memory_claim_conflicts
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
        rows.iter().map(row_to_claim_conflict).collect()
    }

    pub async fn find_memory_claim_conflict(
        &self,
        id: &ClaimConflictId,
    ) -> Result<Option<ClaimConflict>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, subject_id, canonical_entity_id, predicate,
                   claim_ids, status, reason, preferred_claim_id
            FROM memory_claim_conflicts
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_claim_conflict).transpose()
    }

    pub async fn upsert_memory_event(&self, event: &Event) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_events (
                id, tenant_id, project_id, event_type, occurred_at, entity_ids, evidence_ids, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                event_type = EXCLUDED.event_type,
                occurred_at = EXCLUDED.occurred_at,
                entity_ids = EXCLUDED.entity_ids,
                evidence_ids = EXCLUDED.evidence_ids,
                updated_at = now()
            ",
        )
        .bind(event.id().as_str())
        .bind(event.scope().tenant_id())
        .bind(event.scope().project_id())
        .bind(event.event_type())
        .bind(event.occurred_at())
        .bind(id_strings(event.entity_ids()))
        .bind(id_strings(event.evidence_ids()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_memory_events(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Event>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, event_type, occurred_at, entity_ids, evidence_ids
            FROM memory_events
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
        rows.iter().map(row_to_event).collect()
    }

    pub async fn find_memory_event(
        &self,
        id: &EventId,
    ) -> Result<Option<Event>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, event_type, occurred_at, entity_ids, evidence_ids
            FROM memory_events
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_event).transpose()
    }

    pub async fn upsert_memory_relationship(
        &self,
        relationship: &Relationship,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_relationships (
                id, tenant_id, project_id, relationship_type, from_entity_id, to_entity_id, evidence_ids, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                relationship_type = EXCLUDED.relationship_type,
                from_entity_id = EXCLUDED.from_entity_id,
                to_entity_id = EXCLUDED.to_entity_id,
                evidence_ids = EXCLUDED.evidence_ids,
                updated_at = now()
            ",
        )
        .bind(relationship.id().as_str())
        .bind(relationship.scope().tenant_id())
        .bind(relationship.scope().project_id())
        .bind(relationship.relationship_type())
        .bind(relationship.from_entity_id().as_str())
        .bind(relationship.to_entity_id().as_str())
        .bind(id_strings(relationship.evidence_ids()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_memory_relationships(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<Relationship>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, relationship_type, from_entity_id, to_entity_id, evidence_ids
            FROM memory_relationships
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
        rows.iter().map(row_to_relationship).collect()
    }

    pub async fn find_memory_relationship(
        &self,
        id: &RelationshipId,
    ) -> Result<Option<Relationship>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, relationship_type, from_entity_id, to_entity_id, evidence_ids
            FROM memory_relationships
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_relationship).transpose()
    }

    pub async fn upsert_memory_contract(
        &self,
        contract: &MemoryContract,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_contracts (id, tenant_id, project_id, name, source, updated_at)
            VALUES ($1, $2, $3, $4, $5, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                name = EXCLUDED.name,
                source = EXCLUDED.source,
                updated_at = now()
            ",
        )
        .bind(contract.id().as_str())
        .bind(contract.scope().tenant_id())
        .bind(contract.scope().project_id())
        .bind(contract.name())
        .bind(contract.source())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_memory_contracts(
        &self,
        limit: i64,
    ) -> Result<Vec<MemoryContract>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, name, source
            FROM memory_contracts
            ORDER BY updated_at DESC, id ASC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_contract).collect()
    }

    pub async fn find_memory_contract(
        &self,
        id: &MemoryContractId,
    ) -> Result<Option<MemoryContract>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, name, source
            FROM memory_contracts
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_contract).transpose()
    }

    pub async fn upsert_memory_subgraph(
        &self,
        subgraph: &MemorySubgraph,
    ) -> Result<(), PostgresStoreError> {
        let (owner_kind, owner_id) = match subgraph.owner() {
            Some(owner) => (Some(owner.kind().to_string()), Some(owner.id())),
            None => (None, None),
        };
        let permissions = subgraph
            .permissions()
            .map(|permissions| serde_json::from_str::<Value>(permissions.as_json()))
            .transpose()
            .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?;
        sqlx::query(
            r"
            INSERT INTO memory_subgraphs (
                id, tenant_id, project_id, parent_subgraph_id, name, description, owner_kind,
                owner_id, contract_id, summary_claim_id, permissions, status, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                parent_subgraph_id = EXCLUDED.parent_subgraph_id,
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                owner_kind = EXCLUDED.owner_kind,
                owner_id = EXCLUDED.owner_id,
                contract_id = EXCLUDED.contract_id,
                summary_claim_id = EXCLUDED.summary_claim_id,
                permissions = EXCLUDED.permissions,
                status = EXCLUDED.status,
                updated_at = now()
            ",
        )
        .bind(subgraph.id().as_str())
        .bind(subgraph.scope().tenant_id())
        .bind(subgraph.scope().project_id())
        .bind(subgraph.parent_subgraph_id().map(ToString::to_string))
        .bind(subgraph.name())
        .bind(subgraph.description())
        .bind(owner_kind)
        .bind(owner_id)
        .bind(subgraph.contract_id().map(ToString::to_string))
        .bind(subgraph.summary_claim_id().map(ToString::to_string))
        .bind(permissions.map(sqlx::types::Json))
        .bind(subgraph.status().to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_memory_subgraphs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<MemorySubgraph>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, parent_subgraph_id, name, description, owner_kind,
                   owner_id, contract_id, summary_claim_id, permissions, status
            FROM memory_subgraphs
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
        rows.iter().map(row_to_subgraph).collect()
    }

    pub async fn find_memory_subgraph(
        &self,
        id: &MemorySubgraphId,
    ) -> Result<Option<MemorySubgraph>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, parent_subgraph_id, name, description, owner_kind,
                   owner_id, contract_id, summary_claim_id, permissions, status
            FROM memory_subgraphs
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_subgraph).transpose()
    }

    pub async fn upsert_memory_subgraph_member(
        &self,
        member: &MemorySubgraphMember,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_subgraph_members (
                id, tenant_id, project_id, subgraph_id, member_kind, member_id, role, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                subgraph_id = EXCLUDED.subgraph_id,
                member_kind = EXCLUDED.member_kind,
                member_id = EXCLUDED.member_id,
                role = EXCLUDED.role,
                updated_at = now()
            ",
        )
        .bind(member.id().as_str())
        .bind(member.scope().tenant_id())
        .bind(member.scope().project_id())
        .bind(member.subgraph_id().as_str())
        .bind(member.member_kind().to_string())
        .bind(member.member_id().as_str())
        .bind(member.role().to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_memory_canonical_entity(
        &self,
        entity: &CanonicalEntity,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_canonical_entities (
                id, tenant_id, project_id, entity_type, display_name, aliases, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                entity_type = EXCLUDED.entity_type,
                display_name = EXCLUDED.display_name,
                aliases = EXCLUDED.aliases,
                updated_at = now()
            ",
        )
        .bind(entity.id().as_str())
        .bind(entity.scope().tenant_id())
        .bind(entity.scope().project_id())
        .bind(entity.entity_type())
        .bind(entity.display_name())
        .bind(entity.aliases())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_memory_canonical_entities(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<CanonicalEntity>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, entity_type, display_name, aliases
            FROM memory_canonical_entities
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
        rows.iter().map(row_to_canonical_entity).collect()
    }

    pub async fn upsert_memory_entity_resolution(
        &self,
        resolution: &EntityResolution,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_entity_resolutions (
                id, tenant_id, project_id, subgraph_id, entity_id, canonical_entity_id,
                confidence, status, evidence_ids, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                subgraph_id = EXCLUDED.subgraph_id,
                entity_id = EXCLUDED.entity_id,
                canonical_entity_id = EXCLUDED.canonical_entity_id,
                confidence = EXCLUDED.confidence,
                status = EXCLUDED.status,
                evidence_ids = EXCLUDED.evidence_ids,
                updated_at = now()
            ",
        )
        .bind(resolution.id().as_str())
        .bind(resolution.scope().tenant_id())
        .bind(resolution.scope().project_id())
        .bind(resolution.subgraph_id().as_str())
        .bind(resolution.entity_id().as_str())
        .bind(resolution.canonical_entity_id().as_str())
        .bind(resolution.confidence().value())
        .bind(resolution.status().to_string())
        .bind(id_strings(resolution.evidence_ids()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_memory_entity_resolutions(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<EntityResolution>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, subgraph_id, entity_id, canonical_entity_id,
                   confidence, status, evidence_ids
            FROM memory_entity_resolutions
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
        rows.iter().map(row_to_entity_resolution).collect()
    }

    pub async fn find_memory_entity_resolution(
        &self,
        id: &EntityResolutionId,
    ) -> Result<Option<EntityResolution>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, subgraph_id, entity_id, canonical_entity_id,
                   confidence, status, evidence_ids
            FROM memory_entity_resolutions
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_entity_resolution).transpose()
    }

    pub async fn upsert_memory_subgraph_edge(
        &self,
        edge: &SubgraphEdge,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_subgraph_edges (
                id, tenant_id, project_id, edge_type, from_subgraph_id, to_subgraph_id,
                from_member_kind, from_member_id, to_member_kind, to_member_id,
                claim_ids, evidence_ids, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                edge_type = EXCLUDED.edge_type,
                from_subgraph_id = EXCLUDED.from_subgraph_id,
                to_subgraph_id = EXCLUDED.to_subgraph_id,
                from_member_kind = EXCLUDED.from_member_kind,
                from_member_id = EXCLUDED.from_member_id,
                to_member_kind = EXCLUDED.to_member_kind,
                to_member_id = EXCLUDED.to_member_id,
                claim_ids = EXCLUDED.claim_ids,
                evidence_ids = EXCLUDED.evidence_ids,
                updated_at = now()
            ",
        )
        .bind(edge.id().as_str())
        .bind(edge.scope().tenant_id())
        .bind(edge.scope().project_id())
        .bind(edge.edge_type())
        .bind(edge.from_subgraph_id().as_str())
        .bind(edge.to_subgraph_id().as_str())
        .bind(edge.from_member_kind().to_string())
        .bind(edge.from_member_id().as_str())
        .bind(edge.to_member_kind().to_string())
        .bind(edge.to_member_id().as_str())
        .bind(id_strings(edge.claim_ids()))
        .bind(id_strings(edge.evidence_ids()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_memory_summary_trace(
        &self,
        trace: &SummaryTrace,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_summary_traces (
                id, tenant_id, project_id, subgraph_id, summary_claim_id, inner_claim_ids, evidence_ids
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                subgraph_id = EXCLUDED.subgraph_id,
                summary_claim_id = EXCLUDED.summary_claim_id,
                inner_claim_ids = EXCLUDED.inner_claim_ids,
                evidence_ids = EXCLUDED.evidence_ids
            ",
        )
        .bind(trace.id().as_str())
        .bind(trace.scope().tenant_id())
        .bind(trace.scope().project_id())
        .bind(trace.subgraph_id().as_str())
        .bind(trace.summary_claim_id().as_str())
        .bind(id_strings(trace.inner_claim_ids()))
        .bind(id_strings(trace.evidence_ids()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_memory_summary_traces(
        &self,
        subgraph_id: &MemorySubgraphId,
        summary_claim_id: &ClaimId,
    ) -> Result<Vec<SummaryTrace>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, subgraph_id, summary_claim_id, inner_claim_ids, evidence_ids
            FROM memory_summary_traces
            WHERE subgraph_id = $1 AND summary_claim_id = $2
            ORDER BY created_at DESC, id ASC
            ",
        )
        .bind(subgraph_id.as_str())
        .bind(summary_claim_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_summary_trace).collect()
    }

    pub async fn upsert_memory_entity_graph_attachment(
        &self,
        attachment: &EntityGraphAttachment,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO memory_entity_graph_attachments (
                id, tenant_id, project_id, canonical_entity_id, subgraph_id, attachment_type, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, now())
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                project_id = EXCLUDED.project_id,
                canonical_entity_id = EXCLUDED.canonical_entity_id,
                subgraph_id = EXCLUDED.subgraph_id,
                attachment_type = EXCLUDED.attachment_type,
                updated_at = now()
            ",
        )
        .bind(attachment.id().as_str())
        .bind(attachment.scope().tenant_id())
        .bind(attachment.scope().project_id())
        .bind(attachment.canonical_entity_id().as_str())
        .bind(attachment.subgraph_id().as_str())
        .bind(attachment.attachment_type().to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn row_to_source(row: &sqlx::postgres::PgRow) -> Result<Source, PostgresStoreError> {
    Source::new(
        SourceId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        row.try_get::<String, _>("kind")?,
        row.try_get::<Option<String>, _>("uri")?,
        row.try_get::<String, _>("title")?,
        parse_authority(&row.try_get::<String, _>("authority")?)?,
    )
    .map_err(PostgresStoreError::Memory)
}

fn row_to_evidence(row: &sqlx::postgres::PgRow) -> Result<Evidence, PostgresStoreError> {
    Evidence::new(
        EvidenceId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        SourceId::new(row.try_get::<String, _>("source_id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        row.try_get::<String, _>("locator")?,
        row.try_get::<String, _>("excerpt")?,
        row.try_get::<String, _>("observed_at")?,
    )
    .map_err(PostgresStoreError::Memory)
}

fn row_to_entity(row: &sqlx::postgres::PgRow) -> Result<Entity, PostgresStoreError> {
    Entity::new(
        EntityId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        row.try_get::<String, _>("entity_type")?,
        row.try_get::<String, _>("name")?,
        row.try_get::<Vec<String>, _>("aliases")?,
    )
    .map_err(PostgresStoreError::Memory)
}

fn row_to_claim(row: &sqlx::postgres::PgRow) -> Result<Claim, PostgresStoreError> {
    let evidence_ids = row
        .try_get::<Vec<String>, _>("evidence_ids")?
        .into_iter()
        .map(EvidenceId::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(PostgresStoreError::InvalidPersistedValue)?;
    let claim = Claim::new(
        ClaimId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        EntityId::new(row.try_get::<String, _>("subject_id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        row.try_get::<String, _>("predicate")?,
        row.try_get::<String, _>("object")?,
        evidence_ids,
        Confidence::new(row.try_get::<f64, _>("confidence")?)
            .map_err(PostgresStoreError::Memory)?,
        parse_authority(&row.try_get::<String, _>("authority")?)?,
        row.try_get::<String, _>("observed_at")?,
        row.try_get::<Option<String>, _>("valid_from")?.as_deref(),
        row.try_get::<Option<String>, _>("valid_until")?.as_deref(),
    )
    .map_err(PostgresStoreError::Memory)?;
    Ok(claim.with_status(parse_claim_status(&row.try_get::<String, _>("status")?)?))
}

fn row_to_claim_conflict(row: &sqlx::postgres::PgRow) -> Result<ClaimConflict, PostgresStoreError> {
    ClaimConflict::new(
        ClaimConflictId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        EntityId::new(row.try_get::<String, _>("subject_id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        row.try_get::<Option<String>, _>("canonical_entity_id")?
            .map(CanonicalEntityId::new)
            .transpose()
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        row.try_get::<String, _>("predicate")?,
        ids(row.try_get::<Vec<String>, _>("claim_ids")?)?,
        parse_claim_conflict_status(&row.try_get::<String, _>("status")?)?,
        row.try_get::<String, _>("reason")?,
        row.try_get::<Option<String>, _>("preferred_claim_id")?
            .map(ClaimId::new)
            .transpose()
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
    )
    .map_err(PostgresStoreError::MemoryGraph)
}

fn row_to_event(row: &sqlx::postgres::PgRow) -> Result<Event, PostgresStoreError> {
    Event::new(
        EventId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        row.try_get::<String, _>("event_type")?,
        row.try_get::<String, _>("occurred_at")?,
        ids(row.try_get::<Vec<String>, _>("entity_ids")?)?,
        ids(row.try_get::<Vec<String>, _>("evidence_ids")?)?,
    )
    .map_err(PostgresStoreError::Memory)
}

fn row_to_relationship(row: &sqlx::postgres::PgRow) -> Result<Relationship, PostgresStoreError> {
    Relationship::new(
        RelationshipId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        row.try_get::<String, _>("relationship_type")?,
        EntityId::new(row.try_get::<String, _>("from_entity_id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        EntityId::new(row.try_get::<String, _>("to_entity_id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        ids(row.try_get::<Vec<String>, _>("evidence_ids")?)?,
    )
    .map_err(PostgresStoreError::Memory)
}

fn row_to_contract(row: &sqlx::postgres::PgRow) -> Result<MemoryContract, PostgresStoreError> {
    MemoryContract::parse_scoped(
        MemoryContractId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        row.try_get::<String, _>("name")?,
        row.try_get::<String, _>("source")?,
    )
    .map_err(PostgresStoreError::MemoryContract)
}

fn row_to_subgraph(row: &sqlx::postgres::PgRow) -> Result<MemorySubgraph, PostgresStoreError> {
    let owner = match (
        row.try_get::<Option<String>, _>("owner_kind")?,
        row.try_get::<Option<String>, _>("owner_id")?,
    ) {
        (Some(kind), Some(id)) => Some(MemorySubgraphOwner::new(parse_owner_kind(&kind)?, id)?),
        (None, None) => None,
        _ => {
            return Err(PostgresStoreError::InvalidPersistedValue(
                "subgraph owner kind and id must be stored together".to_string(),
            ));
        }
    };
    let permissions = row
        .try_get::<Option<sqlx::types::Json<Value>>, _>("permissions")?
        .map(|value| MemorySubgraphPermissions::new(value.0.to_string()))
        .transpose()?;
    MemorySubgraph::from_record(
        MemorySubgraphId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        row.try_get::<Option<String>, _>("parent_subgraph_id")?
            .map(MemorySubgraphId::new)
            .transpose()
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        row.try_get::<String, _>("name")?,
        row.try_get::<Option<String>, _>("description")?.as_deref(),
        owner,
        row.try_get::<Option<String>, _>("contract_id")?
            .map(MemoryContractId::new)
            .transpose()
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        row.try_get::<Option<String>, _>("summary_claim_id")?
            .map(ClaimId::new)
            .transpose()
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        permissions,
        parse_subgraph_status(&row.try_get::<String, _>("status")?)?,
    )
    .map_err(PostgresStoreError::MemoryGraph)
}

fn row_to_canonical_entity(
    row: &sqlx::postgres::PgRow,
) -> Result<CanonicalEntity, PostgresStoreError> {
    CanonicalEntity::new(
        CanonicalEntityId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        row.try_get::<String, _>("entity_type")?,
        row.try_get::<String, _>("display_name")?,
        row.try_get::<Vec<String>, _>("aliases")?,
    )
    .map_err(PostgresStoreError::MemoryGraph)
}

fn row_to_entity_resolution(
    row: &sqlx::postgres::PgRow,
) -> Result<EntityResolution, PostgresStoreError> {
    EntityResolution::new(
        EntityResolutionId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        MemorySubgraphId::new(row.try_get::<String, _>("subgraph_id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        EntityId::new(row.try_get::<String, _>("entity_id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        CanonicalEntityId::new(row.try_get::<String, _>("canonical_entity_id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        Confidence::new(row.try_get::<f64, _>("confidence")?)
            .map_err(PostgresStoreError::Memory)?,
        parse_entity_resolution_status(&row.try_get::<String, _>("status")?)?,
        ids(row.try_get::<Vec<String>, _>("evidence_ids")?)?,
    )
    .map_err(PostgresStoreError::MemoryGraph)
}

fn row_to_summary_trace(row: &sqlx::postgres::PgRow) -> Result<SummaryTrace, PostgresStoreError> {
    SummaryTrace::new(
        SummaryTraceId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        scope(row)?,
        MemorySubgraphId::new(row.try_get::<String, _>("subgraph_id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        ClaimId::new(row.try_get::<String, _>("summary_claim_id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        ids(row.try_get::<Vec<String>, _>("inner_claim_ids")?)?,
        ids(row.try_get::<Vec<String>, _>("evidence_ids")?)?,
    )
    .map_err(PostgresStoreError::MemoryGraph)
}

fn scope(row: &sqlx::postgres::PgRow) -> Result<MemoryScope, PostgresStoreError> {
    MemoryScope::new(
        row.try_get::<String, _>("tenant_id")?,
        row.try_get::<String, _>("project_id")?,
    )
    .map_err(PostgresStoreError::Memory)
}

fn parse_authority(value: &str) -> Result<Authority, PostgresStoreError> {
    match value {
        "low" => Ok(Authority::Low),
        "medium" => Ok(Authority::Medium),
        "high" => Ok(Authority::High),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown authority {value}"
        ))),
    }
}

fn parse_claim_status(value: &str) -> Result<ClaimStatus, PostgresStoreError> {
    match value {
        "candidate" => Ok(ClaimStatus::Candidate),
        "active" => Ok(ClaimStatus::Active),
        "rejected" => Ok(ClaimStatus::Rejected),
        "superseded" => Ok(ClaimStatus::Superseded),
        "contradicted" => Ok(ClaimStatus::Contradicted),
        "expired" => Ok(ClaimStatus::Expired),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown claim status {value}"
        ))),
    }
}

fn parse_claim_conflict_status(value: &str) -> Result<ClaimConflictStatus, PostgresStoreError> {
    match value {
        "candidate" => Ok(ClaimConflictStatus::Candidate),
        "resolved" => Ok(ClaimConflictStatus::Resolved),
        "dismissed" => Ok(ClaimConflictStatus::Dismissed),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown claim conflict status {value}"
        ))),
    }
}

fn parse_subgraph_status(value: &str) -> Result<MemorySubgraphStatus, PostgresStoreError> {
    match value {
        "draft" => Ok(MemorySubgraphStatus::Draft),
        "active" => Ok(MemorySubgraphStatus::Active),
        "archived" => Ok(MemorySubgraphStatus::Archived),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown memory subgraph status {value}"
        ))),
    }
}

fn parse_owner_kind(value: &str) -> Result<MemorySubgraphOwnerKind, PostgresStoreError> {
    match value {
        "user" => Ok(MemorySubgraphOwnerKind::User),
        "team" => Ok(MemorySubgraphOwnerKind::Team),
        "service" => Ok(MemorySubgraphOwnerKind::Service),
        "organization" => Ok(MemorySubgraphOwnerKind::Organization),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown memory subgraph owner kind {value}"
        ))),
    }
}

#[allow(dead_code)]
fn parse_member_kind(value: &str) -> Result<MemoryMemberKind, PostgresStoreError> {
    match value {
        "source" => Ok(MemoryMemberKind::Source),
        "evidence" => Ok(MemoryMemberKind::Evidence),
        "entity" => Ok(MemoryMemberKind::Entity),
        "canonical_entity" => Ok(MemoryMemberKind::CanonicalEntity),
        "claim" => Ok(MemoryMemberKind::Claim),
        "event" => Ok(MemoryMemberKind::Event),
        "relationship" => Ok(MemoryMemberKind::Relationship),
        "subgraph" => Ok(MemoryMemberKind::Subgraph),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown memory member kind {value}"
        ))),
    }
}

#[allow(dead_code)]
fn parse_member_role(value: &str) -> Result<MemorySubgraphMemberRole, PostgresStoreError> {
    match value {
        "member" => Ok(MemorySubgraphMemberRole::Member),
        "summary" => Ok(MemorySubgraphMemberRole::Summary),
        "inner_claim" => Ok(MemorySubgraphMemberRole::InnerClaim),
        "evidence" => Ok(MemorySubgraphMemberRole::Evidence),
        "canonical_identity" => Ok(MemorySubgraphMemberRole::CanonicalIdentity),
        "child_context" => Ok(MemorySubgraphMemberRole::ChildContext),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown memory subgraph member role {value}"
        ))),
    }
}

#[allow(dead_code)]
fn parse_entity_resolution_status(
    value: &str,
) -> Result<EntityResolutionStatus, PostgresStoreError> {
    match value {
        "candidate" => Ok(EntityResolutionStatus::Candidate),
        "confirmed" => Ok(EntityResolutionStatus::Confirmed),
        "rejected" => Ok(EntityResolutionStatus::Rejected),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown entity resolution status {value}"
        ))),
    }
}

#[allow(dead_code)]
fn parse_entity_graph_attachment_type(
    value: &str,
) -> Result<EntityGraphAttachmentType, PostgresStoreError> {
    match value {
        "primary" => Ok(EntityGraphAttachmentType::Primary),
        "supporting" => Ok(EntityGraphAttachmentType::Supporting),
        "historical" => Ok(EntityGraphAttachmentType::Historical),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown entity graph attachment type {value}"
        ))),
    }
}

fn id_strings<T>(ids: &[T]) -> Vec<String>
where
    T: ToString,
{
    ids.iter().map(ToString::to_string).collect()
}

fn ids<T>(values: Vec<String>) -> Result<Vec<T>, PostgresStoreError>
where
    T: TryFromString,
{
    values.into_iter().map(T::try_from_string).collect()
}

trait TryFromString: Sized {
    fn try_from_string(value: String) -> Result<Self, PostgresStoreError>;
}

impl TryFromString for EntityId {
    fn try_from_string(value: String) -> Result<Self, PostgresStoreError> {
        Self::new(value).map_err(PostgresStoreError::InvalidPersistedValue)
    }
}

impl TryFromString for EvidenceId {
    fn try_from_string(value: String) -> Result<Self, PostgresStoreError> {
        Self::new(value).map_err(PostgresStoreError::InvalidPersistedValue)
    }
}

impl TryFromString for ClaimId {
    fn try_from_string(value: String) -> Result<Self, PostgresStoreError> {
        Self::new(value).map_err(PostgresStoreError::InvalidPersistedValue)
    }
}

impl TryFromString for MemorySubgraphId {
    fn try_from_string(value: String) -> Result<Self, PostgresStoreError> {
        Self::new(value).map_err(PostgresStoreError::InvalidPersistedValue)
    }
}

impl TryFromString for MemoryMemberId {
    fn try_from_string(value: String) -> Result<Self, PostgresStoreError> {
        Self::new(value).map_err(PostgresStoreError::InvalidPersistedValue)
    }
}

impl TryFromString for CanonicalEntityId {
    fn try_from_string(value: String) -> Result<Self, PostgresStoreError> {
        Self::new(value).map_err(PostgresStoreError::InvalidPersistedValue)
    }
}
