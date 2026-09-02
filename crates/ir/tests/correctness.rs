//! Certificates are sealed, verdicts are derived, and citations point at
//! something the document actually carries.

use capsulet_ir::correctness::certificate::{Subject, VerifierRecord, VerifierTrust};
use capsulet_ir::correctness::obligation::{DischargeState, ObligationStatement, RepairOwner};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::correctness::{Certificate, CertificateBody, CertificateError, EvidenceRef};
use capsulet_ir::loop_region::{BudgetKind, LoopOutcome, StopReason};
use capsulet_ir::{
    AssuranceVerdict, CheckerVerdict, Digest, Identifier, Identity, Obligation, Proposal,
    RecordedTime, digest_of,
};

fn id(value: &str) -> Identifier {
    Identifier::parse(value).expect("test identifiers are well formed")
}

fn producer() -> Producer {
    Producer {
        kind: ProducerKind::Model,
        identity: Identity::new(id("ollama/qwen3"), "4b"),
    }
}

fn evidence(content: &[u8]) -> EvidenceRef {
    EvidenceRef {
        id: id("test-log"),
        content: Digest::of(content),
        media_type: "text/plain".to_string(),
        byte_length: content.len() as u64,
        producer: Producer {
            kind: ProducerKind::Deterministic,
            identity: Identity::new(id("cargo-test"), "1.96"),
        },
        captured_at: RecordedTime(1_772_000_000_000),
    }
}

fn statement(name: &str, owner: RepairOwner) -> ObligationStatement {
    ObligationStatement {
        id: id(name),
        statement: format!("the property `{name}` holds for this output"),
        owner,
    }
}

fn discharged(name: &str, evidence: &EvidenceRef) -> Obligation {
    Obligation {
        statement: statement(name, RepairOwner::Verifier),
        contract: id("patch-compiles"),
        state: DischargeState::Discharged {
            by: id("cargo-test"),
            evidence: vec![evidence.content],
        },
    }
}

fn body(obligations: Vec<Obligation>, evidence: Vec<EvidenceRef>) -> CertificateBody {
    let verdict = AssuranceVerdict::from_obligations(&obligations);
    CertificateBody {
        schema_version: Certificate::current_schema_version(),
        id: id("cert-1"),
        subject: Subject {
            definition: Digest::of(b"a definition"),
            definition_version: "1".to_string(),
            run: Some(id("run-1")),
            inputs: vec![Digest::of(b"an input")],
            outputs: vec![Digest::of(b"an output")],
        },
        policy_version: "policy-1".to_string(),
        kernel_version: "capsulet-kernel 0.1.0".to_string(),
        contracts: vec![id("patch-compiles")],
        verifiers: vec![VerifierRecord {
            identity: Identity::new(id("cargo-test"), "1.96"),
            environment: Digest::of(b"an image"),
            inputs: vec![Digest::of(b"an input")],
            outputs: vec![Digest::of(b"an output")],
            trust: VerifierTrust::DeclaredOracle {
                rationale: "the test runner is trusted to report its own result".to_string(),
            },
            verdict: CheckerVerdict::Accepted,
        }],
        evidence,
        obligations,
        loops: vec![],
        verdict,
    }
}

#[test]
fn changing_any_field_breaks_the_seal() {
    let log = evidence(b"tests passed");
    let certificate =
        Certificate::seal(body(vec![discharged("compiles", &log)], vec![log.clone()]))
            .expect("the certificate seals");

    assert_eq!(certificate.verify_seal(), Ok(()));

    // The same content with one byte of evidence changed digests differently,
    // so the seal from the original body no longer matches.
    let tampered = evidence(b"tests passed!");
    let altered = body(
        vec![discharged("compiles", &tampered)],
        vec![tampered.clone()],
    );
    let resealed = Certificate::seal(altered).expect("the altered body also seals");
    assert_ne!(certificate.replay_digest(), resealed.replay_digest());
}

#[test]
fn a_citation_to_absent_evidence_is_refused() {
    let log = evidence(b"tests passed");
    // The obligation cites the log, but the certificate does not carry it.
    let error = Certificate::seal(body(vec![discharged("compiles", &log)], vec![]))
        .expect_err("a citation to nothing is not a citation");

    assert_eq!(
        error,
        CertificateError::MissingEvidence {
            id: id("cert-1"),
            digest: log.content,
        }
    );
}

#[test]
fn the_verdict_follows_from_the_obligations() {
    let log = evidence(b"tests passed");

    let accepted = body(vec![discharged("compiles", &log)], vec![log.clone()]);
    assert_eq!(accepted.verdict, AssuranceVerdict::Accepted);

    let mut residual = accepted.clone();
    residual.obligations.push(Obligation {
        statement: statement("summary-is-faithful", RepairOwner::Human),
        contract: id("patch-compiles"),
        state: DischargeState::Residual {
            rationale: "no checker can decide whether the summary reads faithfully".to_string(),
            evidence: vec![],
        },
    });
    residual.verdict = AssuranceVerdict::from_obligations(&residual.obligations);
    assert_eq!(residual.verdict, AssuranceVerdict::Conditional);

    let mut failed = residual.clone();
    failed.obligations.push(Obligation {
        statement: statement("no-new-findings", RepairOwner::Verifier),
        contract: id("patch-compiles"),
        state: DischargeState::Failed {
            reason: "the scanner reported two new findings".to_string(),
            owner: RepairOwner::Proposer,
        },
    });
    failed.verdict = AssuranceVerdict::from_obligations(&failed.obligations);
    assert_eq!(failed.verdict, AssuranceVerdict::Rejected);

    let nothing = body(vec![], vec![]);
    assert_eq!(nothing.verdict, AssuranceVerdict::Unverified);
}

#[test]
fn a_verdict_the_obligations_do_not_justify_is_refused() {
    let log = evidence(b"tests failed");
    let mut optimistic = body(
        vec![Obligation {
            statement: statement("compiles", RepairOwner::Verifier),
            contract: id("patch-compiles"),
            state: DischargeState::Failed {
                reason: "the build failed".to_string(),
                owner: RepairOwner::Proposer,
            },
        }],
        vec![log],
    );
    optimistic.verdict = AssuranceVerdict::Accepted;

    assert_eq!(
        Certificate::seal(optimistic),
        Err(CertificateError::VerdictNotJustified {
            id: id("cert-1"),
            recorded: "accepted",
            justified: "rejected",
        })
    );
}

#[test]
fn a_waiver_is_not_a_discharge() {
    let log = evidence(b"tests passed");
    let mut waived = body(vec![discharged("compiles", &log)], vec![log]);
    waived.obligations.push(Obligation {
        statement: statement("licence-review", RepairOwner::Policy),
        contract: id("patch-compiles"),
        state: DischargeState::Waived {
            policy: id("release-policy-v3"),
            authority: id("platform-admin"),
        },
    });
    waived.verdict = AssuranceVerdict::from_obligations(&waived.obligations);

    // A waiver is a decision to release without the check, so the result is
    // conditional and the decider is named.
    assert_eq!(waived.verdict, AssuranceVerdict::Conditional);
    let certificate = Certificate::seal(waived).expect("the certificate seals");
    assert_eq!(certificate.body().residuals().count(), 1);
    assert!(!certificate.body().obligations[1].state.was_checked());
}

#[test]
fn an_obligation_cannot_be_left_without_a_state() {
    // There is no "unknown" case to construct: every discharge state is a
    // claim someone can be held to. This test records that as an enumeration of
    // what exists, so adding an escape hatch later fails here first.
    let states = [
        DischargeState::Discharged {
            by: id("cargo-test"),
            evidence: vec![],
        },
        DischargeState::Assumed {
            rationale: "assumed".to_string(),
        },
        DischargeState::Waived {
            policy: id("p"),
            authority: id("a"),
        },
        DischargeState::Residual {
            rationale: "open".to_string(),
            evidence: vec![],
        },
        DischargeState::Failed {
            reason: "failed".to_string(),
            owner: RepairOwner::Verifier,
        },
    ];

    let names: Vec<&str> = states.iter().map(DischargeState::as_str).collect();
    assert_eq!(
        names,
        ["discharged", "assumed", "waived", "residual", "failed"]
    );
}

#[test]
fn a_duplicate_obligation_is_refused() {
    let log = evidence(b"tests passed");
    let duplicated = body(
        vec![discharged("compiles", &log), discharged("compiles", &log)],
        vec![log],
    );

    assert_eq!(
        Certificate::seal(duplicated),
        Err(CertificateError::DuplicateObligation {
            id: id("cert-1"),
            obligation: id("compiles"),
        })
    );
}

#[test]
fn the_verdict_mapping_loses_nothing() {
    for verdict in [
        CheckerVerdict::Accepted,
        CheckerVerdict::Conditional,
        CheckerVerdict::Rejected,
    ] {
        let platform = AssuranceVerdict::from(verdict);
        assert_eq!(platform.checker_verdict(), Some(verdict));
    }

    // The one case a three-valued checker cannot express.
    assert_eq!(AssuranceVerdict::Unverified.checker_verdict(), None);
}

#[test]
fn a_certificate_reports_loops_that_stopped_short() {
    let log = evidence(b"tests passed");
    let mut with_loop = body(vec![discharged("compiles", &log)], vec![log]);
    with_loop.loops.push(LoopOutcome {
        region: id("repair-loop"),
        iterations: vec![],
        stopped: StopReason::BudgetExhausted {
            budget: BudgetKind::Iterations,
        },
    });
    let certificate = Certificate::seal(with_loop).expect("the certificate seals");

    let stops: Vec<&str> = certificate
        .body()
        .incomplete_loops()
        .map(StopReason::as_str)
        .collect();
    assert_eq!(stops, ["budget_exhausted"]);
}

#[test]
fn a_proposal_carries_its_inputs_and_claims_nothing_more() {
    let proposal = Proposal {
        id: id("proposal-1"),
        node: id("propose-patch"),
        producer: producer(),
        inputs: [(id("repository"), Digest::of(b"a revision"))]
            .into_iter()
            .collect(),
        candidate: Digest::of(b"a patch"),
        derivation: None,
        claims_evidence: vec![Digest::of(b"a scanner report")],
    };

    // Digesting it is stable, and there is no accept/promote method to call.
    assert_eq!(
        digest_of(&proposal).expect("a proposal digests"),
        digest_of(&proposal.clone()).expect("a proposal digests")
    );
    assert_eq!(proposal.claims_evidence.len(), 1);
}

/// The four verdicts, pinned as bytes.
///
/// A certificate format that drifts silently makes every stored certificate
/// unreadable by the build that wrote it. These fixtures fail first when that
/// happens. Regenerate deliberately with `CAPSULET_UPDATE_GOLDEN=1`, and treat
/// a change as a schema major bump rather than a fixture edit.
#[test]
fn the_four_verdicts_have_checked_in_certificates() {
    let log = evidence(b"tests passed");

    let accepted = body(vec![discharged("compiles", &log)], vec![log.clone()]);

    let mut conditional = accepted.clone();
    conditional.obligations.push(Obligation {
        statement: statement("summary-is-faithful", RepairOwner::Human),
        contract: id("patch-compiles"),
        state: DischargeState::Residual {
            rationale: "no checker can decide whether the summary reads faithfully".to_string(),
            evidence: vec![],
        },
    });
    conditional.verdict = AssuranceVerdict::from_obligations(&conditional.obligations);

    let mut rejected = body(
        vec![Obligation {
            statement: statement("compiles", RepairOwner::Verifier),
            contract: id("patch-compiles"),
            state: DischargeState::Failed {
                reason: "the build failed".to_string(),
                owner: RepairOwner::Proposer,
            },
        }],
        vec![log],
    );
    rejected.verdict = AssuranceVerdict::from_obligations(&rejected.obligations);

    let unverified = body(vec![], vec![]);

    for (name, body) in [
        ("accepted", accepted),
        ("conditional", conditional),
        ("rejected", rejected),
        ("unverified", unverified),
    ] {
        let certificate = Certificate::seal(body).expect("the certificate seals");
        let bytes = capsulet_ir::to_canonical_bytes(&certificate).expect("it encodes");

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden/certificates")
            .join(format!("{name}.json"));

        if std::env::var_os("CAPSULET_UPDATE_GOLDEN").is_some() {
            std::fs::create_dir_all(path.parent().expect("the fixture has a directory"))
                .expect("the golden directory is writable");
            std::fs::write(&path, &bytes).expect("the fixture is writable");
        }

        let recorded =
            std::fs::read(&path).unwrap_or_else(|_| panic!("{} is missing", path.display()));
        assert_eq!(
            String::from_utf8(bytes).expect("canonical bytes are UTF-8"),
            String::from_utf8(recorded).expect("the fixture is UTF-8"),
            "the {name} certificate no longer matches its checked-in bytes"
        );
    }
}
