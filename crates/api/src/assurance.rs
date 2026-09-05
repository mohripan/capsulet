//! Registering IR definitions and reading the certificates about them.
//!
//! Two things here are deliberate.
//!
//! Registration runs structural admission before it stores anything, and a
//! refusal comes back as the admission result rather than a bare `400`. The
//! caller has to fix something specific, and the response says which rule and
//! which subsystem owns it.
//!
//! Reads return the certificate's canonical bytes, not a re-serialization of a
//! parsed model. A reader who wants to check the seal has to see the same bytes
//! that were digested, and a helpful reformat would silently break that.

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use std::time::{SystemTime, UNIX_EPOCH};

use capsulet_ir::admission::admit;
use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::definition::Definition;
use capsulet_ir::digest::Digest;
use capsulet_postgres::IrRunRecord;
use capsulet_runtime::RunEvent;
use capsulet_storage::ObjectStore;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    auth::Principal,
    error::ApiError,
    http::internal::{generated_id, project_context, require_project_role},
    state::AppState,
    store::ApiStore,
};

/// A definition offered for registration.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterDefinitionRequest {
    /// The definition itself, in the IR's own shape.
    #[schema(value_type = Object)]
    pub definition: Definition,
}

/// What registration recorded.
#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterDefinitionResponse {
    pub definition_id: String,
    pub version: String,
    /// The digest of the canonical bytes: this version's identity.
    pub digest: String,
    pub schema_version: String,
    /// The structural rules that were applied before it was accepted.
    pub rules_applied: Vec<String>,
}

/// One stored definition version.
#[derive(Debug, Serialize, ToSchema)]
pub struct DefinitionVersionResponse {
    pub definition_id: String,
    pub name: String,
    pub version: String,
    pub digest: String,
    pub schema_version: String,
    /// The exact bytes that were digested, so a reader can check the digest
    /// rather than trust it.
    pub canonical_bytes: String,
}

/// A page of definition versions.
#[derive(Debug, Serialize, ToSchema)]
pub struct ListDefinitionVersionsResponse {
    pub versions: Vec<DefinitionVersionResponse>,
}

/// One stored certificate.
#[derive(Debug, Serialize, ToSchema)]
pub struct AssuranceCertificateResponse {
    pub id: String,
    pub definition_digest: String,
    /// The assurance verdict. Never inferred from execution status.
    pub verdict: String,
    /// The mode the run was decided under, because `unverified` under observe
    /// and `unverified` under enforce are different statements.
    pub mode: String,
    pub replay_digest: String,
    pub canonical_bytes: String,
}

/// A page of certificates.
#[derive(Debug, Serialize, ToSchema)]
pub struct ListAssuranceCertificatesResponse {
    pub certificates: Vec<AssuranceCertificateResponse>,
}

/// A certificate with every byte it cites.
#[derive(Debug, Serialize, ToSchema)]
pub struct CertificateBundleResponse {
    /// The bundle's canonical bytes, ready to hand to `capsulet-replay`.
    pub bundle: String,
}

/// Validates, admits, and registers a definition.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission or the store fails. A
/// definition that fails admission returns `422` with the rule that refused it.
pub(crate) async fn register_definition<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<RegisterDefinitionRequest>,
) -> Result<(StatusCode, Json<RegisterDefinitionResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_admin")?;

    // Admission first. Nothing unadmitted is stored, so a stored definition is
    // one somebody could actually run.
    let record = admit(&request.definition).map_err(|refusal| ApiError::AdmissionRefused {
        code: refusal.code.as_str().to_string(),
        owner: refusal.owner.as_str().to_string(),
        detail: refusal.detail,
    })?;

    let digest = state
        .store
        .insert_ir_definition_version(
            &context.tenant_id,
            &context.project_id,
            &request.definition,
            &record,
        )
        .await
        .map_err(ApiError::store)?;

    Ok((
        StatusCode::CREATED,
        Json(RegisterDefinitionResponse {
            definition_id: request.definition.id.as_str().to_string(),
            version: request.definition.version.clone(),
            digest,
            schema_version: request.definition.schema_version.to_string(),
            rules_applied: record
                .rules_applied()
                .iter()
                .map(|code| code.as_str().to_string())
                .collect(),
        }),
    ))
}

/// Lists registered definition versions for the caller's project.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission or the store fails.
pub(crate) async fn list_definitions<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListDefinitionVersionsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_viewer")?;

    let versions = state
        .store
        .list_ir_definition_versions(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;

    Ok(Json(ListDefinitionVersionsResponse {
        versions: versions
            .into_iter()
            .map(|version| DefinitionVersionResponse {
                definition_id: version.definition_id,
                name: version.name,
                version: version.version,
                digest: version.digest,
                schema_version: version.schema_version,
                canonical_bytes: version.canonical_bytes,
            })
            .collect(),
    }))
}

/// Reads one definition version by digest.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission, the version is not in
/// this project, or the store fails.
pub(crate) async fn get_definition_version<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(digest): Path<String>,
) -> Result<Json<DefinitionVersionResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_viewer")?;

    let version = state
        .store
        .get_ir_definition_version(&context.tenant_id, &context.project_id, &digest)
        .await
        .map_err(ApiError::store)?
        .ok_or_else(|| ApiError::IrDefinitionNotFound(digest.clone()))?;

    Ok(Json(DefinitionVersionResponse {
        definition_id: version.definition_id,
        name: version.name,
        version: version.version,
        digest: version.digest,
        schema_version: version.schema_version,
        canonical_bytes: version.canonical_bytes,
    }))
}

/// Lists certificates for the caller's project.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission or the store fails.
pub(crate) async fn list_certificates<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListAssuranceCertificatesResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_viewer")?;

    let certificates = state
        .store
        .list_assurance_certificates(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;

    Ok(Json(ListAssuranceCertificatesResponse {
        certificates: certificates.into_iter().map(to_response).collect(),
    }))
}

/// Reads one certificate.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission, the certificate is
/// not in this project, or the store fails.
pub(crate) async fn get_certificate<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<AssuranceCertificateResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_viewer")?;

    let stored = state
        .store
        .get_assurance_certificate(&context.tenant_id, &context.project_id, &id)
        .await
        .map_err(ApiError::store)?
        .ok_or_else(|| ApiError::AssuranceCertificateNotFound(id.clone()))?;

    Ok(Json(to_response(stored)))
}

/// Exports a certificate with the evidence it cites.
///
/// This is what makes a certificate checkable elsewhere: the response is a
/// bundle `capsulet-replay` can read on a machine with no access to this
/// installation.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission, the certificate is
/// not in this project, the stored bytes do not parse, or a piece of cited
/// evidence is no longer retrievable.
pub(crate) async fn get_certificate_bundle<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<CertificateBundleResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_viewer")?;

    let stored = state
        .store
        .get_assurance_certificate(&context.tenant_id, &context.project_id, &id)
        .await
        .map_err(ApiError::store)?
        .ok_or_else(|| ApiError::AssuranceCertificateNotFound(id.clone()))?;

    let certificate = stored.certificate().map_err(ApiError::store)?;

    let mut evidence = capsulet_kernel::EvidenceMap::new();
    for reference in &certificate.body().evidence {
        let digest = reference.content.to_string();
        let location = state
            .store
            .get_assurance_evidence(&context.tenant_id, &context.project_id, &digest)
            .await
            .map_err(ApiError::store)?
            .ok_or_else(|| ApiError::AssuranceEvidenceMissing(digest.clone()))?;

        let bytes = state
            .object_store
            .get(&location.object_key)
            .await
            .map_err(ApiError::object_store)?
            .ok_or_else(|| ApiError::AssuranceEvidenceMissing(digest.clone()))?;

        // Store the bytes under the digest the certificate cites, not under
        // their own. If they differ, replay is what says so.
        evidence.insert_as(reference.content, bytes);
    }

    let bundle = capsulet_kernel::Bundle::build(certificate, &evidence).map_err(ApiError::store)?;
    let bytes = bundle.to_canonical_bytes().map_err(ApiError::store)?;

    Ok(Json(CertificateBundleResponse {
        bundle: String::from_utf8(bytes).map_err(ApiError::store)?,
    }))
}

fn to_response(stored: capsulet_postgres::StoredCertificate) -> AssuranceCertificateResponse {
    AssuranceCertificateResponse {
        id: stored.id,
        definition_digest: stored.definition_digest,
        verdict: stored.verdict,
        mode: stored.mode,
        replay_digest: stored.replay_digest,
        canonical_bytes: stored.canonical_bytes,
    }
}

/// A request to run a stored definition version.
#[derive(Debug, Deserialize, ToSchema)]
pub struct StartIrRunRequest {
    /// The definition version to run, by the digest of its canonical bytes.
    pub definition_digest: String,
    /// The run's identifier. Supplying one makes a retried request idempotent
    /// rather than a second run; omitting it has the server generate one.
    #[serde(default)]
    pub id: Option<String>,
}

/// One stored run.
#[derive(Debug, Serialize, ToSchema)]
pub struct IrRunResponse {
    pub id: String,
    /// The exact definition version being executed, by digest.
    pub definition_digest: String,
    /// Where the run is, as an execution concept. Never an assurance verdict:
    /// `completed` says the graph finished, not that anything was verified.
    pub status: String,
    /// The lease generation. Every event names the epoch it was written under,
    /// and a worker whose lease was reclaimed cannot append under the old one.
    pub epoch: u64,
    pub lease_owner: Option<String>,
    /// How many events the log holds.
    pub event_count: u64,
}

/// A page of runs.
#[derive(Debug, Serialize, ToSchema)]
pub struct ListIrRunsResponse {
    pub runs: Vec<IrRunResponse>,
}

/// One event from a run's log.
#[derive(Debug, Serialize, ToSchema)]
pub struct IrRunEventResponse {
    /// Position in this run's log, gapless from zero.
    pub position: u64,
    pub epoch: u64,
    /// The event's short name, which is also the key its body sits under.
    pub kind: String,
    /// When the worker says it happened, in milliseconds since the Unix epoch.
    pub recorded_at: i64,
    /// The event itself.
    #[schema(value_type = Object)]
    pub event: RunEvent,
}

/// A run's whole history.
#[derive(Debug, Serialize, ToSchema)]
pub struct IrRunEventsResponse {
    pub run_id: String,
    /// Every event, in the order the run recorded them. This is the run: its
    /// status and everything else about it is a fold over exactly these.
    pub events: Vec<IrRunEventResponse>,
}

fn run_response(record: IrRunRecord) -> IrRunResponse {
    IrRunResponse {
        id: record.id,
        definition_digest: record.definition_digest,
        status: record.status,
        epoch: record.epoch.0,
        lease_owner: record.lease_owner,
        event_count: record.next_position,
    }
}

/// Enqueues a run of a stored definition version.
///
/// Enqueues rather than starts. The run is written with its admission event and
/// no lease, and the graph worker picks it up; nothing in the API advances a
/// run, and a contract test refuses the code that would.
///
/// The assurance mode comes from the definition rather than from the request.
/// Letting a caller choose it per run would let anybody downgrade `enforce` to
/// `observe` at the moment it mattered.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission, the definition
/// version is not in this project, the run id is already taken, or the store
/// fails.
pub(crate) async fn start_ir_run<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<StartIrRunRequest>,
) -> Result<(StatusCode, Json<IrRunResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_editor")?;

    let version = state
        .store
        .get_ir_definition_version(
            &context.tenant_id,
            &context.project_id,
            &request.definition_digest,
        )
        .await
        .map_err(ApiError::store)?
        .ok_or_else(|| ApiError::IrDefinitionNotFound(request.definition_digest.clone()))?;
    let definition = version
        .definition()
        .map_err(|error| ApiError::store(error.to_string()))?;
    // The digest comes back from storage, so a parse failure here would mean a
    // row written by an incompatible build rather than a bad request.
    let digest: Digest = version
        .digest
        .parse()
        .map_err(|error: capsulet_ir::digest::DigestError| ApiError::store(error.to_string()))?;

    let run_id = request.id.clone().unwrap_or_else(|| generated_id("ir_run"));
    if state
        .store
        .get_ir_run(&context.tenant_id, &context.project_id, &run_id)
        .await
        .map_err(ApiError::store)?
        .is_some()
    {
        return Err(ApiError::RunAlreadyExists(run_id));
    }

    let record = state
        .store
        .create_ir_run(
            &context.tenant_id,
            &context.project_id,
            &run_id,
            &digest,
            definition.assurance,
            recorded_now(),
        )
        .await
        .map_err(ApiError::store)?
        .ok_or_else(|| ApiError::store("this store does not execute IR definitions"))?;

    Ok((StatusCode::CREATED, Json(run_response(record))))
}

/// Lists the caller's project's runs, newest first.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission or the store fails.
pub(crate) async fn list_ir_runs<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListIrRunsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_viewer")?;

    let runs = state
        .store
        .list_ir_runs(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;

    Ok(Json(ListIrRunsResponse {
        runs: runs.into_iter().map(run_response).collect(),
    }))
}

/// Reads one run.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission, the run is not in
/// this project, or the store fails.
pub(crate) async fn get_ir_run<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(run_id): Path<String>,
) -> Result<Json<IrRunResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_viewer")?;

    let record = state
        .store
        .get_ir_run(&context.tenant_id, &context.project_id, &run_id)
        .await
        .map_err(ApiError::store)?
        .ok_or_else(|| ApiError::RunNotFound(run_id.clone()))?;

    Ok(Json(run_response(record)))
}

/// Reads a run's event log.
///
/// The log is returned rather than a summary of it, because the log is the run:
/// every other statement about where a run is comes from folding exactly these
/// events, and a caller that wants to check a claim has to see them.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission, the run is not in
/// this project, or the store fails.
pub(crate) async fn get_ir_run_events<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(run_id): Path<String>,
) -> Result<Json<IrRunEventsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, "project_viewer")?;

    if state
        .store
        .get_ir_run(&context.tenant_id, &context.project_id, &run_id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::RunNotFound(run_id));
    }

    let events = state
        .store
        .load_ir_run_events(&context.tenant_id, &context.project_id, &run_id)
        .await
        .map_err(ApiError::store)?;

    Ok(Json(IrRunEventsResponse {
        run_id,
        events: events
            .into_iter()
            .map(|recorded| IrRunEventResponse {
                position: recorded.position,
                epoch: recorded.epoch.0,
                kind: recorded.event.as_str().to_string(),
                recorded_at: recorded.at.epoch_millis(),
                event: recorded.event,
            })
            .collect(),
    }))
}

/// The current moment, as the IR records one.
///
/// The one place in this module that reads a clock. Everything downstream takes
/// time as a parameter so a decision can be replayed; recording when a run was
/// created is the point at which a real time has to enter.
fn recorded_now() -> RecordedTime {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    RecordedTime(i64::try_from(millis).unwrap_or(i64::MAX))
}
