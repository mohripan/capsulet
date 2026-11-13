//! The reference packs pass their own tests, in real containers.
//!
//! Built only with the `container-tests` feature, because each pack's
//! validators run in a container; the `verifier-isolation` gate enables it and
//! holds every test here to running. What the packs answer, and how, is tested
//! without Docker beside each pack's own test file.

mod support;

use std::path::{Path, PathBuf};

use capsulet_ir::AssuranceVerdict;
use capsulet_ir::correctness::obligation::{DischargeState, RepairOwner};
use capsulet_kernel::{Bundle, EvidenceSource, ReplayOutcome, replay};
use capsulet_pack::{ToolReplay, Waiver, decide, read, replay_tools, test, waive};
use capsulet_verifier::ContainerTransport;
use support::fixtures::id;
use support::{AT, change, plan, proposer, sealed, state};

fn pack(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs")
        .join(name)
}

fn passes_its_own_test(name: &str) {
    let pack = read(&pack(name)).expect("the pack reads");
    let reports = test(&pack).expect("its validators read");
    assert!(!reports.is_empty(), "{name} registers no validator");
    for report in &reports {
        assert!(
            !report.cases.is_empty(),
            "{} has no cases",
            report.registration
        );
        for case in &report.cases {
            assert!(
                case.passed,
                "{name}: {}: {}: {}",
                report.registration, case.name, case.detail
            );
            assert!(case.detail.contains("(isolated)"), "{}", case.detail);
        }
    }
}

#[test]
fn the_contract_clauses_pack_passes_its_own_test() {
    passes_its_own_test("contract-clauses");
}

#[test]
fn the_infrastructure_policy_pack_passes_its_own_test() {
    passes_its_own_test("infrastructure-policy");
}

#[test]
fn the_code_changes_pack_passes_its_own_test() {
    passes_its_own_test("code-changes");
}

#[test]
fn in_containers_a_broken_build_is_rejected_with_its_output_pinned_and_replays_to_the_same_verdict()
{
    let pack = read(&pack("code-changes")).expect("the pack reads");
    let transport = ContainerTransport::new();
    let decision =
        decide(&pack, &change("broken-build"), &transport, &proposer(), AT).expect("decides");
    for broken in ["builds", "existing-tests-pass"] {
        assert!(
            matches!(
                state(&decision, broken),
                DischargeState::Failed {
                    owner: RepairOwner::Proposer,
                    ..
                }
            ),
            "{broken}: {:?}",
            state(&decision, broken)
        );
    }
    let certificate = sealed(&pack, &decision);
    assert_eq!(certificate.verdict(), AssuranceVerdict::Rejected);

    let bundle = Bundle::read(
        &Bundle::build(certificate, &decision.blobs)
            .expect("bundles")
            .to_canonical_bytes()
            .expect("encodes"),
    )
    .expect("reads back");
    let evidence = bundle.evidence().expect("its evidence");
    let log = bundle
        .certificate
        .body()
        .evidence
        .iter()
        .find(|reference| reference.id.as_str() == "py-compile/compile.log")
        .expect("the compiler's output is cited");
    let said = String::from_utf8_lossy(evidence.bytes(&log.content).expect("carried")).into_owned();
    assert!(said.contains("FAILED calc.py"), "{said}");

    // Offline, the certificate re-derives its verdict from what it carries.
    assert!(
        matches!(
            replay(&bundle.certificate, &evidence),
            ReplayOutcome::Reproduced {
                verdict: AssuranceVerdict::Rejected,
                ..
            }
        ),
        "the bundle replays offline to the verdict it records"
    );
    // And each tool, run again in the environment its record pins, reaches it.
    let again = replay_tools(&pack, &bundle.certificate, &evidence, &transport).expect("replays");
    assert_eq!(again.len(), 2);
    assert!(again.iter().all(ToolReplay::agrees), "{again:?}");
}

#[test]
fn in_containers_a_sound_fix_waits_only_for_its_review() {
    let pack = read(&pack("code-changes")).expect("the pack reads");
    let transport = ContainerTransport::new();
    let decision = decide(&pack, &change("fix"), &transport, &proposer(), AT).expect("decides");
    for tool in [
        "builds",
        "existing-tests-pass",
        "new-tests-pass-on-change",
        "new-tests-fail-on-base",
    ] {
        assert!(
            matches!(state(&decision, tool), DischargeState::Discharged { .. }),
            "{tool}: {:?}",
            state(&decision, tool)
        );
    }
    assert!(matches!(
        state(&decision, "does-what-was-asked"),
        DischargeState::Residual { .. }
    ));
    let certificate = sealed(&pack, &decision);
    assert_eq!(certificate.verdict(), AssuranceVerdict::Conditional);
}

#[test]
fn in_containers_a_public_bucket_is_rejected_naming_the_rule_and_a_waiver_makes_it_conditional() {
    let pack = read(&pack("infrastructure-policy")).expect("the pack reads");
    let transport = ContainerTransport::new();
    let mut decision =
        decide(&pack, &plan("public-bucket"), &transport, &proposer(), AT).expect("decides");
    let DischargeState::Failed { reason, owner } = state(&decision, "no-public-buckets").clone()
    else {
        panic!("{:?}", state(&decision, "no-public-buckets"));
    };
    assert_eq!(owner, RepairOwner::Proposer);
    assert_eq!(
        reason,
        "no-public-buckets: aws_s3_bucket_acl.docs grants public-read"
    );
    for held in [
        "storage-encrypted",
        "no-open-admin-ports",
        "within-declared-scope",
    ] {
        assert!(
            matches!(state(&decision, held), DischargeState::Discharged { .. }),
            "{held}: {:?}",
            state(&decision, held)
        );
    }
    assert_eq!(
        sealed(&pack, &decision).verdict(),
        AssuranceVerdict::Rejected
    );

    let unused = waive(
        &mut decision,
        &[Waiver {
            id: id("docs-site-is-public"),
            obligation: id("no-public-buckets"),
            authority: id("review:security"),
            by: id("dana"),
            reason: "The documentation site is public by design.".to_string(),
            failure: reason,
        }],
        AT,
    )
    .expect("waives");
    assert!(unused.is_empty(), "{unused:?}");
    assert!(matches!(
        state(&decision, "no-public-buckets"),
        DischargeState::Waived { authority, .. } if authority.as_str() == "review:security"
    ));
    let certificate = sealed(&pack, &decision);
    assert_eq!(certificate.verdict(), AssuranceVerdict::Conditional);

    let bundle = Bundle::read(
        &Bundle::build(certificate, &decision.blobs)
            .expect("bundles")
            .to_canonical_bytes()
            .expect("encodes"),
    )
    .expect("reads back");
    let evidence = bundle.evidence().expect("its evidence");
    assert!(
        matches!(
            replay(&bundle.certificate, &evidence),
            ReplayOutcome::Reproduced {
                verdict: AssuranceVerdict::Conditional,
                ..
            }
        ),
        "the bundle replays offline to the verdict it records"
    );
    // OPA and the scanner, run again with the plan and the policies the
    // bundle carries, reach their records.
    let again = replay_tools(&pack, &bundle.certificate, &evidence, &transport).expect("replays");
    assert_eq!(again.len(), 2);
    assert!(again.iter().all(ToolReplay::agrees), "{again:?}");
}

#[test]
fn in_containers_a_compliant_change_is_accepted_and_one_outside_its_scope_is_rejected() {
    let pack = read(&pack("infrastructure-policy")).expect("the pack reads");
    let transport = ContainerTransport::new();
    let compliant =
        decide(&pack, &plan("compliant"), &transport, &proposer(), AT).expect("decides");
    assert_eq!(
        sealed(&pack, &compliant).verdict(),
        AssuranceVerdict::Accepted
    );

    let outside =
        decide(&pack, &plan("outside-scope"), &transport, &proposer(), AT).expect("decides");
    let DischargeState::Failed { reason, owner } = state(&outside, "within-declared-scope") else {
        panic!("{:?}", state(&outside, "within-declared-scope"));
    };
    assert_eq!(*owner, RepairOwner::Proposer);
    assert!(
        reason.contains("delete aws_db_instance.legacy: the scope does not name it")
            && reason.contains("replace aws_instance.web: the scope allows only update"),
        "{reason}"
    );
    let inventory = outside
        .evidence
        .iter()
        .find(|reference| reference.id.as_str() == "plan-scope/scope.log")
        .expect("the inventory is cited");
    let said = String::from_utf8_lossy(outside.blobs.bytes(&inventory.content).expect("kept"))
        .into_owned();
    assert!(said.contains("OUTSIDE"), "{said}");
    assert_eq!(
        sealed(&pack, &outside).verdict(),
        AssuranceVerdict::Rejected
    );
}
