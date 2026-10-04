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
use std::fmt;

use unicode_segmentation::UnicodeSegmentation;

use crate::ir::Proposition;
use crate::text;

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

/// Why a phrasing could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhrasingError {
    pub template: String,
    pub reason: &'static str,
}

impl fmt::Display for PhrasingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "phrasing {:?} {}", self.template, self.reason)
    }
}

impl std::error::Error for PhrasingError {}

/// One way a contract says a relation is stated.
///
/// A template with exactly one `{from}` slot, exactly one `{to}` slot, and at
/// least one literal word. The word is what makes it a phrasing of *this*
/// relation: `"{from} {to}"` would state every relation at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phrasing {
    template: String,
}

impl Phrasing {
    /// Reads a phrasing.
    ///
    /// # Errors
    ///
    /// Returns [`PhrasingError`] when the template does not have exactly one
    /// `{from}` and one `{to}`, has any other brace, or has no literal word.
    pub fn parse(template: &str) -> Result<Self, PhrasingError> {
        let refuse = |reason| PhrasingError {
            template: template.to_string(),
            reason,
        };
        if template.matches("{from}").count() != 1 {
            return Err(refuse("needs exactly one {from} slot"));
        }
        if template.matches("{to}").count() != 1 {
            return Err(refuse("needs exactly one {to} slot"));
        }
        let literal = template.replace("{from}", " ").replace("{to}", " ");
        if literal.contains(['{', '}']) {
            return Err(refuse("has a brace that is not a slot"));
        }
        if !literal.chars().any(char::is_alphanumeric) {
            return Err(refuse(
                "has no literal word, so it would state any relation between its terms",
            ));
        }
        Ok(Self {
            template: template.to_string(),
        })
    }

    /// The template as the contract wrote it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.template
    }

    fn instantiate(&self, from: &str, to: &str) -> String {
        self.template
            .replace("{from}", from.trim())
            .replace("{to}", to.trim())
    }
}

/// Where a declared phrasing was found to state a relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// The phrasing, as the contract wrote it.
    pub phrasing: String,
    /// The sentence it was found in.
    pub sentence: String,
}

/// The phrasings a decision reads, by relation, and the cues a contract adds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Phrasebook {
    relations: BTreeMap<String, Vec<Phrasing>>,
    cues: Vec<String>,
}

impl Phrasebook {
    /// An empty phrasebook: nothing is declared, so nothing is stated.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
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
            if let Some((phrasing, _)) = wanted
                .iter()
                .find(|(_, instance)| text::contains_folded(&folded, instance))
            {
                return Some(Statement {
                    phrasing: phrasing.as_str().to_string(),
                    sentence: sentence.trim().to_string(),
                });
            }
        }
        None
    }
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
