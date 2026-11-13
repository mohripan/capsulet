//! The memory graph: subgraphs, their members and edges, canonical entities, entity resolutions, summary traces and graph attachments.

#![expect(
    clippy::missing_errors_doc,
    reason = "memory persistence methods all return PostgresStoreError for SQL and domain conversion failures"
)]

use capsulet_core::{
    CanonicalEntity, CanonicalEntityId, ClaimId, Confidence, EntityGraphAttachment,
    EntityGraphAttachmentType, EntityId, EntityResolution, EntityResolutionId,
    EntityResolutionStatus, MemoryContractId, MemoryMemberKind, MemorySubgraph, MemorySubgraphId,
    MemorySubgraphMember, MemorySubgraphMemberRole, MemorySubgraphOwner, MemorySubgraphOwnerKind,
    MemorySubgraphPermissions, MemorySubgraphStatus, SubgraphEdge, SummaryTrace, SummaryTraceId,
};
use serde_json::Value;
use sqlx::Row;

use super::{id_strings, ids, scope, scope_guard};
use crate::{PostgresStore, PostgresStoreError};

impl PostgresStore {
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
        let result = sqlx::query(
            r"
            INSERT INTO memory_subgraphs (
                id, tenant_id, project_id, parent_subgraph_id, name, description, owner_kind,
                owner_id, contract_id, summary_claim_id, permissions, status, updated_at
            )
            SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, now()
            WHERE $4::text IS NULL OR EXISTS (
                SELECT 1 FROM memory_subgraphs parent
                WHERE parent.id = $4 AND parent.tenant_id = $2 AND parent.project_id = $3
                FOR SHARE
            )
            ON CONFLICT (id) DO UPDATE SET
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
            WHERE memory_subgraphs.tenant_id = EXCLUDED.tenant_id
              AND memory_subgraphs.project_id = EXCLUDED.project_id
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
        subgraph_scoped_write(&result)
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
        let result = sqlx::query(
            r"
            INSERT INTO memory_subgraph_members (
                id, tenant_id, project_id, subgraph_id, member_kind, member_id, role, updated_at
            )
            SELECT $1, $2, $3, $4, $5, $6, $7, now()
            WHERE EXISTS (
                SELECT 1 FROM memory_subgraphs parent
                WHERE parent.id = $4 AND parent.tenant_id = $2 AND parent.project_id = $3
                FOR SHARE
            )
            ON CONFLICT (id) DO UPDATE SET
                subgraph_id = EXCLUDED.subgraph_id,
                member_kind = EXCLUDED.member_kind,
                member_id = EXCLUDED.member_id,
                role = EXCLUDED.role,
                updated_at = now()
            WHERE memory_subgraph_members.tenant_id = EXCLUDED.tenant_id
              AND memory_subgraph_members.project_id = EXCLUDED.project_id
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
        subgraph_scoped_write(&result)
    }

    pub async fn upsert_memory_canonical_entity(
        &self,
        entity: &CanonicalEntity,
    ) -> Result<(), PostgresStoreError> {
        let result = sqlx::query(
            r"
            INSERT INTO memory_canonical_entities (
                id, tenant_id, project_id, entity_type, display_name, aliases, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, now())
            ON CONFLICT (id) DO UPDATE SET
                entity_type = EXCLUDED.entity_type,
                display_name = EXCLUDED.display_name,
                aliases = EXCLUDED.aliases,
                updated_at = now()
            WHERE memory_canonical_entities.tenant_id = EXCLUDED.tenant_id
              AND memory_canonical_entities.project_id = EXCLUDED.project_id
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
        scope_guard(&result, "memory_canonical_entities", entity.id().as_str())
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
        let result = sqlx::query(
            r"
            INSERT INTO memory_entity_resolutions (
                id, tenant_id, project_id, subgraph_id, entity_id, canonical_entity_id,
                confidence, status, evidence_ids, updated_at
            )
            SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, now()
            WHERE EXISTS (
                SELECT 1 FROM memory_subgraphs parent
                WHERE parent.id = $4 AND parent.tenant_id = $2 AND parent.project_id = $3
                FOR SHARE
            )
            ON CONFLICT (id) DO UPDATE SET
                subgraph_id = EXCLUDED.subgraph_id,
                entity_id = EXCLUDED.entity_id,
                canonical_entity_id = EXCLUDED.canonical_entity_id,
                confidence = EXCLUDED.confidence,
                status = EXCLUDED.status,
                evidence_ids = EXCLUDED.evidence_ids,
                updated_at = now()
            WHERE memory_entity_resolutions.tenant_id = EXCLUDED.tenant_id
              AND memory_entity_resolutions.project_id = EXCLUDED.project_id
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
        subgraph_scoped_write(&result)
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
        let result = sqlx::query(
            r"
            INSERT INTO memory_subgraph_edges (
                id, tenant_id, project_id, edge_type, from_subgraph_id, to_subgraph_id,
                from_member_kind, from_member_id, to_member_kind, to_member_id,
                claim_ids, evidence_ids, updated_at
            )
            SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, now()
            WHERE EXISTS (
                SELECT 1 FROM memory_subgraphs from_end
                WHERE from_end.id = $5 AND from_end.tenant_id = $2 AND from_end.project_id = $3
                FOR SHARE
            )
            AND EXISTS (
                SELECT 1 FROM memory_subgraphs to_end
                WHERE to_end.id = $6 AND to_end.tenant_id = $2 AND to_end.project_id = $3
                FOR SHARE
            )
            ON CONFLICT (id) DO UPDATE SET
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
            WHERE memory_subgraph_edges.tenant_id = EXCLUDED.tenant_id
              AND memory_subgraph_edges.project_id = EXCLUDED.project_id
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
        subgraph_scoped_write(&result)
    }

    pub async fn upsert_memory_summary_trace(
        &self,
        trace: &SummaryTrace,
    ) -> Result<(), PostgresStoreError> {
        let result = sqlx::query(
            r"
            INSERT INTO memory_summary_traces (
                id, tenant_id, project_id, subgraph_id, summary_claim_id, inner_claim_ids, evidence_ids
            )
            SELECT $1, $2, $3, $4, $5, $6, $7
            WHERE EXISTS (
                SELECT 1 FROM memory_subgraphs parent
                WHERE parent.id = $4 AND parent.tenant_id = $2 AND parent.project_id = $3
                FOR SHARE
            )
            ON CONFLICT (id) DO UPDATE SET
                subgraph_id = EXCLUDED.subgraph_id,
                summary_claim_id = EXCLUDED.summary_claim_id,
                inner_claim_ids = EXCLUDED.inner_claim_ids,
                evidence_ids = EXCLUDED.evidence_ids
            WHERE memory_summary_traces.tenant_id = EXCLUDED.tenant_id
              AND memory_summary_traces.project_id = EXCLUDED.project_id
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
        subgraph_scoped_write(&result)
    }

    pub async fn list_memory_summary_traces(
        &self,
        subgraph_id: &MemorySubgraphId,
        summary_claim_id: &ClaimId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<SummaryTrace>, PostgresStoreError> {
        // Scoped on the trace itself, not only on its subgraph: a trace another
        // project wrote against this subgraph must not count toward its activation.
        let rows = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, subgraph_id, summary_claim_id, inner_claim_ids, evidence_ids
            FROM memory_summary_traces
            WHERE subgraph_id = $1 AND summary_claim_id = $2
              AND tenant_id = $3 AND project_id = $4
            ORDER BY created_at DESC, id ASC
            ",
        )
        .bind(subgraph_id.as_str())
        .bind(summary_claim_id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_summary_trace).collect()
    }

    pub async fn upsert_memory_entity_graph_attachment(
        &self,
        attachment: &EntityGraphAttachment,
    ) -> Result<(), PostgresStoreError> {
        let result = sqlx::query(
            r"
            INSERT INTO memory_entity_graph_attachments (
                id, tenant_id, project_id, canonical_entity_id, subgraph_id, attachment_type, updated_at
            )
            SELECT $1, $2, $3, $4, $5, $6, now()
            WHERE EXISTS (
                SELECT 1 FROM memory_subgraphs parent
                WHERE parent.id = $5 AND parent.tenant_id = $2 AND parent.project_id = $3
                FOR SHARE
            )
            ON CONFLICT (id) DO UPDATE SET
                canonical_entity_id = EXCLUDED.canonical_entity_id,
                subgraph_id = EXCLUDED.subgraph_id,
                attachment_type = EXCLUDED.attachment_type,
                updated_at = now()
            WHERE memory_entity_graph_attachments.tenant_id = EXCLUDED.tenant_id
              AND memory_entity_graph_attachments.project_id = EXCLUDED.project_id
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
        subgraph_scoped_write(&result)
    }

    /// The same lookup, within one project: another project's record matches no row.
    pub async fn find_memory_subgraph_in_project(
        &self,
        id: &MemorySubgraphId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<MemorySubgraph>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, parent_subgraph_id, name, description, owner_kind,
                   owner_id, contract_id, summary_claim_id, permissions, status
            FROM memory_subgraphs
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_subgraph).transpose()
    }

    /// The same lookup, within one project: another project's record matches no row.
    pub async fn find_memory_entity_resolution_in_project(
        &self,
        id: &EntityResolutionId,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Option<EntityResolution>, PostgresStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, tenant_id, project_id, subgraph_id, entity_id, canonical_entity_id,
                   confidence, status, evidence_ids
            FROM memory_entity_resolutions
            WHERE id = $1 AND tenant_id = $2 AND project_id = $3
            ",
        )
        .bind(id.as_str())
        .bind(tenant_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_entity_resolution).transpose()
    }
}

/// A write that names a subgraph is its own ownership check. It inserts nothing when the
/// subgraph it names belongs to another project, so a record can never hang off a
/// subgraph it was not written in, and the scoped `ON CONFLICT ... WHERE` updates nothing
/// when another project holds the id. Both report as [`PostgresStoreError::ScopeMismatch`],
/// the answer a missing subgraph gets, so a write cannot be used to learn whether some
/// other project's id exists.
pub(super) fn subgraph_scoped_write(
    result: &sqlx::postgres::PgQueryResult,
) -> Result<(), PostgresStoreError> {
    if result.rows_affected() == 0 {
        return Err(PostgresStoreError::ScopeMismatch);
    }
    Ok(())
}

pub(super) fn row_to_subgraph(
    row: &sqlx::postgres::PgRow,
) -> Result<MemorySubgraph, PostgresStoreError> {
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

pub(super) fn row_to_canonical_entity(
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

pub(super) fn row_to_entity_resolution(
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

pub(super) fn row_to_summary_trace(
    row: &sqlx::postgres::PgRow,
) -> Result<SummaryTrace, PostgresStoreError> {
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

pub(super) fn parse_subgraph_status(
    value: &str,
) -> Result<MemorySubgraphStatus, PostgresStoreError> {
    match value {
        "draft" => Ok(MemorySubgraphStatus::Draft),
        "active" => Ok(MemorySubgraphStatus::Active),
        "archived" => Ok(MemorySubgraphStatus::Archived),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown memory subgraph status {value}"
        ))),
    }
}

pub(super) fn parse_owner_kind(value: &str) -> Result<MemorySubgraphOwnerKind, PostgresStoreError> {
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
pub(super) fn parse_member_kind(value: &str) -> Result<MemoryMemberKind, PostgresStoreError> {
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
pub(super) fn parse_member_role(
    value: &str,
) -> Result<MemorySubgraphMemberRole, PostgresStoreError> {
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
pub(super) fn parse_entity_resolution_status(
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
pub(super) fn parse_entity_graph_attachment_type(
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
