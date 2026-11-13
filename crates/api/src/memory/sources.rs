//! Sources and the evidence cited from them.

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use capsulet_core::{Evidence, EvidenceId, Source, SourceId};
use capsulet_storage::ObjectStore;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{memory_validation, parse_authority, require_scope, scope, write_context};
use crate::{
    auth::Principal,
    error::ApiError,
    http::{generated_id, project_context},
    state::AppState,
    store::ApiStore,
};

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = CreateMemorySourceRequest)]
pub(crate) struct CreateSourceRequest {
    id: Option<String>,
    kind: String,
    uri: Option<String>,
    title: String,
    authority: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = CreateMemoryEvidenceRequest)]
pub(crate) struct CreateEvidenceRequest {
    id: Option<String>,
    source_id: String,
    locator: String,
    excerpt: String,
    observed_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = MemorySourceResponse)]
pub(crate) struct SourceResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    kind: String,
    uri: Option<String>,
    title: String,
    authority: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = MemoryEvidenceResponse)]
pub(crate) struct EvidenceResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    source_id: String,
    locator: String,
    excerpt: String,
    observed_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = ListMemorySourcesResponse)]
pub(crate) struct ListSourcesResponse {
    sources: Vec<SourceResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = ListMemoryEvidenceResponse)]
pub(crate) struct ListEvidenceResponse {
    evidence: Vec<EvidenceResponse>,
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
    // The id may come from the request body, and the upsert conflicts on the id alone.
    if let Some(existing) = state
        .store
        .find_memory_source(source.id())
        .await
        .map_err(ApiError::store)?
    {
        require_scope(existing.scope(), &context, source.id().as_str())?;
    }
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
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<SourceResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = SourceId::new(id).map_err(ApiError::validation)?;
    let Some(source) = state
        .store
        .find_memory_source_in_project(&id, &context.tenant_id, &context.project_id)
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
    if let Some(existing) = state
        .store
        .find_memory_evidence(evidence.id())
        .await
        .map_err(ApiError::store)?
    {
        require_scope(existing.scope(), &context, evidence.id().as_str())?;
    }
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
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<EvidenceResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = EvidenceId::new(id).map_err(ApiError::validation)?;
    let Some(evidence) = state
        .store
        .find_memory_evidence_in_project(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(EvidenceResponse::from(&evidence)))
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
