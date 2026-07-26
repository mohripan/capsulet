//! Untrusted proposers.
//!
//! A proposer produces candidate derivations for the kernel to decide. Nothing
//! here is trusted: the model may hallucinate an id, misquote a span, or invent
//! a reading, and every one of those is caught downstream. The proposer's only
//! obligations are to stay inside the pinned [`EvidenceAlphabet`] and to emit
//! something the kernel can parse.

pub mod alphabet;
pub mod ollama;

pub use alphabet::{AlphabetEntry, EvidenceAlphabet, chunk_into_spans};
pub use ollama::{OllamaProposer, ProposerError, RawProposal};
