//! IR definitions and certificates are append-only, content-addressed, and
//! scoped to a project — enforced by the database, not only by the code above
//! it.

use capsulet_ir::admission::AdmissionRecord;
use capsulet_ir::capability::CapabilitySet;
use capsulet_ir::correctness::certificate::{Certificate, Subject};
use capsulet_ir::correctness::evidence::EvidenceRef;
use capsulet_ir::correctness::obligation::{DischargeState, ObligationStatement, RepairOwner};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::definition::{AssuranceMode, Definition};
use capsulet_ir::value::LengthBounds;
use capsulet_ir::{
    AssuranceVerdict, Digest, Graph, GraphBuilder, Identifier, Identity, Node, NodeKind,
    Obligation, OutputPort, RecordedTime, ResourceBudget, ValueSchema, admit,
};
use capsulet_kernel::workflow::{Assembly, certify};
use capsulet_postgres::PostgresStore;

mod support;
use support::{fixture_id as unique_id, required_database_url as database_url};

fn id(value: &str) -> Identifier {
    Identifier::parse(value).expect("the fixture identifier is valid")
}

fn definition(name: &str) -> Definition {
    let graph = Graph::new(GraphBuilder {
        nodes: vec![Node {
            id: id("prepare"),
            name: "Prepare".to_string(),
            kind: NodeKind::PureComputation,
            inputs: vec![],
            outputs: vec![OutputPort::new(
                id("patch"),
                ValueSchema::Text {
                    length: LengthBounds::new(0, 1_024),
                },
            )],
            capabilities: vec![],
            effects: vec![],
            budget: ResourceBudget::deterministic(1_000),
            provider: None,
            sub_workflow: None,
        }],
        ..GraphBuilder::default()
    })
    .expect("the fixture graph is valid");

    Definition {
        schema_version: Definition::current_schema_version(),
        id: id(name),
        version: "1".to_string(),
        name: name.to_string(),
        assurance: AssuranceMode::Enforce,
        capabilities: CapabilitySet::empty(),
        budget: ResourceBudget::deterministic(600_000),
        graph,
        boundaries: vec![],
        contracts: vec![],
    }
}

fn admission(definition: &Definition) -> AdmissionRecord {
    admit(definition).expect("the fixture definition is admitted")
}

fn certificate(definition: &Definition, certificate_id: &str, content: &[u8]) -> Certificate {
    let record = admission(definition);
    let evidence = EvidenceRef {
        id: id("test-log"),
        content: Digest::of(content),
        media_type: "text/plain".to_string(),
        byte_length: content.len() as u64,
        producer: Producer {
            kind: ProducerKind::Deterministic,
            identity: Identity::new(id("cargo-test"), "1.96"),
        },
        captured_at: RecordedTime(1_772_000_000_000),
    };

    certify(Assembly {
        id: id(certificate_id),
        subject: Subject {
            definition: *record.definition(),
            definition_version: "1".to_string(),
            run: None,
            inputs: vec![],
            outputs: vec![],
        },
        admission: record,
        mode: AssuranceMode::Enforce,
        policy_version: "release-policy/3".to_string(),
        contracts: vec![id("patch-compiles")],
        verifiers: vec![],
        obligations: vec![Obligation {
            statement: ObligationStatement {
                id: id("compiles"),
                statement: "the patch compiles".to_string(),
                owner: RepairOwner::Verifier,
            },
            contract: id("patch-compiles"),
            state: DischargeState::Discharged {
                by: id("cargo-test"),
                evidence: vec![Digest::of(content)],
            },
        }],
        evidence: vec![evidence],
        loops: vec![],
    })
    .expect("the certificate seals")
}

async fn store() -> PostgresStore {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    store
}

#[tokio::test]
async fn registering_the_same_definition_twice_is_idempotent() {
    let store = store().await;
    let tenant = unique_id("tenant");
    let project = unique_id("project");
    let definition = definition(&unique_id("definition"));
    let record = admission(&definition);

    let first = store
        .insert_ir_definition_version(&tenant, &project, &definition, &record)
        .await
        .expect("register the definition");
    let second = store
        .insert_ir_definition_version(&tenant, &project, &definition, &record)
        .await
        .expect("register it again");

    // The digest is the identity, so there is nothing to conflict about.
    assert_eq!(first, second);
    let versions = store
        .list_ir_definition_versions(&tenant, &project, 10)
        .await
        .expect("list versions");
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].digest, first.to_string());
}

#[tokio::test]
async fn stored_bytes_round_trip_to_the_same_definition_and_digest() {
    let store = store().await;
    let tenant = unique_id("tenant");
    let project = unique_id("project");
    let definition = definition(&unique_id("definition"));
    let digest = store
        .insert_ir_definition_version(&tenant, &project, &definition, &admission(&definition))
        .await
        .expect("register the definition");

    let stored = store
        .get_ir_definition_version(&tenant, &project, &digest.to_string())
        .await
        .expect("read the version")
        .expect("the version exists");

    let read_back = stored.definition().expect("the stored bytes parse");
    assert_eq!(read_back, definition);
    assert_eq!(
        capsulet_ir::digest_of(&read_back).expect("it digests"),
        digest
    );
}

#[tokio::test]
async fn a_definition_version_cannot_be_updated_or_deleted() {
    let store = store().await;
    let tenant = unique_id("tenant");
    let project = unique_id("project");
    let definition = definition(&unique_id("definition"));
    let digest = store
        .insert_ir_definition_version(&tenant, &project, &definition, &admission(&definition))
        .await
        .expect("register the definition");

    // The database refuses both, loudly. A silent no-op would let a caller
    // believe the edit landed.
    let update = sqlx::query(
        "UPDATE ir_definition_versions SET canonical_bytes = 'tampered' WHERE digest = $1",
    )
    .bind(digest.to_string())
    .execute(store.pool())
    .await
    .expect_err("an append-only table refuses an update");
    assert!(
        update.to_string().contains("append-only"),
        "the refusal should say why: {update}"
    );

    let delete = sqlx::query("DELETE FROM ir_definition_versions WHERE digest = $1")
        .bind(digest.to_string())
        .execute(store.pool())
        .await
        .expect_err("an append-only table refuses a delete");
    assert!(
        delete.to_string().contains("append-only"),
        "the refusal should say why: {delete}"
    );

    let stored = store
        .get_ir_definition_version(&tenant, &project, &digest.to_string())
        .await
        .expect("read the version")
        .expect("the version still exists");
    assert_eq!(
        stored.definition().expect("the stored bytes parse"),
        definition,
        "an append-only table must survive an update and a delete"
    );
}

#[tokio::test]
async fn a_certificate_cannot_be_rewritten_to_a_better_verdict() {
    let store = store().await;
    let tenant = unique_id("tenant");
    let project = unique_id("project");
    let definition = definition(&unique_id("definition"));
    let certificate_id = unique_id("cert");
    let certificate = certificate(&definition, &certificate_id, b"tests passed");

    store
        .insert_assurance_certificate(&tenant, &project, &certificate)
        .await
        .expect("record the certificate");

    // Rewriting a verdict to a better one is exactly the edit this table exists
    // to refuse, and it is refused with an error rather than quietly dropped.
    sqlx::query("UPDATE assurance_certificates SET verdict = 'accepted' WHERE id = $1")
        .bind(&certificate_id)
        .execute(store.pool())
        .await
        .expect_err("an append-only table refuses an update");
    sqlx::query("DELETE FROM assurance_certificates WHERE id = $1")
        .bind(&certificate_id)
        .execute(store.pool())
        .await
        .expect_err("an append-only table refuses a delete");

    let stored = store
        .get_assurance_certificate(&tenant, &project, &certificate_id)
        .await
        .expect("read the certificate")
        .expect("the certificate still exists");
    assert_eq!(stored.verdict, certificate.verdict().as_str());

    // And the bytes still seal, so the record can still be checked by someone
    // who was not here when it was written.
    let read_back = stored.certificate().expect("the stored bytes parse");
    assert_eq!(read_back.replay_digest(), certificate.replay_digest());
    assert_eq!(read_back.verify_seal(), Ok(()));
}

#[tokio::test]
async fn a_certificate_from_another_project_is_not_visible() {
    let store = store().await;
    let tenant = unique_id("tenant");
    let ours = unique_id("project");
    let theirs = unique_id("project");
    let definition = definition(&unique_id("definition"));
    let certificate_id = unique_id("cert");

    store
        .insert_assurance_certificate(
            &tenant,
            &ours,
            &certificate(&definition, &certificate_id, b"tests passed"),
        )
        .await
        .expect("record the certificate");

    assert!(
        store
            .get_assurance_certificate(&tenant, &theirs, &certificate_id)
            .await
            .expect("read across projects")
            .is_none(),
        "project scope is part of the key, not a filter applied afterwards"
    );
    assert!(
        store
            .list_assurance_certificates(&tenant, &theirs, 10)
            .await
            .expect("list across projects")
            .is_empty()
    );
}

#[tokio::test]
async fn obligations_are_projected_so_outstanding_work_is_queryable() {
    let store = store().await;
    let tenant = unique_id("tenant");
    let project = unique_id("project");
    let definition = definition(&unique_id("definition"));

    let discharged = certificate(&definition, &unique_id("cert"), b"tests passed");
    store
        .insert_assurance_certificate(&tenant, &project, &discharged)
        .await
        .expect("record the certificate");
    assert_eq!(
        store
            .count_outstanding_obligations(&tenant, &project)
            .await
            .expect("count obligations"),
        0
    );

    // A residual obligation is outstanding, and stays visible as such.
    let record = admission(&definition);
    let residual = certify(Assembly {
        id: id(&unique_id("cert")),
        subject: Subject {
            definition: *record.definition(),
            definition_version: "1".to_string(),
            run: None,
            inputs: vec![],
            outputs: vec![],
        },
        admission: record,
        mode: AssuranceMode::Enforce,
        policy_version: "release-policy/3".to_string(),
        contracts: vec![id("patch-compiles")],
        verifiers: vec![],
        obligations: vec![Obligation {
            statement: ObligationStatement {
                id: id("summary-is-faithful"),
                statement: "the summary reads faithfully".to_string(),
                owner: RepairOwner::Human,
            },
            contract: id("patch-compiles"),
            state: DischargeState::Residual {
                rationale: "no checker can decide this".to_string(),
                evidence: vec![],
            },
        }],
        evidence: vec![],
        loops: vec![],
    })
    .expect("the certificate seals");
    assert_eq!(residual.verdict(), AssuranceVerdict::Conditional);

    store
        .insert_assurance_certificate(&tenant, &project, &residual)
        .await
        .expect("record the conditional certificate");
    assert_eq!(
        store
            .count_outstanding_obligations(&tenant, &project)
            .await
            .expect("count obligations"),
        1
    );
}

#[tokio::test]
async fn evidence_metadata_records_where_the_bytes_live() {
    let store = store().await;
    let tenant = unique_id("tenant");
    let project = unique_id("project");
    let definition = definition(&unique_id("definition"));
    let content = b"tests passed";

    store
        .insert_assurance_certificate(
            &tenant,
            &project,
            &certificate(&definition, &unique_id("cert"), content),
        )
        .await
        .expect("record the certificate");

    let digest = Digest::of(content).to_string();
    let location = store
        .get_assurance_evidence(&tenant, &project, &digest)
        .await
        .expect("read the evidence location")
        .expect("the evidence is recorded");

    assert_eq!(location.digest, digest);
    assert_eq!(
        location.byte_length,
        i64::try_from(content.len()).expect("the fixture is small")
    );
    assert!(
        location.object_key.ends_with(&digest),
        "the object key is derived from the digest so it cannot point at other bytes"
    );
}
