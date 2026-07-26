//! The propose-check-certify loop.
//!
//! One request runs the whole architecture: text is stored immutably and cut
//! into byte-exact citable evidence, retrieval pins the legal alphabet, an
//! untrusted proposer emits a derivation, and the kernel decides it. The answer
//! a caller receives is a certificate, never a bare string.

use axum::{
    Extension, Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use capsulet_core::{
    Authority, Evidence, EvidenceId, EvidenceSpan, MemoryScope, Source, SourceContent, SourceId,
};
use capsulet_kernel::{Certificate, Snapshot, check};
use capsulet_postgres::CertificateRecord;
use capsulet_proposer::{EvidenceAlphabet, OllamaProposer, RawProposal, chunk_into_spans};
use capsulet_storage::ObjectStore;
use serde::{Deserialize, Serialize};

use crate::{
    auth::Principal,
    error::ApiError,
    http::internal::{generated_id, project_context, require_project_role},
    state::AppState,
    store::ApiStore,
};

#[derive(Debug, Deserialize)]
pub struct AskRequest {
    /// Text to reason over. Stored immutably and cut into citable spans.
    pub text: String,
    pub question: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub authority: Option<String>,
    /// Overrides the configured model for this request.
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AskResponse {
    pub certificate_id: String,
    pub question: String,
    pub source_id: String,
    pub model: String,
    pub alphabet_digest: String,
    pub alphabet_size: usize,
    pub certificate: Certificate,
    /// What the model actually emitted, retained so a rejection can be read
    /// against the proposal that caused it.
    pub proposal: RawProposal,
}

#[derive(Debug, Serialize)]
pub struct CertificateListResponse {
    pub certificates: Vec<CertificateSummary>,
}

#[derive(Debug, Serialize)]
pub struct CertificateSummary {
    pub id: String,
    pub question: String,
    pub verdict: String,
    pub model: String,
    pub alphabet_digest: String,
    pub certificate: Certificate,
}

/// Runs the full loop and returns the kernel's decision.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission, the text yields no
/// citable spans, or the proposer is unreachable.
pub(crate) async fn ask<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<AskRequest>,
) -> Result<(StatusCode, Json<AskResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_operator")?;

    if request.text.trim().is_empty() {
        return Err(ApiError::validation("text cannot be empty".to_string()));
    }
    if request.question.trim().is_empty() {
        return Err(ApiError::validation("question cannot be empty".to_string()));
    }

    let scope = MemoryScope::new(&context.tenant_id, &context.project_id)
        .map_err(|error| ApiError::validation(error.to_string()))?;
    let authority = parse_authority(request.authority.as_deref())?;

    // Store the source and its bytes before anything cites them, so every span
    // below resolves against content that is already durable.
    let source_id = SourceId::new(generated_id("source")).map_err(ApiError::validation)?;
    let source = Source::new(
        source_id.clone(),
        scope.clone(),
        "inline",
        None,
        request.title.as_deref().unwrap_or("Inline text"),
        authority,
    )
    .map_err(|error| ApiError::validation(error.to_string()))?;
    state
        .store
        .upsert_memory_source(&source)
        .await
        .map_err(ApiError::store)?;

    let stored_text = SourceContent::new(source_id.clone(), &request.text)
        .map_err(|error| ApiError::validation(error.to_string()))?;
    state
        .store
        .insert_memory_source_content(&stored_text)
        .await
        .map_err(ApiError::store)?;

    let evidence = persist_span_evidence(&state, &scope, &source_id, &stored_text).await?;
    if evidence.is_empty() {
        return Err(ApiError::validation(
            "text produced no citable spans".to_string(),
        ));
    }
    let alphabet = EvidenceAlphabet::from_evidence(&evidence);

    let proposer = build_proposer(request.model.as_deref())?;
    let (proposal, raw) = proposer
        .propose(&request.question, &alphabet)
        .await
        .map_err(|error| ApiError::Proposer(error.to_string()))?;

    // The kernel sees only what was pinned: the same source, the same bytes, the
    // same evidence the proposer was offered.
    let mut snapshot = Snapshot::new()
        .with_source(source)
        .with_source_content(stored_text);
    for item in evidence {
        snapshot = snapshot.with_evidence(item);
    }
    let certificate = check(&proposal, &snapshot);

    let record = CertificateRecord {
        id: generated_id("certificate"),
        tenant_id: context.tenant_id.clone(),
        project_id: context.project_id.clone(),
        question: request.question.clone(),
        certificate: certificate.clone(),
        alphabet_digest: alphabet.digest().to_string(),
        model: proposer.model().to_string(),
    };
    state
        .store
        .insert_certificate(&record)
        .await
        .map_err(ApiError::store)?;

    Ok((
        StatusCode::OK,
        Json(AskResponse {
            certificate_id: record.id,
            question: request.question,
            source_id: source_id.as_str().to_string(),
            model: proposer.model().to_string(),
            alphabet_digest: alphabet.digest().to_string(),
            alphabet_size: alphabet.entries().len(),
            certificate,
            proposal: raw,
        }),
    ))
}

/// Lists recorded certificates for the caller's project.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission or the query fails.
pub(crate) async fn list_certificates<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<CertificateListResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_viewer")?;
    let records = state
        .store
        .list_certificates(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(CertificateListResponse {
        certificates: records
            .into_iter()
            .map(|record| CertificateSummary {
                id: record.id,
                question: record.question,
                verdict: record.certificate.verdict.as_str().to_string(),
                model: record.model,
                alphabet_digest: record.alphabet_digest,
                certificate: record.certificate,
            })
            .collect(),
    }))
}

/// Cuts the stored text into sentence-sized evidence carrying exact spans.
async fn persist_span_evidence<S, O>(
    state: &AppState<S, O>,
    scope: &MemoryScope,
    source_id: &SourceId,
    content: &SourceContent,
) -> Result<Vec<Evidence>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let mut stored = Vec::new();
    for (index, (start, end)) in chunk_into_spans(content).into_iter().enumerate() {
        let id = EvidenceId::new(format!("{}_ev_{index}", source_id.as_str()))
            .map_err(ApiError::validation)?;
        let span = EvidenceSpan::new(start, end, content.content_hash())
            .map_err(|error| ApiError::validation(error.to_string()))?;
        let evidence = Evidence::new(
            id,
            scope.clone(),
            source_id.clone(),
            format!("bytes {start}..{end}"),
            &content.text()[start..end],
            "inline",
        )
        .map_err(|error| ApiError::validation(error.to_string()))?
        .with_span(span);
        state
            .store
            .upsert_memory_evidence(&evidence)
            .await
            .map_err(ApiError::store)?;
        stored.push(evidence);
    }
    Ok(stored)
}

fn build_proposer(model: Option<&str>) -> Result<OllamaProposer, ApiError> {
    let proposer =
        OllamaProposer::from_env().map_err(|error| ApiError::Proposer(error.to_string()))?;
    match model {
        Some(model) if !model.trim().is_empty() => {
            let base_url = std::env::var("CAPSULET_OLLAMA_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
            OllamaProposer::new(base_url, model)
                .map_err(|error| ApiError::Proposer(error.to_string()))
        }
        _ => Ok(proposer),
    }
}

fn parse_authority(value: Option<&str>) -> Result<Authority, ApiError> {
    match value.unwrap_or("medium").trim().to_lowercase().as_str() {
        "low" => Ok(Authority::Low),
        "medium" => Ok(Authority::Medium),
        "high" => Ok(Authority::High),
        other => Err(ApiError::validation(format!("unknown authority: {other}"))),
    }
}
