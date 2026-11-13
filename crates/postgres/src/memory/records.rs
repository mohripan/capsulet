//! Entities, claims and their conflicts, events, and relationships.

#![expect(
    clippy::missing_errors_doc,
    reason = "memory persistence methods all return PostgresStoreError for SQL and domain conversion failures"
)]

use capsulet_core::{
    CanonicalEntityId, Claim, ClaimConflict, ClaimConflictId, ClaimConflictStatus, ClaimId,
    ClaimStatus, Confidence, Entity, EntityId, Event, EventId, EvidenceId, Relationship,
    RelationshipId,
};
use sqlx::Row;

use super::{id_strings, ids, parse_authority, scope, scope_guard};
use crate::{PostgresStore, PostgresStoreError};

impl PostgresStore {
    pub async fn upsert_memory_entity(&self, entity: &Entity) -> Result<(), PostgresStoreError> {
        let result = sqlx::query(
            r"
            INSERT INTO memory_entities (id, tenant_id, project_id, entity_type, name, aliases, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, now())
            ON CONFLICT (id) DO UPDATE SET
                entity_type = EXCLUDED.entity_type,
                name = EXCLUDED.name,
                aliases = EXCLUDED.aliases,
                updated_at = now()
            WHERE memory_entities.tenant_id = EXCLUDED.tenant_id
              AND memory_entities.project_id = EXCLUDED.project_id
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
        scope_guard(&result, "memory_entities", entity.id().as_str())
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
        let result = sqlx::query(
            r"
            INSERT INTO memory_claims (
                id, tenant_id, project_id, subject_id, predicate, object, evidence_ids,
                confidence, authority, status, observed_at, valid_from, valid_until, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, now())
            ON CONFLICT (id) DO UPDATE SET
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
            WHERE memory_claims.tenant_id = EXCLUDED.tenant_id
              AND memory_claims.project_id = EXCLUDED.project_id
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
        scope_guard(&result, "memory_claims", claim.id().as_str())
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
        let result = sqlx::query(
            r"
            INSERT INTO memory_claim_conflicts (
                id, tenant_id, project_id, subject_id, canonical_entity_id, predicate,
                claim_ids, status, reason, preferred_claim_id, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now())
            ON CONFLICT (id) DO UPDATE SET
                subject_id = EXCLUDED.subject_id,
                canonical_entity_id = EXCLUDED.canonical_entity_id,
                predicate = EXCLUDED.predicate,
                claim_ids = EXCLUDED.claim_ids,
                status = EXCLUDED.status,
                reason = EXCLUDED.reason,
                preferred_claim_id = EXCLUDED.preferred_claim_id,
                updated_at = now()
            WHERE memory_claim_conflicts.tenant_id = EXCLUDED.tenant_id
              AND memory_claim_conflicts.project_id = EXCLUDED.project_id
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
        scope_guard(&result, "memory_claim_conflicts", conflict.id().as_str())
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
        let result = sqlx::query(
            r"
            INSERT INTO memory_events (
                id, tenant_id, project_id, event_type, occurred_at, entity_ids, evidence_ids, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, now())
            ON CONFLICT (id) DO UPDATE SET
                event_type = EXCLUDED.event_type,
                occurred_at = EXCLUDED.occurred_at,
                entity_ids = EXCLUDED.entity_ids,
                evidence_ids = EXCLUDED.evidence_ids,
                updated_at = now()
            WHERE memory_events.tenant_id = EXCLUDED.tenant_id
              AND memory_events.project_id = EXCLUDED.project_id
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
        scope_guard(&result, "memory_events", event.id().as_str())
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
        let result = sqlx::query(
            r"
            INSERT INTO memory_relationships (
                id, tenant_id, project_id, relationship_type, from_entity_id, to_entity_id, evidence_ids, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, now())
            ON CONFLICT (id) DO UPDATE SET
                relationship_type = EXCLUDED.relationship_type,
                from_entity_id = EXCLUDED.from_entity_id,
                to_entity_id = EXCLUDED.to_entity_id,
                evidence_ids = EXCLUDED.evidence_ids,
                updated_at = now()
            WHERE memory_relationships.tenant_id = EXCLUDED.tenant_id
              AND memory_relationships.project_id = EXCLUDED.project_id
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
        scope_guard(&result, "memory_relationships", relationship.id().as_str())
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

    /// The same lookup, within one project: another project's record matches no row.
    pub async fn find_memory_entity_in_project(
        &self,
        id: &EntityId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Entity>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, entity_type, name, aliases
            FROM memory_entities
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_entity).transpose()
    }

    /// The same lookup, within one project: another project's record matches no row.
    pub async fn find_memory_claim_in_project(
        &self,
        id: &ClaimId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Claim>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, subject_id, predicate, object, evidence_ids,
                   confidence, authority, status, observed_at, valid_from, valid_until
            FROM memory_claims
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_claim).transpose()
    }

    /// The same lookup, within one project: another project's record matches no row.
    pub async fn find_memory_claim_conflict_in_project(
        &self,
        id: &ClaimConflictId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<ClaimConflict>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, subject_id, canonical_entity_id, predicate,
                   claim_ids, status, reason, preferred_claim_id
            FROM memory_claim_conflicts
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_claim_conflict).transpose()
    }

    /// The same lookup, within one project: another project's record matches no row.
    pub async fn find_memory_event_in_project(
        &self,
        id: &EventId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Event>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, event_type, occurred_at, entity_ids, evidence_ids
            FROM memory_events
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_event).transpose()
    }

    /// The same lookup, within one project: another project's record matches no row.
    pub async fn find_memory_relationship_in_project(
        &self,
        id: &RelationshipId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<Relationship>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, relationship_type, from_entity_id, to_entity_id, evidence_ids
            FROM memory_relationships
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_relationship).transpose()
    }
}

pub(super) fn row_to_entity(row: &sqlx::postgres::PgRow) -> Result<Entity, PostgresStoreError> {
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

pub(super) fn row_to_claim(row: &sqlx::postgres::PgRow) -> Result<Claim, PostgresStoreError> {
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

pub(super) fn row_to_claim_conflict(
    row: &sqlx::postgres::PgRow,
) -> Result<ClaimConflict, PostgresStoreError> {
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

pub(super) fn row_to_event(row: &sqlx::postgres::PgRow) -> Result<Event, PostgresStoreError> {
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

pub(super) fn row_to_relationship(
    row: &sqlx::postgres::PgRow,
) -> Result<Relationship, PostgresStoreError> {
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

pub(super) fn parse_claim_status(value: &str) -> Result<ClaimStatus, PostgresStoreError> {
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

pub(super) fn parse_claim_conflict_status(
    value: &str,
) -> Result<ClaimConflictStatus, PostgresStoreError> {
    match value {
        "candidate" => Ok(ClaimConflictStatus::Candidate),
        "resolved" => Ok(ClaimConflictStatus::Resolved),
        "dismissed" => Ok(ClaimConflictStatus::Dismissed),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown claim conflict status {value}"
        ))),
    }
}
