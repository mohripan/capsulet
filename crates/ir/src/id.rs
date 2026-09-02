//! Identifiers used throughout the IR.
//!
//! One validated type instead of a dozen bare `String` fields. Identifiers end
//! up in digests, in policy references, and in certificate text, so a blank or
//! whitespace-padded identifier is not a cosmetic problem: two documents that
//! look identical to a reader would digest differently, and a policy that names
//! `"publish "` would silently fail to match the boundary called `"publish"`.

use std::fmt::{self, Display};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;

/// Why an identifier was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IdentifierError {
    #[error("an identifier must not be empty")]
    Empty,
    #[error("identifier `{found}` has leading or trailing whitespace")]
    Padded { found: String },
    #[error("identifier `{found}` contains `{character}`, which is not allowed")]
    Character { found: String, character: char },
    #[error("identifier `{found}` is longer than {maximum} characters")]
    TooLong { found: String, maximum: usize },
}

/// The longest identifier the IR accepts.
const MAXIMUM_LENGTH: usize = 200;

/// A validated identifier: letters, digits, and `_ - . : / @ +`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identifier(String);

impl Identifier {
    /// Validates an identifier.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierError`] when the text is empty, padded, too long, or
    /// contains a character outside the accepted set.
    pub fn parse(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        if value.is_empty() {
            return Err(IdentifierError::Empty);
        }
        if value.trim() != value {
            return Err(IdentifierError::Padded { found: value });
        }
        if value.chars().count() > MAXIMUM_LENGTH {
            return Err(IdentifierError::TooLong {
                found: value,
                maximum: MAXIMUM_LENGTH,
            });
        }
        if let Some(character) = value.chars().find(|character| !is_allowed(*character)) {
            return Err(IdentifierError::Character {
                found: value,
                character,
            });
        }
        Ok(Self(value))
    }

    /// The identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

const fn is_allowed(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || matches!(character, '_' | '-' | '.' | ':' | '/' | '@' | '+')
}

impl Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for Identifier {
    type Err = IdentifierError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for Identifier {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Identifier {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(de::Error::custom)
    }
}
