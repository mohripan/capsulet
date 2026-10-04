//! A proposer backed by a local Ollama model.
//!
//! The model is asked for one thing only: a single hop from one cited excerpt
//! to one proposition. Composition, arithmetic, and trust decisions belong to
//! the kernel, so nothing here asks a small model to do what it is bad at.

use std::time::Duration;

use capsulet_kernel::text::contains_term;
use capsulet_kernel::{Phrasebook, Proposal, Proposition, Rule};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::alphabet::EvidenceAlphabet;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:11434";
const DEFAULT_TIMEOUT_SECONDS: u64 = 180;

/// The flat shape the model is constrained to emit.
///
/// Deliberately not the kernel's recursive [`Rule`] tree: a 1.5B model produces
/// a flat record far more reliably, and the derivation it implies is assembled
/// here where the structure can be guaranteed rather than hoped for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawProposal {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub evidence_id: String,
    /// The exact substring of the excerpt the model says supports the claim.
    pub quote: String,
    /// Whether the model says reaching the proposition required reading rather
    /// than quoting.
    ///
    /// Recorded, and no longer acted on: which derivation an answer gets is
    /// decided by the kernel's own phrasing check (see [`build_proposal`]), not
    /// by the party that check constrains.
    #[serde(default)]
    pub needs_interpretation: bool,
    #[serde(default)]
    pub rationale: String,
}

#[derive(Debug, Error)]
pub enum ProposerError {
    #[error("ollama request failed: {0}")]
    Transport(String),
    #[error("ollama returned status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("could not parse the model's output as a proposal: {0}")]
    Malformed(String),
    #[error("the model cited {evidence_id}, which is not in the pinned alphabet")]
    OutsideAlphabet { evidence_id: String },
    #[error("the evidence alphabet is empty, so there is nothing to cite")]
    EmptyAlphabet,
}

#[derive(Debug, Clone)]
pub struct OllamaProposer {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl OllamaProposer {
    /// Creates a proposer against a local Ollama server.
    ///
    /// # Errors
    ///
    /// Returns [`ProposerError`] when the HTTP client cannot be built.
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ProposerError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECONDS))
            .build()
            .map_err(|error| ProposerError::Transport(error.to_string()))?;
        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: model.into(),
        })
    }

    /// Creates a proposer from `CAPSULET_OLLAMA_URL` and `CAPSULET_OLLAMA_MODEL`.
    ///
    /// # Errors
    ///
    /// Returns [`ProposerError`] when the HTTP client cannot be built.
    pub fn from_env() -> Result<Self, ProposerError> {
        let base_url =
            std::env::var("CAPSULET_OLLAMA_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        let model =
            std::env::var("CAPSULET_OLLAMA_MODEL").unwrap_or_else(|_| "qwen2.5:1.5b".to_string());
        Self::new(base_url, model)
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Asks the model for one derivation answering `question` from `alphabet`,
    /// judged by the phrasings in `phrasebook` — the ones the kernel will decide
    /// it against.
    ///
    /// # Errors
    ///
    /// Returns [`ProposerError`] when the server is unreachable, the response
    /// cannot be parsed, or the model cited outside the pinned alphabet.
    pub async fn propose(
        &self,
        question: &str,
        alphabet: &EvidenceAlphabet,
        phrasebook: &Phrasebook,
    ) -> Result<(Proposal, RawProposal), ProposerError> {
        if alphabet.is_empty() {
            return Err(ProposerError::EmptyAlphabet);
        }
        let raw = self.request(question, alphabet, phrasebook).await?;

        let Some(entry) = alphabet.entry(&raw.evidence_id) else {
            return Err(ProposerError::OutsideAlphabet {
                evidence_id: raw.evidence_id.clone(),
            });
        };

        Ok((build_proposal(&raw, &entry.excerpt, phrasebook), raw))
    }

    async fn request(
        &self,
        question: &str,
        alphabet: &EvidenceAlphabet,
        phrasebook: &Phrasebook,
    ) -> Result<RawProposal, ProposerError> {
        let predicates = declared_predicates(phrasebook);
        let response = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&json!({
                "model": self.model,
                "prompt": build_prompt(question, alphabet, &predicates),
                "stream": false,
                "format": output_schema(&predicates),
                "options": { "temperature": 0.0, "num_predict": 512 },
            }))
            .send()
            .await
            .map_err(|error| ProposerError::Transport(error.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| ProposerError::Transport(error.to_string()))?;
        if !status.is_success() {
            return Err(ProposerError::Status {
                status: status.as_u16(),
                body,
            });
        }

        let envelope: Value = serde_json::from_str(&body)
            .map_err(|error| ProposerError::Malformed(format!("envelope: {error}")))?;
        let generated = envelope
            .get("response")
            .and_then(Value::as_str)
            .ok_or_else(|| ProposerError::Malformed("no response field".to_string()))?;
        serde_json::from_str::<RawProposal>(generated)
            .map_err(|error| ProposerError::Malformed(format!("{error}: {generated}")))
    }
}

/// Assembles the kernel derivation implied by a flat proposal, cited from
/// `excerpt`.
///
/// Which derivation an answer gets is decided by the check the kernel will
/// run, not by the model's word:
///
/// - when a phrasing in `phrasebook` states the answer in the excerpt, it is a
///   trusted citation, which the kernel discharges;
/// - when none does but both of the answer's terms occur there, it is a reading
///   of a citation of those terms, which the kernel records as a residual;
/// - otherwise it is a reading of the quote the model offered, which is the
///   most the excerpt can be shown to contain.
///
/// The last case used to cite the quote under the subject `"quote"`. Once a
/// citation had to contain its subject, that word had to occur in the excerpt,
/// so every reading the proposer built was rejected, and no answer on this path
/// could come back `conditional`.
#[must_use]
pub fn build_proposal(raw: &RawProposal, excerpt: &str, phrasebook: &Phrasebook) -> Proposal {
    let goal = Proposition::new(&raw.subject, &raw.predicate, &raw.object);
    let cite = |proposition: Proposition| {
        Box::new(Rule::Cite {
            evidence_id: raw.evidence_id.clone(),
            proposition,
        })
    };

    let derivation = if phrasebook.statement_in(excerpt, &goal).is_some() {
        Rule::Trust {
            premise: cite(goal.clone()),
            min_authority: "low".to_string(),
        }
    } else {
        let terms_occur =
            contains_term(excerpt, &raw.subject) && contains_term(excerpt, &raw.object);
        let premise = if terms_occur {
            goal.clone()
        } else {
            Proposition::new(&raw.quote, "quotes", &raw.quote)
        };
        Rule::Interpret {
            premise: cite(premise),
            proposition: goal.clone(),
            rationale: raw.rationale.clone(),
        }
    };
    Proposal { goal, derivation }
}

/// The relations a contract declares phrasings for, in order.
///
/// When there are any, they are the only predicates a model may answer with,
/// the way the evidence alphabet closes the set of citable ids. A predicate
/// outside the set has no phrasing, so it could never be stated, and offering it
/// would only invite answers that can at best be readings.
fn declared_predicates(phrasebook: &Phrasebook) -> Vec<&str> {
    phrasebook.relations().map(|(name, _)| name).collect()
}

/// JSON schema handed to Ollama so the output is structurally constrained.
fn output_schema(predicates: &[&str]) -> Value {
    let predicate = if predicates.is_empty() {
        json!({ "type": "string" })
    } else {
        json!({ "type": "string", "enum": predicates })
    };
    json!({
        "type": "object",
        "properties": {
            "subject": { "type": "string" },
            "predicate": predicate,
            "object": { "type": "string" },
            "evidence_id": { "type": "string" },
            "quote": { "type": "string" },
            "needs_interpretation": { "type": "boolean" },
            "rationale": { "type": "string" }
        },
        "required": [
            "subject", "predicate", "object", "evidence_id",
            "quote", "needs_interpretation", "rationale"
        ]
    })
}

fn build_prompt(question: &str, alphabet: &EvidenceAlphabet, predicates: &[&str]) -> String {
    let predicate_rule = if predicates.is_empty() {
        String::new()
    } else {
        format!(
            "PREDICATES (the predicate must be exactly one of these):\n{}\n\n",
            predicates.join("\n")
        )
    };
    format!(
        "You extract a single fact from quoted evidence. Do not use outside knowledge.\n\
         \n\
         EVIDENCE (you may only cite these ids):\n{}\n\
         \n\
         {predicate_rule}\
         QUESTION: {question}\n\
         \n\
         Answer with one fact as subject, predicate, object.\n\
         Rules:\n\
         - evidence_id must be exactly one of the ids listed above.\n\
         - quote must be copied character for character from that evidence. Do not paraphrase it.\n\
         - If the object appears word for word inside the quote, set needs_interpretation to false.\n\
         - If you had to infer or rephrase to get the object, set needs_interpretation to true.\n\
         - rationale: one short sentence saying why the quote supports the fact.\n\
         - If the evidence does not answer the question, still pick the closest evidence id and \
           set needs_interpretation to true.\n",
        alphabet.render()
    )
}

#[cfg(test)]
mod tests {
    use capsulet_core::{
        Authority, Evidence, EvidenceId, EvidenceSpan, MemoryScope, Source, SourceContent, SourceId,
    };
    use capsulet_kernel::{Phrasebook, Phrasing, Rule, Snapshot, Verdict, check};

    use super::{RawProposal, build_proposal};

    const EXCERPT: &str = "Acme renewed the Contoso contract.";

    fn raw(object: &str, quote: &str, needs_interpretation: bool) -> RawProposal {
        RawProposal {
            subject: "Acme".to_string(),
            predicate: "renewed".to_string(),
            object: object.to_string(),
            evidence_id: "ev_1".to_string(),
            quote: quote.to_string(),
            needs_interpretation,
            rationale: "the sentence states it".to_string(),
        }
    }

    fn renewals() -> Phrasebook {
        Phrasebook::new().with_relation(
            "renewed",
            vec![Phrasing::parse("{from} renewed {to}").expect("a phrasing")],
        )
    }

    /// The snapshot the kernel would decide against: the excerpt, cited whole.
    ///
    /// Every test below runs what the proposer built through the kernel. A test
    /// of the shape alone is how a reading path that the kernel always rejected
    /// went unnoticed.
    fn snapshot(phrasebook: Phrasebook) -> Snapshot {
        let scope = MemoryScope::new("acme", "prod").expect("scope");
        let source_id = SourceId::new("src_1").expect("source id");
        let content = SourceContent::new(source_id.clone(), EXCERPT).expect("content");
        let evidence = Evidence::new(
            EvidenceId::new("ev_1").expect("evidence id"),
            scope.clone(),
            source_id.clone(),
            "para-1",
            EXCERPT,
            "2026-03-02T00:00:00Z",
        )
        .expect("evidence")
        .with_span(EvidenceSpan::new(0, EXCERPT.len(), content.content_hash()).expect("span"));
        Snapshot::new()
            .with_source(
                Source::new(source_id, scope, "document", None, "Memo", Authority::High)
                    .expect("source"),
            )
            .with_source_content(content)
            .with_evidence(evidence)
            .with_phrasebook(phrasebook)
    }

    #[test]
    fn a_stated_answer_becomes_a_trusted_citation_and_is_accepted() {
        let proposal = build_proposal(
            &raw(
                "the Contoso contract",
                "Acme renewed the Contoso contract",
                true,
            ),
            EXCERPT,
            &renewals(),
        );

        assert!(matches!(proposal.derivation, Rule::Trust { .. }));
        assert_eq!(
            check(&proposal, &snapshot(renewals())).verdict,
            Verdict::Accepted
        );
    }

    #[test]
    fn the_model_saying_it_quoted_does_not_make_an_answer_a_statement() {
        // No phrasing states `renewed`, so however sure the model is, the answer
        // is a reading, and the kernel records it as one rather than rejecting.
        let proposal = build_proposal(
            &raw(
                "the Contoso contract",
                "Acme renewed the Contoso contract",
                false,
            ),
            EXCERPT,
            &Phrasebook::new(),
        );

        assert!(matches!(proposal.derivation, Rule::Interpret { .. }));
        assert_eq!(
            check(&proposal, &snapshot(Phrasebook::new())).verdict,
            Verdict::Conditional
        );
    }

    #[test]
    fn a_reading_of_a_paraphrase_rests_on_the_quote_and_is_decidable() {
        // The answer's object is not in the excerpt, so the reading cites the
        // quote. It used to cite it under the subject "quote", which no excerpt
        // contained, so every such reading was rejected.
        let proposal = build_proposal(
            &raw(
                "a contract with Contoso",
                "renewed the Contoso contract",
                true,
            ),
            EXCERPT,
            &renewals(),
        );

        let Rule::Interpret { premise, .. } = &proposal.derivation else {
            panic!("expected a reading, got {:?}", proposal.derivation);
        };
        let Rule::Cite { proposition, .. } = premise.as_ref() else {
            panic!("expected a citation");
        };
        assert_eq!(proposition.subject, "renewed the Contoso contract");
        assert_eq!(
            check(&proposal, &snapshot(renewals())).verdict,
            Verdict::Conditional
        );
    }

    #[test]
    fn a_quote_the_excerpt_does_not_contain_is_still_rejected() {
        let proposal = build_proposal(
            &raw(
                "a contract with Contoso",
                "Acme terminated everything",
                true,
            ),
            EXCERPT,
            &renewals(),
        );

        let certificate = check(&proposal, &snapshot(renewals()));
        assert_eq!(certificate.verdict, Verdict::Rejected);
        assert_eq!(certificate.errors[0].code, "term_not_in_span");
    }

    #[test]
    fn a_contract_closes_the_set_of_predicates_a_model_may_answer_with() {
        let phrasebook = renewals();
        let predicates = super::declared_predicates(&phrasebook);
        assert_eq!(predicates, ["renewed"]);

        let schema = super::output_schema(&predicates);
        assert_eq!(
            schema["properties"]["predicate"]["enum"],
            serde_json::json!(["renewed"])
        );
        let prompt = super::build_prompt(
            "Who renewed?",
            &crate::alphabet::EvidenceAlphabet::from_entries(vec![]),
            &predicates,
        );
        assert!(prompt.contains("PREDICATES"), "{prompt}");
        assert!(prompt.contains("renewed"), "{prompt}");
    }

    #[test]
    fn without_a_contract_the_predicate_is_left_open() {
        let phrasebook = Phrasebook::new();
        let predicates = super::declared_predicates(&phrasebook);
        assert!(predicates.is_empty());
        assert!(
            super::output_schema(&predicates)["properties"]["predicate"]
                .get("enum")
                .is_none()
        );
    }

    #[test]
    fn the_goal_always_matches_the_model_stated_fact() {
        for phrasebook in [renewals(), Phrasebook::new()] {
            let raw = raw(
                "the Contoso contract",
                "Acme renewed the Contoso contract",
                false,
            );
            let proposal = build_proposal(&raw, EXCERPT, &phrasebook);
            assert_eq!(proposal.goal.subject, raw.subject);
            assert_eq!(proposal.goal.predicate, raw.predicate);
            assert_eq!(proposal.goal.object, raw.object);
        }
    }
}
