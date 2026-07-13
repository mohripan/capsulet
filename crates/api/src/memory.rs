use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use capsulet_core::{
    Authority, CanonicalEntity, CanonicalEntityId, Claim, ClaimConflict, ClaimConflictId,
    ClaimConflictStatus, ClaimId, ClaimStatus, CompiledMemoryPolicy, Confidence, Entity,
    EntityGraphAttachment, EntityGraphAttachmentId, EntityGraphAttachmentType, EntityId,
    EntityResolution, EntityResolutionId, EntityResolutionStatus, Event, EventId, Evidence,
    EvidenceId, MemoryContract, MemoryContractId, MemoryMemberId, MemoryMemberKind, MemoryScope,
    MemorySubgraph, MemorySubgraphActivation, MemorySubgraphId, MemorySubgraphMember,
    MemorySubgraphMemberId, MemorySubgraphMemberRole, MemorySubgraphOwner, MemorySubgraphOwnerKind,
    MemorySubgraphPermissions, RelationTypeSpec, Relationship, RelationshipId, RetrievalPolicySpec,
    Source, SourceId, SubgraphEdge, SubgraphEdgeId, SummaryTrace, SummaryTraceId,
};
use capsulet_storage::ObjectStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    auth::Principal,
    error::ApiError,
    http::{ProjectContext, generated_id, project_context, require_project_role},
    state::AppState,
    store::ApiStore,
};

#[derive(Debug, Deserialize)]
pub(crate) struct CreateSourceRequest {
    id: Option<String>,
    kind: String,
    uri: Option<String>,
    title: String,
    authority: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateEvidenceRequest {
    id: Option<String>,
    source_id: String,
    locator: String,
    excerpt: String,
    observed_at: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateEntityRequest {
    id: Option<String>,
    entity_type: String,
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateClaimRequest {
    id: Option<String>,
    subject_id: String,
    predicate: String,
    object: String,
    evidence_ids: Vec<String>,
    confidence: f64,
    authority: String,
    status: Option<String>,
    observed_at: String,
    valid_from: Option<String>,
    valid_until: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateEventRequest {
    id: Option<String>,
    event_type: String,
    occurred_at: String,
    #[serde(default)]
    entity_ids: Vec<String>,
    evidence_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateRelationshipRequest {
    id: Option<String>,
    relationship_type: String,
    from_entity_id: String,
    to_entity_id: String,
    evidence_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateMemoryContractRequest {
    id: Option<String>,
    name: String,
    source: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateMemorySubgraphRequest {
    id: Option<String>,
    parent_subgraph_id: Option<String>,
    name: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ActivateMemorySubgraphRequest {
    owner_kind: String,
    owner_id: String,
    contract_id: String,
    permissions: Value,
    summary_claim_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateMemorySubgraphMemberRequest {
    id: Option<String>,
    member_kind: String,
    member_id: String,
    role: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateCanonicalEntityRequest {
    id: Option<String>,
    entity_type: String,
    display_name: String,
    #[serde(default)]
    aliases: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateEntityResolutionRequest {
    id: Option<String>,
    subgraph_id: String,
    entity_id: String,
    canonical_entity_id: String,
    confidence: f64,
    status: String,
    evidence_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListEntityResolutionsQuery {
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListClaimConflictsQuery {
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResolveClaimConflictRequest {
    preferred_claim_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateSummaryTraceRequest {
    id: Option<String>,
    subgraph_id: String,
    summary_claim_id: String,
    #[serde(default)]
    inner_claim_ids: Vec<String>,
    #[serde(default)]
    evidence_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateEntityGraphAttachmentRequest {
    id: Option<String>,
    canonical_entity_id: String,
    subgraph_id: String,
    attachment_type: String,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Serialize)]
pub(crate) struct SourceResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    kind: String,
    uri: Option<String>,
    title: String,
    authority: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct EvidenceResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    source_id: String,
    locator: String,
    excerpt: String,
    observed_at: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct EntityResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    entity_type: String,
    name: String,
    aliases: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ClaimResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    subject_id: String,
    predicate: String,
    object: String,
    evidence_ids: Vec<String>,
    confidence: f64,
    authority: String,
    status: String,
    observed_at: String,
    valid_from: Option<String>,
    valid_until: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct EventResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    event_type: String,
    occurred_at: String,
    entity_ids: Vec<String>,
    evidence_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RelationshipResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    relationship_type: String,
    from_entity_id: String,
    to_entity_id: String,
    evidence_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryContractResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    name: String,
    source: String,
    compiled: CompiledMemoryPolicyResponse,
}

#[derive(Debug, Serialize)]
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
    permissions: Option<Value>,
    status: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemorySubgraphMemberResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    subgraph_id: String,
    member_kind: String,
    member_id: String,
    role: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct CanonicalEntityResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    entity_type: String,
    display_name: String,
    aliases: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct EntityResolutionResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    subgraph_id: String,
    entity_id: String,
    canonical_entity_id: String,
    confidence: f64,
    status: String,
    evidence_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ClaimConflictResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    subject_id: String,
    canonical_entity_id: Option<String>,
    predicate: String,
    claim_ids: Vec<String>,
    status: String,
    reason: String,
    preferred_claim_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SummaryTraceResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    subgraph_id: String,
    summary_claim_id: String,
    inner_claim_ids: Vec<String>,
    evidence_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct EntityGraphAttachmentResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    canonical_entity_id: String,
    subgraph_id: String,
    attachment_type: String,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub(crate) struct CompiledMemoryPolicyResponse {
    entity_types: Vec<String>,
    relations: Vec<RelationPolicyResponse>,
    claim_policy: ClaimPolicyResponse,
    source_priority: Vec<String>,
    retrieval_policies: Vec<RetrievalPolicyResponse>,
    contradiction_rules: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RelationPolicyResponse {
    name: String,
    from: String,
    to: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ClaimPolicyResponse {
    require_source: bool,
    store_confidence: bool,
    allow_contradictions: bool,
    min_confidence: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct RetrievalPolicyResponse {
    name: String,
    seed_from: Vec<String>,
    max_hops: Option<u32>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListSourcesResponse {
    sources: Vec<SourceResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListEvidenceResponse {
    evidence: Vec<EvidenceResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListEntitiesResponse {
    entities: Vec<EntityResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListClaimsResponse {
    claims: Vec<ClaimResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListEventsResponse {
    events: Vec<EventResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListRelationshipsResponse {
    relationships: Vec<RelationshipResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListMemoryContractsResponse {
    contracts: Vec<MemoryContractResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListMemorySubgraphsResponse {
    subgraphs: Vec<MemorySubgraphResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListCanonicalEntitiesResponse {
    canonical_entities: Vec<CanonicalEntityResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListEntityResolutionsResponse {
    entity_resolutions: Vec<EntityResolutionResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListClaimConflictsResponse {
    conflicts: Vec<ClaimConflictResponse>,
}

pub(crate) async fn create_source<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateSourceRequest>,
) -> Result<(StatusCode, Json<SourceResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let source = Source::new(
        SourceId::new(request.id.unwrap_or_else(|| generated_id("source")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        request.kind,
        request.uri,
        request.title,
        parse_authority(&request.authority)?,
    )
    .map_err(memory_validation)?;
    state
        .store
        .upsert_memory_source(&source)
        .await
        .map_err(ApiError::store)?;
    Ok((StatusCode::CREATED, Json(SourceResponse::from(&source))))
}

pub(crate) async fn list_sources<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListSourcesResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let sources = state
        .store
        .list_memory_sources(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListSourcesResponse {
        sources: sources.iter().map(SourceResponse::from).collect(),
    }))
}

pub(crate) async fn get_source<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<SourceResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = SourceId::new(id).map_err(ApiError::validation)?;
    let Some(source) = state
        .store
        .find_memory_source(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(SourceResponse::from(&source)))
}

pub(crate) async fn create_evidence<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateEvidenceRequest>,
) -> Result<(StatusCode, Json<EvidenceResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let evidence = Evidence::new(
        EvidenceId::new(request.id.unwrap_or_else(|| generated_id("evidence")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        SourceId::new(request.source_id).map_err(ApiError::validation)?,
        request.locator,
        request.excerpt,
        request.observed_at,
    )
    .map_err(memory_validation)?;
    state
        .store
        .upsert_memory_evidence(&evidence)
        .await
        .map_err(ApiError::store)?;
    Ok((StatusCode::CREATED, Json(EvidenceResponse::from(&evidence))))
}

pub(crate) async fn list_evidence<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListEvidenceResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let evidence = state
        .store
        .list_memory_evidence(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListEvidenceResponse {
        evidence: evidence.iter().map(EvidenceResponse::from).collect(),
    }))
}

pub(crate) async fn get_evidence<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<EvidenceResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = EvidenceId::new(id).map_err(ApiError::validation)?;
    let Some(evidence) = state
        .store
        .find_memory_evidence(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(EvidenceResponse::from(&evidence)))
}

pub(crate) async fn create_entity<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateEntityRequest>,
) -> Result<(StatusCode, Json<EntityResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let entity = Entity::new(
        EntityId::new(request.id.unwrap_or_else(|| generated_id("entity")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        request.entity_type,
        request.name,
        request.aliases,
    )
    .map_err(memory_validation)?;
    state
        .store
        .upsert_memory_entity(&entity)
        .await
        .map_err(ApiError::store)?;
    Ok((StatusCode::CREATED, Json(EntityResponse::from(&entity))))
}

pub(crate) async fn list_entities<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListEntitiesResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let entities = state
        .store
        .list_memory_entities(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListEntitiesResponse {
        entities: entities.iter().map(EntityResponse::from).collect(),
    }))
}

pub(crate) async fn get_entity<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<EntityResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = EntityId::new(id).map_err(ApiError::validation)?;
    let Some(entity) = state
        .store
        .find_memory_entity(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(EntityResponse::from(&entity)))
}

pub(crate) async fn create_claim<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateClaimRequest>,
) -> Result<(StatusCode, Json<ClaimResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let claim = Claim::new(
        ClaimId::new(request.id.unwrap_or_else(|| generated_id("claim")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        EntityId::new(request.subject_id).map_err(ApiError::validation)?,
        request.predicate,
        request.object,
        request
            .evidence_ids
            .into_iter()
            .map(EvidenceId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::validation)?,
        Confidence::new(request.confidence).map_err(memory_validation)?,
        parse_authority(&request.authority)?,
        request.observed_at,
        request.valid_from.as_deref(),
        request.valid_until.as_deref(),
    )
    .map_err(memory_validation)?
    .with_status(match request.status {
        Some(status) => parse_claim_status(&status)?,
        None => ClaimStatus::Candidate,
    });
    state
        .store
        .upsert_memory_claim(&claim)
        .await
        .map_err(ApiError::store)?;
    Ok((StatusCode::CREATED, Json(ClaimResponse::from(&claim))))
}

pub(crate) async fn list_claims<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListClaimsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let claims = state
        .store
        .list_memory_claims(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListClaimsResponse {
        claims: claims.iter().map(ClaimResponse::from).collect(),
    }))
}

pub(crate) async fn get_claim<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<ClaimResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = ClaimId::new(id).map_err(ApiError::validation)?;
    let Some(claim) = state
        .store
        .find_memory_claim(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(ClaimResponse::from(&claim)))
}

pub(crate) async fn create_event<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateEventRequest>,
) -> Result<(StatusCode, Json<EventResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let event = Event::new(
        EventId::new(request.id.unwrap_or_else(|| generated_id("event")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        request.event_type,
        request.occurred_at,
        request
            .entity_ids
            .into_iter()
            .map(EntityId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::validation)?,
        request
            .evidence_ids
            .into_iter()
            .map(EvidenceId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::validation)?,
    )
    .map_err(memory_validation)?;
    state
        .store
        .upsert_memory_event(&event)
        .await
        .map_err(ApiError::store)?;
    Ok((StatusCode::CREATED, Json(EventResponse::from(&event))))
}

pub(crate) async fn list_events<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListEventsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let events = state
        .store
        .list_memory_events(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListEventsResponse {
        events: events.iter().map(EventResponse::from).collect(),
    }))
}

pub(crate) async fn get_event<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<EventResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = EventId::new(id).map_err(ApiError::validation)?;
    let Some(event) = state
        .store
        .find_memory_event(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(EventResponse::from(&event)))
}

pub(crate) async fn create_relationship<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateRelationshipRequest>,
) -> Result<(StatusCode, Json<RelationshipResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let relationship = Relationship::new(
        RelationshipId::new(request.id.unwrap_or_else(|| generated_id("relationship")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        request.relationship_type,
        EntityId::new(request.from_entity_id).map_err(ApiError::validation)?,
        EntityId::new(request.to_entity_id).map_err(ApiError::validation)?,
        request
            .evidence_ids
            .into_iter()
            .map(EvidenceId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::validation)?,
    )
    .map_err(memory_validation)?;
    state
        .store
        .upsert_memory_relationship(&relationship)
        .await
        .map_err(ApiError::store)?;
    Ok((
        StatusCode::CREATED,
        Json(RelationshipResponse::from(&relationship)),
    ))
}

pub(crate) async fn list_relationships<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListRelationshipsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let relationships = state
        .store
        .list_memory_relationships(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListRelationshipsResponse {
        relationships: relationships
            .iter()
            .map(RelationshipResponse::from)
            .collect(),
    }))
}

pub(crate) async fn get_relationship<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<RelationshipResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = RelationshipId::new(id).map_err(ApiError::validation)?;
    let Some(relationship) = state
        .store
        .find_memory_relationship(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(RelationshipResponse::from(&relationship)))
}

pub(crate) async fn create_contract<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateMemoryContractRequest>,
) -> Result<(StatusCode, Json<MemoryContractResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let contract = MemoryContract::parse_scoped(
        MemoryContractId::new(request.id.unwrap_or_else(|| generated_id("contract")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        request.name,
        request.source,
    )
    .map_err(contract_validation)?;
    contract.compile().map_err(contract_validation)?;
    state
        .store
        .upsert_memory_contract(&contract)
        .await
        .map_err(ApiError::store)?;
    Ok((
        StatusCode::CREATED,
        Json(MemoryContractResponse::new(&contract)?),
    ))
}

pub(crate) async fn list_contracts<S, O>(
    State(state): State<AppState<S, O>>,
) -> Result<Json<ListMemoryContractsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let contracts = state
        .store
        .list_memory_contracts(100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListMemoryContractsResponse {
        contracts: contracts
            .iter()
            .map(MemoryContractResponse::new)
            .collect::<Result<Vec<_>, _>>()?,
    }))
}

pub(crate) async fn get_contract<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<MemoryContractResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = MemoryContractId::new(id).map_err(ApiError::validation)?;
    let Some(contract) = state
        .store
        .find_memory_contract(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(MemoryContractResponse::new(&contract)?))
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
    state
        .store
        .upsert_memory_subgraph(&subgraph)
        .await
        .map_err(ApiError::store)?;
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
    Path(id): Path<String>,
) -> Result<Json<MemorySubgraphResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = MemorySubgraphId::new(id).map_err(ApiError::validation)?;
    let Some(subgraph) = state
        .store
        .find_memory_subgraph(&id)
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
    let _context = write_context(&headers, &principal)?;
    let id = MemorySubgraphId::new(id).map_err(ApiError::validation)?;
    let summary_claim_id = ClaimId::new(request.summary_claim_id).map_err(ApiError::validation)?;
    let traces = state
        .store
        .list_memory_summary_traces(&id, &summary_claim_id)
        .await
        .map_err(ApiError::store)?;
    let Some(subgraph) = state
        .store
        .find_memory_subgraph(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
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
        .map_err(ApiError::store)?;
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
        .map_err(ApiError::store)?;
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

pub(crate) async fn create_entity_resolution<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateEntityResolutionRequest>,
) -> Result<(StatusCode, Json<EntityResolutionResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let resolution = EntityResolution::new(
        EntityResolutionId::new(request.id.unwrap_or_else(|| generated_id("resolution")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        MemorySubgraphId::new(request.subgraph_id).map_err(ApiError::validation)?,
        EntityId::new(request.entity_id).map_err(ApiError::validation)?,
        CanonicalEntityId::new(request.canonical_entity_id).map_err(ApiError::validation)?,
        Confidence::new(request.confidence).map_err(memory_validation)?,
        parse_resolution_status(&request.status)?,
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
        .upsert_memory_entity_resolution(&resolution)
        .await
        .map_err(ApiError::store)?;
    Ok((
        StatusCode::CREATED,
        Json(EntityResolutionResponse::from(&resolution)),
    ))
}

pub(crate) async fn list_entity_resolutions<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Query(query): Query<ListEntityResolutionsQuery>,
) -> Result<Json<ListEntityResolutionsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let status_filter = query
        .status
        .as_deref()
        .map(parse_resolution_status)
        .transpose()?;
    let resolutions = state
        .store
        .list_memory_entity_resolutions(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListEntityResolutionsResponse {
        entity_resolutions: resolutions
            .iter()
            .filter(|resolution| status_filter.is_none_or(|status| resolution.status() == status))
            .map(EntityResolutionResponse::from)
            .collect(),
    }))
}

pub(crate) async fn confirm_entity_resolution<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<EntityResolutionResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    review_entity_resolution(
        state,
        headers,
        principal,
        id,
        EntityResolutionStatus::Confirmed,
    )
    .await
}

pub(crate) async fn reject_entity_resolution<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<EntityResolutionResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    review_entity_resolution(
        state,
        headers,
        principal,
        id,
        EntityResolutionStatus::Rejected,
    )
    .await
}

async fn review_entity_resolution<S, O>(
    state: AppState<S, O>,
    headers: HeaderMap,
    principal: Principal,
    id: String,
    next_status: EntityResolutionStatus,
) -> Result<Json<EntityResolutionResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let id = EntityResolutionId::new(id).map_err(ApiError::validation)?;
    let Some(resolution) = state
        .store
        .find_memory_entity_resolution(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    if resolution.scope().tenant_id() != context.tenant_id
        || resolution.scope().project_id() != context.project_id
    {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    }
    if resolution.status() != EntityResolutionStatus::Candidate {
        return Err(ApiError::validation(format!(
            "entity resolution {} is not awaiting review",
            resolution.id().as_str()
        )));
    }
    let reviewed = resolution
        .with_status(next_status)
        .map_err(graph_validation)?;
    state
        .store
        .upsert_memory_entity_resolution(&reviewed)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(EntityResolutionResponse::from(&reviewed)))
}

pub(crate) async fn list_claim_conflicts<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Query(query): Query<ListClaimConflictsQuery>,
) -> Result<Json<ListClaimConflictsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let status_filter = query
        .status
        .as_deref()
        .map(parse_claim_conflict_status)
        .transpose()?;
    let conflicts = state
        .store
        .list_memory_claim_conflicts(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListClaimConflictsResponse {
        conflicts: conflicts
            .iter()
            .filter(|conflict| status_filter.is_none_or(|status| conflict.status() == status))
            .map(ClaimConflictResponse::from)
            .collect(),
    }))
}

pub(crate) async fn resolve_claim_conflict<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
    Json(request): Json<ResolveClaimConflictRequest>,
) -> Result<Json<ClaimConflictResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = ClaimConflictId::new(id).map_err(ApiError::validation)?;
    let preferred_claim_id =
        ClaimId::new(request.preferred_claim_id).map_err(ApiError::validation)?;
    review_claim_conflict(
        &state.store,
        &headers,
        &principal,
        id,
        ClaimConflictReview::Resolve(preferred_claim_id),
    )
    .await
}

pub(crate) async fn dismiss_claim_conflict<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<ClaimConflictResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = ClaimConflictId::new(id).map_err(ApiError::validation)?;
    review_claim_conflict(
        &state.store,
        &headers,
        &principal,
        id,
        ClaimConflictReview::Dismiss,
    )
    .await
}

enum ClaimConflictReview {
    Resolve(ClaimId),
    Dismiss,
}

async fn review_claim_conflict<S>(
    store: &S,
    headers: &HeaderMap,
    principal: &Principal,
    id: ClaimConflictId,
    review: ClaimConflictReview,
) -> Result<Json<ClaimConflictResponse>, ApiError>
where
    S: ApiStore,
{
    let context = write_context(headers, principal)?;
    let Some(conflict) = store
        .find_memory_claim_conflict(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    if conflict.scope().tenant_id() != context.tenant_id
        || conflict.scope().project_id() != context.project_id
    {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    }
    if conflict.status() != ClaimConflictStatus::Candidate {
        return Err(ApiError::validation(format!(
            "claim conflict {} is not awaiting review",
            conflict.id().as_str()
        )));
    }
    let reviewed = match review {
        ClaimConflictReview::Resolve(preferred_claim_id) => conflict
            .with_resolution(preferred_claim_id)
            .map_err(graph_validation)?,
        ClaimConflictReview::Dismiss => conflict.dismissed().map_err(graph_validation)?,
    };
    store
        .upsert_memory_claim_conflict(&reviewed)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ClaimConflictResponse::from(&reviewed)))
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
        .map_err(ApiError::store)?;
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
        .map_err(ApiError::store)?;
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
        .map_err(ApiError::store)?;
    Ok((StatusCode::CREATED, Json(SubgraphEdgeResponse::from(&edge))))
}

fn write_context(headers: &HeaderMap, principal: &Principal) -> Result<ProjectContext, ApiError> {
    let context = project_context(headers, principal)?;
    require_project_role(&context, "project_operator")?;
    Ok(context)
}

fn scope(context: &ProjectContext) -> Result<MemoryScope, ApiError> {
    MemoryScope::new(context.tenant_id.clone(), context.project_id.clone())
        .map_err(memory_validation)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "map_err supplies the owned MemoryError and this helper keeps conversions concise"
)]
fn memory_validation(error: capsulet_core::MemoryError) -> ApiError {
    ApiError::Validation(error.to_string())
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "map_err supplies the owned MemoryContractError and this helper keeps conversions concise"
)]
fn contract_validation(error: capsulet_core::MemoryContractError) -> ApiError {
    ApiError::Validation(error.to_string())
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "map_err supplies the owned MemoryGraphError and this helper keeps conversions concise"
)]
fn graph_validation(error: capsulet_core::MemoryGraphError) -> ApiError {
    ApiError::Validation(error.to_string())
}

fn parse_authority(value: &str) -> Result<Authority, ApiError> {
    match value {
        "low" => Ok(Authority::Low),
        "medium" => Ok(Authority::Medium),
        "high" => Ok(Authority::High),
        value => Err(ApiError::Validation(format!("unknown authority {value}"))),
    }
}

fn parse_claim_status(value: &str) -> Result<ClaimStatus, ApiError> {
    match value {
        "candidate" => Ok(ClaimStatus::Candidate),
        "active" => Ok(ClaimStatus::Active),
        "rejected" => Ok(ClaimStatus::Rejected),
        "superseded" => Ok(ClaimStatus::Superseded),
        "contradicted" => Ok(ClaimStatus::Contradicted),
        "expired" => Ok(ClaimStatus::Expired),
        value => Err(ApiError::Validation(format!(
            "unknown claim status {value}"
        ))),
    }
}

fn parse_owner_kind(value: &str) -> Result<MemorySubgraphOwnerKind, ApiError> {
    match value {
        "user" => Ok(MemorySubgraphOwnerKind::User),
        "team" => Ok(MemorySubgraphOwnerKind::Team),
        "service" => Ok(MemorySubgraphOwnerKind::Service),
        "organization" => Ok(MemorySubgraphOwnerKind::Organization),
        value => Err(ApiError::Validation(format!("unknown owner kind {value}"))),
    }
}

fn parse_member_kind(value: &str) -> Result<MemoryMemberKind, ApiError> {
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

fn parse_member_role(value: &str) -> Result<MemorySubgraphMemberRole, ApiError> {
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

fn parse_resolution_status(value: &str) -> Result<EntityResolutionStatus, ApiError> {
    match value {
        "candidate" => Ok(EntityResolutionStatus::Candidate),
        "confirmed" => Ok(EntityResolutionStatus::Confirmed),
        "rejected" => Ok(EntityResolutionStatus::Rejected),
        value => Err(ApiError::Validation(format!(
            "unknown entity resolution status {value}"
        ))),
    }
}

fn parse_claim_conflict_status(value: &str) -> Result<ClaimConflictStatus, ApiError> {
    match value {
        "candidate" => Ok(ClaimConflictStatus::Candidate),
        "resolved" => Ok(ClaimConflictStatus::Resolved),
        "dismissed" => Ok(ClaimConflictStatus::Dismissed),
        value => Err(ApiError::Validation(format!(
            "unknown claim conflict status {value}"
        ))),
    }
}

fn parse_attachment_type(value: &str) -> Result<EntityGraphAttachmentType, ApiError> {
    match value {
        "primary" => Ok(EntityGraphAttachmentType::Primary),
        "supporting" => Ok(EntityGraphAttachmentType::Supporting),
        "historical" => Ok(EntityGraphAttachmentType::Historical),
        value => Err(ApiError::Validation(format!(
            "unknown entity graph attachment type {value}"
        ))),
    }
}

impl From<&Source> for SourceResponse {
    fn from(source: &Source) -> Self {
        Self {
            id: source.id().as_str().to_string(),
            tenant_id: source.scope().tenant_id().to_string(),
            project_id: source.scope().project_id().to_string(),
            kind: source.kind().to_string(),
            uri: source.uri().map(str::to_string),
            title: source.title().to_string(),
            authority: source.authority().to_string(),
        }
    }
}

impl From<&Evidence> for EvidenceResponse {
    fn from(evidence: &Evidence) -> Self {
        Self {
            id: evidence.id().as_str().to_string(),
            tenant_id: evidence.scope().tenant_id().to_string(),
            project_id: evidence.scope().project_id().to_string(),
            source_id: evidence.source_id().as_str().to_string(),
            locator: evidence.locator().to_string(),
            excerpt: evidence.excerpt().to_string(),
            observed_at: evidence.observed_at().to_string(),
        }
    }
}

impl From<&Entity> for EntityResponse {
    fn from(entity: &Entity) -> Self {
        Self {
            id: entity.id().as_str().to_string(),
            tenant_id: entity.scope().tenant_id().to_string(),
            project_id: entity.scope().project_id().to_string(),
            entity_type: entity.entity_type().to_string(),
            name: entity.name().to_string(),
            aliases: entity.aliases().to_vec(),
        }
    }
}

impl From<&Claim> for ClaimResponse {
    fn from(claim: &Claim) -> Self {
        Self {
            id: claim.id().as_str().to_string(),
            tenant_id: claim.scope().tenant_id().to_string(),
            project_id: claim.scope().project_id().to_string(),
            subject_id: claim.subject_id().as_str().to_string(),
            predicate: claim.predicate().to_string(),
            object: claim.object().to_string(),
            evidence_ids: claim
                .evidence_ids()
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
            confidence: claim.confidence().value(),
            authority: claim.authority().to_string(),
            status: claim.status().to_string(),
            observed_at: claim.observed_at().to_string(),
            valid_from: claim.valid_from().map(str::to_string),
            valid_until: claim.valid_until().map(str::to_string),
        }
    }
}

impl From<&Event> for EventResponse {
    fn from(event: &Event) -> Self {
        Self {
            id: event.id().as_str().to_string(),
            tenant_id: event.scope().tenant_id().to_string(),
            project_id: event.scope().project_id().to_string(),
            event_type: event.event_type().to_string(),
            occurred_at: event.occurred_at().to_string(),
            entity_ids: event
                .entity_ids()
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
            evidence_ids: event
                .evidence_ids()
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
        }
    }
}

impl From<&Relationship> for RelationshipResponse {
    fn from(relationship: &Relationship) -> Self {
        Self {
            id: relationship.id().as_str().to_string(),
            tenant_id: relationship.scope().tenant_id().to_string(),
            project_id: relationship.scope().project_id().to_string(),
            relationship_type: relationship.relationship_type().to_string(),
            from_entity_id: relationship.from_entity_id().as_str().to_string(),
            to_entity_id: relationship.to_entity_id().as_str().to_string(),
            evidence_ids: relationship
                .evidence_ids()
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
        }
    }
}

impl MemoryContractResponse {
    fn new(contract: &MemoryContract) -> Result<Self, ApiError> {
        Ok(Self {
            id: contract.id().as_str().to_string(),
            tenant_id: contract.scope().tenant_id().to_string(),
            project_id: contract.scope().project_id().to_string(),
            name: contract.name().to_string(),
            source: contract.source().to_string(),
            compiled: CompiledMemoryPolicyResponse::from(
                &contract.compile().map_err(contract_validation)?,
            ),
        })
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

impl From<&EntityResolution> for EntityResolutionResponse {
    fn from(resolution: &EntityResolution) -> Self {
        Self {
            id: resolution.id().as_str().to_string(),
            tenant_id: resolution.scope().tenant_id().to_string(),
            project_id: resolution.scope().project_id().to_string(),
            subgraph_id: resolution.subgraph_id().as_str().to_string(),
            entity_id: resolution.entity_id().as_str().to_string(),
            canonical_entity_id: resolution.canonical_entity_id().as_str().to_string(),
            confidence: resolution.confidence().value(),
            status: resolution.status().to_string(),
            evidence_ids: resolution
                .evidence_ids()
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }
}

impl From<&ClaimConflict> for ClaimConflictResponse {
    fn from(conflict: &ClaimConflict) -> Self {
        Self {
            id: conflict.id().as_str().to_string(),
            tenant_id: conflict.scope().tenant_id().to_string(),
            project_id: conflict.scope().project_id().to_string(),
            subject_id: conflict.subject_id().as_str().to_string(),
            canonical_entity_id: conflict
                .canonical_entity_id()
                .map(|id| id.as_str().to_string()),
            predicate: conflict.predicate().to_string(),
            claim_ids: conflict
                .claim_ids()
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
            status: conflict.status().to_string(),
            reason: conflict.reason().to_string(),
            preferred_claim_id: conflict
                .preferred_claim_id()
                .map(|id| id.as_str().to_string()),
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

impl From<&CompiledMemoryPolicy> for CompiledMemoryPolicyResponse {
    fn from(policy: &CompiledMemoryPolicy) -> Self {
        Self {
            entity_types: policy
                .entity_types()
                .iter()
                .map(|entity| entity.name().to_string())
                .collect(),
            relations: policy
                .relation_types()
                .iter()
                .map(RelationPolicyResponse::from)
                .collect(),
            claim_policy: ClaimPolicyResponse {
                require_source: policy.claim_policy().require_source(),
                store_confidence: policy.claim_policy().store_confidence(),
                allow_contradictions: policy.claim_policy().allow_contradictions(),
                min_confidence: policy.claim_policy().min_confidence().value(),
            },
            source_priority: policy.trust_policy().source_priority().to_vec(),
            retrieval_policies: policy
                .retrieval_policies()
                .iter()
                .map(RetrievalPolicyResponse::from)
                .collect(),
            contradiction_rules: policy
                .contradiction_rules()
                .iter()
                .map(|rule| rule.name().to_string())
                .collect(),
        }
    }
}

impl From<&RelationTypeSpec> for RelationPolicyResponse {
    fn from(relation: &RelationTypeSpec) -> Self {
        Self {
            name: relation.name().to_string(),
            from: relation.from_entity().to_string(),
            to: relation.to_entity().to_string(),
        }
    }
}

impl From<&RetrievalPolicySpec> for RetrievalPolicyResponse {
    fn from(policy: &RetrievalPolicySpec) -> Self {
        Self {
            name: policy.name().to_string(),
            seed_from: policy.seed_from().to_vec(),
            max_hops: policy.max_hops(),
        }
    }
}
