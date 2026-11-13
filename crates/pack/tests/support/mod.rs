#![allow(
    dead_code,
    reason = "each test binary uses a different part of what is shared here"
)]

//! What the code-changes pack's tests share: the pack, its fixtures as a
//! change's inputs, and a certificate sealed from a decision the way a run
//! under `enforce` would seal it.

#[path = "../../../ir/tests/fixtures/mod.rs"]
pub mod fixtures;

use std::fs;
use std::path::{Path, PathBuf};

use capsulet_ir::correctness::certificate::Subject;
use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::correctness::obligation::DischargeState;
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::correctness::{Certificate, Identity};
use capsulet_ir::{AssuranceMode, PackPin};
use capsulet_kernel::{Assembly, certify};
use capsulet_pack::{Decision, INPUT_PREFIX, Pack, obligations};
use capsulet_verifier::Input;

use fixtures::{definition_in, id};

/// When the inputs and outputs were captured.
pub const AT: RecordedTime = RecordedTime(1_794_600_000_000);

#[must_use]
pub fn pack_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs")
        .join(name)
}

/// One of the code-changes pack's fixtures as a change's inputs: both
/// revisions, at their paths.
#[must_use]
pub fn change(scenario: &str) -> Vec<Input> {
    let mut inputs = Vec::new();
    for side in ["base", "change"] {
        for name in ["calc.py", "test_calc.py"] {
            let path = pack_dir("code-changes")
                .join("fixtures")
                .join(scenario)
                .join(side)
                .join(name);
            inputs.push(Input {
                name: format!("{side}/{name}"),
                bytes: fs::read(path).expect("a fixture file"),
                media_type: "text/x-python".to_string(),
            });
        }
    }
    inputs
}

/// Who proposed the change.
#[must_use]
pub fn proposer() -> Producer {
    Producer {
        kind: ProducerKind::Model,
        identity: Identity::new(id("coding-agent"), "1"),
    }
}

/// A certificate sealed from `decision`, under `enforce`, by a definition that
/// pins the pack and declares its obligations.
///
/// # Panics
///
/// Panics if the pack's obligations do not read or the certificate does not seal.
#[must_use]
pub fn sealed(pack: &Pack, decision: &Decision) -> Certificate {
    let mut definition = definition_in(AssuranceMode::Enforce);
    let contracts = obligations(pack).expect("the pack's obligations");
    definition.contracts.extend(contracts.iter().cloned());
    definition.packs = vec![PackPin {
        name: pack.manifest.name.clone(),
        version: pack.manifest.version.clone(),
        manifest: pack.manifest.digest().expect("a digest"),
    }];
    let admission = capsulet_ir::admit(&definition).expect("admitted");
    certify(Assembly {
        id: id("cert-change"),
        admission: admission.clone(),
        mode: AssuranceMode::Enforce,
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: Some(id("run-change")),
            inputs: decision
                .evidence
                .iter()
                .filter(|reference| reference.id.as_str().starts_with(INPUT_PREFIX))
                .map(|reference| reference.content)
                .collect(),
            outputs: vec![],
        },
        policy_version: "code-changes/1".to_string(),
        contracts: contracts
            .iter()
            .map(|contract| contract.id.clone())
            .collect(),
        verifiers: decision.verifiers.clone(),
        evidence: decision.evidence.clone(),
        obligations: decision.obligations.clone(),
        loops: vec![],
    })
    .expect("seals")
}

/// The state `decision` left `obligation` in.
///
/// # Panics
///
/// Panics if the pack states no such obligation.
#[must_use]
pub fn state<'a>(decision: &'a Decision, obligation: &str) -> &'a DischargeState {
    &decision
        .obligations
        .iter()
        .find(|o| o.statement.id.as_str() == obligation)
        .unwrap_or_else(|| panic!("{obligation} is stated"))
        .state
}

/// Copies a directory tree.
pub fn copy(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            fs::create_dir_all(&target).expect("dir");
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy");
        }
    }
}
