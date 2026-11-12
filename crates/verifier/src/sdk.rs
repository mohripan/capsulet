//! Writing a validator in Rust.
//!
//! A validator reads one request on standard input, decides each obligation,
//! writes anything it produced under the outputs directory, and writes one
//! response on standard output. [`serve`] does the reading and writing; the
//! validator supplies the deciding.
//!
//! ```no_run
//! use capsulet_verifier::protocol::{ObligationResult, Outcome};
//!
//! capsulet_verifier::sdk::serve(|request, _inputs, _outputs| {
//!     request
//!         .obligations
//!         .iter()
//!         .map(|obligation| ObligationResult {
//!             obligation: obligation.id.clone(),
//!             contract: obligation.contract.clone(),
//!             outcome: Outcome::CannotDecide,
//!             reason: "this validator decides nothing".to_string(),
//!         })
//!         .collect()
//! });
//! ```

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::protocol::{ObligationResult, PROTOCOL, VerifyRequest, VerifyResponse};

/// Where the inputs are: `CAPSULET_VERIFIER_INPUTS`, or `/inputs` in a container.
#[must_use]
pub fn inputs_directory() -> PathBuf {
    std::env::var_os("CAPSULET_VERIFIER_INPUTS")
        .map_or_else(|| PathBuf::from("/inputs"), PathBuf::from)
}

/// Where outputs go: `CAPSULET_VERIFIER_OUTPUTS`, or `/outputs` in a container.
#[must_use]
pub fn outputs_directory() -> PathBuf {
    std::env::var_os("CAPSULET_VERIFIER_OUTPUTS")
        .map_or_else(|| PathBuf::from("/outputs"), PathBuf::from)
}

/// Reads the request a host sent.
///
/// # Errors
///
/// Returns why standard input is not a request in this protocol.
pub fn read_request() -> Result<VerifyRequest, String> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    let request: VerifyRequest =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if request.protocol != PROTOCOL {
        return Err(format!(
            "this validator speaks {PROTOCOL}, not {}",
            request.protocol
        ));
    }
    Ok(request)
}

/// Writes the response for `results`.
///
/// # Errors
///
/// Returns why standard output could not be written.
pub fn respond(results: Vec<ObligationResult>) -> Result<(), String> {
    let response = VerifyResponse {
        protocol: PROTOCOL.to_string(),
        results,
    };
    let bytes = serde_json::to_vec(&response).map_err(|error| error.to_string())?;
    std::io::stdout()
        .write_all(&bytes)
        .map_err(|error| error.to_string())
}

/// Reads the request, decides it with `decide`, and writes the response. A
/// request that cannot be read exits non-zero, which the host reports as a crash
/// rather than a verdict.
pub fn serve(decide: impl FnOnce(&VerifyRequest, &Path, &Path) -> Vec<ObligationResult>) {
    let request = match read_request() {
        Ok(request) => request,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let results = decide(&request, &inputs_directory(), &outputs_directory());
    if let Err(error) = respond(results) {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
