//! The artifacts a job run produced.

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use capsulet_core::{ArtifactId, JobRunId};
use capsulet_storage::ObjectStore;

use super::identity::project_context;
use crate::{
    auth::Principal,
    error::ApiError,
    models::{ArtifactResponse, ListArtifactsResponse},
    state::AppState,
    store::ApiStore,
};

pub(super) async fn list_artifacts<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<ListArtifactsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = JobRunId::new(id).map_err(ApiError::validation)?;
    if state
        .store
        .find_run(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::RunNotFound(id.as_str().to_string()));
    }

    let artifacts = state
        .store
        .list_artifacts(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?;

    Ok(Json(ListArtifactsResponse {
        artifacts: artifacts.iter().map(ArtifactResponse::from).collect(),
    }))
}

pub(super) async fn download_artifact<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path((id, artifact_id)): Path<(String, String)>,
) -> Result<Response, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = JobRunId::new(id).map_err(ApiError::validation)?;
    let artifact_id = ArtifactId::new(artifact_id).map_err(ApiError::validation)?;
    if state
        .store
        .find_run(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
        .is_none()
    {
        return Err(ApiError::RunNotFound(id.as_str().to_string()));
    }

    let Some(artifact) = state
        .store
        .find_artifact(&id, &artifact_id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::ArtifactNotFound(artifact_id.as_str().to_string()));
    };
    let Some(bytes) = state
        .object_store
        .get(artifact.object_key())
        .await
        .map_err(ApiError::object_store)?
    else {
        return Err(ApiError::ArtifactObjectNotFound(
            artifact.object_key().to_string(),
        ));
    };

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, artifact.content_type().to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", artifact.name()),
            ),
        ],
        bytes,
    )
        .into_response())
}
