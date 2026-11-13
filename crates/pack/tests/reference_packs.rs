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
use capsulet_pack::{ToolReplay, decide, read, replay_tools, test};
use capsulet_verifier::ContainerTransport;
use support::{AT, change, proposer, sealed, state};

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
