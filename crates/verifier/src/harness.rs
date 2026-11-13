//! The local development harness: run one validator against cases, without a
//! stack.
//!
//! A cases file names, for each case, the obligations to ask about, the inputs
//! to hand over, and either the outcome expected for each obligation or the
//! failure expected of the run. `capsulet verifier test` reads one, runs the
//! validator through the transport its registration names, and reports each
//! case. A `process` validator runs unisolated here, and the report says its
//! results decide nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use capsulet_ir::Identifier;
use capsulet_ir::correctness::obligation::{ObligationStatement, RepairOwner};
use serde::Deserialize;

use crate::host::{ContainerTransport, Input, ProcessTransport, Requested, verify};
use crate::protocol::Outcome;
use crate::registration::{Environment, Registration};

/// Every case for one validator.
#[derive(Debug, Clone, Deserialize)]
pub struct Cases {
    pub cases: Vec<Case>,
}

/// One case: what to ask, what to hand over, and what should happen.
#[derive(Debug, Clone, Deserialize)]
pub struct Case {
    pub name: String,
    pub obligations: Vec<CaseObligation>,
    #[serde(default)]
    pub inputs: Vec<CaseInput>,
    /// The outcome each obligation should come back with.
    #[serde(default)]
    pub expect: BTreeMap<String, Outcome>,
    /// The failure code the run should end with instead, such as `timed_out`.
    #[serde(default)]
    pub expect_failure: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CaseObligation {
    pub id: String,
    pub contract: String,
    pub statement: String,
}

/// An input, from a file beside the cases file or written out in it.
#[derive(Debug, Clone, Deserialize)]
pub struct CaseInput {
    pub name: String,
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default = "octets")]
    pub media_type: String,
}

fn octets() -> String {
    "application/octet-stream".to_string()
}

/// How one case came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseReport {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// Runs every case, through the transport the registration's environment names.
/// Paths in the cases resolve against `base`.
#[must_use]
pub fn run_cases(registration: &Registration, cases: &Cases, base: &Path) -> Vec<CaseReport> {
    cases
        .cases
        .iter()
        .map(|case| run_case(registration, case, base))
        .collect()
}

fn run_case(registration: &Registration, case: &Case, base: &Path) -> CaseReport {
    let report = |passed: bool, detail: String| CaseReport {
        name: case.name.clone(),
        passed,
        detail,
    };
    let requested = match requested(case) {
        Ok(requested) => requested,
        Err(detail) => return report(false, detail),
    };
    let inputs = match inputs(case, base) {
        Ok(inputs) => inputs,
        Err(detail) => return report(false, detail),
    };
    let outcome = match &registration.environment {
        Environment::Container { .. } => verify(
            registration,
            &ContainerTransport::new(),
            &requested,
            &inputs,
        ),
        Environment::Process { .. } => verify(registration, &ProcessTransport, &requested, &inputs),
    };
    match (outcome, &case.expect_failure) {
        (Err(failure), Some(expected)) if failure.code() == expected => {
            let isolation = match &registration.environment {
                Environment::Container { .. } => "isolated",
                Environment::Process { .. } => "unisolated",
            };
            report(true, format!("failed as expected ({isolation}): {failure}"))
        }
        (Err(failure), _) => report(false, format!("failed: {} ({failure})", failure.code())),
        (Ok(_), Some(expected)) => report(
            false,
            format!("ran, and was expected to fail with {expected}"),
        ),
        (Ok(verification), None) => {
            let mut wrong = Vec::new();
            for (obligation, expected) in &case.expect {
                let found = verification
                    .results
                    .iter()
                    .find(|result| &result.obligation == obligation)
                    .map(|result| result.outcome);
                if found != Some(*expected) {
                    wrong.push(format!(
                        "{obligation}: expected {expected:?}, got {found:?}"
                    ));
                }
            }
            let isolation = if verification.isolated {
                "isolated"
            } else {
                "unisolated, so in a real run these results would decide nothing"
            };
            if wrong.is_empty() {
                report(
                    true,
                    format!(
                        "{} results as expected ({isolation})",
                        verification.results.len()
                    ),
                )
            } else {
                report(false, wrong.join("; "))
            }
        }
    }
}

fn requested(case: &Case) -> Result<Vec<Requested>, String> {
    case.obligations
        .iter()
        .map(|obligation| {
            let identifier = |value: &str| {
                Identifier::parse(value.to_string()).map_err(|error| format!("{value:?}: {error}"))
            };
            Ok(Requested {
                contract: identifier(&obligation.contract)?,
                statement: ObligationStatement::new(
                    identifier(&obligation.id)?,
                    obligation.statement.clone(),
                    RepairOwner::Verifier,
                ),
            })
        })
        .collect()
}

fn inputs(case: &Case, base: &Path) -> Result<Vec<Input>, String> {
    case.inputs
        .iter()
        .map(|input| {
            let bytes = match (&input.path, &input.text) {
                (Some(path), None) => std::fs::read(base.join(path))
                    .map_err(|error| format!("input {}: {error}", input.name))?,
                (None, Some(text)) => text.clone().into_bytes(),
                _ => {
                    return Err(format!(
                        "input {} needs exactly one of path and text",
                        input.name
                    ));
                }
            };
            Ok(Input {
                name: input.name.clone(),
                bytes,
                media_type: input.media_type.clone(),
            })
        })
        .collect()
}

/// The report as lines, and whether every case passed.
#[must_use]
pub fn render(reports: &[CaseReport]) -> (String, bool) {
    use std::fmt::Write as _;
    let mut text = String::new();
    for report in reports {
        let mark = if report.passed { "pass" } else { "FAIL" };
        let _ = writeln!(text, "{mark} {}: {}", report.name, report.detail);
    }
    let failed = reports.iter().filter(|report| !report.passed).count();
    let _ = writeln!(
        text,
        "{} of {} cases passed",
        reports.len() - failed,
        reports.len()
    );
    (text, failed == 0)
}
