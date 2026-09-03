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
use capsulet_ir::admission::admit;
use capsulet_ir::definition::Definition;
use capsulet_storage::ObjectStore;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    auth::Principal,
    error::ApiError,
    http::internal::{project_context, require_project_role},
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
