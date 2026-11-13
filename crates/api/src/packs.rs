//! Installing domain packs into a project (ADR 0022).
//!
//! A pack arrives as one JSON document, the manifest, the signature, and each
//! file's bytes, and installs only when `capsulet_pack::admit` admits it under
//! the project's trust store. An installed version is held as it arrived and
//! never changes, so a definition pinning it, and every certificate decided
//! under that definition, name exactly these bytes.

use std::collections::BTreeMap;

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::HeaderMap,
};
use capsulet_pack::{Bundle, Installed, Pack, TrustStore, admit};
use capsulet_postgres::DomainPackRecord;
use capsulet_storage::ObjectStore;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    auth::Principal,
    error::ApiError,
    http::{ProjectContext, project_context, require_project_role},
    state::AppState,
    store::ApiStore,
};

/// A pack, as one JSON document.
#[derive(Debug, Deserialize, ToSchema)]
pub struct InstallPackRequest {
    /// The pack's manifest, `capsulet.pack/v1`.
    #[schema(value_type = Object)]
    pub manifest: serde_json::Value,
    /// Its signature, `capsulet.pack-signature/v1`.
    #[schema(value_type = Option<Object>)]
    #[serde(default)]
    pub signature: Option<serde_json::Value>,
    /// Each file's bytes, hex-encoded, by path.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

/// An installed pack version.
#[derive(Debug, Serialize, ToSchema)]
pub struct InstalledPackResponse {
    pub name: String,
    pub version: String,
    /// The manifest digest: what a definition pins.
    pub manifest: String,
    /// The key that signed it.
    pub signed_by: String,
    pub installed_by: String,
    pub description: String,
    pub contracts: Vec<String>,
    /// The validator registrations it carries, by path.
    pub validators: Vec<String>,
    /// The policy defaults for its obligations, by path: IR contracts a
    /// definition using the pack declares.
    pub obligations: Vec<String>,
    /// Whether this request installed it, or found it already installed.
    pub newly_installed: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListPacksResponse {
    pub packs: Vec<InstalledPackResponse>,
}

/// Installs a pack into the caller's project, or reports it already installed.
/// Idempotent: installing the same pack again answers the same, with
/// `newly_installed` false.
///
/// # Errors
///
/// Returns [`ApiError::PackRefused`] naming the first reason the pack is refused:
/// unreadable, unsigned, tampered, signed by a key the project does not trust
/// for it, a downgrade or a reused version, or content that does not read.
pub(crate) async fn install_pack<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<InstallPackRequest>,
) -> Result<Json<InstalledPackResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Admin)?;

    let bundle: Bundle = serde_json::from_value(serde_json::json!({
        "manifest": request.manifest,
        "signature": request.signature,
        "files": request.files,
    }))
    .map_err(|error| {
        refused(
            &format!("the pack is not a pack bundle: {error}"),
            "unreadable",
        )
    })?;
    let pack =
        Pack::from_bundle(bundle).map_err(|error| refused(&error.to_string(), error.code()))?;

    let installed: Vec<Installed> = state
        .store
        .list_domain_packs(&context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
        .into_iter()
        .map(|record| Installed {
            name: record.name,
            version: record.version,
            manifest: record.manifest_digest,
        })
        .collect();
    let trust = trust_store(&state, &context)?;
    let admitted = admit(&pack, &trust, &installed)
        .map_err(|error| refused(&error.to_string(), error.code()))?;

    let record = DomainPackRecord {
        tenant_id: context.tenant_id.clone(),
        project_id: context.project_id.clone(),
        name: admitted.name.clone(),
        version: admitted.version.clone(),
        manifest_digest: admitted.manifest.clone(),
        signed_by: admitted.key.clone(),
        bundle: serde_json::to_value(pack.bundle())
            .map_err(|error| ApiError::Validation(error.to_string()))?,
        installed_by: principal.name.to_string(),
    };
    let newly_installed = !admitted.already_installed
        && state
            .store
            .insert_domain_pack(&record)
            .await
            .map_err(ApiError::store)?;
    let stored = if newly_installed {
        record
    } else {
        state
            .store
            .find_domain_pack(
                &context.tenant_id,
                &context.project_id,
                &admitted.name,
                &admitted.version,
            )
            .await
            .map_err(ApiError::store)?
            .unwrap_or(record)
    };
    Ok(Json(response(&stored, newly_installed)))
}

/// Lists the pack versions installed in the caller's project.
///
/// # Errors
///
/// Returns [`ApiError`] when the caller lacks permission or the store fails.
pub(crate) async fn list_packs<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListPacksResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Viewer)?;
    let packs = state
        .store
        .list_domain_packs(&context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListPacksResponse {
        packs: packs.iter().map(|record| response(record, false)).collect(),
    }))
}

/// One installed pack version.
///
/// # Errors
///
/// Returns [`ApiError::PackNotFound`] when the caller's project does not hold it.
pub(crate) async fn get_pack<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path((name, version)): Path<(String, String)>,
) -> Result<Json<InstalledPackResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    require_project_role(&context, crate::ProjectRole::Viewer)?;
    let record = state
        .store
        .find_domain_pack(&context.tenant_id, &context.project_id, &name, &version)
        .await
        .map_err(ApiError::store)?
        .ok_or_else(|| ApiError::PackNotFound(format!("{name} {version}")))?;
    Ok(Json(response(&record, false)))
}

/// Refuses a definition pinning a pack its project does not hold at exactly
/// that version and manifest digest.
///
/// # Errors
///
/// Returns [`ApiError::PackNotInstalled`] for the first pin no installed pack
/// satisfies.
pub(crate) async fn check_pins<S, O>(
    state: &AppState<S, O>,
    context: &ProjectContext,
    definition: &capsulet_ir::Definition,
) -> Result<(), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    for pin in &definition.packs {
        let held = state
            .store
            .find_domain_pack(
                &context.tenant_id,
                &context.project_id,
                &pin.name,
                &pin.version,
            )
            .await
            .map_err(ApiError::store)?;
        match held {
            Some(record) if record.manifest_digest == pin.manifest.to_string() => {}
            Some(record) => {
                return Err(ApiError::PackNotInstalled(format!(
                    "the definition pins {} {} at {}, and this project holds it at {}",
                    pin.name, pin.version, pin.manifest, record.manifest_digest
                )));
            }
            None => {
                return Err(ApiError::PackNotInstalled(format!(
                    "the definition pins {} {}, which this project has not installed",
                    pin.name, pin.version
                )));
            }
        }
    }
    Ok(())
}

/// The project's trust store, or an empty one when none is configured or the
/// project has none: then no signer is trusted, and every pack is refused for
/// its key.
fn trust_store<S, O>(
    state: &AppState<S, O>,
    context: &ProjectContext,
) -> Result<TrustStore, ApiError> {
    let Some(directory) = &state.pack_trust else {
        return Ok(TrustStore::new());
    };
    let plain =
        |part: &str| !part.is_empty() && !part.contains(['/', '\\', ':']) && !part.starts_with('.');
    if !plain(&context.tenant_id) || !plain(&context.project_id) {
        return Ok(TrustStore::new());
    }
    let path = directory
        .join(&context.tenant_id)
        .join(format!("{}.json", context.project_id));
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
            refused(
                &format!("the project's trust store does not read: {error}"),
                "trust_store",
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(TrustStore::new()),
        Err(error) => Err(refused(
            &format!("the project's trust store cannot be read: {error}"),
            "trust_store",
        )),
    }
}

fn refused(detail: &str, code: &str) -> ApiError {
    ApiError::PackRefused(format!("{detail} ({code})"))
}

fn response(record: &DomainPackRecord, newly_installed: bool) -> InstalledPackResponse {
    let manifest = record.bundle.get("manifest");
    let strings = |key: &str| -> Vec<String> {
        manifest
            .and_then(|manifest| manifest.get(key))
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        item.as_str().map(str::to_string).or_else(|| {
                            item.get("registration")
                                .and_then(|path| path.as_str())
                                .map(str::to_string)
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    InstalledPackResponse {
        name: record.name.clone(),
        version: record.version.clone(),
        manifest: record.manifest_digest.clone(),
        signed_by: record.signed_by.clone(),
        installed_by: record.installed_by.clone(),
        description: manifest
            .and_then(|manifest| manifest.get("description"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        contracts: strings("contracts"),
        validators: strings("validators"),
        obligations: strings("obligations"),
        newly_installed,
    }
}
