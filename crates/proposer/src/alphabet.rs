//! The closed set of evidence a proposer is allowed to cite.
//!
//! This is the anti-fabrication guarantee, and it does not depend on the model
//! cooperating: retrieval pins the legal ids before generation, the prompt only
//! offers those ids, and the kernel resolves citations against the same pinned
//! set. A model that invents an id produces a dangling reference the kernel
//! rejects and routes to retrieval — it can never reach a certificate.

use capsulet_core::{Evidence, SourceContent, content_digest};
use serde::{Deserialize, Serialize};

/// One citable excerpt offered to the proposer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlphabetEntry {
    pub evidence_id: String,
    pub source_id: String,
    pub excerpt: String,
}

/// A run-pinned alphabet, with a digest so a proposal can be tied to the exact
/// candidate set it was generated against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceAlphabet {
    entries: Vec<AlphabetEntry>,
    digest: String,
}

impl EvidenceAlphabet {
    /// Builds an alphabet from evidence that is actually citable.
    ///
    /// Evidence without a span is skipped: it cannot ground a claim in the
    /// kernel, so offering it to a proposer would only invite rejections.
    #[must_use]
    pub fn from_evidence(evidence: &[Evidence]) -> Self {
        let entries: Vec<AlphabetEntry> = evidence
            .iter()
            .filter(|item| item.span().is_some())
            .map(|item| AlphabetEntry {
                evidence_id: item.id().as_str().to_string(),
                source_id: item.source_id().as_str().to_string(),
                excerpt: item.excerpt().to_string(),
            })
            .collect();
        Self::from_entries(entries)
    }

    #[must_use]
    pub fn from_entries(entries: Vec<AlphabetEntry>) -> Self {
        let joined = entries
            .iter()
            .map(|entry| format!("{}:{}", entry.evidence_id, entry.excerpt))
            .collect::<Vec<_>>()
            .join("\n");
        let digest = content_digest(joined.as_bytes());
        Self { entries, digest }
    }

    #[must_use]
    pub fn entries(&self) -> &[AlphabetEntry] {
        &self.entries
    }

    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn contains(&self, evidence_id: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.evidence_id == evidence_id)
    }

    #[must_use]
    pub fn entry(&self, evidence_id: &str) -> Option<&AlphabetEntry> {
        self.entries
            .iter()
            .find(|entry| entry.evidence_id == evidence_id)
    }

    /// Renders the alphabet for a prompt.
    #[must_use]
    pub fn render(&self) -> String {
        self.entries
            .iter()
            .map(|entry| format!("[{}] {}", entry.evidence_id, entry.excerpt))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Splits source text into sentence-sized citable evidence with exact spans.
///
/// Deterministic and byte-exact: the spans it produces are what the kernel will
/// later re-derive, so a chunk that does not round-trip is a bug here, not a
/// model failure.
#[must_use]
pub fn chunk_into_spans(content: &SourceContent) -> Vec<(usize, usize)> {
    let text = content.text();
    let mut spans = Vec::new();
    let mut start = 0usize;
    let bytes = text.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(byte, b'.' | b'!' | b'?' | b'\n') {
            let end = index + 1;
            if !text[start..end].trim().is_empty() {
                let trimmed_start = start + leading_space(&text[start..end]);
                if trimmed_start < end {
                    spans.push((trimmed_start, end));
                }
            }
            start = end;
        }
    }
    if start < text.len() && !text[start..].trim().is_empty() {
        let trimmed_start = start + leading_space(&text[start..]);
        spans.push((trimmed_start, text.len()));
    }
    spans
}

fn leading_space(value: &str) -> usize {
    value.len() - value.trim_start().len()
}

#[cfg(test)]
mod tests {
    use capsulet_core::SourceId;

    use super::{EvidenceAlphabet, chunk_into_spans};
    use capsulet_core::SourceContent;

    #[test]
    fn chunks_round_trip_to_exact_byte_ranges() {
        let content = SourceContent::new(
            SourceId::new("src_1").expect("source id"),
            "Acme renewed the contract. Notice is 30 days. Owner is Dana.",
        )
        .expect("content");

        let spans = chunk_into_spans(&content);

        assert_eq!(spans.len(), 3);
        for (start, end) in spans {
            // Every produced span must be a valid slice, or the kernel would
            // reject citations this module created.
            assert!(content.text().is_char_boundary(start));
            assert!(content.text().is_char_boundary(end));
            assert!(!content.text()[start..end].trim().is_empty());
        }
    }

    #[test]
    fn chunking_handles_multibyte_text_without_splitting_characters() {
        let content = SourceContent::new(
            SourceId::new("src_1").expect("source id"),
            "Le café a renouvelé le contrat. Préavis de 30 jours.",
        )
        .expect("content");

        for (start, end) in chunk_into_spans(&content) {
            assert!(content.text().is_char_boundary(start));
            assert!(content.text().is_char_boundary(end));
        }
    }

    #[test]
    fn alphabet_digest_changes_with_its_contents() {
        let first = EvidenceAlphabet::from_entries(vec![]);
        let second = EvidenceAlphabet::from_entries(vec![super::AlphabetEntry {
            evidence_id: "ev_1".to_string(),
            source_id: "src_1".to_string(),
            excerpt: "Acme renewed the contract.".to_string(),
        }]);

        assert_ne!(first.digest(), second.digest());
        assert!(second.contains("ev_1"));
        assert!(!second.contains("ev_2"));
    }
}
