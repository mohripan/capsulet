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
    Authority, EntityResolutionStatus, Evidence, EvidenceId, EvidenceSpan, MemoryContractId,
    MemoryScope, Source, SourceContent, SourceId,
};
use capsulet_kernel::{
    Certificate, NameGroup, Phrasebook, Query, Question, Slot, Snapshot, check,
    confirmed_name_groups,
};
use capsulet_postgres::CertificateRecord;
use capsulet_proposer::{EvidenceAlphabet, OllamaProposer, RawProposal, chunk_into_spans};
use capsulet_storage::ObjectStore;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    auth::Principal,
    error::ApiError,
    http::internal::{ProjectContext, generated_id, project_context, require_project_role},
    state::AppState,
    store::ApiStore,
};

/// The weakest source authority a trust step accepts on this endpoint.
///
/// `low`, which is what every answer was effectively held to before the floor
/// was policy: the shipped proposer always asks for `low`, and the floor used to
/// be whatever the proposal asked for. Raising it is a decision about this
/// endpoint, made here, rather than a field the model fills in.
const AUTHORITY_FLOOR: Authority = Authority::Low;

#[derive(Debug, Deserialize, ToSchema)]
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
    /// A memory contract to reason under. Its relations' declared phrasings are
    /// the only way a citation can state something, and its relations are the
    /// only predicates the model may answer with. Without one nothing is
    /// declared, and every answer is a reading whose verdict is `conditional`
    /// at best.
    #[serde(default)]
    pub contract_id: Option<String>,
    /// What the question asks, as a typed query over the contract's relations.
    /// The answer is then held to it mechanically. Without one the question is
    /// pinned in words alone, the model has to say how it read it, and that
    /// reading is a residual: such an answer is `conditional` at best.
    #[serde(default)]
    pub query: Option<AskQuery>,
}

/// A question as a typed query: the relation asked about and the terms it names.
///
/// Leave out the end the question asks for. "Which company acquired Contoso?" is
/// `{"relation": "acquired", "object": "Contoso"}`; naming both ends asks whether
/// the relation holds between them.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AskQuery {
    pub relation: String,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub object: Option<String>,
}

impl AskQuery {
    /// The kernel's query, or why this is not one.
    fn to_query(&self, phrasebook: &Phrasebook) -> Result<Query, ApiError> {
        let slot = |term: &Option<String>| {
            term.as_ref()
                .map_or(Slot::Wanted, |term| Slot::bound(term.trim()))
        };
        let query = Query::new(
            self.relation.trim(),
            slot(&self.subject),
            slot(&self.object),
        );
        if let Some(problem) = query.problem() {
            return Err(ApiError::validation(format!(
                "query {} is not one an answer can be held to: {problem}",
                query.canonical()
            )));
        }
        // With a contract, its relations are the only predicates an answer can
        // state, so a query about any other relation could never be answered.
        let declared: Vec<&str> = phrasebook.relations().map(|(name, _)| name).collect();
        if !declared.is_empty() && !declared.contains(&query.relation.as_str()) {
            return Err(ApiError::validation(format!(
                "query relation `{}` is not one the contract declares",
                query.relation
            )));
        }
        Ok(query)
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = CertificateResponse)]
pub struct AskResponse {
    pub certificate_id: String,
    pub question: String,
    pub source_id: String,
    pub model: String,
    pub alphabet_digest: String,
    pub alphabet_size: usize,
    #[schema(value_type = crate::wire_schemas::KernelCertificate)]
    pub certificate: Certificate,
    /// What the model actually emitted, retained so a rejection can be read
    /// against the proposal that caused it.
    #[schema(value_type = crate::wire_schemas::RawProposal)]
    pub proposal: RawProposal,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = ListCertificatesResponse)]
pub struct CertificateListResponse {
    pub certificates: Vec<CertificateSummary>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CertificateSummary {
    pub id: String,
    pub question: String,
    #[schema(value_type = crate::wire_schemas::KernelVerdict)]
    pub verdict: String,
    pub model: String,
    pub alphabet_digest: String,
    #[schema(value_type = crate::wire_schemas::KernelCertificate)]
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
    require_project_role(&context, crate::ProjectRole::Operator)?;

    if request.text.trim().is_empty() {
        return Err(ApiError::validation("text cannot be empty".to_string()));
    }
    if request.question.trim().is_empty() {
        return Err(ApiError::validation("question cannot be empty".to_string()));
    }

    let scope = MemoryScope::new(&context.tenant_id, &context.project_id)
        .map_err(|error| ApiError::validation(error.to_string()))?;
    let authority = parse_authority(request.authority.as_deref())?;
    // Resolved before anything is written, so a contract the caller cannot see
    // fails the request rather than leaving a stored source behind it.
    let phrasebook = match request.contract_id.as_deref() {
        Some(id) => contract_phrasebook(&state, &context, id).await?,
        None => Phrasebook::new(),
    };
    let phrasebook = phrasebook.with_name_groups(project_name_groups(&state, &context).await?);
    let query = request
        .query
        .as_ref()
        .map(|query| query.to_query(&phrasebook))
        .transpose()?;

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
        .propose(&request.question, query.as_ref(), &alphabet, &phrasebook)
        .await
        .map_err(|error| ApiError::Proposer(error.to_string()))?;

    // The kernel sees only what was pinned: the same source, the same bytes, the
    // same evidence the proposer was offered, the same phrasings — and the floor
    // a trust step is held to and the question an answer is held to, which are
    // this endpoint's to set and not the proposer's.
    let question = Question {
        text: request.question.clone(),
        query,
    };
    let mut snapshot = Snapshot::new()
        .with_authority_floor(AUTHORITY_FLOOR)
        .with_question(question)
        .with_phrasebook(phrasebook)
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
    require_project_role(&context, crate::ProjectRole::Viewer)?;
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

/// The most resolutions a decision reads names from. Past this it finds fewer other
/// names, which can only make a citation match less.
const MAX_NAME_RESOLUTIONS: i64 = 1_000;

/// The names this project's reviewers have confirmed as one entity.
pub(crate) async fn project_name_groups<S, O>(
    state: &AppState<S, O>,
    context: &ProjectContext,
) -> Result<Vec<NameGroup>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let resolutions = state
        .store
        .list_memory_entity_resolutions(
            &context.tenant_id,
            &context.project_id,
            MAX_NAME_RESOLUTIONS,
        )
        .await
        .map_err(ApiError::store)?;
    let mut entities = Vec::new();
    for resolution in &resolutions {
        if resolution.status() != EntityResolutionStatus::Confirmed {
            continue;
        }
        if let Some(entity) = state
            .store
            .find_memory_entity(resolution.entity_id())
            .await
            .map_err(ApiError::store)?
        {
            entities.push(entity);
        }
    }
    Ok(confirmed_name_groups(&resolutions, &entities))
}

/// The phrasings and cues a memory contract declares, for a contract the
/// caller's project can see.
///
/// A contract in another project reports as missing, the way every memory
/// route reports it: ids share one namespace, and existence is not something a
/// caller outside the project gets to learn.
pub(crate) async fn contract_phrasebook<S, O>(
    state: &AppState<S, O>,
    context: &ProjectContext,
    id: &str,
) -> Result<Phrasebook, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let id = MemoryContractId::new(id).map_err(ApiError::validation)?;
    let Some(contract) = state
        .store
        .find_memory_contract_in_project(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    contract
        .compile()
        .map_err(|error| ApiError::validation(error.to_string()))?;
    Ok(Phrasebook::from_contract(contract.ast()))
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
