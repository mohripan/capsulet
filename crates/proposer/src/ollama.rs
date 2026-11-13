//! A proposer backed by a local Ollama model.
//!
//! The model is asked for one thing only: a single hop from one cited excerpt
//! to one proposition. Composition, arithmetic, and trust decisions belong to
//! the kernel, so nothing here asks a small model to do what it is bad at.

use std::time::Duration;

use capsulet_kernel::{Phrasebook, Proposal, Proposition, Query, QuestionReading, Rule, Slot};
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
    /// The relation the model says the question asks about. With the two terms
    /// below, its reading of a question asked only in words: the kernel holds the
    /// answer to it, and a reviewer decides whether the words ask it. Ignored
    /// when the caller pinned the question to a query.
    #[serde(default)]
    pub asks_relation: String,
    /// The subject the question names, or `?` when the subject is what it asks for.
    #[serde(default)]
    pub asks_subject: String,
    /// The object the question names, or `?` when the object is what it asks for.
    #[serde(default)]
    pub asks_object: String,
    /// Why the question asks that, in one sentence.
    #[serde(default)]
    pub asks_rationale: String,
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
    /// When the caller pinned the question to `query`, the model is told what it
    /// asks and offers no reading of its own. Otherwise the proposal carries the
    /// model's reading of the question, which the kernel records as a residual.
    ///
    /// # Errors
    ///
    /// Returns [`ProposerError`] when the server is unreachable, the response
    /// cannot be parsed, or the model cited outside the pinned alphabet.
    pub async fn propose(
        &self,
        question: &str,
        query: Option<&Query>,
        alphabet: &EvidenceAlphabet,
        phrasebook: &Phrasebook,
    ) -> Result<(Proposal, RawProposal), ProposerError> {
        if alphabet.is_empty() {
            return Err(ProposerError::EmptyAlphabet);
        }
        let raw = self.request(question, query, alphabet, phrasebook).await?;
        let mut proposal = build_proposal_in(&raw, alphabet, phrasebook)?;
        if query.is_none() {
            proposal.question_reading = question_reading(&raw);
        }
        Ok((proposal, raw))
    }

    async fn request(
        &self,
        question: &str,
        query: Option<&Query>,
        alphabet: &EvidenceAlphabet,
        phrasebook: &Phrasebook,
    ) -> Result<RawProposal, ProposerError> {
        let predicates = declared_predicates(phrasebook);
        let response = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&json!({
                "model": self.model,
                "prompt": build_prompt(question, query, alphabet, &predicates),
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
            read_with: Vec::new(),
        })
    };

    let derivation = if phrasebook.statement_in(excerpt, &goal).is_some() {
        Rule::Trust {
            premise: cite(goal.clone()),
            min_authority: "low".to_string(),
        }
    } else {
        let terms_occur =
            phrasebook.subject_in(excerpt, &goal) && phrasebook.object_in(excerpt, &goal).is_some();
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
    Proposal {
        schema_version: Proposal::current_schema_version(),
        question_reading: None,
        goal,
        derivation,
    }
}

/// Assembles the derivation as [`build_proposal`] does, from the alphabet entry
/// the model cited, and then reads the cited clause with what its document's
/// structure says it depends on.
///
/// The phrasing check above looks at the excerpt alone, and an excerpt can state
/// a relation that its document qualifies elsewhere: "subject to Section 9.3",
/// or a later clause that applies "notwithstanding" this one. Quoted bare, the
/// kernel refuses that citation as qualified, and the answer comes back
/// `rejected` when the honest outcome is a reading for a person to decide. So,
/// by the same lookup the kernel makes ([`Phrasebook::reading_needs`]):
///
/// - each provision or definition the clause depends on is read with it, by the
///   alphabet entries that cover it;
/// - a clause something qualifies becomes a reading of the clause read that
///   way, whose rationale names what qualifies it, so it comes back
///   `conditional` until a reviewer decides it.
///
/// Without the source text or the cited entry's span there is no structure to
/// read, and the proposal is [`build_proposal`]'s.
///
/// # Errors
///
/// Returns [`ProposerError::OutsideAlphabet`] when the model cited an id the
/// alphabet does not hold.
pub fn build_proposal_in(
    raw: &RawProposal,
    alphabet: &EvidenceAlphabet,
    phrasebook: &Phrasebook,
) -> Result<Proposal, ProposerError> {
    let Some(entry) = alphabet.entry(&raw.evidence_id) else {
        return Err(ProposerError::OutsideAlphabet {
            evidence_id: raw.evidence_id.clone(),
        });
    };
    let mut proposal = build_proposal(raw, &entry.excerpt, phrasebook);
    let (Some(text), Some((start, end))) = (alphabet.source_text(&entry.source_id), entry.span)
    else {
        return Ok(proposal);
    };
    let needs = phrasebook.reading_needs(text, start, end);
    let mut read_with: Vec<String> = Vec::new();
    for &(a, b) in &needs.read_with {
        for covering in alphabet.overlapping(&entry.source_id, a, b) {
            if covering.evidence_id != entry.evidence_id
                && !read_with.contains(&covering.evidence_id)
            {
                read_with.push(covering.evidence_id.clone());
            }
        }
    }
    proposal.derivation = match proposal.derivation {
        Rule::Trust { premise, .. } if !needs.qualified.is_empty() => Rule::Interpret {
            premise: with_read_with(*premise, &read_with),
            proposition: proposal.goal.clone(),
            rationale: qualified_rationale(&raw.rationale, &needs.qualified),
        },
        Rule::Trust {
            premise,
            min_authority,
        } => Rule::Trust {
            premise: with_read_with(*premise, &read_with),
            min_authority,
        },
        Rule::Interpret {
            premise,
            proposition,
            rationale,
        } => Rule::Interpret {
            premise: with_read_with(*premise, &read_with),
            proposition,
            rationale,
        },
        other => other,
    };
    Ok(proposal)
}

fn with_read_with(premise: Rule, read_with: &[String]) -> Box<Rule> {
    match premise {
        Rule::Cite {
            evidence_id,
            proposition,
            ..
        } => Box::new(Rule::Cite {
            evidence_id,
            proposition,
            read_with: read_with.to_vec(),
        }),
        other => Box::new(other),
    }
}

/// What a reviewer of a qualified clause's reading is told.
fn qualified_rationale(model: &str, qualified: &[String]) -> String {
    let because = qualified.join("; ");
    let said = model.trim().trim_end_matches('.');
    let lead = if said.is_empty() {
        String::new()
    } else {
        format!("{said}. ")
    };
    format!(
        "{lead}The cited clause does not say this outright: {because}. It is read here with \
         what qualifies it, and whether it still says this, read that way, is for a reviewer \
         to decide."
    )
}

/// The model's reading of the question, when it offered one.
///
/// `?` marks the slot the question asks for, and anything else is a term the
/// question names. A reading with no relation is no reading, so the kernel
/// refuses the answer for not saying how it read the question. An empty term is
/// passed on as written, and refused by the kernel as the empty term it is,
/// rather than guessed into a wanted slot.
#[must_use]
pub fn question_reading(raw: &RawProposal) -> Option<QuestionReading> {
    if raw.asks_relation.trim().is_empty() {
        return None;
    }
    let slot = |term: &str| {
        if term.trim() == "?" {
            Slot::Wanted
        } else {
            Slot::bound(term.trim())
        }
    };
    Some(QuestionReading {
        query: Query::new(
            raw.asks_relation.trim(),
            slot(&raw.asks_subject),
            slot(&raw.asks_object),
        ),
        rationale: raw.asks_rationale.clone(),
    })
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
            "rationale": { "type": "string" },
            "asks_relation": predicate,
            "asks_subject": { "type": "string" },
            "asks_object": { "type": "string" },
            "asks_rationale": { "type": "string" }
        },
        "required": [
            "subject", "predicate", "object", "evidence_id",
            "quote", "needs_interpretation", "rationale",
            "asks_relation", "asks_subject", "asks_object", "asks_rationale"
        ]
    })
}

fn build_prompt(
    question: &str,
    query: Option<&Query>,
    alphabet: &EvidenceAlphabet,
    predicates: &[&str],
) -> String {
    let asks = match query {
        Some(query) => format!(
            "THE QUESTION ASKS FOR: {}. Answer with that predicate and the terms it names; \
             ? is the one to find. Copy the same into asks_relation, asks_subject and \
             asks_object.\n\n",
            query.canonical()
        ),
        None => "Also say what the question asks, before answering it:\n\
                 - asks_relation: the predicate the question asks about.\n\
                 - asks_subject and asks_object: the term the question names in that position, \
                 copied from the question, or ? for the one it asks you to find.\n\
                 - asks_rationale: one short sentence saying why the question asks that.\n\n"
            .to_string(),
    };
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
         {asks}\
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
            asks_relation: String::new(),
            asks_subject: String::new(),
            asks_object: String::new(),
            asks_rationale: String::new(),
        }
    }

    #[test]
    fn a_reading_marks_the_wanted_slot_and_keeps_named_terms_as_written() {
        let mut proposal = raw(
            "the Contoso contract",
            "Acme renewed the Contoso contract.",
            false,
        );
        assert_eq!(super::question_reading(&proposal), None);

        proposal.asks_relation = "renewed".to_string();
        proposal.asks_subject = " ? ".to_string();
        proposal.asks_object = " the Contoso contract ".to_string();
        proposal.asks_rationale = "asks who renewed it".to_string();
        let reading = super::question_reading(&proposal).expect("a reading");
        assert_eq!(
            reading.query.canonical(),
            "renewed(?, the Contoso contract)"
        );
        assert_eq!(reading.rationale, "asks who renewed it");

        // An empty term is passed on, for the kernel to refuse, not guessed wanted.
        proposal.asks_object = String::new();
        let reading = super::question_reading(&proposal).expect("a reading");
        assert!(reading.query.problem().is_some());
    }

    #[test]
    fn a_pinned_query_is_put_to_the_model_and_words_alone_ask_for_its_reading() {
        let alphabet = crate::alphabet::EvidenceAlphabet::from_evidence(&[]);
        let query = capsulet_kernel::Query::new(
            "renewed",
            capsulet_kernel::Slot::Wanted,
            capsulet_kernel::Slot::bound("the Contoso contract"),
        );
        let pinned = super::build_prompt("Who renewed it?", Some(&query), &alphabet, &["renewed"]);
        assert!(pinned.contains("THE QUESTION ASKS FOR: renewed(?, the Contoso contract)"));
        let words = super::build_prompt("Who renewed it?", None, &alphabet, &["renewed"]);
        assert!(words.contains("asks_relation: the predicate the question asks about"));
        assert!(!words.contains("THE QUESTION ASKS FOR"));
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
        snapshot_over(phrasebook, EXCERPT)
    }

    fn snapshot_over(phrasebook: Phrasebook, excerpt: &str) -> Snapshot {
        let scope = MemoryScope::new("acme", "prod").expect("scope");
        let source_id = SourceId::new("src_1").expect("source id");
        let content = SourceContent::new(source_id.clone(), excerpt).expect("content");
        let evidence = Evidence::new(
            EvidenceId::new("ev_1").expect("evidence id"),
            scope.clone(),
            source_id.clone(),
            "para-1",
            excerpt,
            "2026-03-02T00:00:00Z",
        )
        .expect("evidence")
        .with_span(EvidenceSpan::new(0, excerpt.len(), content.content_hash()).expect("span"));
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
    fn a_typed_answer_runs_through_the_kernel_without_a_literal_object_match() {
        let excerpt = "Acme is $1.2 million.";
        let mut answer = raw("USD 1200000", excerpt, false);
        answer.predicate = "value".into();
        let book = Phrasebook::new()
            .with_object_type("value", capsulet_kernel::value::ValueType::Money)
            .with_relation(
                "value",
                vec![Phrasing::parse("{from} is {to}").expect("phrasing")],
            );
        for (currency, verdict) in [
            (None, Verdict::Conditional),
            (Some("USD"), Verdict::Accepted),
        ] {
            let book = book
                .clone()
                .with_value_policy(currency, None)
                .expect("policy");
            let proposal = build_proposal(&answer, excerpt, &book);
            assert!(matches!(proposal.derivation, Rule::Trust { .. }));
            assert_eq!(
                check(&proposal, &snapshot_over(book, excerpt)).verdict,
                verdict
            );
        }
    }

    #[test]
    fn an_unphrased_typed_reading_keeps_the_kernels_value_residual() {
        let excerpt = "Acme mentioned 05/01/2026.";
        let mut answer = raw("2026-01-05", excerpt, false);
        answer.predicate = "value".into();
        let book =
            Phrasebook::new().with_object_type("value", capsulet_kernel::value::ValueType::Date);
        let proposal = build_proposal(&answer, excerpt, &book);
        let certificate = check(&proposal, &snapshot_over(book, excerpt));
        assert_eq!(certificate.verdict, Verdict::Conditional);
        assert!(
            certificate
                .residuals
                .iter()
                .any(|r| r.rationale.contains("2026-01-05") && r.rationale.contains("2026-05-01")),
            "{certificate:?}"
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
            None,
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
        assert_eq!(predicates, [] as [&str; 0]);
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

    const AGREEMENT: &str = "1. Definitions\n\
\"Affiliate\" means an entity that controls, or is controlled by, a party.\n\
9. Termination\n\
9.1. Subject to Section 9.3, either party may terminate this agreement on 60 days' notice.\n\
9.2. The Supplier may assign this agreement to an Affiliate.\n\
9.3. Notwithstanding Section 9.1, no party may terminate in the first year.\n";

    fn clauses() -> Phrasebook {
        let phrasing = |template: &str| Phrasing::parse(template).expect("a phrasing");
        Phrasebook::new()
            .with_relation("may_terminate", vec![phrasing("{from} may terminate {to}")])
            .with_relation(
                "may_assign_to",
                vec![phrasing("{from} may assign this agreement to an {to}")],
            )
            .with_context(capsulet_kernel::ContextPolicy {
                provisions: vec!["{ref}.".to_string()],
                references: vec!["Section {ref}".to_string()],
                subject_to: vec!["subject to".to_string()],
                overrides: vec!["notwithstanding".to_string()],
                conditions: vec!["provided that".to_string()],
                definitions: vec!["\"{term}\" means".to_string()],
            })
    }

    /// The agreement chunked and stored as the ask endpoint does it, and the
    /// alphabet built from it, with or without the text it came from.
    fn chunked(with_text: bool) -> (Snapshot, crate::alphabet::EvidenceAlphabet) {
        let scope = MemoryScope::new("acme", "legal").expect("scope");
        let source_id = SourceId::new("agreement").expect("source id");
        let content = SourceContent::new(source_id.clone(), AGREEMENT).expect("content");
        let evidence: Vec<Evidence> = crate::alphabet::chunk_into_spans(&content)
            .into_iter()
            .enumerate()
            .map(|(index, (start, end))| {
                Evidence::new(
                    EvidenceId::new(format!("ev_{index}")).expect("evidence id"),
                    scope.clone(),
                    source_id.clone(),
                    format!("bytes {start}..{end}"),
                    &AGREEMENT[start..end],
                    "inline",
                )
                .expect("evidence")
                .with_span(EvidenceSpan::new(start, end, content.content_hash()).expect("span"))
            })
            .collect();
        let mut alphabet = crate::alphabet::EvidenceAlphabet::from_evidence(&evidence);
        if with_text {
            alphabet = alphabet.with_source(&content);
        }
        let mut snapshot = Snapshot::new()
            .with_phrasebook(clauses())
            .with_source(
                Source::new(
                    source_id,
                    scope,
                    "inline",
                    None,
                    "Agreement",
                    Authority::High,
                )
                .expect("source"),
            )
            .with_source_content(content);
        for item in evidence {
            snapshot = snapshot.with_evidence(item);
        }
        (snapshot, alphabet)
    }

    fn citing(
        alphabet: &crate::alphabet::EvidenceAlphabet,
        clause: &str,
        answer: (&str, &str, &str),
    ) -> RawProposal {
        let entry = alphabet
            .entries()
            .iter()
            .find(|entry| entry.excerpt == clause)
            .expect("the clause is one chunk");
        RawProposal {
            subject: answer.0.to_string(),
            predicate: answer.1.to_string(),
            object: answer.2.to_string(),
            evidence_id: entry.evidence_id.clone(),
            quote: clause.to_string(),
            needs_interpretation: false,
            rationale: String::new(),
            asks_relation: String::new(),
            asks_subject: String::new(),
            asks_object: String::new(),
            asks_rationale: String::new(),
        }
    }

    #[test]
    fn a_qualified_clause_is_read_with_what_qualifies_it_and_left_for_a_reviewer() {
        let clause =
            "Subject to Section 9.3, either party may terminate this agreement on 60 days' notice.";
        let answer = ("either party", "may_terminate", "this agreement");
        let (snapshot, alphabet) = chunked(true);
        let raw = citing(&alphabet, clause, answer);

        let proposal = super::build_proposal_in(&raw, &alphabet, &clauses()).expect("in alphabet");
        let Rule::Interpret {
            premise, rationale, ..
        } = &proposal.derivation
        else {
            panic!("a qualified clause is a reading: {:?}", proposal.derivation);
        };
        assert!(
            rationale.contains("\"subject to\" provision 9.3"),
            "{rationale}"
        );
        let Rule::Cite { read_with, .. } = premise.as_ref() else {
            panic!("it rests on the citation");
        };
        let qualifying = alphabet
            .entries()
            .iter()
            .find(|entry| entry.excerpt.starts_with("Notwithstanding Section 9.1"))
            .expect("provision 9.3 is a chunk");
        assert!(read_with.contains(&qualifying.evidence_id), "{read_with:?}");

        let certificate = check(&proposal, &snapshot);
        assert_eq!(certificate.verdict, Verdict::Conditional, "{certificate:?}");
        assert_eq!(
            certificate.residuals.len(),
            1,
            "{:?}",
            certificate.residuals
        );
        assert!(
            certificate.residuals[0]
                .evidence_ids
                .contains(&qualifying.evidence_id),
            "the reviewer is shown the clause that qualifies it"
        );

        // Without the document, the clause is quoted bare and refused for it.
        let (_, bare) = chunked(false);
        let proposal = super::build_proposal_in(&raw, &bare, &clauses()).expect("in alphabet");
        assert!(matches!(proposal.derivation, Rule::Trust { .. }));
        let certificate = check(&proposal, &snapshot);
        assert!(
            certificate
                .errors
                .iter()
                .any(|error| error.code == "relation_qualified"),
            "{certificate:?}"
        );
    }

    #[test]
    fn a_clause_using_a_defined_term_is_read_with_its_definition() {
        let clause = "The Supplier may assign this agreement to an Affiliate.";
        let (snapshot, alphabet) = chunked(true);
        let raw = citing(
            &alphabet,
            clause,
            ("The Supplier", "may_assign_to", "Affiliate"),
        );
        let proposal = super::build_proposal_in(&raw, &alphabet, &clauses()).expect("in alphabet");
        assert!(matches!(proposal.derivation, Rule::Trust { .. }));
        let certificate = check(&proposal, &snapshot);
        assert_eq!(certificate.verdict, Verdict::Accepted, "{certificate:?}");

        let (_, bare) = chunked(false);
        let proposal = super::build_proposal_in(&raw, &bare, &clauses()).expect("in alphabet");
        let certificate = check(&proposal, &snapshot);
        assert_eq!(certificate.verdict, Verdict::Conditional);
        assert!(certificate.residuals[0].rationale.contains("defined term"));
    }

    #[test]
    fn an_id_outside_the_alphabet_is_refused_before_anything_is_built() {
        let (_, alphabet) = chunked(true);
        let mut raw = raw("the Contoso contract", "x", false);
        raw.evidence_id = "ev_invented".to_string();
        assert!(matches!(
            super::build_proposal_in(&raw, &alphabet, &clauses()),
            Err(super::ProposerError::OutsideAlphabet { .. })
        ));
    }
}
