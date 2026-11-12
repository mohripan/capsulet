//! IR definitions and certificates are append-only, content-addressed, and
//! scoped to a project — enforced by the database, not only by the code above
//! it.

use capsulet_ir::correctness::certificate::{Certificate, Subject, VerifierRecord, VerifierTrust};
use capsulet_ir::correctness::evidence::EvidenceRef;
use capsulet_ir::correctness::obligation::{DischargeState, ObligationStatement, RepairOwner};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::definition::{AssuranceMode, Definition};
use capsulet_ir::{AssuranceVerdict, CheckerVerdict, Digest, Identity, Obligation, RecordedTime};
use capsulet_kernel::workflow::{Assembly, certify};

mod support;
use support::ir::{admission, definition, id};
use support::{fixture_id as unique_id, store};

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
                authority: None,
                accepts: Vec::new(),
            },
            contract: id("patch-compiles"),
            state: DischargeState::Discharged {
                by: id("cargo-test"),
                evidence: vec![Digest::of(content)],
                basis: capsulet_ir::correctness::obligation::DischargeBasis::Unrecorded,
            },
        }],
        evidence: vec![evidence],
        loops: vec![],
    })
    .expect("the certificate seals")
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
    assert_eq!(
        store
            .list_assurance_certificates(&tenant, &theirs, 10)
            .await
            .expect("list across projects"),
        [] as [capsulet_postgres::StoredCertificate; 0]
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
                authority: None,
                accepts: Vec::new(),
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
async fn a_models_endorsement_or_dispute_is_projected_as_outstanding() {
    // A model endorses or disputes and never discharges, so both states land in
    // the projection and both count as work still to do.
    let store = store().await;
    let tenant = unique_id("tenant");
    let project = unique_id("project");
    let definition = definition(&unique_id("definition"));
    let record = admission(&definition);
    let reading = |name: &str, state: DischargeState| Obligation {
        statement: ObligationStatement::new(
            id(name),
            format!("`{name}` reads faithfully"),
            RepairOwner::Human,
        ),
        contract: id("patch-compiles"),
        state,
    };
    let modelled = certify(Assembly {
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
        policy_version: "release-policy/4".to_string(),
        contracts: vec![id("patch-compiles")],
        verifiers: vec![VerifierRecord {
            identity: Identity::new(id("second-reader"), "1"),
            environment: Digest::of(b"a model"),
            inputs: vec![],
            outputs: vec![],
            trust: VerifierTrust::ModelJudgment {
                family: "other".to_string(),
                independent_of_proposer: true,
                calibration: None,
            },
            verdict: CheckerVerdict::Conditional,
        }],
        obligations: vec![
            reading(
                "summary-is-faithful",
                DischargeState::Endorsed {
                    by: id("second-reader"),
                    evidence: vec![],
                },
            ),
            reading(
                "clause-is-unqualified",
                DischargeState::Disputed {
                    by: id("second-reader"),
                    reason: "section 12 qualifies it".to_string(),
                    evidence: vec![],
                },
            ),
        ],
        evidence: vec![],
        loops: vec![],
    })
    .expect("the certificate seals");
    assert_eq!(modelled.verdict(), AssuranceVerdict::Conditional);

    store
        .insert_assurance_certificate(&tenant, &project, &modelled)
        .await
        .expect("record a certificate a model read");
    assert_eq!(
        store
            .count_outstanding_obligations(&tenant, &project)
            .await
            .expect("count obligations"),
        2
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

fn reasoning_record(tenant: &str, project: &str, id: &str) -> capsulet_postgres::CertificateRecord {
    let goal = capsulet_kernel::Proposition::new("either party", "may_terminate", "this agreement");
    capsulet_postgres::CertificateRecord {
        id: id.to_string(),
        tenant_id: tenant.to_string(),
        project_id: project.to_string(),
        question: "May either party terminate?".to_string(),
        certificate: capsulet_kernel::Certificate {
            verdict: capsulet_kernel::Verdict::Conditional,
            goal: goal.clone(),
            discharged: Vec::new(),
            residuals: vec![capsulet_kernel::Residual {
                kind: capsulet_kernel::ResidualKind::Source,
                from: "mentions(contract,either party|may_terminate|this agreement)".to_string(),
                to: goal,
                rationale: "qualified by provision 12".to_string(),
                evidence_ids: vec!["ev_termination".to_string()],
            }],
            errors: Vec::new(),
            replay_digest: format!("sha256:{id}"),
            derivation_depth_limit: None,
        },
        alphabet_digest: "sha256:alphabet".to_string(),
        model: "mini".to_string(),
    }
}

#[tokio::test]
async fn a_reading_is_decided_once_kept_and_kept_in_its_own_project() {
    let store = store().await;
    let tenant = unique_id("tenant");
    let project = unique_id("project");
    let record = reasoning_record(&tenant, &project, &unique_id("certificate"));
    store
        .insert_certificate(&record)
        .await
        .expect("record the certificate");
    let found = store
        .find_certificate(&tenant, &project, &record.id)
        .await
        .expect("find the certificate")
        .expect("it is in its project");
    let reading =
        capsulet_kernel::reading_digest(&found.certificate, &found.certificate.residuals[0]);
    assert_eq!(
        reading,
        capsulet_kernel::reading_digest(&record.certificate, &record.certificate.residuals[0]),
        "a reading's digest survives the round trip through the store"
    );
    assert!(
        store
            .find_certificate(&tenant, &unique_id("elsewhere"), &record.id)
            .await
            .expect("query")
            .is_none(),
        "another project does not see it"
    );

    let review = |project: &str, decision: capsulet_kernel::ReadingDecision| {
        capsulet_postgres::ReadingReviewRecord {
            tenant_id: tenant.clone(),
            project_id: project.to_string(),
            certificate_id: record.id.clone(),
            review: capsulet_kernel::ReadingReview {
                reading: reading.clone(),
                decision,
                reviewer: "dana".to_string(),
                authority: capsulet_kernel::READING_AUTHORITY.to_string(),
                rationale: "read with provision 12".to_string(),
            },
        }
    };
    assert!(
        store
            .insert_reading_review(&review(&project, capsulet_kernel::ReadingDecision::Holds))
            .await
            .expect("record the review")
    );
    assert!(
        !store
            .insert_reading_review(&review(
                &project,
                capsulet_kernel::ReadingDecision::DoesNotHold
            ))
            .await
            .expect("a second decision is reported, not written"),
    );
    // The foreign key ties a review to its certificate's own project.
    store
        .insert_reading_review(&review(
            &unique_id("elsewhere"),
            capsulet_kernel::ReadingDecision::Holds,
        ))
        .await
        .expect_err("a review in another project names no certificate there");

    let reviews = store
        .list_reading_reviews(&tenant, &project, std::slice::from_ref(&record.id))
        .await
        .expect("list reviews");
    assert_eq!(reviews.len(), 1);
    assert_eq!(
        reviews[0].review.decision,
        capsulet_kernel::ReadingDecision::Holds
    );

    let rewrite = sqlx::query(
        "UPDATE reasoning_reading_reviews SET decision = 'does_not_hold' WHERE certificate_id = $1",
    )
    .bind(&record.id)
    .execute(store.pool())
    .await
    .expect_err("a decision cannot be rewritten");
    assert!(rewrite.to_string().contains("append-only"), "{rewrite}");
}
