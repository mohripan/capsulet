//! The infrastructure-policy reference pack (#161), decided, waived and
//! replayed without Docker.
//!
//! A scripted transport stands in for OPA and the scope scanner, so these tests
//! are about what the pack crate does around them: which files each validator is
//! handed, what a certificate sealed from their answers says, and what a waiver
//! does to a failure. The same pack runs in real containers in
//! `tests/reference_packs.rs`.

mod support;

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use capsulet_ir::AssuranceVerdict;
use capsulet_ir::correctness::obligation::{DischargeState, RepairOwner};
use capsulet_ir::correctness::proposal::ProducerKind;
use capsulet_kernel::{Bundle, EvidenceSource, ReplayOutcome, replay};
use capsulet_pack::{
    Decision, PackError, ToolReplay, Waiver, admit, decide, files_of, generate, read, replay_tools,
    sign, waive,
};
use capsulet_verifier::{Input, RawRun, Registration, Transport};
use support::fixtures::id;
use support::{AT, copy, pack_dir, plan, proposer, sealed, state};

const PUBLIC: &str = "no-public-buckets: aws_s3_bucket_acl.docs grants public-read";

/// Answers as OPA and the scanner would for the fixtures, and remembers which
/// inputs each was handed.
#[derive(Default)]
struct Scripted {
    handed: Mutex<Vec<(String, Vec<String>)>>,
}

fn files_under(root: &Path, at: &Path, found: &mut Vec<String>) {
    for entry in fs::read_dir(at).expect("read") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            files_under(root, &path, found);
        } else {
            let name = path.strip_prefix(root).expect("inside").to_string_lossy();
            found.push(name.replace('\\', "/"));
        }
    }
}

impl Transport for Scripted {
    fn run(
        &self,
        registration: &Registration,
        workdir: &Path,
        request: &[u8],
    ) -> Result<RawRun, String> {
        let inputs = workdir.join("inputs");
        let mut names = Vec::new();
        files_under(&inputs, &inputs, &mut names);
        names.sort();
        self.handed
            .lock()
            .expect("lock")
            .push((registration.name.clone(), names));
        let request: serde_json::Value =
            serde_json::from_slice(request).map_err(|e| e.to_string())?;
        let plan =
            fs::read_to_string(inputs.join("change/plan.json")).map_err(|e| e.to_string())?;
        if registration.name == "plan-scope" {
            fs::write(workdir.join("outputs/scope.log"), "the inventory\n")
                .map_err(|e| e.to_string())?;
        }
        let results: Vec<serde_json::Value> = request["obligations"]
            .as_array()
            .expect("obligations")
            .iter()
            .map(|obligation| {
                let id = obligation["id"].as_str().expect("an id");
                let failure = match id {
                    "no-public-buckets" if plan.contains("public-read") => Some(PUBLIC),
                    "within-declared-scope" if plan.contains("aws_db_instance.legacy") => {
                        Some("delete aws_db_instance.legacy: the scope does not name it")
                    }
                    _ => None,
                };
                serde_json::json!({
                    "obligation": id,
                    "contract": obligation["contract"],
                    "outcome": if failure.is_some() { "does_not_hold" } else { "holds" },
                    "reason": failure.unwrap_or("nothing breaks it"),
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

fn decided(scenario: &str, transport: &Scripted) -> Decision {
    let pack = read(&pack_dir("infrastructure-policy")).expect("the pack reads");
    decide(&pack, &plan(scenario), transport, &proposer(), AT).expect("decides")
}

fn waiver(obligation: &str, authority: &str, failure: &str) -> Waiver {
    Waiver {
        id: id("docs-site-is-public"),
        obligation: id(obligation),
        authority: id(authority),
        by: id("dana"),
        reason: "The documentation site is public by design.".to_string(),
        failure: failure.to_string(),
    }
}

#[test]
fn a_compliant_change_is_accepted_on_the_tools_word_alone() {
    let pack = read(&pack_dir("infrastructure-policy")).expect("the pack reads");
    let decision = decided("compliant", &Scripted::default());
    assert_eq!(
        sealed(&pack, &decision).verdict(),
        AssuranceVerdict::Accepted
    );
}

#[test]
fn a_policy_violation_is_rejected_naming_the_rule_owned_by_the_proposer() {
    let pack = read(&pack_dir("infrastructure-policy")).expect("the pack reads");
    let decision = decided("public-bucket", &Scripted::default());

    let DischargeState::Failed { reason, owner } = state(&decision, "no-public-buckets") else {
        panic!("{:?}", state(&decision, "no-public-buckets"));
    };
    assert_eq!(*owner, RepairOwner::Proposer);
    assert_eq!(
        reason, PUBLIC,
        "the failure names the rule and the resource"
    );
    assert!(matches!(
        state(&decision, "storage-encrypted"),
        DischargeState::Discharged { .. }
    ));
    assert_eq!(
        sealed(&pack, &decision).verdict(),
        AssuranceVerdict::Rejected
    );
}

#[test]
fn a_waived_violation_is_conditional_names_its_authority_and_carries_the_waiver() {
    let pack = read(&pack_dir("infrastructure-policy")).expect("the pack reads");
    let mut decision = decided("public-bucket", &Scripted::default());
    let granted = waiver("no-public-buckets", "review:security", PUBLIC);

    let unused = waive(&mut decision, std::slice::from_ref(&granted), AT).expect("waives");
    assert!(unused.is_empty(), "{unused:?}");

    let DischargeState::Waived {
        policy,
        authority,
        by,
        evidence,
    } = state(&decision, "no-public-buckets")
    else {
        panic!("{:?}", state(&decision, "no-public-buckets"));
    };
    assert_eq!(policy.as_str(), "docs-site-is-public");
    assert_eq!(authority.as_str(), "review:security");
    assert_eq!(
        by.as_ref().map(capsulet_ir::Identifier::as_str),
        Some("dana")
    );

    let certificate = sealed(&pack, &decision);
    assert_eq!(certificate.verdict(), AssuranceVerdict::Conditional);

    // The waiver itself is evidence: cited, granted by a person under the
    // authority, and carried by the bundle with who, why and what it accepted.
    let cited = certificate
        .body()
        .evidence
        .iter()
        .find(|reference| reference.id.as_str() == "waivers/docs-site-is-public")
        .expect("the waiver is cited");
    assert_eq!(evidence, &vec![cited.content]);
    assert_eq!(cited.producer.kind, ProducerKind::Human);
    assert_eq!(cited.producer.identity.name.as_str(), "dana");
    let bundle = Bundle::build(certificate.clone(), &decision.blobs).expect("bundles");
    let carried = bundle.evidence().expect("reads");
    let read_back: Waiver =
        serde_json::from_slice(carried.bytes(&cited.content).expect("carried")).expect("a waiver");
    assert_eq!(read_back, granted);

    match replay(&certificate, &carried) {
        ReplayOutcome::Reproduced { verdict, .. } => {
            assert_eq!(verdict, AssuranceVerdict::Conditional);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_waiver_covers_the_failure_it_names_and_no_other() {
    let pack = read(&pack_dir("infrastructure-policy")).expect("the pack reads");
    let mut decision = decided("public-bucket", &Scripted::default());
    let other = waiver(
        "no-public-buckets",
        "review:security",
        "no-public-buckets: aws_s3_bucket_acl.blog grants public-read",
    );

    let unused = waive(&mut decision, &[other], AT).expect("a waiver that covers nothing");
    assert_eq!(unused, vec![id("docs-site-is-public")]);
    assert!(matches!(
        state(&decision, "no-public-buckets"),
        DischargeState::Failed { .. }
    ));
    assert_eq!(
        sealed(&pack, &decision).verdict(),
        AssuranceVerdict::Rejected
    );

    // Nor does a waiver of an obligation that held apply to anything.
    let mut compliant = decided("compliant", &Scripted::default());
    let unused = waive(
        &mut compliant,
        &[waiver("no-public-buckets", "review:security", PUBLIC)],
        AT,
    )
    .expect("waives nothing");
    assert_eq!(unused.len(), 1);
}

#[test]
fn a_waiver_no_authority_could_grant_here_is_refused_and_changes_nothing() {
    let outside = decided("outside-scope", &Scripted::default());
    let scope_failure = match state(&outside, "within-declared-scope") {
        DischargeState::Failed { reason, .. } => reason.clone(),
        other => panic!("{other:?}"),
    };
    let public = decided("public-bucket", &Scripted::default());

    for (decision, waiver, why) in [
        (
            &public,
            waiver("no-public-buckets", "review:code", PUBLIC),
            "another authority",
        ),
        (
            &outside,
            waiver("within-declared-scope", "review:security", &scope_failure),
            "an obligation naming no waiver authority",
        ),
        (
            &public,
            waiver("no-such-policy", "review:security", PUBLIC),
            "an obligation the pack does not state",
        ),
    ] {
        let mut changed = decision.clone();
        let error = waive(&mut changed, &[waiver], AT).expect_err(why);
        assert!(
            matches!(error, PackError::BadWaiver { .. }),
            "{why}: {error}"
        );
        assert_eq!(error.code(), "bad_waiver");
        assert_eq!(changed.obligations, decision.obligations, "{why}");
        assert_eq!(changed.evidence, decision.evidence, "{why}");
    }
}

#[test]
fn the_policy_engine_is_handed_the_packs_policies_and_a_change_cannot_bring_its_own() {
    let pack = read(&pack_dir("infrastructure-policy")).expect("the pack reads");
    let transport = Scripted::default();
    let decision = decided("public-bucket", &transport);

    let handed = transport.handed.lock().expect("lock").clone();
    let policies: Vec<String> = pack.manifest.validators[0]
        .reads
        .iter()
        .map(|path| format!("pack/{path}"))
        .collect();
    let (engine, scanner) = (&handed[0], &handed[1]);
    assert_eq!(engine.0, "opa-policies");
    for policy in &policies {
        assert!(engine.1.contains(policy), "{policy} in {:?}", engine.1);
    }
    assert_eq!(scanner.0, "plan-scope");
    assert_eq!(scanner.1, vec!["change/plan.json", "scope.json"]);

    // Each policy is cited once, supplied by the pack at its version, with the
    // pack's bytes.
    for path in &pack.manifest.validators[0].reads {
        let cited: Vec<_> = decision
            .evidence
            .iter()
            .filter(|reference| reference.id.as_str() == format!("inputs/pack/{path}"))
            .collect();
        assert_eq!(cited.len(), 1, "{path}");
        assert_eq!(cited[0].producer.kind, ProducerKind::Retrieval);
        assert_eq!(
            cited[0].producer.identity.name.as_str(),
            "infrastructure-policy"
        );
        assert_eq!(
            decision.blobs.bytes(&cited[0].content),
            Some(pack.files[path].as_slice())
        );
    }

    let mut smuggled = plan("public-bucket");
    smuggled.push(Input {
        name: "pack/policies/allow_everything.rego".to_string(),
        bytes: b"package capsulet\n".to_vec(),
        media_type: "text/x-rego".to_string(),
    });
    let error = decide(&pack, &smuggled, &transport, &proposer(), AT)
        .expect_err("a change may not bring a policy");
    assert_eq!(error.code(), "reserved_input");
}

#[test]
fn the_tools_run_again_from_the_bundle_with_the_policies_it_carries() {
    let pack = read(&pack_dir("infrastructure-policy")).expect("the pack reads");
    let first = Scripted::default();
    let mut decision = decided("public-bucket", &first);
    waive(
        &mut decision,
        &[waiver("no-public-buckets", "review:security", PUBLIC)],
        AT,
    )
    .expect("waives");
    let certificate = sealed(&pack, &decision);
    let bundle = Bundle::build(certificate.clone(), &decision.blobs).expect("bundles");
    let carried = bundle.evidence().expect("reads");

    let again = Scripted::default();
    let replays = replay_tools(&pack, &certificate, &carried, &again).expect("replays");
    assert_eq!(replays.len(), 2);
    assert!(replays.iter().all(ToolReplay::agrees), "{replays:?}");
    assert_eq!(
        *again.handed.lock().expect("lock"),
        *first.handed.lock().expect("lock"),
        "each tool is handed again exactly what it read"
    );
}

#[test]
fn every_policy_obligation_has_its_policy_the_scanner_runs_its_script_and_the_pack_installs() {
    let pack = read(&pack_dir("infrastructure-policy")).expect("the pack reads");
    let engine = &pack.manifest.validators[0];
    for obligation in &engine.decides {
        let policy = format!("policies/{}.rego", obligation.replace('-', "_"));
        assert!(engine.reads.contains(&policy), "{obligation} has {policy}");
        let source = std::str::from_utf8(&pack.files[&policy]).expect("text");
        assert!(
            source.contains(&format!(
                "package capsulet.policies.{}",
                obligation.replace('-', "_")
            )),
            "{policy}"
        );
    }
    let scanner: Registration =
        serde_json::from_slice(&pack.files["validators/plan-scope.json"]).expect("reads");
    let capsulet_verifier::Environment::Container { command, .. } = &scanner.environment else {
        panic!("a container");
    };
    assert_eq!(
        command[2].as_bytes(),
        pack.files["validators/src/plan_scope_check.py"].as_slice(),
        "plan-scope runs validators/src/plan_scope_check.py as it is: regenerate the registration"
    );

    let root = pack_dir("infrastructure-policy");
    assert_eq!(
        pack.manifest.files,
        files_of(&root).expect("its files"),
        "packs/infrastructure-policy/pack.json lists every file as it is: run `capsulet pack refresh`"
    );
    let directory = tempfile::tempdir().expect("dir");
    copy(&root, directory.path());
    let (key, _) = generate().expect("a key");
    sign(directory.path(), &key).expect("signs");
    let mut trust = capsulet_pack::TrustStore::new();
    trust.keys.push(
        key.trusted_for(vec!["infrastructure-policy".to_string()])
            .expect("a public key"),
    );
    let admitted = admit(&read(directory.path()).expect("reads"), &trust, &[]).expect("installs");
    assert_eq!(
        (admitted.name.as_str(), admitted.version.as_str()),
        ("infrastructure-policy", "1.0.0")
    );
}
