#![expect(
    clippy::missing_errors_doc,
    reason = "memory persistence methods all return PostgresStoreError for SQL and domain conversion failures"
)]

use capsulet_core::{
    Authority, Claim, ClaimId, ClaimStatus, Confidence, Entity, EntityId, Event, EventId, Evidence,
    EvidenceId, MemoryContract, MemoryContractId, MemoryScope, Relationship, RelationshipId,
    Source, SourceId,
};
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
