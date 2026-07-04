use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use capsulet_core::{
    Authority, Claim, ClaimId, ClaimStatus, IngestionConnector, IngestionConnectorConfig,
    IngestionConnectorId, IngestionConnectorKind, IngestionRun, IngestionRunId, IngestionRunOutput,
    IngestionRunOutputRecord, IngestionRunStatus, MemoryScope, run_local_text_ingestion,
};
use capsulet_storage::ObjectStore;
use serde::{Deserialize, Serialize};

use crate::{
    auth::Principal,
    error::ApiError,
    http::{ProjectContext, generated_id, project_context, require_project_role},
    state::AppState,
    store::ApiStore,
};

#[derive(Debug, Deserialize)]
pub(crate) struct CreateIngestionConnectorRequest {
    id: Option<String>,
    name: String,
    kind: String,
    config: LocalTextConnectorConfigRequest,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LocalTextConnectorConfigRequest {
    title: String,
    content: String,
    content_type: String,
    uri: Option<String>,
    authority: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct IngestionConnectorResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    name: String,
    kind: String,
    enabled: bool,
    config: LocalTextConnectorConfigResponse,
}

#[derive(Debug, Serialize)]
pub(crate) struct LocalTextConnectorConfigResponse {
    title: String,
    content_type: String,
    uri: Option<String>,
    authority: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListIngestionConnectorsResponse {
    connectors: Vec<IngestionConnectorResponse>,
}

#[derive(Debug, Serialize)]
pub(crate) struct IngestionRunResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    connector_id: String,
    status: String,
    error: Option<String>,
    source_count: u32,
    evidence_count: u32,
    entity_count: u32,
    claim_count: u32,
    event_count: u32,
    relationship_count: u32,
}

#[derive(Debug, Serialize)]
pub(crate) struct IngestionRunOutputsResponse {
    sources: Vec<String>,
    evidence: Vec<String>,
    entities: Vec<String>,
    claims: Vec<String>,
    events: Vec<String>,
    relationships: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct IngestionRunWithOutputsResponse {
    run: IngestionRunResponse,
    outputs: IngestionRunOutputsResponse,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListIngestionRunsResponse {
    runs: Vec<IngestionRunResponse>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReviewClaimsQuery {
    status: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewClaimResponse {
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
pub(crate) struct ListReviewClaimsResponse {
    claims: Vec<ReviewClaimResponse>,
}

pub(crate) async fn create_connector<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateIngestionConnectorRequest>,
) -> Result<(StatusCode, Json<IngestionConnectorResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let connector = connector_from_request(request, &context)?;
    state
        .store
        .upsert_ingestion_connector(&connector)
        .await
        .map_err(ApiError::store)?;
    Ok((
        StatusCode::CREATED,
        Json(IngestionConnectorResponse::from(&connector)),
    ))
}

pub(crate) async fn list_connectors<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListIngestionConnectorsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let connectors = state
        .store
        .list_ingestion_connectors(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListIngestionConnectorsResponse {
        connectors: connectors
            .iter()
            .map(IngestionConnectorResponse::from)
            .collect(),
    }))
}

pub(crate) async fn get_connector<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<IngestionConnectorResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = IngestionConnectorId::new(id).map_err(ApiError::validation)?;
    let Some(connector) = state
        .store
        .find_ingestion_connector(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(IngestionConnectorResponse::from(&connector)))
}

pub(crate) async fn run_connector<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<IngestionRunWithOutputsResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let id = IngestionConnectorId::new(id).map_err(ApiError::validation)?;
    let Some(connector) = state
        .store
        .find_ingestion_connector(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    if connector.scope().tenant_id() != context.tenant_id
        || connector.scope().project_id() != context.project_id
    {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    }
    let run = IngestionRun::queued(
        IngestionRunId::new(generated_id("ingestion_run")).map_err(ApiError::validation)?,
        scope(&context)?,
        connector.id().clone(),
    );
    let output = run_local_text_ingestion(&connector, run).map_err(ingestion_validation)?;
    persist_output(&state.store, &output).await?;
    let response = response_from_output(&output)?;
    Ok((StatusCode::CREATED, Json(response)))
}

pub(crate) async fn list_runs<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListIngestionRunsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let runs = state
        .store
        .list_ingestion_runs(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListIngestionRunsResponse {
        runs: runs.iter().map(IngestionRunResponse::from).collect(),
    }))
}

pub(crate) async fn get_run<S, O>(
    State(state): State<AppState<S, O>>,
    Path(id): Path<String>,
) -> Result<Json<IngestionRunWithOutputsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = IngestionRunId::new(id).map_err(ApiError::validation)?;
    let Some(run) = state
        .store
        .find_ingestion_run(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    let outputs = state
        .store
        .list_ingestion_run_outputs(&id)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(IngestionRunWithOutputsResponse {
        run: IngestionRunResponse::from(&run),
        outputs: outputs_response(&outputs),
    }))
}

pub(crate) async fn list_review_claims<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Query(query): Query<ReviewClaimsQuery>,
) -> Result<Json<ListReviewClaimsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let status_filter = query
        .status
        .as_deref()
        .map(parse_claim_status)
        .transpose()?;
    let claims = state
        .store
        .list_memory_claims(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListReviewClaimsResponse {
        claims: claims
            .iter()
            .filter(|claim| {
                status_filter
                    .map(|status| claim.status() == status)
                    .unwrap_or_else(|| reviewable_status(claim.status()))
            })
            .map(ReviewClaimResponse::from)
            .collect(),
    }))
}

pub(crate) async fn approve_review_claim<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<ReviewClaimResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    review_claim(state, headers, principal, id, ClaimStatus::Active).await
}

pub(crate) async fn reject_review_claim<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<ReviewClaimResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    review_claim(state, headers, principal, id, ClaimStatus::Rejected).await
}

async fn review_claim<S, O>(
    state: AppState<S, O>,
    headers: HeaderMap,
    principal: Principal,
    id: String,
    next_status: ClaimStatus,
) -> Result<Json<ReviewClaimResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let id = ClaimId::new(id).map_err(ApiError::validation)?;
    let Some(claim) = state
        .store
        .find_memory_claim(&id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    if claim.scope().tenant_id() != context.tenant_id
        || claim.scope().project_id() != context.project_id
    {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    }
    if claim.status() != ClaimStatus::Candidate {
        return Err(ApiError::validation(format!(
            "claim {} is not awaiting review",
            claim.id().as_str()
        )));
    }
    let reviewed = claim.with_status(next_status);
    state
        .store
        .upsert_memory_claim(&reviewed)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ReviewClaimResponse::from(&reviewed)))
}

async fn persist_output<S>(store: &S, output: &IngestionRunOutput) -> Result<(), ApiError>
where
    S: ApiStore,
{
    for source in output.sources() {
        store
            .upsert_memory_source(source)
            .await
            .map_err(ApiError::store)?;
    }
    for evidence in output.evidence() {
        store
            .upsert_memory_evidence(evidence)
            .await
            .map_err(ApiError::store)?;
    }
    for entity in output.entities() {
        store
            .upsert_memory_entity(entity)
            .await
            .map_err(ApiError::store)?;
    }
    for claim in output.claims() {
        store
            .upsert_memory_claim(claim)
            .await
            .map_err(ApiError::store)?;
    }
    store
        .upsert_ingestion_run(output.run())
        .await
        .map_err(ApiError::store)?;
    for record in output_records(output)? {
        store
            .upsert_ingestion_run_output(&record)
            .await
            .map_err(ApiError::store)?;
    }
    Ok(())
}

fn output_records(output: &IngestionRunOutput) -> Result<Vec<IngestionRunOutputRecord>, ApiError> {
    let mut records = Vec::new();
    for source in output.sources() {
        records.push(output_record(output.run(), "source", source.id().as_str())?);
    }
    for evidence in output.evidence() {
        records.push(output_record(
            output.run(),
            "evidence",
            evidence.id().as_str(),
        )?);
    }
    for entity in output.entities() {
        records.push(output_record(output.run(), "entity", entity.id().as_str())?);
    }
    for claim in output.claims() {
        records.push(output_record(output.run(), "claim", claim.id().as_str())?);
    }
    Ok(records)
}

fn output_record(
    run: &IngestionRun,
    kind: &str,
    memory_id: &str,
) -> Result<IngestionRunOutputRecord, ApiError> {
    IngestionRunOutputRecord::new(run.id().clone(), kind, memory_id).map_err(ingestion_validation)
}

fn response_from_output(
    output: &IngestionRunOutput,
) -> Result<IngestionRunWithOutputsResponse, ApiError> {
    let records = output_records(output)?;
    Ok(IngestionRunWithOutputsResponse {
        run: IngestionRunResponse::from(output.run()),
        outputs: outputs_response(&records),
    })
}

fn connector_from_request(
    request: CreateIngestionConnectorRequest,
    context: &ProjectContext,
) -> Result<IngestionConnector, ApiError> {
    if request.kind != "local_text" {
        return Err(ApiError::validation(format!(
            "unsupported ingestion connector kind: {}",
            request.kind
        )));
    }
    IngestionConnector::new(
        IngestionConnectorId::new(
            request
                .id
                .unwrap_or_else(|| generated_id("ingestion_connector")),
        )
        .map_err(ApiError::validation)?,
        scope(context)?,
        request.name,
        IngestionConnectorKind::LocalText,
        IngestionConnectorConfig::local_text(
            request.config.title,
            request.config.content,
            request.config.content_type,
            request.config.uri,
            parse_authority(&request.config.authority)?,
        ),
        request.enabled,
    )
    .map_err(ingestion_validation)
}

fn outputs_response(records: &[IngestionRunOutputRecord]) -> IngestionRunOutputsResponse {
    IngestionRunOutputsResponse {
        sources: output_ids(records, "source"),
        evidence: output_ids(records, "evidence"),
        entities: output_ids(records, "entity"),
        claims: output_ids(records, "claim"),
        events: output_ids(records, "event"),
        relationships: output_ids(records, "relationship"),
    }
}

fn output_ids(records: &[IngestionRunOutputRecord], kind: &str) -> Vec<String> {
    records
        .iter()
        .filter(|record| record.kind() == kind)
        .map(|record| record.memory_id().to_string())
        .collect()
}

fn write_context(headers: &HeaderMap, principal: &Principal) -> Result<ProjectContext, ApiError> {
    let context = project_context(headers, principal)?;
    require_project_role(&context, "editor")?;
    Ok(context)
}

fn scope(context: &ProjectContext) -> Result<MemoryScope, ApiError> {
    MemoryScope::new(context.tenant_id.clone(), context.project_id.clone())
        .map_err(|error| ApiError::validation(error.to_string()))
}

fn parse_authority(value: &str) -> Result<Authority, ApiError> {
    match value {
        "low" => Ok(Authority::Low),
        "medium" => Ok(Authority::Medium),
        "high" => Ok(Authority::High),
        value => Err(ApiError::validation(format!("unknown authority: {value}"))),
    }
}

fn parse_claim_status(value: &str) -> Result<ClaimStatus, ApiError> {
    match value {
        "candidate" => Ok(ClaimStatus::Candidate),
        "active" => Ok(ClaimStatus::Active),
        "rejected" => Ok(ClaimStatus::Rejected),
        value => Err(ApiError::validation(format!(
            "unknown claim status: {value}"
        ))),
    }
}

const fn reviewable_status(status: ClaimStatus) -> bool {
    matches!(
        status,
        ClaimStatus::Candidate | ClaimStatus::Active | ClaimStatus::Rejected
    )
}

fn ingestion_validation(error: capsulet_core::IngestionError) -> ApiError {
    ApiError::validation(error.to_string())
}

const fn default_enabled() -> bool {
    true
}

impl From<&IngestionConnector> for IngestionConnectorResponse {
    fn from(connector: &IngestionConnector) -> Self {
        Self {
            id: connector.id().as_str().to_string(),
            tenant_id: connector.scope().tenant_id().to_string(),
            project_id: connector.scope().project_id().to_string(),
            name: connector.name().to_string(),
            kind: connector.kind().to_string(),
            enabled: connector.enabled(),
            config: LocalTextConnectorConfigResponse {
                title: connector.config().title().to_string(),
                content_type: connector.config().content_type().to_string(),
                uri: connector.config().uri().map(str::to_string),
                authority: connector.config().authority().to_string(),
            },
        }
    }
}

impl From<&IngestionRun> for IngestionRunResponse {
    fn from(run: &IngestionRun) -> Self {
        Self {
            id: run.id().as_str().to_string(),
            tenant_id: run.scope().tenant_id().to_string(),
            project_id: run.scope().project_id().to_string(),
            connector_id: run.connector_id().as_str().to_string(),
            status: status_string(run.status()),
            error: run.error().map(str::to_string),
            source_count: run.source_count(),
            evidence_count: run.evidence_count(),
            entity_count: run.entity_count(),
            claim_count: run.claim_count(),
            event_count: run.event_count(),
            relationship_count: run.relationship_count(),
        }
    }
}

impl From<&Claim> for ReviewClaimResponse {
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

fn status_string(status: IngestionRunStatus) -> String {
    status.to_string()
}
