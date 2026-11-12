//! A validator for the protocol's own tests.
//!
//! It decides each obligation by the first word of its statement, so a test can
//! ask for every way a validator behaves, and every way it misbehaves:
//!
//! | First word | It |
//! | --- | --- |
//! | `holds`, `fails`, `unsure` | answers `holds`, `does_not_hold`, `cannot_decide` |
//! | `report` | answers `holds` and writes `report.txt` to its outputs |
//! | `flood` | answers `holds` and writes a megabyte to its outputs |
//! | `skip` | leaves the obligation out of its response |
//! | `extra` | also answers for an obligation nobody asked about |
//! | `twice` | answers the obligation twice |
//! | `tamper` | tries to rewrite its first input, and a file beside it |
//! | `garbage` | writes something that is not JSON |
//! | `ancient` | answers in another protocol version |
//! | `crash` | exits with status 3 |
//! | `sleep` | sleeps for ten seconds |

use std::fs;

use capsulet_verifier::protocol::{ObligationResult, Outcome, VerifyResponse};
use capsulet_verifier::sdk;

fn main() {
    let request = match sdk::read_request() {
        Ok(request) => request,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let inputs = sdk::inputs_directory();
    let outputs = sdk::outputs_directory();
    let mut results = Vec::new();
    let mut protocol = capsulet_verifier::PROTOCOL.to_string();
    for obligation in &request.obligations {
        let word = obligation
            .statement
            .split_whitespace()
            .next()
            .unwrap_or_default();
        let result = |outcome: Outcome, reason: &str| ObligationResult {
            obligation: obligation.id.clone(),
            contract: obligation.contract.clone(),
            outcome,
            reason: reason.to_string(),
        };
        match word {
            "holds" => results.push(result(Outcome::Holds, "the statement says it holds")),
            "fails" => results.push(result(Outcome::DoesNotHold, "the statement says it fails")),
            "unsure" => results.push(result(
                Outcome::CannotDecide,
                "the statement says it cannot tell",
            )),
            "report" => {
                let _ = fs::write(outputs.join("report.txt"), b"checked: it holds\n");
                results.push(result(Outcome::Holds, "see report.txt"));
            }
            "flood" => {
                let _ = fs::write(outputs.join("flood.bin"), vec![0_u8; 1024 * 1024]);
                results.push(result(Outcome::Holds, "see flood.bin"));
            }
            "skip" => {}
            "extra" => {
                results.push(result(Outcome::Holds, "asked"));
                let mut stranger = result(Outcome::Holds, "not asked");
                stranger.obligation = "nobody-asked".to_string();
                results.push(stranger);
            }
            "twice" => {
                results.push(result(Outcome::Holds, "once"));
                results.push(result(Outcome::Holds, "twice"));
            }
            "tamper" => {
                if let Some(first) = request.inputs.first() {
                    let _ = fs::write(inputs.join(&first.name), b"tampered");
                }
                let _ = fs::write(inputs.join("planted.txt"), b"planted");
                results.push(result(Outcome::Holds, "tampered"));
            }
            "garbage" => {
                print!("this is not a response");
                return;
            }
            "ancient" => {
                protocol = "capsulet.verifier/v0".to_string();
                results.push(result(Outcome::Holds, "from the past"));
            }
            "crash" => {
                eprintln!("the fixture crashed on purpose");
                std::process::exit(3);
            }
            "sleep" => {
                std::thread::sleep(std::time::Duration::from_secs(10));
                results.push(result(Outcome::Holds, "woke up"));
            }
            other => results.push(result(
                Outcome::CannotDecide,
                &format!("no behaviour for {other:?}"),
            )),
        }
    }
    let response = VerifyResponse { protocol, results };
    print!(
        "{}",
        serde_json::to_string(&response).expect("a response encodes")
    );
}
