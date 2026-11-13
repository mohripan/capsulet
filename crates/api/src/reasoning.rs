//! The propose-check-certify loop.
//!
//! One request runs the whole architecture: text is stored immutably and cut
//! into byte-exact citable evidence, retrieval pins the legal alphabet, an
//! untrusted proposer emits a derivation, and the kernel decides it. The answer
//! a caller receives is a certificate, never a bare string.

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use capsulet_core::{
    Authority, EntityResolutionStatus, Evidence, EvidenceId, EvidenceSpan, MemoryContractId,
    MemoryScope, Source, SourceContent, SourceId,
};
use capsulet_kernel::{
    Certificate, NameGroup, Phrasebook, Proposition, Query, Question, ReadingDecision,
    ReadingReview, ReadingState, ResidualKind, Slot, Snapshot, Verdict, check,
    confirmed_name_groups, reading_authority, reviewed,
};
use capsulet_postgres::{CertificateRecord, ReadingReviewRecord};
use capsulet_proposer::{EvidenceAlphabet, OllamaProposer, RawProposal, chunk_into_spans};
use capsulet_storage::ObjectStore;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    auth::Principal,
    error::ApiError,
    http::{ProjectContext, generated_id, project_context, require_project_role},
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
    pub certificate: Certificate,
    /// What the model actually emitted, retained so a rejection can be read
    /// against the proposal that caused it.
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
    #[schema(value_type = capsulet_kernel::Verdict)]
    pub verdict: String,
    pub model: String,
    pub alphabet_digest: String,
    pub certificate: Certificate,
}

/// One reading a certificate carries, and where it stands.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReadingResponse {
    /// The reading's digest, which is what a review names.
    pub reading: String,
    /// `source` for a reading of a cited source, `question` for a reading of
    /// the question the answer was asked.
    pub kind: String,
    /// The attributed content the reading starts from, or the question asked.
    pub from: String,
    pub to: Proposition,
    /// Why the proposer read it so, or what the kernel says it depends on.
    pub rationale: String,
    pub evidence_ids: Vec<String>,
    /// `open`, `holds` or `does_not_hold`.
    pub state: String,
    /// The authority a decision on this reading has to be made under: the one
    /// the answer's contract names, or `review:write` when it names none.
    pub review_authority: String,
    pub reviewer: Option<String>,
    pub authority: Option<String>,
    pub review_rationale: Option<String>,
}

/// A certificate's outcome once the reviews recorded against it are read.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReviewedCertificateResponse {
    pub certificate_id: String,
    pub question: String,
    /// What the kernel decided.
    #[schema(value_type = capsulet_kernel::Verdict)]
    pub kernel_verdict: String,
    /// What the kernel's decision and the reviews together support. Re-derived
    /// on every read, never stored.
    #[schema(value_type = capsulet_kernel::Verdict)]
    pub verdict: String,
    /// `mechanical` when the kernel accepted it alone, `human_review` when
    /// reviews of its readings did, and absent while it is not accepted.
    pub basis: Option<String>,
    pub readings: Vec<ReadingResponse>,
}

/// A reading no one with the authority has decided yet.
#[derive(Debug, Serialize, ToSchema)]
pub struct OpenReadingResponse {
    pub certificate_id: String,
    pub question: String,
    pub reading: ReadingResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OpenReadingsResponse {
    pub readings: Vec<OpenReadingResponse>,
}

/// A decision on one reading.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ReviewReadingRequest {
    /// `holds` or `does_not_hold`.
    pub decision: String,
    /// Why, in words whoever reads the outcome can check against the evidence.
    pub rationale: String,
}

/// The readings in this project's recent certificates that nobody has decided.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission or the store fails.
pub(crate) async fn list_open_readings<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<OpenReadingsResponse>, ApiError>
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
    let ids: Vec<String> = records.iter().map(|record| record.id.clone()).collect();
    let reviews = state
        .store
        .list_reading_reviews(&context.tenant_id, &context.project_id, &ids)
        .await
        .map_err(ApiError::store)?;
    let mut readings = Vec::new();
    for record in &records {
        if record.certificate.verdict == Verdict::Rejected {
            continue;
        }
        let response = reviewed_response(record, &reviews_of(&reviews, &record.id));
        readings.extend(
            response
                .readings
                .into_iter()
                .filter(|reading| reading.state == "open")
                .map(|reading| OpenReadingResponse {
                    certificate_id: record.id.clone(),
                    question: record.question.clone(),
                    reading,
                }),
        );
    }
    Ok(Json(OpenReadingsResponse { readings }))
}

/// A certificate's outcome with the reviews recorded against it.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission, the certificate is not
/// in the caller's project, or the store fails.
pub(crate) async fn get_reviewed_certificate<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<ReviewedCertificateResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Viewer)?;
    let record = certificate_in_project(&state, &context, &id).await?;
    let reviews = reviews_for(&state, &context, &record.id).await?;
    Ok(Json(reviewed_response(&record, &reviews)))
}

/// Records a decision on one reading, under the reading authority.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks the authority, the decision or its
/// rationale is missing, the certificate or the reading is not in the caller's
/// project, the certificate is rejected, or the reading was already decided.
pub(crate) async fn review_reading<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path((id, reading)): Path<(String, String)>,
    Json(request): Json<ReviewReadingRequest>,
) -> Result<(StatusCode, Json<ReviewedCertificateResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Admin)?;
    let Some(decision) = ReadingDecision::parse(request.decision.trim()) else {
        return Err(ApiError::validation(
            "decision must be holds or does_not_hold".to_string(),
        ));
    };
    if request.rationale.trim().is_empty() {
        return Err(ApiError::validation(
            "a review has to say why: rationale cannot be empty".to_string(),
        ));
    }
    let record = certificate_in_project(&state, &context, &id).await?;
    // The contract the answer was read under says whose decision counts. A
    // decision under any other authority would decide nothing, so it is refused
    // rather than recorded.
    let authority = reading_authority(&record.certificate).to_string();
    if !principal.has_scope(&authority) {
        return Err(ApiError::ReadingAuthorityMissing(authority));
    }
    let known = reviewed(&record.certificate, &[])
        .readings
        .iter()
        .any(|(digest, _)| digest == &reading);
    if !known {
        return Err(ApiError::ReasoningCertificateNotFound(format!(
            "{id} reading {reading}"
        )));
    }
    if record.certificate.verdict == Verdict::Rejected {
        return Err(ApiError::validation(
            "the certificate is rejected; a premise failed, and no review of a reading can change that"
                .to_string(),
        ));
    }
    let inserted = state
        .store
        .insert_reading_review(&ReadingReviewRecord {
            tenant_id: context.tenant_id.clone(),
            project_id: context.project_id.clone(),
            certificate_id: record.id.clone(),
            review: ReadingReview {
                reading: reading.clone(),
                decision,
                reviewer: principal.name.to_string(),
                authority,
                rationale: request.rationale.trim().to_string(),
            },
        })
        .await
        .map_err(ApiError::store)?;
    if !inserted {
        return Err(ApiError::ReadingAlreadyReviewed(format!(
            "reading {reading} of certificate {id} has already been decided, and a decision is not \
             rewritten"
        )));
    }
    let reviews = reviews_for(&state, &context, &record.id).await?;
    Ok((
        StatusCode::CREATED,
        Json(reviewed_response(&record, &reviews)),
    ))
}

/// A reasoning certificate, when it is in the caller's project. One elsewhere
/// reports as missing, so the id space stays opaque.
async fn certificate_in_project<S, O>(
    state: &AppState<S, O>,
    context: &ProjectContext,
    id: &str,
) -> Result<CertificateRecord, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    state
        .store
        .find_certificate(&context.tenant_id, &context.project_id, id)
        .await
        .map_err(ApiError::store)?
        .ok_or_else(|| ApiError::ReasoningCertificateNotFound(id.to_string()))
}

async fn reviews_for<S, O>(
    state: &AppState<S, O>,
    context: &ProjectContext,
    id: &str,
) -> Result<Vec<ReadingReview>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let records = state
        .store
        .list_reading_reviews(&context.tenant_id, &context.project_id, &[id.to_string()])
        .await
        .map_err(ApiError::store)?;
    Ok(reviews_of(&records, id))
}

fn reviews_of(records: &[ReadingReviewRecord], id: &str) -> Vec<ReadingReview> {
    records
        .iter()
        .filter(|record| record.certificate_id == id)
        .map(|record| record.review.clone())
        .collect()
}

fn reviewed_response(
    record: &CertificateRecord,
    reviews: &[ReadingReview],
) -> ReviewedCertificateResponse {
    let certificate = &record.certificate;
    let outcome = reviewed(certificate, reviews);
    let review_authority = reading_authority(certificate);
    let basis = match (outcome.verdict, outcome.by_review) {
        (Verdict::Accepted, true) => Some("human_review".to_string()),
        (Verdict::Accepted, false) => Some("mechanical".to_string()),
        _ => None,
    };
    let readings = certificate
        .residuals
        .iter()
        .zip(outcome.readings)
        .map(|(residual, (digest, state))| {
            let (state, review) = match state {
                ReadingState::Open => ("open".to_string(), None),
                ReadingState::Decided(review) => {
                    (review.decision.as_str().to_string(), Some(review))
                }
            };
            ReadingResponse {
                reading: digest,
                kind: match residual.kind {
                    ResidualKind::Source => "source",
                    ResidualKind::Question => "question",
                }
                .to_string(),
                from: residual.from.clone(),
                to: residual.to.clone(),
                rationale: residual.rationale.clone(),
                evidence_ids: residual.evidence_ids.clone(),
                state,
                review_authority: review_authority.to_string(),
                reviewer: review.as_ref().map(|review| review.reviewer.clone()),
                authority: review.as_ref().map(|review| review.authority.clone()),
                review_rationale: review.map(|review| review.rationale),
            }
        })
        .collect();
    ReviewedCertificateResponse {
        certificate_id: record.id.clone(),
        question: record.question.clone(),
        kernel_verdict: certificate.verdict.as_str().to_string(),
        verdict: outcome.verdict.as_str().to_string(),
        basis,
        readings,
    }
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
    let (phrasebook, reading_authority) = match request.contract_id.as_deref() {
        Some(id) => contract_policy(&state, &context, id).await?,
        None => (Phrasebook::new(), None),
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
    // The text goes with the alphabet so the proposer can read a cited clause
    // with what its document says qualifies it. The digest names only what may
    // be cited, so it is the same with or without it.
    let alphabet = EvidenceAlphabet::from_evidence(&evidence).with_source(&stored_text);

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
        .with_reading_authority(reading_authority.as_deref())
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
///
/// With it, the authority the contract's readings are reviewed under, when it
/// names one.
pub(crate) async fn contract_policy<S, O>(
    state: &AppState<S, O>,
    context: &ProjectContext,
    id: &str,
) -> Result<(Phrasebook, Option<String>), ApiError>
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
    Ok((
        Phrasebook::from_contract(contract.ast()),
        contract
            .ast()
            .review_policy()
            .reading_authority()
            .map(str::to_string),
    ))
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
