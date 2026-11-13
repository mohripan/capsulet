//! The live activity stream: a snapshot of what is running, and the events after it.

use std::{convert::Infallible, time::Duration};

use axum::{
    Extension,
    extract::State,
    http::HeaderMap,
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use capsulet_observability::{self as observability};
use capsulet_storage::ObjectStore;
use serde::Serialize;

use super::identity::{ProjectContext, project_context};
use crate::{auth::Principal, error::ApiError, state::AppState, store::ApiStore};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct ActivitySnapshot {
    pub(super) job_definition_count: usize,
    pub(super) automation_count: usize,
    pub(super) job_runs: Vec<ActivityRunSnapshot>,
    pub(super) workflow_runs: Vec<ActivityWorkflowRunSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct ActivityRunSnapshot {
    pub(super) id: String,
    pub(super) status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct ActivityWorkflowRunSnapshot {
    pub(super) id: String,
    pub(super) status: String,
    pub(super) current_step_position: i32,
    pub(super) step_runs: Vec<ActivityStepRunSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct ActivityStepRunSnapshot {
    pub(super) id: String,
    pub(super) job_run_id: Option<String>,
    pub(super) position: i32,
    pub(super) status: String,
}

pub(super) async fn stream_activity_events<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Response, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let stream_state = state.clone();
    let stream = async_stream::stream! {
        let mut last_snapshot: Option<ActivitySnapshot> = None;
        loop {
            match load_activity_snapshot(&stream_state, &context).await {
                Ok(snapshot) => {
                    if last_snapshot.as_ref() != Some(&snapshot) {
                        match serde_json::to_string(&snapshot) {
                            Ok(data) => {
                                last_snapshot = Some(snapshot);
                                yield Ok::<_, Infallible>(Event::default().event("snapshot").data(data));
                            }
                            Err(error) => {
                                observability::tracing::warn!(%error, "failed to serialize activity stream snapshot");
                                break;
                            }
                        }
                    }
                }
                Err(error) => {
                    yield Ok(Event::default().event("error").data(error.to_string()));
                    break;
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    };
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response())
}

pub(super) async fn load_activity_snapshot<S, O>(
    state: &AppState<S, O>,
    context: &ProjectContext,
) -> Result<ActivitySnapshot, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let (definitions, automations, runs, workflow_runs) = tokio::try_join!(
        state
            .store
            .list_job_definitions(&context.tenant_id, &context.project_id, 500),
        state
            .store
            .list_automations(&context.tenant_id, &context.project_id, 500),
        state
            .store
            .list_runs(&context.tenant_id, &context.project_id, 200),
        state
            .store
            .list_workflow_runs(&context.tenant_id, &context.project_id, 200),
    )
    .map_err(ApiError::store)?;

    let mut job_runs = Vec::new();
    for run in runs {
        job_runs.push(ActivityRunSnapshot {
            id: run.id().as_str().to_string(),
            status: run.status().to_string(),
        });
    }
    job_runs.sort_by(|left, right| left.id.cmp(&right.id));

    let mut workflow_snapshots = Vec::with_capacity(workflow_runs.len());
    for run in workflow_runs {
        let mut step_runs = state
            .store
            .list_workflow_step_runs(run.id(), &context.tenant_id, &context.project_id)
            .await
            .map_err(ApiError::store)?
            .iter()
            .map(|step_run| ActivityStepRunSnapshot {
                id: step_run.id().as_str().to_string(),
                job_run_id: step_run
                    .maybe_job_run_id()
                    .map(|job_run_id| job_run_id.as_str().to_string()),
                position: step_run.position(),
                status: step_run.status().to_string(),
            })
            .collect::<Vec<_>>();
        step_runs.sort_by_key(|step_run| step_run.position);
        workflow_snapshots.push(ActivityWorkflowRunSnapshot {
            id: run.id().as_str().to_string(),
            status: run.status().to_string(),
            current_step_position: run.current_step_position(),
            step_runs,
        });
    }
    workflow_snapshots.sort_by(|left, right| left.id.cmp(&right.id));

    // Both lists are scoped in their queries.
    let job_definition_count = definitions.len();
    let automation_count = automations.len();

    Ok(ActivitySnapshot {
        job_definition_count,
        automation_count,
        job_runs,
        workflow_runs: workflow_snapshots,
    })
}
