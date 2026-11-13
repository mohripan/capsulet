//! Liveness, health and readiness, Prometheus metrics, and the `OpenAPI` document.

use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use capsulet_observability::{self as observability};
use capsulet_storage::ObjectStore;

use crate::{models::HealthResponse, state::AppState, store::ApiStore};

pub(super) async fn metrics<S, O>(State(state): State<AppState<S, O>>) -> Response
where
    S: ApiStore,
    O: ObjectStore,
{
    match state.store.prometheus_metrics().await {
        Ok(db_body) => {
            let mut body = observability::render_metrics();
            if !body.is_empty() && !body.ends_with('\n') {
                body.push('\n');
            }
            body.push_str(&db_body);
            ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body).into_response()
        }
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(super) async fn openapi_spec() -> Response {
    match crate::canonical_openapi_json() {
        Ok(document) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "application/vnd.oai.openapi+json; charset=utf-8",
            )],
            document,
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to generate OpenAPI: {error}"),
        )
            .into_response(),
    }
}

pub(super) async fn livez() -> Json<HealthResponse> {
    Json(HealthResponse::new("alive"))
}

pub(super) async fn healthz<S, O>(
    State(state): State<AppState<S, O>>,
) -> (StatusCode, Json<HealthResponse>)
where
    S: ApiStore,
    O: ObjectStore,
{
    match state.store.ping().await {
        Ok(()) => (StatusCode::OK, Json(HealthResponse::new("ok"))),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse::new("not_ready")),
        ),
    }
}

pub(super) async fn readyz<S, O>(
    State(state): State<AppState<S, O>>,
) -> (StatusCode, Json<HealthResponse>)
where
    S: ApiStore,
    O: ObjectStore,
{
    match state.store.ping().await {
        Ok(()) => (StatusCode::OK, Json(HealthResponse::new("ready"))),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse::new("not_ready")),
        ),
    }
}
