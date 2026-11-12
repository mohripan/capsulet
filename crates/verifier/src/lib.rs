//! The verifier protocol (`capsulet.verifier/v1`).
//!
//! M4 opens the correctness plane to checkers Capsulet does not contain:
//! compilers, test runners, policy engines, parsers, and models. ADR 0021 fixes
//! what they may do, and this crate is that decision in code:
//!
//! - [`protocol`]: what crosses the boundary, one request in and one response
//!   out;
//! - [`registration`]: what a validator is allowed to be, checked before it
//!   runs, so its basis comes from its registration and never from its own
//!   word;
//! - [`host`]: running one, and turning what it said into a verifier record and
//!   obligation states a certificate can carry. A tool's `holds` discharges, a
//!   model's endorses, and an unisolated run decides nothing;
//! - [`sdk`]: writing a validator in Rust.
//!
//! Isolation is the transport's. [`host::ProcessTransport`] runs a local program
//! for the development harness, and its records say it was unisolated.

pub mod host;
pub mod protocol;
pub mod registration;
pub mod sdk;

pub use host::{
    Input, Output, ProcessTransport, RawRun, Requested, Transport, Verification, VerifierFailure,
    verify,
};
pub use protocol::{
    InputFile, MAX_RESPONSE_BYTES, ObligationResult, Outcome, PROTOCOL, RequestedObligation,
    VerifyRequest, VerifyResponse,
};
pub use registration::{
    Capability, Environment, Limits, RegisteredBasis, Registration, RegistrationError,
};
