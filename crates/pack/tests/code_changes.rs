//! The code-changes reference pack (#160), decided and replayed without Docker.
//!
//! A scripted transport stands in for the containers, so these tests are about
//! what the pack crate does with its validators' answers: which obligations
//! each decides, what a certificate sealed from them says, what its bundle
//! carries, and what running the tools again from that bundle finds. The same
//! pack runs in real containers in `tests/reference_packs.rs`.

mod support;

use std::fs;
use std::path::Path;

use capsulet_ir::AssuranceVerdict;
use capsulet_ir::correctness::certificate::VerifierTrust;
use capsulet_ir::correctness::obligation::{DischargeState, RepairOwner};
use capsulet_kernel::{Bundle, EvidenceSource};
use capsulet_pack::{ToolReplay, admit, decide, files_of, generate, read, replay_tools, sign};
use capsulet_verifier::{RawRun, Registration, Transport, VerifierFailure};
use support::{AT, change, copy, pack_dir, proposer, sealed, state};

/// Answers as the pack's two validators would, from what the change holds, and
/// leaves a log behind the way they do. `shift` changes what the compiler
/// writes, to stand for a tool that does not reproduce.
struct Scripted {
    shift: bool,
}

impl Transport for Scripted {
    fn run(
        &self,
        registration: &Registration,
        workdir: &Path,
        request: &[u8],
    ) -> Result<RawRun, String> {
        let request: serde_json::Value =
            serde_json::from_slice(request).map_err(|e| e.to_string())?;
        let calc =
            fs::read_to_string(workdir.join("inputs/change/calc.py")).map_err(|e| e.to_string())?;
        let broken = calc.contains("def mean(values)\n");
        let regressed = calc.contains("return a - b");
        let (log, text) = if registration.name == "py-compile" {
            let text = if broken {
                "FAILED calc.py\ninvalid syntax"
            } else {
                "ok calc.py"
            };
            let again = if self.shift { " (again)" } else { "" };
            ("compile.log", format!("{text}{again}"))
        } else {
            (
                "tests-existing.log",
                format!("broken={broken} regressed={regressed}"),
            )
        };
        fs::write(workdir.join("outputs").join(log), text).map_err(|e| e.to_string())?;
        let results: Vec<serde_json::Value> = request["obligations"]
            .as_array()
            .expect("obligations")
            .iter()
            .map(|obligation| {
                let id = obligation["id"].as_str().expect("an id");
                let holds = match id {
                    "builds" | "new-tests-pass-on-change" => !broken,
                    "existing-tests-pass" => !broken && !regressed,
                    _ => true,
                };
                serde_json::json!({
                    "obligation": id,
                    "contract": obligation["contract"],
                    "outcome": if holds { "holds" } else { "does_not_hold" },
                    "reason": format!("{id} {}", if holds { "holds" } else { "does not hold" }),
                })
            })
            .collect();
        Ok(RawRun {
            exit: Some(0),
            timed_out: false,
            stdout: serde_json::to_vec(
                &serde_json::json!({"protocol": "capsulet.verifier/v1", "results": results}),
            )
            .expect("encodes"),
            stderr_tail: String::new(),
        })
    }
}

/// A transport that cannot start anything.
struct Unreachable;

impl Transport for Unreachable {
    fn run(&self, _: &Registration, _: &Path, _: &[u8]) -> Result<RawRun, String> {
        Err("no container runtime".to_string())
    }
}

#[test]
fn a_change_that_breaks_the_build_is_rejected_owned_by_the_proposer_with_its_output_pinned() {
    let pack = read(&pack_dir("code-changes")).expect("the pack reads");
    let decision = decide(
        &pack,
        &change("broken-build"),
        &Scripted { shift: false },
        &proposer(),
        AT,
    )
    .expect("decides");

    assert!(
        matches!(
            state(&decision, "builds"),
            DischargeState::Failed {
                owner: RepairOwner::Proposer,
                ..
            }
        ),
        "{:?}",
        state(&decision, "builds")
    );
    let certificate = sealed(&pack, &decision);
    assert_eq!(certificate.verdict(), AssuranceVerdict::Rejected);

    // The compiler's own words are pinned: by digest in its record, as evidence
    // the certificate cites, and as bytes its bundle carries.
    let compiler = &certificate.body().verifiers[0];
    assert_eq!(compiler.identity.name.as_str(), "py-compile");
    assert!(matches!(
        compiler.trust,
        VerifierTrust::DeclaredOracle { .. }
    ));
    let log = certificate
        .body()
        .evidence
        .iter()
        .find(|reference| reference.id.as_str() == "py-compile/compile.log")
        .expect("the compile log is cited");
    assert!(compiler.outputs.contains(&log.content));
    let bundle = Bundle::build(certificate.clone(), &decision.blobs).expect("bundles");
    let carried = bundle.evidence().expect("reads");
    let bytes = carried.bytes(&log.content).expect("carried");
    assert!(String::from_utf8_lossy(bytes).contains("FAILED calc.py"));
}

#[test]
fn a_change_that_breaks_a_test_is_rejected_and_a_sound_one_waits_for_its_review() {
    let pack = read(&pack_dir("code-changes")).expect("the pack reads");
    let regressed = decide(
        &pack,
        &change("breaks-a-test"),
        &Scripted { shift: false },
        &proposer(),
        AT,
    )
    .expect("decides");
    assert!(matches!(
        state(&regressed, "builds"),
        DischargeState::Discharged { .. }
    ));
    assert!(matches!(
        state(&regressed, "existing-tests-pass"),
        DischargeState::Failed {
            owner: RepairOwner::Proposer,
            ..
        }
    ));
    assert_eq!(
        sealed(&pack, &regressed).verdict(),
        AssuranceVerdict::Rejected
    );

    let fix = decide(
        &pack,
        &change("fix"),
        &Scripted { shift: false },
        &proposer(),
        AT,
    )
    .expect("decides");
    for tool in [
        "builds",
        "existing-tests-pass",
        "new-tests-pass-on-change",
        "new-tests-fail-on-base",
    ] {
        assert!(
            matches!(state(&fix, tool), DischargeState::Discharged { .. }),
            "{tool}"
        );
    }
    let DischargeState::Residual { rationale, .. } = state(&fix, "does-what-was-asked") else {
        panic!("whether it does what was asked is a review");
    };
    assert!(rationale.contains("review:code"), "{rationale}");
    assert_eq!(sealed(&pack, &fix).verdict(), AssuranceVerdict::Conditional);
}

#[test]
fn a_tool_that_cannot_run_decides_nothing_and_says_why() {
    let pack = read(&pack_dir("code-changes")).expect("the pack reads");
    let decision = decide(&pack, &change("fix"), &Unreachable, &proposer(), AT).expect("decides");
    let DischargeState::Residual { rationale, .. } = state(&decision, "builds") else {
        panic!("an unrun tool leaves its obligation residual");
    };
    assert!(rationale.contains("transport_failed"), "{rationale}");
    assert!(decision.verifiers.is_empty(), "{:?}", decision.verifiers);
    assert!(
        decision
            .runs
            .iter()
            .all(|run| matches!(run.outcome, Err(VerifierFailure::Transport { .. })))
    );
}

#[test]
fn the_tools_run_again_from_the_bundle_and_reach_their_records_or_say_they_do_not() {
    let pack = read(&pack_dir("code-changes")).expect("the pack reads");
    let decision = decide(
        &pack,
        &change("broken-build"),
        &Scripted { shift: false },
        &proposer(),
        AT,
    )
    .expect("decides");
    let certificate = sealed(&pack, &decision);
    let bundle = Bundle::read(
        &Bundle::build(certificate, &decision.blobs)
            .expect("bundles")
            .to_canonical_bytes()
            .expect("encodes"),
    )
    .expect("reads back");
    let evidence = bundle.evidence().expect("its evidence");

    let again = replay_tools(
        &pack,
        &bundle.certificate,
        &evidence,
        &Scripted { shift: false },
    )
    .expect("replays");
    assert_eq!(again.len(), 2, "both tools run again");
    assert!(again.iter().all(ToolReplay::agrees), "{again:?}");

    let drifted = replay_tools(
        &pack,
        &bundle.certificate,
        &evidence,
        &Scripted { shift: true },
    )
    .expect("replays");
    let compiler = drifted
        .iter()
        .find(|replay| replay.validator == "py-compile 1")
        .expect("ran");
    let differences = &compiler.outcome.as_ref().expect("ran").differences;
    assert!(
        differences.iter().any(|d| d.contains("outputs")),
        "{differences:?}"
    );
}

#[test]
fn each_registration_runs_the_script_beside_it_and_the_pack_installs_once_signed() {
    let root = pack_dir("code-changes");
    for (registration, script) in [
        (
            "validators/py-compile.json",
            "validators/src/py_compile_check.py",
        ),
        (
            "validators/py-unittest.json",
            "validators/src/py_unittest_check.py",
        ),
    ] {
        let registration: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(registration)).expect("reads"))
                .expect("json");
        let command = &registration["environment"]["command"];
        assert_eq!(command[0], "python");
        assert_eq!(command[1], "-c");
        assert_eq!(
            command[2].as_str(),
            Some(
                fs::read_to_string(root.join(script))
                    .expect("reads")
                    .as_str()
            ),
            "{registration} runs {script} as it is: regenerate the registration"
        );
    }

    let committed = read(&root).expect("the pack reads");
    assert_eq!(
        committed.manifest.files,
        files_of(&root).expect("its files"),
        "packs/code-changes/pack.json lists every file as it is: run `capsulet pack refresh`"
    );
    let directory = tempfile::tempdir().expect("dir");
    copy(&root, directory.path());
    let (key, _) = generate().expect("a key");
    sign(directory.path(), &key).expect("signs");
    let mut trust = capsulet_pack::TrustStore::new();
    trust.keys.push(
        key.trusted_for(vec!["code-changes".to_string()])
            .expect("a public key"),
    );
    let admitted = admit(&read(directory.path()).expect("reads"), &trust, &[]).expect("installs");
    assert_eq!(
        (admitted.name.as_str(), admitted.version.as_str()),
        ("code-changes", "1.0.0")
    );
}
