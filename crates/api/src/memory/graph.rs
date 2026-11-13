//! The memory graph: subgraphs, their members and edges, canonical entities, summary traces and graph attachments.

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use capsulet_core::{
    CanonicalEntity, CanonicalEntityId, ClaimId, EntityGraphAttachment, EntityGraphAttachmentId,
    EntityGraphAttachmentType, EvidenceId, MemoryContractId, MemoryMemberId, MemoryMemberKind,
    MemorySubgraph, MemorySubgraphActivation, MemorySubgraphId, MemorySubgraphMember,
    MemorySubgraphMemberId, MemorySubgraphMemberRole, MemorySubgraphOwner, MemorySubgraphOwnerKind,
    MemorySubgraphPermissions, SubgraphEdge, SubgraphEdgeId, SummaryTrace, SummaryTraceId,
};
use capsulet_storage::ObjectStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use super::{require_scope, scope, write_context};
use crate::{
    auth::Principal,
    error::ApiError,
    http::{generated_id, project_context},
    state::AppState,
    store::ApiStore,
};

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CreateMemorySubgraphRequest {
    id: Option<String>,
    parent_subgraph_id: Option<String>,
    name: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ActivateMemorySubgraphRequest {
    owner_kind: String,
    owner_id: String,
    contract_id: String,
    #[schema(value_type = Object)]
    permissions: Value,
    summary_claim_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CreateMemorySubgraphMemberRequest {
    id: Option<String>,
    member_kind: String,
    member_id: String,
    role: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CreateCanonicalEntityRequest {
    id: Option<String>,
    entity_type: String,
    display_name: String,
    #[serde(default)]
    aliases: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CreateSummaryTraceRequest {
    id: Option<String>,
    subgraph_id: String,
    summary_claim_id: String,
    #[serde(default)]
    inner_claim_ids: Vec<String>,
    #[serde(default)]
    evidence_ids: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CreateEntityGraphAttachmentRequest {
    id: Option<String>,
    canonical_entity_id: String,
    subgraph_id: String,
    attachment_type: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CreateSubgraphEdgeRequest {
    id: Option<String>,
    edge_type: String,
    from_subgraph_id: String,
    to_subgraph_id: String,
    from_member_kind: String,
    from_member_id: String,
    to_member_kind: String,
    to_member_id: String,
    #[serde(default)]
    claim_ids: Vec<String>,
    #[serde(default)]
    evidence_ids: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct MemorySubgraphResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    parent_subgraph_id: Option<String>,
    name: String,
    description: Option<String>,
    owner_kind: Option<String>,
    owner_id: Option<String>,
    contract_id: Option<String>,
    summary_claim_id: Option<String>,
    #[schema(value_type = Option<Object>)]
    permissions: Option<Value>,
    status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct MemorySubgraphMemberResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    subgraph_id: String,
    member_kind: String,
    member_id: String,
    role: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CanonicalEntityResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    entity_type: String,
    display_name: String,
    aliases: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct SummaryTraceResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    subgraph_id: String,
    summary_claim_id: String,
    inner_claim_ids: Vec<String>,
    evidence_ids: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct EntityGraphAttachmentResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    canonical_entity_id: String,
    subgraph_id: String,
    attachment_type: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct SubgraphEdgeResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    edge_type: String,
    from_subgraph_id: String,
    to_subgraph_id: String,
    from_member_kind: String,
    from_member_id: String,
    to_member_kind: String,
    to_member_id: String,
    claim_ids: Vec<String>,
    evidence_ids: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListMemorySubgraphsResponse {
    subgraphs: Vec<MemorySubgraphResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListCanonicalEntitiesResponse {
    canonical_entities: Vec<CanonicalEntityResponse>,
}

pub(crate) async fn create_subgraph<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateMemorySubgraphRequest>,
) -> Result<(StatusCode, Json<MemorySubgraphResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let subgraph = MemorySubgraph::draft(
        MemorySubgraphId::new(request.id.unwrap_or_else(|| generated_id("subgraph")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        request
            .parent_subgraph_id
            .map(MemorySubgraphId::new)
            .transpose()
            .map_err(ApiError::validation)?,
        request.name,
        request.description.as_deref(),
    )
    .map_err(graph_validation)?;
    if let Some(existing) = state
        .store
        .find_memory_subgraph(subgraph.id())
        .await
        .map_err(ApiError::store)?
    {
        require_scope(existing.scope(), &context, subgraph.id().as_str())?;
    }
    state
        .store
        .upsert_memory_subgraph(&subgraph)
        .await
        .map_err(|error| subgraph_write_error::<S>(error, &named_subgraph(&subgraph)))?;
    Ok((
        StatusCode::CREATED,
        Json(MemorySubgraphResponse::from(&subgraph)),
    ))
}

pub(crate) async fn list_subgraphs<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListMemorySubgraphsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let subgraphs = state
        .store
        .list_memory_subgraphs(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListMemorySubgraphsResponse {
        subgraphs: subgraphs.iter().map(MemorySubgraphResponse::from).collect(),
    }))
}

pub(crate) async fn get_subgraph<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<MemorySubgraphResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = MemorySubgraphId::new(id).map_err(ApiError::validation)?;
    let Some(subgraph) = state
        .store
        .find_memory_subgraph_in_project(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(MemorySubgraphResponse::from(&subgraph)))
}

pub(crate) async fn activate_subgraph<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
    Json(request): Json<ActivateMemorySubgraphRequest>,
) -> Result<Json<MemorySubgraphResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let id = MemorySubgraphId::new(id).map_err(ApiError::validation)?;
    let summary_claim_id = ClaimId::new(request.summary_claim_id).map_err(ApiError::validation)?;
    let Some(subgraph) = state
        .store
        .find_memory_subgraph_in_project(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    // Only this project's traces can support its summary, whoever wrote against the id.
    let traces = state
        .store
        .list_memory_summary_traces(
            &id,
            &summary_claim_id,
            &context.tenant_id,
            &context.project_id,
        )
        .await
        .map_err(ApiError::store)?;
    let activated = subgraph
        .activate(MemorySubgraphActivation::new(
            Some(
                MemorySubgraphOwner::new(parse_owner_kind(&request.owner_kind)?, request.owner_id)
                    .map_err(graph_validation)?,
            ),
            Some(MemoryContractId::new(request.contract_id).map_err(ApiError::validation)?),
            Some(
                MemorySubgraphPermissions::new(request.permissions.to_string())
                    .map_err(graph_validation)?,
            ),
            Some(summary_claim_id),
            traces,
        ))
        .map_err(graph_validation)?;
    state
        .store
        .upsert_memory_subgraph(&activated)
        .await
        .map_err(|error| subgraph_write_error::<S>(error, &named_subgraph(&activated)))?;
    Ok(Json(MemorySubgraphResponse::from(&activated)))
}

pub(crate) async fn create_subgraph_member<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(subgraph_id): Path<String>,
    Json(request): Json<CreateMemorySubgraphMemberRequest>,
) -> Result<(StatusCode, Json<MemorySubgraphMemberResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let member = MemorySubgraphMember::new(
        MemorySubgraphMemberId::new(request.id.unwrap_or_else(|| generated_id("member")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        MemorySubgraphId::new(subgraph_id).map_err(ApiError::validation)?,
        parse_member_kind(&request.member_kind)?,
        MemoryMemberId::new(request.member_id).map_err(ApiError::validation)?,
        parse_member_role(&request.role)?,
    )
    .map_err(graph_validation)?;
    state
        .store
        .upsert_memory_subgraph_member(&member)
        .await
        .map_err(|error| subgraph_write_error::<S>(error, member.subgraph_id().as_str()))?;
    Ok((
        StatusCode::CREATED,
        Json(MemorySubgraphMemberResponse::from(&member)),
    ))
}

pub(crate) async fn create_canonical_entity<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateCanonicalEntityRequest>,
) -> Result<(StatusCode, Json<CanonicalEntityResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let entity = CanonicalEntity::new(
        CanonicalEntityId::new(
            request
                .id
                .unwrap_or_else(|| generated_id("canonical_entity")),
        )
        .map_err(ApiError::validation)?,
        scope(&context)?,
        request.entity_type,
        request.display_name,
        request.aliases,
    )
    .map_err(graph_validation)?;
    state
        .store
        .upsert_memory_canonical_entity(&entity)
        .await
        .map_err(ApiError::store)?;
    Ok((
        StatusCode::CREATED,
        Json(CanonicalEntityResponse::from(&entity)),
    ))
}

pub(crate) async fn list_canonical_entities<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListCanonicalEntitiesResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let entities = state
        .store
        .list_memory_canonical_entities(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListCanonicalEntitiesResponse {
        canonical_entities: entities.iter().map(CanonicalEntityResponse::from).collect(),
    }))
}

pub(crate) async fn create_summary_trace<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateSummaryTraceRequest>,
) -> Result<(StatusCode, Json<SummaryTraceResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let trace = SummaryTrace::new(
        SummaryTraceId::new(request.id.unwrap_or_else(|| generated_id("trace")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        MemorySubgraphId::new(request.subgraph_id).map_err(ApiError::validation)?,
        ClaimId::new(request.summary_claim_id).map_err(ApiError::validation)?,
        request
            .inner_claim_ids
            .into_iter()
            .map(ClaimId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::validation)?,
        request
            .evidence_ids
            .into_iter()
            .map(EvidenceId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::validation)?,
    )
    .map_err(graph_validation)?;
    state
        .store
        .upsert_memory_summary_trace(&trace)
        .await
        .map_err(|error| subgraph_write_error::<S>(error, trace.subgraph_id().as_str()))?;
    Ok((
        StatusCode::CREATED,
        Json(SummaryTraceResponse::from(&trace)),
    ))
}

pub(crate) async fn create_entity_graph_attachment<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateEntityGraphAttachmentRequest>,
) -> Result<(StatusCode, Json<EntityGraphAttachmentResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let attachment = EntityGraphAttachment::new(
        EntityGraphAttachmentId::new(request.id.unwrap_or_else(|| generated_id("attachment")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        CanonicalEntityId::new(request.canonical_entity_id).map_err(ApiError::validation)?,
        MemorySubgraphId::new(request.subgraph_id).map_err(ApiError::validation)?,
        parse_attachment_type(&request.attachment_type)?,
    )
    .map_err(graph_validation)?;
    state
        .store
        .upsert_memory_entity_graph_attachment(&attachment)
        .await
        .map_err(|error| subgraph_write_error::<S>(error, attachment.subgraph_id().as_str()))?;
    Ok((
        StatusCode::CREATED,
        Json(EntityGraphAttachmentResponse::from(&attachment)),
    ))
}

pub(crate) async fn create_subgraph_edge<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateSubgraphEdgeRequest>,
) -> Result<(StatusCode, Json<SubgraphEdgeResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let edge = SubgraphEdge::new(
        SubgraphEdgeId::new(request.id.unwrap_or_else(|| generated_id("edge")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        request.edge_type,
        MemorySubgraphId::new(request.from_subgraph_id).map_err(ApiError::validation)?,
        MemorySubgraphId::new(request.to_subgraph_id).map_err(ApiError::validation)?,
        parse_member_kind(&request.from_member_kind)?,
        MemoryMemberId::new(request.from_member_id).map_err(ApiError::validation)?,
        parse_member_kind(&request.to_member_kind)?,
        MemoryMemberId::new(request.to_member_id).map_err(ApiError::validation)?,
        request
            .claim_ids
            .into_iter()
            .map(ClaimId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::validation)?,
        request
            .evidence_ids
            .into_iter()
            .map(EvidenceId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::validation)?,
    )
    .map_err(graph_validation)?;
    state
        .store
        .upsert_memory_subgraph_edge(&edge)
        .await
        .map_err(|error| {
            subgraph_write_error::<S>(
                error,
                &format!(
                    "{} or {}",
                    edge.from_subgraph_id().as_str(),
                    edge.to_subgraph_id().as_str()
                ),
            )
        })?;
    Ok((StatusCode::CREATED, Json(SubgraphEdgeResponse::from(&edge))))
}

/// The store refuses a write that names a subgraph outside the caller's project, and the
/// API reports that subgraph as missing, the same answer reading it would give.
pub(super) fn subgraph_write_error<S: ApiStore>(error: S::Error, subgraph: &str) -> ApiError {
    if S::is_scope_mismatch(&error) {
        ApiError::MemoryNotFound(subgraph.to_string())
    } else {
        ApiError::store(error)
    }
}

/// The subgraph a refused subgraph write is about: its parent when it names one.
pub(super) fn named_subgraph(subgraph: &MemorySubgraph) -> String {
    subgraph
        .parent_subgraph_id()
        .unwrap_or_else(|| subgraph.id())
        .as_str()
        .to_string()
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "map_err supplies the owned MemoryGraphError and this helper keeps conversions concise"
)]
pub(super) fn graph_validation(error: capsulet_core::MemoryGraphError) -> ApiError {
    ApiError::Validation(error.to_string())
}

pub(super) fn parse_owner_kind(value: &str) -> Result<MemorySubgraphOwnerKind, ApiError> {
    match value {
        "user" => Ok(MemorySubgraphOwnerKind::User),
        "team" => Ok(MemorySubgraphOwnerKind::Team),
        "service" => Ok(MemorySubgraphOwnerKind::Service),
        "organization" => Ok(MemorySubgraphOwnerKind::Organization),
        value => Err(ApiError::Validation(format!("unknown owner kind {value}"))),
    }
}

pub(super) fn parse_member_kind(value: &str) -> Result<MemoryMemberKind, ApiError> {
    match value {
        "source" => Ok(MemoryMemberKind::Source),
        "evidence" => Ok(MemoryMemberKind::Evidence),
        "entity" => Ok(MemoryMemberKind::Entity),
        "canonical_entity" => Ok(MemoryMemberKind::CanonicalEntity),
        "claim" => Ok(MemoryMemberKind::Claim),
        "event" => Ok(MemoryMemberKind::Event),
        "relationship" => Ok(MemoryMemberKind::Relationship),
        "subgraph" => Ok(MemoryMemberKind::Subgraph),
        value => Err(ApiError::Validation(format!("unknown member kind {value}"))),
    }
}

pub(super) fn parse_member_role(value: &str) -> Result<MemorySubgraphMemberRole, ApiError> {
    match value {
        "member" => Ok(MemorySubgraphMemberRole::Member),
        "summary" => Ok(MemorySubgraphMemberRole::Summary),
        "inner_claim" => Ok(MemorySubgraphMemberRole::InnerClaim),
        "evidence" => Ok(MemorySubgraphMemberRole::Evidence),
        "canonical_identity" => Ok(MemorySubgraphMemberRole::CanonicalIdentity),
        "child_context" => Ok(MemorySubgraphMemberRole::ChildContext),
        value => Err(ApiError::Validation(format!("unknown member role {value}"))),
    }
}

pub(super) fn parse_attachment_type(value: &str) -> Result<EntityGraphAttachmentType, ApiError> {
    match value {
        "primary" => Ok(EntityGraphAttachmentType::Primary),
        "supporting" => Ok(EntityGraphAttachmentType::Supporting),
        "historical" => Ok(EntityGraphAttachmentType::Historical),
        value => Err(ApiError::Validation(format!(
            "unknown entity graph attachment type {value}"
        ))),
    }
}

impl From<&MemorySubgraph> for MemorySubgraphResponse {
    fn from(subgraph: &MemorySubgraph) -> Self {
        let owner = subgraph.owner();
        Self {
            id: subgraph.id().as_str().to_string(),
            tenant_id: subgraph.scope().tenant_id().to_string(),
            project_id: subgraph.scope().project_id().to_string(),
            parent_subgraph_id: subgraph.parent_subgraph_id().map(ToString::to_string),
            name: subgraph.name().to_string(),
            description: subgraph.description().map(str::to_string),
            owner_kind: owner.map(|owner| owner.kind().to_string()),
            owner_id: owner.map(|owner| owner.id().to_string()),
            contract_id: subgraph.contract_id().map(ToString::to_string),
            summary_claim_id: subgraph.summary_claim_id().map(ToString::to_string),
            permissions: subgraph
                .permissions()
                .and_then(|permissions| serde_json::from_str(permissions.as_json()).ok()),
            status: subgraph.status().to_string(),
        }
    }
}

impl From<&MemorySubgraphMember> for MemorySubgraphMemberResponse {
    fn from(member: &MemorySubgraphMember) -> Self {
        Self {
            id: member.id().as_str().to_string(),
            tenant_id: member.scope().tenant_id().to_string(),
            project_id: member.scope().project_id().to_string(),
            subgraph_id: member.subgraph_id().as_str().to_string(),
            member_kind: member.member_kind().to_string(),
            member_id: member.member_id().as_str().to_string(),
            role: member.role().to_string(),
        }
    }
}

impl From<&CanonicalEntity> for CanonicalEntityResponse {
    fn from(entity: &CanonicalEntity) -> Self {
        Self {
            id: entity.id().as_str().to_string(),
            tenant_id: entity.scope().tenant_id().to_string(),
            project_id: entity.scope().project_id().to_string(),
            entity_type: entity.entity_type().to_string(),
            display_name: entity.display_name().to_string(),
            aliases: entity.aliases().to_vec(),
        }
    }
}

impl From<&SummaryTrace> for SummaryTraceResponse {
    fn from(trace: &SummaryTrace) -> Self {
        Self {
            id: trace.id().as_str().to_string(),
            tenant_id: trace.scope().tenant_id().to_string(),
            project_id: trace.scope().project_id().to_string(),
            subgraph_id: trace.subgraph_id().as_str().to_string(),
            summary_claim_id: trace.summary_claim_id().as_str().to_string(),
            inner_claim_ids: trace
                .inner_claim_ids()
                .iter()
                .map(ToString::to_string)
                .collect(),
            evidence_ids: trace
                .evidence_ids()
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }
}

impl From<&EntityGraphAttachment> for EntityGraphAttachmentResponse {
    fn from(attachment: &EntityGraphAttachment) -> Self {
        Self {
            id: attachment.id().as_str().to_string(),
            tenant_id: attachment.scope().tenant_id().to_string(),
            project_id: attachment.scope().project_id().to_string(),
            canonical_entity_id: attachment.canonical_entity_id().as_str().to_string(),
            subgraph_id: attachment.subgraph_id().as_str().to_string(),
            attachment_type: attachment.attachment_type().to_string(),
        }
    }
}

impl From<&SubgraphEdge> for SubgraphEdgeResponse {
    fn from(edge: &SubgraphEdge) -> Self {
        Self {
            id: edge.id().as_str().to_string(),
            tenant_id: edge.scope().tenant_id().to_string(),
            project_id: edge.scope().project_id().to_string(),
            edge_type: edge.edge_type().to_string(),
            from_subgraph_id: edge.from_subgraph_id().as_str().to_string(),
            to_subgraph_id: edge.to_subgraph_id().as_str().to_string(),
            from_member_kind: edge.from_member_kind().to_string(),
            from_member_id: edge.from_member_id().as_str().to_string(),
            to_member_kind: edge.to_member_kind().to_string(),
            to_member_id: edge.to_member_id().as_str().to_string(),
            claim_ids: edge.claim_ids().iter().map(ToString::to_string).collect(),
            evidence_ids: edge
                .evidence_ids()
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }
}
