//! Gaps between what the assurance layer claims and what it enforces.
//!
//! **Every test in this file asserts the wrong behaviour on purpose.** Each one
//! passing is the bug. They exist so that a gap which is currently invisible —
//! because nothing fails — becomes something CI holds a shape for.
//!
//! These are tripwires, not specifications. Fixing a gap makes its test fail;
//! the task that fixes it inverts the assertion and updates
//! `docs/contracts/product-claims.json` in the same commit. Do not write new
//! code against anything asserted here.
//!
//! Tracked by `docs/superpowers/plans/2026-09-06-correctness-kernel-robustness.md`.

mod fixtures;

use std::collections::BTreeMap;

use capsulet_ir::assurance::{BoundaryDecision, BoundaryPolicy};
use capsulet_ir::correctness::certificate::Subject;
use capsulet_ir::correctness::obligation::{DischargeState, ObligationStatement, RepairOwner};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::correctness::{Certificate, CertificateBody, EvidenceRef};
use capsulet_ir::digest::Digest;
use capsulet_ir::{
    AssuranceMode, AssurancePolicy, AssuranceVerdict, Identity, Obligation, RecordedTime, admit,
    decide_boundary,
};

use fixtures::{definition_in, id};

/// Gap 1: a boundary's required contract is satisfied by *listing* it.
///
/// `decide_boundary` checks `body.contracts.contains(contract)`. That list is
/// written by whoever assembled the certificate. Nothing checks that any
/// obligation is about the contract, and nothing consults the contract's own
/// `obligations` to see whether they were covered — even though `Contract`
/// carries them and `Definition::contract` resolves them. The gate cannot look,
/// because it is handed only the definition's digest.
///
/// Fixed by Task 3.
#[test]
fn gap_a_required_contract_is_covered_by_naming_it() {
    let mode = AssuranceMode::Enforce;
    let definition = definition_in(mode);
    let admission = admit(&definition).expect("admitted");

    let content = b"an unrelated log";
    let evidence = EvidenceRef {
        id: id("unrelated-log"),
        content: Digest::of(content),
        media_type: "text/plain".to_string(),
        byte_length: content.len() as u64,
        producer: Producer {
            kind: ProducerKind::Deterministic,
            identity: Identity::new(id("some-tool"), "1.0"),
        },
        captured_at: RecordedTime(1_772_000_000_000),
    };

    // The one obligation this run discharged is about spelling. It says nothing
    // whatever about `no-secrets-leaked`.
    let obligations = vec![Obligation {
        statement: ObligationStatement {
            id: id("spelling-is-british"),
            statement: "the prose uses British spelling".to_string(),
            owner: RepairOwner::Verifier,
        },
        contract: id("house-style"),
        state: DischargeState::Discharged {
            by: id("some-tool"),
            evidence: vec![evidence.content],
        },
    }];

    let certificate = Certificate::seal(CertificateBody {
        schema_version: Certificate::current_schema_version(),
        id: id("cert-known-gap"),
        admission: admission.clone(),
        mode,
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: Some(id("run-known-gap")),
            inputs: vec![],
            outputs: vec![],
        },
        policy_version: "release-policy/3".to_string(),
        kernel_version: "capsulet-kernel 0.1.0".to_string(),
        // Listing the contract is the whole trick.
        contracts: vec![id("house-style"), id("no-secrets-leaked")],
        verifiers: vec![],
        evidence: vec![evidence],
        obligations,
        loops: vec![],
        verdict: AssuranceVerdict::Accepted,
    })
    .expect("the body seals, which is itself part of the gap");

    let policy = AssurancePolicy {
        id: id("release-policy"),
        version: "3".to_string(),
        mode,
        // Dead field: declared here, set by tests, read by no decision.
        required_contracts: vec![id("no-secrets-leaked")],
        required_verifiers: vec![],
        boundaries: BTreeMap::from([(
            id("publish"),
            BoundaryPolicy {
                minimum: AssuranceVerdict::Accepted,
                contract: Some(id("no-secrets-leaked")),
                requires_approval: None,
            },
        )]),
        waiver_authorities: vec![],
        trust_routes: vec![],
    };

    let decision = decide_boundary(
        &policy,
        mode,
        admission.definition(),
        Some(&certificate),
        &id("publish"),
    );

    assert_eq!(
        decision,
        BoundaryDecision::Allowed {
            verdict: AssuranceVerdict::Accepted
        },
        "GAP: the boundary opened for a certificate that proved nothing about the contract it \
         required. Task 3 makes this a denial naming the uncovered statements."
    );
}
