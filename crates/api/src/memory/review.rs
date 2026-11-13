//! The review queues: entity resolutions to confirm or reject, and claim conflicts to resolve or dismiss.

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use capsulet_core::{
    CanonicalEntityId, ClaimConflict, ClaimConflictId, ClaimConflictStatus, ClaimId, Confidence,
    EntityId, EntityResolution, EntityResolutionId, EntityResolutionStatus, EvidenceId,
    MemorySubgraphId,
};
use capsulet_storage::ObjectStore;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::graph::{graph_validation, subgraph_write_error};
use super::{memory_validation, scope, write_context};
use crate::{
    auth::Principal,
    error::ApiError,
    http::{generated_id, project_context},
    state::AppState,
    store::ApiStore,
};

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CreateEntityResolutionRequest {
    id: Option<String>,
    subgraph_id: String,
    entity_id: String,
    canonical_entity_id: String,
    confidence: f64,
    status: String,
    evidence_ids: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListEntityResolutionsQuery {
    /// Only those with this status.
    status: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListClaimConflictsQuery {
    /// Only those with this status.
    status: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ResolveClaimConflictRequest {
    preferred_claim_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
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

#[derive(Debug, Serialize, ToSchema)]
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

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListEntityResolutionsResponse {
    entity_resolutions: Vec<EntityResolutionResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListClaimConflictsResponse {
    conflicts: Vec<ClaimConflictResponse>,
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
        .map_err(|error| subgraph_write_error::<S>(error, resolution.subgraph_id().as_str()))?;
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

pub(super) async fn review_entity_resolution<S, O>(
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
        .find_memory_entity_resolution_in_project(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
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
        .map_err(|error| subgraph_write_error::<S>(error, reviewed.subgraph_id().as_str()))?;
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

pub(super) enum ClaimConflictReview {
    Resolve(ClaimId),
    Dismiss,
}

pub(super) async fn review_claim_conflict<S>(
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
        .find_memory_claim_conflict_in_project(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
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

pub(super) fn parse_resolution_status(value: &str) -> Result<EntityResolutionStatus, ApiError> {
    match value {
        "candidate" => Ok(EntityResolutionStatus::Candidate),
        "confirmed" => Ok(EntityResolutionStatus::Confirmed),
        "rejected" => Ok(EntityResolutionStatus::Rejected),
        value => Err(ApiError::Validation(format!(
            "unknown entity resolution status {value}"
        ))),
    }
}

pub(super) fn parse_claim_conflict_status(value: &str) -> Result<ClaimConflictStatus, ApiError> {
    match value {
        "candidate" => Ok(ClaimConflictStatus::Candidate),
        "resolved" => Ok(ClaimConflictStatus::Resolved),
        "dismissed" => Ok(ClaimConflictStatus::Dismissed),
        value => Err(ApiError::Validation(format!(
            "unknown claim conflict status {value}"
        ))),
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
