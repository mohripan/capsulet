//! The one digest type used everywhere a digest appears in this milestone.
//!
//! A digest is always SHA-256 over canonical bytes, and always renders as
//! `sha256:<64 lowercase hex characters>`. There is no second spelling: a value
//! that does not parse as that form is not a digest, and refusing it at the
//! parse boundary keeps a malformed reference from ever reaching a certificate.

use std::fmt::{self, Display, Write as _};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

/// The textual prefix every digest carries.
pub const DIGEST_PREFIX: &str = "sha256:";

/// Why a textual digest could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DigestError {
    #[error("digest must start with `{DIGEST_PREFIX}`, found `{found}`")]
    MissingPrefix { found: String },
    #[error("digest must carry 64 hexadecimal characters, found {length}")]
    Length { length: usize },
    #[error("digest must be lowercase hexadecimal, found `{found}`")]
    NotLowercaseHex { found: String },
}

/// A SHA-256 content digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest([u8; 32]);

impl Digest {
    /// Digests the given bytes.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        let output = Sha256::digest(bytes);
        let mut raw = [0_u8; 32];
        raw.copy_from_slice(&output);
        Self(raw)
    }

    /// The raw 32 bytes, for callers that need to compare or store them.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Display for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(DIGEST_PREFIX)?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for Digest {
    type Err = DigestError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let hex = value
            .strip_prefix(DIGEST_PREFIX)
            .ok_or_else(|| DigestError::MissingPrefix {
                found: value.to_string(),
            })?;
        if hex.len() != 64 {
            return Err(DigestError::Length { length: hex.len() });
        }

        let mut raw = [0_u8; 32];
        for (index, byte) in raw.iter_mut().enumerate() {
            let pair = &hex[index * 2..index * 2 + 2];
            *byte = decode_hex_pair(pair).ok_or_else(|| DigestError::NotLowercaseHex {
                found: pair.to_string(),
            })?;
        }
        Ok(Self(raw))
    }
}

fn decode_hex_pair(pair: &str) -> Option<u8> {
    let mut value = 0_u8;
    for character in pair.chars() {
        let nibble = match character {
            '0'..='9' => character as u8 - b'0',
            'a'..='f' => character as u8 - b'a' + 10,
            _ => return None,
        };
        value = value * 16 + nibble;
    }
    Some(value)
}

impl Serialize for Digest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut rendered = String::with_capacity(DIGEST_PREFIX.len() + 64);
        rendered.push_str(DIGEST_PREFIX);
        for byte in self.0 {
            let _ = write!(rendered, "{byte:02x}");
        }
        serializer.serialize_str(&rendered)
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let rendered = String::deserialize(deserializer)?;
        rendered.parse().map_err(de::Error::custom)
    }
}
