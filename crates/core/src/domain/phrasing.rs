//! The syntax of a phrasing: how a memory contract says a relation is stated.
//!
//! A phrasing is a template — `"{from} acquired {to}"` — that a contract
//! declares for a relation. What a phrasing *means* for a citation is decided by
//! the correctness kernel; what one may *look like* is decided here, once, so a
//! contract and a pinned snapshot document cannot disagree about whether a
//! template is a phrasing at all.

use std::fmt;

/// Why a template is not a phrasing.
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

    /// The template with its slots filled.
    ///
    /// Filled in one pass over the template, never by successive replacement:
    /// a term that happens to contain `{to}` must not have it filled in turn.
    #[must_use]
    pub fn instantiate(&self, from: &str, to: &str) -> String {
        let mut out = String::with_capacity(self.template.len() + from.len() + to.len());
        let mut rest = self.template.as_str();
        while let Some(at) = rest.find('{') {
            out.push_str(&rest[..at]);
            let tail = &rest[at..];
            if let Some(after) = tail.strip_prefix("{from}") {
                out.push_str(from.trim());
                rest = after;
            } else if let Some(after) = tail.strip_prefix("{to}") {
                out.push_str(to.trim());
                rest = after;
            } else {
                // Unreachable for a parsed phrasing, which has no other brace;
                // kept literal rather than dropped if it ever were not.
                out.push('{');
                rest = &tail[1..];
            }
        }
        out.push_str(rest);
        out
    }
}
