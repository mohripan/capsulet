//! The verdict rule is load-bearing for the archive, not just for new runs.
//!
//! `CertificateBody::check` re-derives the verdict with **this build's** rule
//! and refuses the certificate if the recorded verdict disagrees. `Deserialize`
//! runs that check, so refused means unreadable. Changing
//! `AssuranceVerdict::from_obligations` is therefore not a local edit: it
//! decides whether certificates already sealed can still be opened.
//!
//! Nothing stopped that happening silently. These tests are the tripwire — a
//! certificate sealed here, and the four cases of the rule pinned. Editing the
//! rule makes them fail, which turns an accident into a decision.

mod fixtures;

use capsulet_ir::correctness::certificate::Subject;
use capsulet_ir::correctness::obligation::{DischargeState, ObligationStatement, RepairOwner};
use capsulet_ir::correctness::{Certificate, CertificateBody};
use capsulet_ir::{AssuranceMode, AssuranceVerdict, Obligation, admit};

use fixtures::{definition_with_scan, id};

fn obligation(statement: &str, contract: &str, state: DischargeState) -> Obligation {
    Obligation {
        statement: ObligationStatement {
            id: id(statement),
            statement: format!("`{statement}` holds"),
            owner: RepairOwner::Verifier,
        },
        contract: id(contract),
        state,
    }
}

fn discharged(statement: &str, contract: &str) -> Obligation {
    obligation(
        statement,
        contract,
        DischargeState::Discharged {
            by: id("the-test-runner"),
            evidence: vec![],
        },
    )
}

fn seal(obligations: Vec<Obligation>) -> Result<Certificate, capsulet_ir::CertificateError> {
    let mode = AssuranceMode::Enforce;
    let definition = definition_with_scan(mode);
    let admission = admit(&definition).expect("admitted");
    let verdict = AssuranceVerdict::under_mode(mode, &obligations);

    Certificate::seal(CertificateBody {
        schema_version: Certificate::current_schema_version(),
        id: id("cert-verdict-rule"),
        admission: admission.clone(),
        mode,
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: Some(id("run-verdict-rule")),
            inputs: vec![],
            outputs: vec![],
        },
        policy_version: "release-policy/3".to_string(),
        kernel_version: "capsulet-kernel 0.1.0".to_string(),
        contracts: vec![id("patch-compiles")],
        verifiers: vec![],
        evidence: vec![],
        obligations,
        loops: vec![],
        verdict,
    })
}

#[test]
fn the_verdict_rule_this_build_applies_is_the_one_the_archive_was_sealed_under() {
    // Each case of the rule, stated separately. If a change to
    // `from_obligations` is deliberate, these are what to update — and updating
    // them is the moment to ask what happens to certificates already sealed,
    // because `Deserialize` will refuse every one whose verdict no longer
    // follows.
    let discharged_only = vec![discharged("compiles", "patch-compiles")];
    let with_residual = vec![
        discharged("compiles", "patch-compiles"),
        obligation(
            "no-secrets-in-output",
            "scanned-under-named-rules",
            DischargeState::Residual {
                rationale: "nobody checked".to_string(),
                evidence: vec![],
            },
        ),
    ];
    let with_failure = vec![obligation(
        "compiles",
        "patch-compiles",
        DischargeState::Failed {
            reason: "it did not compile".to_string(),
            owner: RepairOwner::Verifier,
        },
    )];

    assert_eq!(
        AssuranceVerdict::from_obligations(&[]),
        AssuranceVerdict::Unverified,
        "no obligations is `unverified`: nothing was evaluated, which is not the same as nothing \
         being wrong"
    );
    assert_eq!(
        AssuranceVerdict::from_obligations(&discharged_only),
        AssuranceVerdict::Accepted
    );
    assert_eq!(
        AssuranceVerdict::from_obligations(&with_residual),
        AssuranceVerdict::Conditional
    );
    assert_eq!(
        AssuranceVerdict::from_obligations(&with_failure),
        AssuranceVerdict::Rejected
    );

    // Observe never reaches a positive verdict, whatever the obligations say.
    assert_eq!(
        AssuranceVerdict::under_mode(AssuranceMode::Observe, &discharged_only),
        AssuranceVerdict::Unverified
    );
}

#[test]
fn a_sealed_certificate_still_opens_under_this_builds_rule() {
    // The round trip is the thing that breaks when the rule moves: sealing uses
    // the rule, and deserializing re-checks it.
    let certificate = seal(vec![discharged("compiles", "patch-compiles")]).expect("seals");
    let bytes = serde_json::to_vec(&certificate).expect("encodes");

    let reopened: Certificate = serde_json::from_slice(&bytes).expect(
        "a certificate this build sealed must still deserialize; if this fails, the verdict rule \
         moved and every stored certificate moved with it",
    );
    assert_eq!(reopened.verdict(), AssuranceVerdict::Accepted);
}

#[test]
fn two_contracts_may_each_declare_an_obligation_of_the_same_name() {
    // An obligation is identified by its contract *and* its statement. Two
    // contracts that both call something `compiles` have two obligations that
    // share a name, not one recorded twice, and a certificate covering both has
    // to be sealable.
    let certificate = seal(vec![
        discharged("compiles", "patch-compiles"),
        discharged("compiles", "scanned-under-named-rules"),
    ])
    .expect("a certificate covering both contracts seals");

    assert_eq!(certificate.verdict(), AssuranceVerdict::Accepted);
}

#[test]
fn the_same_obligation_of_the_same_contract_twice_is_still_refused() {
    let refused = seal(vec![
        discharged("compiles", "patch-compiles"),
        discharged("compiles", "patch-compiles"),
    ]);

    assert!(
        refused.is_err(),
        "one obligation recorded twice is still a duplicate"
    );
}
