//! Ingestion connectors and their runs.

use super::*;

#[tokio::test]
async fn saves_ingestion_connector_run_and_outputs_when_database_is_available() {
    let database_url = database_url();
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");

    let scope =
        MemoryScope::new("tenant_ingestion", unique_id("project_ingestion")).expect("scope");
    let connector = IngestionConnector::new(
        IngestionConnectorId::new(unique_id("connector_ingestion")).expect("connector id"),
        scope.clone(),
        "Project notes",
        IngestionConnectorKind::LocalText,
        IngestionConnectorConfig::local_text(
            "Project Atlas Notes",
            "- Project Atlas is blocked by Legal Review",
            "text/markdown",
            Some("local://project-atlas.md".to_string()),
            Authority::High,
        ),
        true,
    )
    .expect("connector");
    let run = IngestionRun::queued(
        IngestionRunId::new(unique_id("ingestion_run")).expect("run id"),
        scope.clone(),
        connector.id().clone(),
    )
    .succeeded(1, 1, 1, 1);
    let output = IngestionRunOutputRecord::new(run.id().clone(), "claim", "claim_ingestion_1")
        .expect("output");

    store
        .upsert_ingestion_connector(&connector)
        .await
        .expect("save connector");
    store.upsert_ingestion_run(&run).await.expect("save run");
    store
        .upsert_ingestion_run_output(&output)
        .await
        .expect("save output");

    let connectors = store
        .list_ingestion_connectors(scope.tenant_id(), scope.project_id(), 10)
        .await
        .expect("list connectors");
    let runs = store
        .list_ingestion_runs(scope.tenant_id(), scope.project_id(), 10)
        .await
        .expect("list runs");
    let outputs = store
        .list_ingestion_run_outputs(run.id(), scope.tenant_id(), scope.project_id())
        .await
        .expect("list outputs");

    assert_eq!(connectors[0].id(), connector.id());
    assert_eq!(runs[0].claim_count(), 1);
    assert_eq!(outputs, vec![output]);
}

#[tokio::test]
async fn connector_upsert_cannot_take_over_another_projects_identifier() {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let id = IngestionConnectorId::new(unique_id("isolated_connector")).expect("id");
    let original_scope =
        MemoryScope::new("tenant_isolation", unique_id("project_a")).expect("scope");
    let other_scope = MemoryScope::new("tenant_isolation", unique_id("project_b")).expect("scope");
    let connector = |scope, name| {
        IngestionConnector::new(
            id.clone(),
            scope,
            name,
            IngestionConnectorKind::LocalText,
            IngestionConnectorConfig::local_text(
                "Notes",
                "Public notes",
                "text/plain",
                None,
                Authority::High,
            ),
            true,
        )
        .expect("connector")
    };
    let original = connector(original_scope.clone(), "Original");
    store
        .upsert_ingestion_connector(&original)
        .await
        .expect("create");
    let attempted = store
        .upsert_ingestion_connector(&connector(other_scope.clone(), "Taken over"))
        .await;
    assert!(
        attempted.is_err(),
        "a cross-project upsert must be rejected"
    );
    assert_eq!(
        store
            .list_ingestion_connectors(original_scope.tenant_id(), original_scope.project_id(), 10)
            .await
            .expect("list"),
        vec![original]
    );
    assert_eq!(
        store
            .list_ingestion_connectors(other_scope.tenant_id(), other_scope.project_id(), 10)
            .await
            .expect("list"),
        [] as [capsulet_core::IngestionConnector; 0]
    );
}
