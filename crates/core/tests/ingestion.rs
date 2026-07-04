use capsulet_core::{
    Authority, IngestionConnector, IngestionConnectorConfig, IngestionConnectorId,
    IngestionConnectorKind, IngestionRun, IngestionRunId, IngestionRunStatus, MemoryScope,
    run_local_text_ingestion,
};

fn scope() -> MemoryScope {
    MemoryScope::new("tenant_alpha", "project_memory").expect("scope")
}

#[test]
fn local_text_connector_rejects_empty_content() {
    let error = IngestionConnector::new(
        IngestionConnectorId::new("connector_empty").expect("connector id"),
        scope(),
        "Empty local connector",
        IngestionConnectorKind::LocalText,
        IngestionConnectorConfig::local_text("Empty", "", "text/markdown", None, Authority::Medium),
        true,
    )
    .expect_err("empty local text content should be rejected");

    assert!(error.to_string().contains("content cannot be empty"));
}

#[test]
fn local_text_ingestion_creates_evidence_and_candidate_claims() {
    let connector = IngestionConnector::new(
        IngestionConnectorId::new("connector_project_notes").expect("connector id"),
        scope(),
        "Project notes",
        IngestionConnectorKind::LocalText,
        IngestionConnectorConfig::local_text(
            "Project Atlas Notes",
            "# Project Atlas\n\n- Project Atlas is blocked by Legal Review\n- Sarah approved Project Atlas\n",
            "text/markdown",
            Some("local://project-atlas.md".to_string()),
            Authority::High,
        ),
        true,
    )
    .expect("connector");
    let run = IngestionRun::queued(
        IngestionRunId::new("ingestion_run_project_notes").expect("run id"),
        scope(),
        connector.id().clone(),
    );

    let output = run_local_text_ingestion(&connector, run).expect("ingestion output");

    assert_eq!(output.run().status(), IngestionRunStatus::Succeeded);
    assert_eq!(output.sources().len(), 1);
    assert!(!output.evidence().is_empty());
    assert!(!output.entities().is_empty());
    assert_eq!(output.claims().len(), 2);
    assert!(
        output
            .claims()
            .iter()
            .all(|claim| !claim.evidence_ids().is_empty())
    );
}
