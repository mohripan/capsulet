//! Schema identity for everything this crate persists.
//!
//! Every persisted root object carries its schema version as data, so a reader
//! never has to guess which shape it was handed. Unknown majors fail closed: a
//! reader that cannot prove it understands a document refuses it rather than
//! interpreting it permissively.

use std::borrow::Cow;
use std::fmt::{self, Display};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;

/// The namespace of a workflow definition document.
pub const IR_NAMESPACE: &str = "capsulet.ir";
/// The namespace of a platform certificate document.
pub const CERTIFICATE_NAMESPACE: &str = "capsulet.certificate";
/// The namespace of a certificate bundle manifest.
pub const BUNDLE_NAMESPACE: &str = "capsulet.bundle";

/// The IR schema version this build writes.
pub const IR_SCHEMA_VERSION: SchemaVersion = SchemaVersion::new(IR_NAMESPACE, 1);
/// The certificate schema version this build writes.
pub const CERTIFICATE_SCHEMA_VERSION: SchemaVersion = SchemaVersion::new(CERTIFICATE_NAMESPACE, 1);
/// The bundle schema version this build writes.
pub const BUNDLE_SCHEMA_VERSION: SchemaVersion = SchemaVersion::new(BUNDLE_NAMESPACE, 1);

/// Why a schema version could not be read or accepted.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SchemaVersionError {
    #[error("schema version `{found}` is not `<namespace>/v<major>`")]
    Malformed { found: String },
    #[error("expected schema namespace `{expected}`, found `{found}`")]
    Namespace { expected: String, found: String },
    #[error("schema major version {found} is not supported; this build reads {supported}")]
    UnsupportedMajor { found: u32, supported: u32 },
}

/// A namespace plus a major version, rendered as `capsulet.ir/v1`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaVersion {
    namespace: Cow<'static, str>,
    major: u32,
}

impl SchemaVersion {
    /// Builds a schema version from a namespace and major.
    #[must_use]
    pub const fn new(namespace: &'static str, major: u32) -> Self {
        Self {
            namespace: Cow::Borrowed(namespace),
            major,
        }
    }

    /// The namespace, such as `capsulet.ir`.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// The major version. A change here is a breaking change.
    #[must_use]
    pub const fn major(&self) -> u32 {
        self.major
    }
}

impl Display for SchemaVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/v{}", self.namespace, self.major)
    }
}

impl FromStr for SchemaVersion {
    type Err = SchemaVersionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let malformed = || SchemaVersionError::Malformed {
            found: value.to_string(),
        };
        let (namespace, major) = value.rsplit_once("/v").ok_or_else(malformed)?;
        if namespace.is_empty() || major.is_empty() {
            return Err(malformed());
        }
        let major: u32 = major.parse().map_err(|_| malformed())?;
        Ok(Self {
            namespace: Cow::Owned(namespace.to_string()),
            major,
        })
    }
}

impl Serialize for SchemaVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let rendered = String::deserialize(deserializer)?;
        rendered.parse().map_err(de::Error::custom)
    }
}

/// Reads a schema version and proves this build understands it.
///
/// This is the compatibility reader: a known namespace at a known major is
/// accepted, and anything else is refused with the reason. There is no
/// permissive fallback, because a document this build cannot interpret must not
/// be interpreted as though it were empty.
///
/// # Errors
///
/// Returns [`SchemaVersionError`] when the text is malformed, the namespace is
/// not the expected one, or the major version is not supported.
pub fn read_compatible(
    found: &str,
    expected: &SchemaVersion,
) -> Result<SchemaVersion, SchemaVersionError> {
    let parsed: SchemaVersion = found.parse()?;
    if parsed.namespace() != expected.namespace() {
        return Err(SchemaVersionError::Namespace {
            expected: expected.namespace().to_string(),
            found: parsed.namespace().to_string(),
        });
    }
    if parsed.major() != expected.major() {
        return Err(SchemaVersionError::UnsupportedMajor {
            found: parsed.major(),
            supported: expected.major(),
        });
    }
    Ok(parsed)
}
