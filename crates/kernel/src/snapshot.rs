//! The pre-resolved records a proposal is decided against.
//!
//! The kernel performs no I/O. The caller loads everything a derivation
//! references into a snapshot first, which keeps the kernel a pure total
//! function and makes a decision reproducible from stored inputs alone.

use std::collections::HashMap;

use capsulet_core::{Authority, Claim, Evidence, Source, SourceContent};

#[derive(Debug, Default, Clone)]
pub struct Snapshot {
    evidence: HashMap<String, Evidence>,
    sources: HashMap<String, Source>,
    /// Keyed by `(source_id, content_hash)` so a span always resolves against
    /// the exact bytes it named, never merely the newest version.
    contents: HashMap<(String, String), SourceContent>,
    claims: HashMap<String, Claim>,
}

impl Snapshot {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_evidence(mut self, evidence: Evidence) -> Self {
        self.evidence
            .insert(evidence.id().as_str().to_string(), evidence);
        self
    }

    #[must_use]
    pub fn with_source(mut self, source: Source) -> Self {
        self.sources
            .insert(source.id().as_str().to_string(), source);
        self
    }

    #[must_use]
    pub fn with_source_content(mut self, content: SourceContent) -> Self {
        self.contents.insert(
            (
                content.source_id().as_str().to_string(),
                content.content_hash().to_string(),
            ),
            content,
        );
        self
    }

    #[must_use]
    pub fn with_claim(mut self, claim: Claim) -> Self {
        self.claims.insert(claim.id().as_str().to_string(), claim);
        self
    }

    #[must_use]
    pub fn evidence(&self, id: &str) -> Option<&Evidence> {
        self.evidence.get(id)
    }

    #[must_use]
    pub fn source(&self, id: &str) -> Option<&Source> {
        self.sources.get(id)
    }

    #[must_use]
    pub fn source_content(&self, source_id: &str, content_hash: &str) -> Option<&SourceContent> {
        self.contents
            .get(&(source_id.to_string(), content_hash.to_string()))
    }

    #[must_use]
    pub fn claim(&self, id: &str) -> Option<&Claim> {
        self.claims.get(id)
    }

    #[must_use]
    pub fn authority_of(&self, source_id: &str) -> Option<Authority> {
        self.sources.get(source_id).map(Source::authority)
    }
}
