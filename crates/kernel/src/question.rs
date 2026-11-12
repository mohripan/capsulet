//! What an answer is held to: the question it was asked.
//!
//! A certificate used to say that a claim was grounded and nothing about whether
//! it answered anything. Asked which company acquired Contoso, about a text that
//! did not say, `Dana Whitfield | is the named account owner | Dana Whitfield` was
//! accepted: grounded, accurately cited, and beside the point.
//!
//! Whether a sentence asks for a fact is not decidable without a reading. Whether
//! a fact fills a *query* is. So a question is pinned in the snapshot with its
//! words and, when there is one, a [`Query`]: the relation asked about, the terms
//! it binds, and the slot it wants filled. The kernel holds the goal to the query
//! mechanically. Turning the words into a query is a reading, and it goes where
//! readings go: a query the caller pins is none, and one the proposer offers in a
//! [`QuestionReading`] is a residual naming the question, the query, and why.
//!
//! The question lives in the snapshot rather than the proposal because the party
//! a check constrains does not get to choose what it is checked against — the
//! same reason the authority floor lives there. A proposer may say how it read a
//! question asked in words, and that reading is what a reviewer is shown.

use serde::{Deserialize, Serialize};

use crate::ir::Proposition;
use crate::phrasing::Phrasebook;

/// One end of a query: a term the question names, or the one it asks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "slot", rename_all = "snake_case")]
pub enum Slot {
    /// The question names this term, so an answer must name it here too.
    Bound { term: String },
    /// What the question asks for. An answer fills it with a term.
    Wanted,
}

impl Slot {
    /// A slot the question binds to `term`.
    #[must_use]
    pub fn bound(term: impl Into<String>) -> Self {
        Self::Bound { term: term.into() }
    }

    fn render(&self) -> String {
        match self {
            Self::Bound { term } => term.trim().replace('\\', "\\\\").replace(',', "\\,"),
            Self::Wanted => "?".to_string(),
        }
    }
}

/// What a question asks, as a pattern over a contract's relations.
///
/// One relation, and at most one wanted slot: "which company acquired Contoso?"
/// is `acquired(?, Contoso)`, and "did Fabrikam acquire Contoso?" binds both. A
/// question that asks for a set, a comparison, or several facts at once is not
/// expressible yet, and is not squeezed into this shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    pub relation: String,
    pub subject: Slot,
    pub object: Slot,
}

impl Query {
    #[must_use]
    pub fn new(relation: impl Into<String>, subject: Slot, object: Slot) -> Self {
        Self {
            relation: relation.into(),
            subject,
            object,
        }
    }

    /// Why this is not a query an answer can be held to, if it is not one.
    #[must_use]
    pub fn problem(&self) -> Option<&'static str> {
        if self.relation.trim().is_empty() {
            return Some("it names no relation");
        }
        if matches!((&self.subject, &self.object), (Slot::Wanted, Slot::Wanted)) {
            return Some("it wants both ends of the relation, so nothing in it is asked about");
        }
        for slot in [&self.subject, &self.object] {
            if let Slot::Bound { term } = slot
                && term.trim().is_empty()
            {
                return Some("it binds an empty term");
            }
        }
        None
    }

    /// A rendering for people, such as `acquired(?, Contoso)`. No decision reads
    /// it; a `,` or `\` inside a term is escaped so two queries never render alike.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}({}, {})",
            self.relation.trim(),
            self.subject.render(),
            self.object.render()
        )
    }
}

/// A question an answer was asked, pinned with what it is decided against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    /// The words the caller asked, kept for whoever reviews a reading of them.
    /// No decision reads them.
    pub text: String,
    /// What the caller pinned the question to, when they did. Without one, an
    /// answer has to say how it read the question, and that reading is residual.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<Query>,
}

impl Question {
    /// A question asked in words alone.
    #[must_use]
    pub fn asked(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            query: None,
        }
    }

    /// The same question, pinned to `query`.
    #[must_use]
    pub fn with_query(mut self, query: Query) -> Self {
        self.query = Some(query);
        self
    }
}

/// How a proposer read a question that was asked only in words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionReading {
    pub query: Query,
    /// Why the words ask this, for the reviewer who decides whether they do.
    pub rationale: String,
}

/// How a goal filled a query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Answered {
    /// The confirmed resolutions a bound term was matched through. Empty when
    /// every bound term was found as written.
    pub resolutions: Vec<String>,
    /// Unresolved value readings of a bound typed object.
    pub residuals: Vec<String>,
}

/// Whether `goal` answers `query`, and if not, why not.
///
/// The predicate must be the asked relation, compared as the goal check compares
/// fields. Each bound term must name the same thing as the goal's term in that
/// role: as written under the citation text relation, by exact value for a typed
/// object, or through a name group reviewers confirmed. A wanted slot must be
/// filled.
pub(crate) fn answers(
    goal: &Proposition,
    query: &Query,
    phrasebook: &Phrasebook,
) -> Result<Answered, String> {
    let relation = query.relation.trim();
    if goal.predicate.trim() != relation {
        return Err(format!(
            "it states `{}`, and the question asks about `{relation}`",
            goal.predicate.trim()
        ));
    }
    let mut answered = Answered::default();
    for (role, slot, found) in [
        ("subject", &query.subject, &goal.subject),
        ("object", &query.object, &goal.object),
    ] {
        match slot {
            Slot::Wanted => {
                if found.trim().is_empty() {
                    return Err(format!("it leaves the wanted {role} empty"));
                }
            }
            Slot::Bound { term } => {
                let Some(same) = phrasebook.names_same(relation, role == "object", term, found)
                else {
                    return Err(format!(
                        "its {role} is {:?}, and the question asks about {:?}",
                        found.trim(),
                        term.trim()
                    ));
                };
                answered.resolutions.extend(same.resolutions);
                answered.residuals.extend(same.residuals);
            }
        }
    }
    answered.resolutions.sort();
    answered.resolutions.dedup();
    Ok(answered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_renders_its_wanted_slot_and_escapes_its_terms() {
        let query = Query::new("acquired", Slot::Wanted, Slot::bound("Contoso, Ltd"));
        assert_eq!(query.canonical(), "acquired(?, Contoso\\, Ltd)");
    }

    #[test]
    fn a_query_wanting_both_ends_or_naming_nothing_is_not_one() {
        assert!(
            Query::new("acquired", Slot::Wanted, Slot::Wanted)
                .problem()
                .is_some()
        );
        assert!(
            Query::new(" ", Slot::Wanted, Slot::bound("Contoso"))
                .problem()
                .is_some()
        );
        assert!(
            Query::new("acquired", Slot::bound(" "), Slot::Wanted)
                .problem()
                .is_some()
        );
        assert_eq!(
            Query::new("acquired", Slot::bound("Fabrikam"), Slot::bound("Contoso")).problem(),
            None
        );
    }

    #[test]
    fn a_goal_about_another_relation_or_term_does_not_answer() {
        let query = Query::new("acquired", Slot::Wanted, Slot::bound("Contoso"));
        let phrasebook = Phrasebook::new();
        let owner = Proposition::new(
            "Dana Whitfield",
            "is the named account owner",
            "Dana Whitfield",
        );
        assert!(answers(&owner, &query, &phrasebook).is_err());
        let other = Proposition::new("Fabrikam", "acquired", "Northwind");
        assert!(answers(&other, &query, &phrasebook).is_err());
        let fits = Proposition::new("Fabrikam", "acquired", "contoso");
        assert_eq!(answers(&fits, &query, &phrasebook), Ok(Answered::default()));
    }
}
