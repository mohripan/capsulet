//! Phrasings: how a contract says a relation is stated in text.
//!
//! A citation used to ground a proposition's subject and object and conclude
//! its whole statement, predicate included. The predicate was written by the
//! proposer and checked against nothing, so a span that negated, reversed or
//! hedged a statement grounded it as readily as one that made it.
//!
//! A relation is now stated in a span only in a phrasing its contract
//! declares — `"{from} acquired {to}"`, `"{to} was acquired by {from}"` — found
//! inside one sentence, with the proposition's subject in the `{from}` slot and
//! its object in the `{to}` slot, under the citation text relation, and with no
//! cue in that sentence that it is negated, hedged, conditional or asked rather
//! than asserted.
//!
//! This is not comprehension and does not pretend to be. "Rumours that
//! Fabrikam acquired Contoso were false" contains a declared phrasing, and no
//! cue list is ever complete. What it changes is the claim: a statement means
//! the span contains a phrasing somebody declared for the relation, with the
//! right terms in the right roles — narrow, checkable, and recorded on the
//! certificate. Where there is none, a citation says only that the terms are
//! mentioned, and getting from there to the relation is a reading.
//!
//! Phrasings and the extra cues a contract adds are data, pinned in the
//! snapshot a decision is made against. The default cues and the matching
//! below are part of the claim-reasoning rules: changing either changes what a
//! citation states, and has to bump [`crate::family::CLAIM_REASONING_RULES`].

use std::collections::BTreeMap;

use capsulet_core::MemoryContractAst;
pub use capsulet_core::{Phrasing, PhrasingError};
use unicode_segmentation::UnicodeSegmentation;

use crate::ir::Proposition;
use crate::text;
use crate::value::{self, ValueType};

/// A pinned value policy cannot introduce an unknown currency or date order.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{detail}")]
pub struct ValuePolicyError {
    pub detail: String,
}

/// Words that, anywhere in a sentence, stop it from stating a relation.
///
/// Negation, denial and falsity; conditionals and modal hedges; and reports at
/// one remove. Matched as whole words under the citation text relation, so
/// "can't" is a cue and "cantilever" is not. A contract may add cues and cannot
/// remove these.
///
/// `may` is missing on purpose. It is also a month, and a statement made "in
/// May" would be refused for it, while the hedge it would catch — "Fabrikam may
/// acquire Contoso" — already fails to match `"{from} acquired {to}"`.
pub const DEFAULT_CUES: &[&str] = &[
    // Negation, denial, falsity.
    "not",
    "no",
    "never",
    "neither",
    "nor",
    "without",
    "cannot",
    "can't",
    "don't",
    "doesn't",
    "didn't",
    "isn't",
    "aren't",
    "wasn't",
    "weren't",
    "hasn't",
    "haven't",
    "hadn't",
    "won't",
    "wouldn't",
    "couldn't",
    "shouldn't",
    "mustn't",
    "deny",
    "denies",
    "denied",
    "false",
    "untrue",
    // Conditionals and hedges.
    "if",
    "unless",
    "whether",
    "might",
    "could",
    "would",
    "should",
    "plans to",
    "planned to",
    "intends to",
    "intended to",
    "expects to",
    "expected to",
    // Reports at one remove.
    "reportedly",
    "allegedly",
    "rumor",
    "rumors",
    "rumored",
    "rumour",
    "rumours",
    "rumoured",
];

/// Where a declared phrasing was found to state a relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// The phrasing, as the contract wrote it.
    pub phrasing: String,
    /// The sentence it was found in.
    pub sentence: String,
    /// Unresolved value readings; a matching phrasing does not discharge these.
    pub residuals: Vec<String>,
}

/// The phrasings a decision reads, by relation, and the cues a contract adds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Phrasebook {
    relations: BTreeMap<String, Vec<Phrasing>>,
    cues: Vec<String>,
    object_types: BTreeMap<String, ValueType>,
    default_currency: Option<String>,
    date_order: Option<String>,
}

impl Phrasebook {
    /// An empty phrasebook: nothing is declared, so nothing is stated.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The phrasings and cues a memory contract declares.
    #[must_use]
    pub fn from_contract(contract: &MemoryContractAst) -> Self {
        let mut phrasebook = contract
            .relation_types()
            .iter()
            .filter(|relation| !relation.phrasings().is_empty())
            .fold(Self::new(), |phrasebook, relation| {
                phrasebook.with_relation(relation.name(), relation.phrasings().to_vec())
            });
        for cue in contract.statement_policy().cues() {
            phrasebook = phrasebook.with_cue(cue);
        }
        for relation in contract.relation_types() {
            if let Some(kind) = ValueType::parse(relation.to_entity()) {
                phrasebook
                    .object_types
                    .insert(relation.name().to_string(), kind);
            }
        }
        phrasebook.default_currency = contract
            .statement_policy()
            .default_currency()
            .map(str::to_string);
        phrasebook.date_order = contract.statement_policy().date_order().map(str::to_string);
        phrasebook
    }

    /// Declares the phrasings that state `relation`.
    #[must_use]
    pub fn with_relation(mut self, relation: &str, phrasings: Vec<Phrasing>) -> Self {
        self.relations
            .entry(relation.trim().to_string())
            .or_default()
            .extend(phrasings);
        self
    }

    /// Adds a cue to [`DEFAULT_CUES`] for every decision this phrasebook reads.
    #[must_use]
    pub fn with_cue(mut self, cue: &str) -> Self {
        self.cues.push(cue.to_string());
        self
    }

    /// Declares how this relation's object is read, independently of phrasings.
    #[must_use]
    pub fn with_object_type(mut self, relation: &str, kind: ValueType) -> Self {
        self.object_types.insert(relation.trim().to_string(), kind);
        self
    }

    /// Pins the explicit policies that disambiguate values.
    ///
    /// # Errors
    /// Returns [`ValuePolicyError`] for a currency outside the pinned registry
    /// or an order other than `day_first` / `month_first`.
    pub fn with_value_policy(
        mut self,
        currency: Option<&str>,
        order: Option<&str>,
    ) -> Result<Self, ValuePolicyError> {
        if currency.is_some_and(|code| !capsulet_core::is_currency_code(code)) {
            return Err(ValuePolicyError {
                detail: "unknown default_currency".into(),
            });
        }
        if order.is_some_and(|order| !matches!(order, "day_first" | "month_first")) {
            return Err(ValuePolicyError {
                detail: "unknown date_order".into(),
            });
        }
        self.default_currency = currency.map(str::to_string);
        self.date_order = order.map(str::to_string);
        Ok(self)
    }

    /// The phrasings declared for `relation`, if any.
    #[must_use]
    pub fn phrasings(&self, relation: &str) -> &[Phrasing] {
        self.relations
            .get(relation.trim())
            .map_or(&[], Vec::as_slice)
    }

    /// The relations with at least one declared phrasing, in order.
    pub fn relations(&self) -> impl Iterator<Item = (&str, &[Phrasing])> {
        self.relations
            .iter()
            .map(|(name, phrasings)| (name.as_str(), phrasings.as_slice()))
    }

    /// The cues a contract added, beyond [`DEFAULT_CUES`].
    #[must_use]
    pub fn added_cues(&self) -> &[String] {
        &self.cues
    }

    /// Whether the object occurs, by the endpoint's declared text or value relation.
    #[must_use]
    pub fn object_in(&self, excerpt: &str, proposition: &Proposition) -> Option<Vec<String>> {
        match self.object_types.get(proposition.predicate.trim()) {
            Some(&kind) => value::find(
                kind,
                excerpt,
                &proposition.object,
                self.default_currency.as_deref(),
                self.date_order.as_deref(),
            ),
            None => text::contains_term(excerpt, &proposition.object).then(Vec::new),
        }
    }

    /// The sentence of `excerpt` that states `proposition` in a declared
    /// phrasing of its predicate, if one does.
    #[must_use]
    pub fn statement_in(&self, excerpt: &str, proposition: &Proposition) -> Option<Statement> {
        let phrasings = self.phrasings(&proposition.predicate);
        if phrasings.is_empty() {
            return None;
        }
        let wanted: Vec<(&Phrasing, String)> = phrasings
            .iter()
            .map(|phrasing| {
                let instance = phrasing.instantiate(&proposition.subject, &proposition.object);
                (phrasing, text::fold(&instance))
            })
            .collect();
        let cues: Vec<String> = DEFAULT_CUES
            .iter()
            .copied()
            .chain(self.cues.iter().map(String::as_str))
            .map(text::fold)
            .filter(|cue| !cue.is_empty())
            .collect();

        for sentence in sentences(excerpt) {
            if is_a_question(&sentence) {
                continue;
            }
            let folded = text::fold(&sentence);
            if cues.iter().any(|cue| text::contains_folded(&folded, cue)) {
                continue;
            }
            for (phrasing, instance) in &wanted {
                let found = match self.object_types.get(proposition.predicate.trim()) {
                    Some(&kind) => typed_statement(
                        &folded,
                        phrasing,
                        proposition,
                        kind,
                        self.default_currency.as_deref(),
                        self.date_order.as_deref(),
                    ),
                    None => text::contains_folded(&folded, instance).then(Vec::new),
                };
                if let Some(residuals) = found {
                    return Some(Statement {
                        phrasing: phrasing.as_str().to_string(),
                        sentence: sentence.trim().to_string(),
                        residuals,
                    });
                }
            }
        }
        None
    }
}

fn typed_statement(
    sentence: &str,
    phrasing: &Phrasing,
    proposition: &Proposition,
    kind: ValueType,
    currency: Option<&str>,
    order: Option<&str>,
) -> Option<Vec<String>> {
    let matcher = value::Matcher::new(kind, &proposition.object, currency, order)?;
    // Split the validated template, not instantiated subject text: a subject
    // containing braces must not become another slot.
    let (prefix, suffix) = phrasing.as_str().split_once("{to}")?;
    let prefix = literal(&prefix.replace("{from}", proposition.subject.trim()));
    let suffix = literal(&suffix.replace("{from}", proposition.subject.trim()));
    let boundaries = text::word_boundaries(sentence);
    for &at in &boundaries {
        if !sentence[at..].starts_with(&prefix) {
            continue;
        }
        let start = at + prefix.len();
        if boundaries.binary_search(&start).is_err() {
            continue;
        }
        for &end in boundaries[boundaries.partition_point(|&at| at <= start)..]
            .iter()
            .take_while(|&&end| end - start <= 128)
        {
            if !sentence[end..].starts_with(&suffix)
                || boundaries.binary_search(&(end + suffix.len())).is_err()
                || !value::closed_region(sentence, start, end)
            {
                continue;
            }
            if let Some(found) = matcher.compare_folded(&sentence[start..end]) {
                return Some(found);
            }
        }
    }
    None
}

fn literal(input: &str) -> String {
    // Sentinels keep edge whitespace through folding without mistaking an
    // ignored character for a space; remove only the added outer characters.
    text::fold(&format!("#{input}#"))
        .strip_prefix('#')
        .and_then(|text| text.strip_suffix('#'))
        .unwrap_or_default()
        .to_string()
}

/// The sentences of `excerpt`, by Unicode's sentence boundaries (UAX #29).
///
/// A single line break is read as a space first: hard-wrapped text, which is
/// what PDF and email extraction produce, is one sentence across its lines,
/// and Unicode would otherwise end a sentence at every one. A blank line still
/// ends a paragraph, and with it a sentence.
fn sentences(excerpt: &str) -> Vec<String> {
    let unwrapped = unwrap_lines(excerpt);
    unwrapped
        .split_sentence_bounds()
        .map(str::to_string)
        .collect()
}

/// `text` with every whitespace run holding fewer than two line breaks made
/// a single space, and every other run a paragraph break.
fn unwrap_lines(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    for character in text.chars() {
        if character.is_whitespace() {
            run.push(character);
            continue;
        }
        flush_whitespace(&mut out, &mut run);
        out.push(character);
    }
    flush_whitespace(&mut out, &mut run);
    out
}

fn flush_whitespace(out: &mut String, run: &mut String) {
    if run.is_empty() {
        return;
    }
    let breaks = run
        .chars()
        .filter(|character| matches!(character, '\n' | '\u{2028}' | '\u{2029}'))
        .count();
    out.push_str(if breaks >= 2 { "\n\n" } else { " " });
    run.clear();
}

/// Whether a sentence asks rather than asserts.
///
/// Closing quotes and brackets are looked through, so `"Did it?"` is a question.
fn is_a_question(sentence: &str) -> bool {
    sentence
        .trim_end()
        .trim_end_matches(['"', '\'', ')', ']', '\u{201D}', '\u{2019}', '\u{00BB}'])
        .ends_with(['?', '\u{FF1F}', '\u{061F}'])
}
