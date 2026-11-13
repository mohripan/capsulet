//! Records read back over the API in the shapes their wire contracts publish.

use super::*;

/// Each kind of memory record reads back as its contract publishes it.
#[tokio::test]
async fn memory_records_read_back_as_their_contracts_publish() {
    let app = authenticated_app(FakeStore::default());
    for (uri, body) in [
        (
            "/v1/memory/sources",
            json!({"id": "source_readback", "kind": "executive_update", "uri": "file:///readback.md", "title": "Readback", "authority": "high"}),
        ),
        (
            "/v1/memory/evidence",
            json!({"id": "evidence_readback", "source_id": "source_readback", "locator": "readback.md#L1", "excerpt": "Atlas launches in August.", "observed_at": "2026-07-05T10:00:00Z"}),
        ),
        (
            "/v1/memory/entities",
            json!({"id": "entity_atlas", "entity_type": "Project", "name": "Atlas", "aliases": ["Project Atlas"]}),
        ),
        (
            "/v1/memory/entities",
            json!({"id": "entity_launch", "entity_type": "Milestone", "name": "Launch"}),
        ),
        (
            "/v1/memory/events",
            json!({"id": "event_readback", "event_type": "launch_scheduled", "occurred_at": "2026-07-05T10:00:00Z", "entity_ids": ["entity_atlas"], "evidence_ids": ["evidence_readback"]}),
        ),
        (
            "/v1/memory/relationships",
            json!({"id": "relationship_readback", "relationship_type": "has_milestone", "from_entity_id": "entity_atlas", "to_entity_id": "entity_launch", "evidence_ids": ["evidence_readback"]}),
        ),
    ] {
        succeeds(&app, admin_request(Method::POST, uri, Some(body))).await;
    }
    for uri in [
        "/v1/memory/sources/source_readback",
        "/v1/memory/evidence/evidence_readback",
        "/v1/memory/entities/entity_atlas",
        "/v1/memory/events/event_readback",
        "/v1/memory/relationships/relationship_readback",
    ] {
        let record = succeeds(&app, admin_request(Method::GET, uri, None)).await;
        assert_eq!(
            Some(uri.rsplit('/').next().expect("an id")),
            record["id"].as_str(),
            "{uri}"
        );
    }
}

/// Control-plane records read back, change and go away as their contracts publish.
#[tokio::test]
async fn control_plane_records_read_back_change_and_go_as_their_contracts_publish() {
    let app = authenticated_app(FakeStore::default());
    for (uri, body) in [
        (
            "/v1/job-definitions",
            json!({"id": "job_readback", "name": "Readback", "python_script": "print('readback')"}),
        ),
        (
            "/v1/job-definitions",
            json!({"id": "job_disposable", "name": "Disposable", "python_script": "print('gone')"}),
        ),
        (
            "/v1/workflows",
            json!({"id": "wf_readback", "name": "Readback", "steps": [{"id": "only", "name": "Only", "job_definition_id": "job_readback", "execution_pool": "mini"}]}),
        ),
        (
            "/v1/workflows",
            json!({"id": "wf_disposable", "name": "Disposable", "steps": [{"id": "only", "name": "Only", "job_definition_id": "job_readback", "execution_pool": "mini"}]}),
        ),
        (
            "/v1/trigger-plugins",
            json!({"id": "plugin_readback", "name": "Readback", "runtime_image": "python:3.12-slim", "python_script": "import json\nprint(json.dumps({'matched': False}))"}),
        ),
        (
            "/v1/automations",
            json!({"id": "automation_readback", "name": "Readback", "workflow_id": "wf_readback", "triggers": [{"name": "nightly", "kind": "schedule", "config": {"interval_seconds": 300}}]}),
        ),
    ] {
        succeeds(&app, admin_request(Method::POST, uri, Some(body))).await;
    }

    succeeds(
        &app,
        admin_request(
            Method::PUT,
            "/v1/workflows/wf_readback",
            Some(json!({"name": "Readback, renamed", "steps": [{"id": "only", "name": "Only", "job_definition_id": "job_readback", "execution_pool": "mini"}]})),
        ),
    )
    .await;
    for uri in [
        "/v1/trigger-plugins/plugin_readback",
        "/v1/automations/automation_readback",
        "/v1/automations/automation_readback/triggers",
    ] {
        succeeds(&app, admin_request(Method::GET, uri, None)).await;
    }
    let run = succeeds(
        &app,
        admin_request(
            Method::POST,
            "/v1/automations/automation_readback/trigger",
            Some(json!({})),
        ),
    )
    .await;
    let run_id = run["id"].as_str().expect("a triggered run has an id");
    succeeds(
        &app,
        admin_request(Method::GET, &format!("/v1/workflow-runs/{run_id}"), None),
    )
    .await;

    // Service accounts are not here: the in-memory store does not keep them, so
    // their reads and revocation are exercised against PostgreSQL instead.

    succeeds(
        &app,
        admin_request(
            Method::POST,
            "/v1/projects/default/memberships",
            Some(json!({"principal_kind": "user", "principal_name": "readback-user", "role": "project_viewer"})),
        ),
    )
    .await;
    for uri in [
        "/v1/projects/default/memberships/user/readback-user",
        "/v1/workflows/wf_disposable",
        "/v1/job-definitions/job_disposable",
    ] {
        succeeds(&app, admin_request(Method::DELETE, uri, None)).await;
    }
}
