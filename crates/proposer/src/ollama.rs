//! A proposer backed by a local Ollama model.
//!
//! The model is asked for one thing only: a single hop from one cited excerpt
//! to one proposition. Composition, arithmetic, and trust decisions belong to
//! the kernel, so nothing here asks a small model to do what it is bad at.

use std::time::Duration;

use capsulet_kernel::{Proposal, Proposition, Rule};
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
    /// Whether reaching the proposition required reading rather than quoting.
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

    /// Asks the model for one derivation answering `question` from `alphabet`.
    ///
    /// # Errors
    ///
    /// Returns [`ProposerError`] when the server is unreachable, the response
    /// cannot be parsed, or the model cited outside the pinned alphabet.
    pub async fn propose(
        &self,
        question: &str,
        alphabet: &EvidenceAlphabet,
    ) -> Result<(Proposal, RawProposal), ProposerError> {
        if alphabet.is_empty() {
            return Err(ProposerError::EmptyAlphabet);
        }
        let raw = self.request(question, alphabet).await?;

        if !alphabet.contains(&raw.evidence_id) {
            return Err(ProposerError::OutsideAlphabet {
                evidence_id: raw.evidence_id.clone(),
            });
        }

        Ok((build_proposal(&raw), raw))
    }

    async fn request(
        &self,
        question: &str,
        alphabet: &EvidenceAlphabet,
    ) -> Result<RawProposal, ProposerError> {
        let response = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&json!({
                "model": self.model,
                "prompt": build_prompt(question, alphabet),
                "stream": false,
                "format": output_schema(),
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

/// Assembles the kernel derivation implied by a flat proposal.
///
/// When the model says it only quoted, the claim's object must appear in the
/// span, so a direct `Cite` is offered and the kernel will reject it if that is
/// untrue. When the model says it had to read, the `Cite` is narrowed to the
/// quote itself — a fact the kernel can check — and the step from quote to
/// proposition becomes an explicit, recorded `Interpret`.
#[must_use]
pub fn build_proposal(raw: &RawProposal) -> Proposal {
    let goal = Proposition::new(&raw.subject, &raw.predicate, &raw.object);
    let derivation = if raw.needs_interpretation {
        Rule::Interpret {
            premise: Box::new(Rule::Cite {
                evidence_id: raw.evidence_id.clone(),
                proposition: Proposition::new("quote", "text", &raw.quote),
            }),
            proposition: goal.clone(),
            rationale: raw.rationale.clone(),
        }
    } else {
        Rule::Trust {
            premise: Box::new(Rule::Cite {
                evidence_id: raw.evidence_id.clone(),
                proposition: goal.clone(),
            }),
            min_authority: "low".to_string(),
        }
    };
    Proposal { goal, derivation }
}

/// JSON schema handed to Ollama so the output is structurally constrained.
fn output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "subject": { "type": "string" },
            "predicate": { "type": "string" },
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

fn build_prompt(question: &str, alphabet: &EvidenceAlphabet) -> String {
    format!(
        "You extract a single fact from quoted evidence. Do not use outside knowledge.\n\
         \n\
         EVIDENCE (you may only cite these ids):\n{}\n\
         \n\
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
    use capsulet_kernel::Rule;

    use super::{RawProposal, build_proposal};

    fn raw(needs_interpretation: bool) -> RawProposal {
        RawProposal {
            subject: "Acme".to_string(),
            predicate: "renewed".to_string(),
            object: "the Contoso contract".to_string(),
            evidence_id: "ev_1".to_string(),
            quote: "Acme renewed the Contoso contract".to_string(),
            needs_interpretation,
            rationale: "the sentence states it".to_string(),
        }
    }

    #[test]
    fn a_quoted_answer_becomes_a_trusted_citation() {
        let proposal = build_proposal(&raw(false));

        let Rule::Trust { premise, .. } = &proposal.derivation else {
            panic!("expected trust, got {:?}", proposal.derivation);
        };
        let Rule::Cite { proposition, .. } = premise.as_ref() else {
            panic!("expected cite");
        };
        // The claim's own object is what the kernel will look for in the span.
        assert_eq!(proposition.object, "the Contoso contract");
    }

    #[test]
    fn an_inferred_answer_becomes_an_explicit_interpretation() {
        let proposal = build_proposal(&raw(true));

        let Rule::Interpret { premise, .. } = &proposal.derivation else {
            panic!("expected interpret, got {:?}", proposal.derivation);
        };
        let Rule::Cite { proposition, .. } = premise.as_ref() else {
            panic!("expected cite");
        };
        // The citation is narrowed to the quote, which is checkable; the step
        // from quote to claim is the residual.
        assert_eq!(proposition.object, "Acme renewed the Contoso contract");
    }

    #[test]
    fn the_goal_always_matches_the_model_stated_fact() {
        for needs in [true, false] {
            let raw = raw(needs);
            let proposal = build_proposal(&raw);
            assert_eq!(proposal.goal.subject, raw.subject);
            assert_eq!(proposal.goal.predicate, raw.predicate);
            assert_eq!(proposal.goal.object, raw.object);
        }
    }
}
