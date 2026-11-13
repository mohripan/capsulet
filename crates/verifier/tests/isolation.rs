//! A validator in a real container, and what the container lets it reach.
//!
//! Built only with the `container-tests` feature, which the `verifier-isolation`
//! gate enables and holds to every test running: these need Docker, and a test
//! that could not reach it fails rather than passing by skipping.
//!
//! The validator is a shell script in a small image pinned by digest. It tries
//! what a validator must not be able to do — reach the network, write its
//! inputs, write its own root — and reports what happened, so the tests read the
//! isolation from inside the container rather than trusting the flags.

#[path = "../../ir/tests/fixtures/mod.rs"]
mod fixtures;

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use capsulet_ir::correctness::certificate::VerifierTrust;
use capsulet_ir::correctness::obligation::{
    DischargeBasis, DischargeState, ObligationStatement, RepairOwner,
};
use capsulet_ir::{CheckerVerdict, Digest};
use capsulet_verifier::{
    ContainerTransport, Environment, Input, Limits, RegisteredBasis, Registration, Requested,
    rerun, verify,
};

use fixtures::id;

/// busybox 1.37.0, by the digest of its multi-platform index.
const IMAGE: &str =
    "busybox@sha256:a7dbc319b1438115a207e11930dc4bb3e643dcb3ec0bdd694bb36cd8fad289ec";

/// Probes its sandbox, writes a report, and answers the one obligation it is
/// asked, `probe`, with what it found.
const PROBE: &str = r#"cat >/dev/null
if wget -q -T 2 -O /dev/null http://1.1.1.1 2>/dev/null; then n=reached; else n=blocked; fi
if (echo x > /inputs/patch.diff) 2>/dev/null; then i=writable; else i=read-only; fi
if (echo x > /etc/probe) 2>/dev/null; then r=writable; else r=read-only; fi
echo "checked the patch" > /outputs/report.txt
printf '{"protocol":"capsulet.verifier/v1","results":[{"obligation":"probe","contract":"isolation","outcome":"holds","reason":"network=%s inputs=%s root=%s"}]}' "$n" "$i" "$r"
"#;

fn registration(script: &str) -> Registration {
    Registration {
        name: "isolation-probe".to_string(),
        version: "1".to_string(),
        basis: RegisteredBasis::ReplayableTool,
        environment: Environment::Container {
            image: IMAGE.to_string(),
            command: vec!["sh".to_string(), "-c".to_string(), script.to_string()],
        },
        capabilities: Vec::new(),
        limits: Limits {
            timeout_ms: 60_000,
            memory_mb: 64,
            max_output_bytes: 64 * 1024,
        },
        parameters: BTreeMap::new(),
    }
}

fn requested() -> Vec<Requested> {
    vec![Requested {
        contract: id("isolation"),
        statement: ObligationStatement::new(
            id("probe"),
            "the validator cannot reach what it must not",
            RepairOwner::Verifier,
        ),
    }]
}

fn inputs() -> Vec<Input> {
    vec![Input {
        name: "patch.diff".to_string(),
        bytes: b"--- a\n+++ b\n".to_vec(),
        media_type: "text/x-diff".to_string(),
    }]
}

/// One test at a time: each checks that the containers it started are gone, and
/// another test's container still running would read as one of its own.
static DOCKER: Mutex<()> = Mutex::new(());

/// Docker has to be here: a missing daemon fails the test rather than skipping it.
/// Holds the lock for the rest of the test.
fn docker_is_running() -> MutexGuard<'static, ()> {
    let guard = DOCKER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let probe = Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .expect("docker is on PATH");
    assert!(
        probe.status.success(),
        "these tests need a running Docker daemon: {}",
        String::from_utf8_lossy(&probe.stderr)
    );
    guard
}

/// No container a run started is left behind. A killed one is removed by the
/// daemon a moment after the kill, so this waits a little for it to go.
fn nothing_left_behind() {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let output = Command::new("docker")
            .args([
                "ps",
                "-a",
                "--filter",
                "label=capsulet.verifier=run",
                "--format",
                "{{.Names}}",
            ])
            .output()
            .expect("docker ps");
        let left = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if left.is_empty() {
            return;
        }
        assert!(Instant::now() < deadline, "containers left behind: {left}");
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[test]
fn a_validator_in_a_container_reaches_no_network_and_writes_only_its_outputs() {
    let _docker = docker_is_running();
    let wanted = requested();
    let verification = verify(
        &registration(PROBE),
        &ContainerTransport::new(),
        &wanted,
        &inputs(),
    )
    .expect("the probe runs");
    let reason = &verification.results[0].reason;
    assert_eq!(reason, "network=blocked inputs=read-only root=read-only");

    assert!(verification.isolated);
    assert!(matches!(
        verification.record.trust,
        VerifierTrust::DeclaredOracle { .. }
    ));
    assert_eq!(
        verification.record.environment.to_string(),
        "sha256:a7dbc319b1438115a207e11930dc4bb3e643dcb3ec0bdd694bb36cd8fad289ec",
        "the record pins the image by its own digest"
    );
    assert_eq!(verification.outputs.len(), 1);
    assert_eq!(
        verification.outputs[0].digest,
        Digest::of(b"checked the patch\n")
    );
    assert!(matches!(
        verification.obligations(&wanted)[0].state,
        DischargeState::Discharged {
            basis: DischargeBasis::ReplayableTool,
            ..
        }
    ));
    nothing_left_behind();
}

#[test]
fn a_validator_past_its_memory_or_its_time_is_stopped_and_decides_nothing() {
    let _docker = docker_is_running();
    let greedy =
        registration("cat >/dev/null; x=$(head -c 200000000 /dev/zero | tr '\\0' a); echo done");
    let failed = verify(&greedy, &ContainerTransport::new(), &requested(), &inputs())
        .expect_err("a validator over its memory is killed");
    assert_eq!(failed.code(), "crashed", "{failed}");

    let mut slow = registration("cat >/dev/null; sleep 30");
    slow.limits.timeout_ms = 3_000;
    let failed = verify(&slow, &ContainerTransport::new(), &requested(), &inputs())
        .expect_err("a validator past its time is stopped");
    assert_eq!(failed.code(), "timed_out", "{failed}");
    nothing_left_behind();
}

#[test]
fn a_replay_runs_the_tool_again_in_its_pinned_image_and_compares_outputs_by_digest() {
    let _docker = docker_is_running();
    let wanted = requested();
    let first = verify(
        &registration(PROBE),
        &ContainerTransport::new(),
        &wanted,
        &inputs(),
    )
    .expect("the probe runs");
    assert_eq!(first.record.verdict, CheckerVerdict::Accepted);

    let again = rerun(
        &registration(PROBE),
        &ContainerTransport::new(),
        &wanted,
        &inputs(),
        &first.record,
    )
    .expect("the probe runs again");
    assert!(again.agrees(), "{:?}", again.differences);

    let mut altered = first.record.clone();
    altered.outputs = vec![Digest::of(b"something else")];
    let again = rerun(
        &registration(PROBE),
        &ContainerTransport::new(),
        &wanted,
        &inputs(),
        &altered,
    )
    .expect("the probe runs again");
    assert_eq!(again.differences, ["its outputs differ from the record"]);
    nothing_left_behind();
}
