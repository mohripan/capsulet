//! The pre-resolved records a proposal is decided against.
//!
//! The kernel performs no I/O. The caller loads everything a derivation
//! references into a snapshot first, which keeps the kernel a pure total
//! function and makes a decision reproducible from stored inputs alone.

use std::collections::HashMap;

use capsulet_core::{Authority, Claim, Evidence, Source, SourceContent};

use crate::phrasing::Phrasebook;

#[derive(Debug, Clone)]
pub struct Snapshot {
    evidence: HashMap<String, Evidence>,
    sources: HashMap<String, Source>,
    /// Keyed by `(source_id, content_hash)` so a span always resolves against
    /// the exact bytes it named, never merely the newest version.
    contents: HashMap<(String, String), SourceContent>,
    claims: HashMap<String, Claim>,
    /// The weakest source authority a trust step may accept.
    ///
    /// Policy, so it travels with what a decision was made against rather than
    /// with the proposal: the party a check constrains does not get to set it. A
    /// proposal may still ask for more, and gets the stricter of the two.
    authority_floor: Authority,
    /// How a relation is stated in text: the phrasings and cues a citation is
    /// judged against. Empty unless a contract supplies them, and then nothing
    /// is stated — a citation can only mention its terms.
    phrasebook: Phrasebook,
}

impl Default for Snapshot {
    /// An empty snapshot, held to the lowest floor — the floor every decision
    /// was effectively made under before the snapshot carried one.
    fn default() -> Self {
        Self {
            evidence: HashMap::new(),
            sources: HashMap::new(),
            contents: HashMap::new(),
            claims: HashMap::new(),
            authority_floor: Authority::Low,
            phrasebook: Phrasebook::new(),
        }
    }
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

    /// Holds every trust step decided against this snapshot to `floor`.
    #[must_use]
    pub const fn with_authority_floor(mut self, floor: Authority) -> Self {
        self.authority_floor = floor;
        self
    }

    #[must_use]
    pub const fn authority_floor(&self) -> Authority {
        self.authority_floor
    }

    /// Judges every citation decided against this snapshot by `phrasebook`.
    #[must_use]
    pub fn with_phrasebook(mut self, phrasebook: Phrasebook) -> Self {
        self.phrasebook = phrasebook;
        self
    }

    #[must_use]
    pub const fn phrasebook(&self) -> &Phrasebook {
        &self.phrasebook
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
