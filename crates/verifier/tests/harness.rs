//! The development harness, against the fixture validator run as a process.

use std::collections::BTreeMap;

use capsulet_verifier::harness::{Cases, render, run_cases};
use capsulet_verifier::{Environment, Limits, RegisteredBasis, Registration};

const FIXTURE: &str = env!("CARGO_BIN_EXE_fixture-validator");

fn registration() -> Registration {
    Registration {
        name: "fixture-validator".to_string(),
        version: "1".to_string(),
        basis: RegisteredBasis::ReplayableTool,
        environment: Environment::Process {
            program: FIXTURE.to_string(),
            args: Vec::new(),
        },
        capabilities: Vec::new(),
        limits: Limits {
            timeout_ms: 5_000,
            memory_mb: 256,
            max_output_bytes: 64 * 1024,
        },
        parameters: BTreeMap::new(),
    }
}

fn cases(json: &str) -> Cases {
    serde_json::from_str(json).expect("cases")
}

#[test]
fn every_case_is_reported_and_a_wrong_outcome_or_an_unexpected_failure_fails_it() {
    let cases = cases(
        r#"{"cases": [
            {"name": "builds", "obligations": [{"id": "builds", "contract": "patch", "statement": "holds"}],
             "inputs": [{"name": "patch.diff", "text": "--- a\n"}], "expect": {"builds": "holds"}},
            {"name": "breaks", "obligations": [{"id": "builds", "contract": "patch", "statement": "fails"}],
             "expect": {"builds": "holds"}},
            {"name": "stays quiet", "obligations": [{"id": "builds", "contract": "patch", "statement": "skip"}],
             "expect_failure": "missing_result"},
            {"name": "crashes", "obligations": [{"id": "builds", "contract": "patch", "statement": "crash"}],
             "expect": {"builds": "holds"}}
        ]}"#,
    );
    let reports = run_cases(&registration(), &cases, std::path::Path::new("."));
    let passed: Vec<bool> = reports.iter().map(|report| report.passed).collect();
    assert_eq!(passed, [true, false, true, false], "{reports:?}");
    assert!(
        reports[0].detail.contains("unisolated"),
        "{}",
        reports[0].detail
    );
    assert!(
        reports[3].detail.contains("crashed"),
        "{}",
        reports[3].detail
    );

    let (text, all_passed) = render(&reports);
    assert!(!all_passed);
    assert!(text.ends_with("2 of 4 cases passed\n"), "{text}");
    assert!(
        text.contains("FAIL breaks: builds: expected Holds, got Some(DoesNotHold)"),
        "{text}"
    );
}

#[test]
fn an_input_names_exactly_one_source() {
    let cases = cases(
        r#"{"cases": [{"name": "ambiguous", "obligations": [{"id": "builds", "contract": "patch", "statement": "holds"}],
            "inputs": [{"name": "patch.diff"}]}]}"#,
    );
    let reports = run_cases(&registration(), &cases, std::path::Path::new("."));
    assert!(!reports[0].passed);
    assert!(
        reports[0].detail.contains("exactly one of path and text"),
        "{}",
        reports[0].detail
    );
}
