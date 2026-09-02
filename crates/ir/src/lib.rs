//! The verified-computation intermediate representation.
//!
//! This crate is deliberately inert. It performs no I/O, spawns nothing, reads
//! no clock, and consults no randomness, because everything it produces has to
//! be reproducible by someone else — on another machine, at another time, with
//! no access to this installation. A digest computed here must be computable
//! there, or the certificate that carries it means nothing.
//!
//! That constraint is enforced, not merely stated: `tests/purity.rs` fails if a
//! database, HTTP, async-runtime, randomness, or clock crate ever enters this
//! crate's dependency closure.
//!
//! # Stability of the bytes
//!
//! [`canonical`] defines the exact encoding every digest is taken over, and
//! `tests/golden/` pins it to checked-in bytes. Changing those bytes changes
//! every digest ever stored, so it is a breaking change: bump the schema major
//! in [`version`] and provide a compatibility reader rather than editing a
//! golden file to match new output.

pub mod admission;
pub mod assurance;
pub mod canonical;
pub mod capability;
pub mod correctness;
pub mod definition;
pub mod digest;
pub mod effect;
pub mod graph;
pub mod id;
pub mod loop_region;
pub mod node;
pub mod port;
pub mod reader;
pub mod region;
pub mod trust;
pub mod value;
pub mod version;

pub use admission::{AdmissionCode, AdmissionRecord, AdmissionRefusal, admit};
pub use assurance::{
    AssurancePolicy, BoundaryDecision, BoundaryPolicy, DenialReason, TrustRoute, check_trust_route,
    decide_boundary,
};
pub use canonical::{CanonicalError, CanonicalValue, Decimal, to_canonical_bytes};
pub use capability::{Capability, CapabilityError, CapabilitySet, Grant};
pub use correctness::{
    Artifact, AssuranceVerdict, Certificate, CertificateBody, CertificateError, CheckerVerdict,
    Contract, DischargeState, EvidenceRef, Identity, Obligation, ObligationStatement, Producer,
    ProducerKind, Proposal, RecordedTime, RepairOwner, Subject, VerifierRecord, VerifierTrust,
};
pub use definition::{AssuranceMode, Definition};
pub use digest::{DIGEST_PREFIX, Digest, DigestError};
pub use effect::{
    BoundaryError, Crossing, Effect, EffectError, EffectKind, Idempotency, ProtectedBoundary,
    Reversibility,
};
pub use graph::{
    Combine, ConditionalBranch, ControlEdge, Endpoint, Graph, GraphBuilder, GraphError, Hyperedge,
    TrustDerivation,
};
pub use id::{Identifier, IdentifierError};
pub use loop_region::{
    BudgetKind, Continuation, FailureKind, Invariant, IterationRecord, LoopBudget, LoopError,
    LoopOutcome, LoopSpec, ProgressMeasure, RepairRoute, Route, StopReason,
};
pub use node::{Node, NodeError, NodeKind, ProviderBinding, ResourceBudget};
pub use port::{InputPort, OutputPort, TrustLevel, TrustRequirement};
pub use reader::{from_json_slice, verify_canonical};
pub use region::{Region, RegionError, RegionKind};
pub use trust::{
    ProvenanceLoss, RawTrustClass, RawVerificationRecord, RecordVerdict, TrustClass, TrustError,
    VerificationRecord,
};
pub use value::{Field, IntegerRange, LengthBounds, SchemaMismatch, ValueSchema};
pub use version::{
    BUNDLE_SCHEMA_VERSION, CERTIFICATE_SCHEMA_VERSION, IR_SCHEMA_VERSION, SchemaVersion,
    SchemaVersionError, read_compatible,
};

use serde::Serialize;

/// The digest of a value, taken over its canonical bytes.
///
/// This is the only way a digest should be produced for anything this milestone
/// persists. Digesting some other serialization would mean two honest readers
/// could disagree about what a certificate covers.
///
/// # Errors
///
/// Returns [`CanonicalError`] when the value has no canonical encoding, for
/// example because it contains floating point or unnormalized text.
pub fn digest_of<T: Serialize>(value: &T) -> Result<Digest, CanonicalError> {
    Ok(Digest::of(&to_canonical_bytes(value)?))
}
