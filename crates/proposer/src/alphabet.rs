//! The closed set of evidence a proposer is allowed to cite.
//!
//! This is the anti-fabrication guarantee, and it does not depend on the model
//! cooperating: retrieval pins the legal ids before generation, the prompt only
//! offers those ids, and the kernel resolves citations against the same pinned
//! set. A model that invents an id produces a dangling reference the kernel
//! rejects and routes to retrieval — it can never reach a certificate.

use std::collections::BTreeMap;

use capsulet_core::{Evidence, SourceContent, content_digest};
use serde::{Deserialize, Serialize};

/// One citable excerpt offered to the proposer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlphabetEntry {
    pub evidence_id: String,
    pub source_id: String,
    pub excerpt: String,
    /// The bytes of its source the excerpt is, when it was taken from a span.
    /// Not part of the digest: the digest names what may be cited, and the span
    /// is where it sits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<(usize, usize)>,
}

/// A run-pinned alphabet, with a digest so a proposal can be tied to the exact
/// candidate set it was generated against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceAlphabet {
    entries: Vec<AlphabetEntry>,
    digest: String,
    /// The text of each source the entries were taken from, by source id, when
    /// the caller supplied it. What lets a proposal read a clause with the rest
    /// of its document. Not part of the digest, for the same reason as a span.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    sources: BTreeMap<String, String>,
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
                span: item.span().map(|span| (span.start(), span.end())),
            })
            .collect();
        Self::from_entries(entries)
    }

    /// Supplies the text the entries from `content`'s source were taken from,
    /// so a proposal can read a cited clause with the rest of its document.
    #[must_use]
    pub fn with_source(mut self, content: &SourceContent) -> Self {
        self.sources.insert(
            content.source_id().as_str().to_string(),
            content.text().to_string(),
        );
        self
    }

    /// The text of `source_id`, when it was supplied.
    #[must_use]
    pub fn source_text(&self, source_id: &str) -> Option<&str> {
        self.sources.get(source_id).map(String::as_str)
    }

    /// The entries of `source_id` that overlap `start..end`, in order.
    pub fn overlapping<'a>(
        &'a self,
        source_id: &'a str,
        start: usize,
        end: usize,
    ) -> impl Iterator<Item = &'a AlphabetEntry> + 'a {
        self.entries.iter().filter(move |entry| {
            entry.source_id == source_id && entry.span.is_some_and(|(a, b)| a < end && start < b)
        })
    }

    #[must_use]
    pub fn from_entries(entries: Vec<AlphabetEntry>) -> Self {
        let joined = entries
            .iter()
            .map(|entry| format!("{}:{}", entry.evidence_id, entry.excerpt))
            .collect::<Vec<_>>()
            .join("\n");
        let digest = content_digest(joined.as_bytes());
        Self {
            entries,
            digest,
            sources: BTreeMap::new(),
        }
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
///
/// A full stop with a letter or digit right after it ends nothing: it is inside
/// "Section 9.3" or "1.5 million", and splitting there would cut a reference
/// from the clause it qualifies.
#[must_use]
pub fn chunk_into_spans(content: &SourceContent) -> Vec<(usize, usize)> {
    let text = content.text();
    let mut spans = Vec::new();
    let mut start = 0usize;
    let bytes = text.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        let inside = *byte == b'.' && bytes.get(index + 1).is_some_and(u8::is_ascii_alphanumeric);
        if matches!(byte, b'.' | b'!' | b'?' | b'\n') && !inside {
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
            assert_ne!(content.text()[start..end].trim(), "");
        }
    }

    #[test]
    fn a_dotted_reference_stays_in_the_clause_it_qualifies() {
        let text = "9.1. Subject to Section 9.3, either party may terminate on notice. \
                    The fee is 1.5 million.\n";
        let content =
            SourceContent::new(SourceId::new("src_1").expect("source id"), text).expect("content");
        let chunks: Vec<&str> = chunk_into_spans(&content)
            .into_iter()
            .map(|(start, end)| &text[start..end])
            .collect();
        assert_eq!(
            chunks,
            [
                "9.1.",
                "Subject to Section 9.3, either party may terminate on notice.",
                "The fee is 1.5 million.",
            ]
        );
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
            span: None,
        }]);

        assert_ne!(first.digest(), second.digest());
        assert!(second.contains("ev_1"));
        assert!(!second.contains("ev_2"));

        // Where an excerpt sits, and the text around it, are not what may be
        // cited, so they do not change what the alphabet is.
        let placed = EvidenceAlphabet::from_entries(vec![super::AlphabetEntry {
            span: Some((0, 26)),
            ..second.entries()[0].clone()
        }])
        .with_source(
            &SourceContent::new(
                SourceId::new("src_1").expect("source id"),
                "Acme renewed the contract.",
            )
            .expect("content"),
        );
        assert_eq!(placed.digest(), second.digest());
        assert_eq!(
            placed.source_text("src_1"),
            Some("Acme renewed the contract.")
        );
    }
}
