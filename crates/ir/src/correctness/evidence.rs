//! Evidence and artifacts, addressed by content.
//!
//! Evidence is referenced by the digest of its bytes, never by a path or a URL.
//! A path says where something was; a digest says what it was. Only the second
//! survives a replay on a machine that has never seen this installation, and
//! only the second makes tampering detectable.

use serde::{Deserialize, Serialize};

use crate::correctness::proposal::Producer;
use crate::digest::Digest;
use crate::id::Identifier;
use crate::trust::TrustClass;

/// A moment, recorded as data.
///
/// Milliseconds since the Unix epoch, as an integer: canonical, comparable, and
/// impossible to produce by accident from a clock this crate is not allowed to
/// read. Whoever captured the evidence records when; this crate only carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecordedTime(pub i64);

impl RecordedTime {
    /// Milliseconds since the Unix epoch.
    #[must_use]
    pub const fn epoch_millis(self) -> i64 {
        self.0
    }
}

/// A reference to a piece of evidence.
///
/// The bytes live in the certificate bundle or in object storage; this is the
/// handle a certificate carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub id: Identifier,
    /// The digest of the bytes. Changing one byte changes this, which is how a
    /// replay notices.
    pub content: Digest,
    pub media_type: String,
    pub byte_length: u64,
    pub producer: Producer,
    pub captured_at: RecordedTime,
}

/// A value the run produced and may hand onward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: Identifier,
    pub content: Digest,
    pub media_type: String,
    pub byte_length: u64,
    /// The assurance this artifact carries. Established by a record or not at
    /// all; see [`crate::trust`].
    pub trust: TrustClass,
    /// The artifacts and evidence this was derived from, by digest, so lineage
    /// is walkable without a side index.
    pub derived_from: Vec<Digest>,
}

impl Artifact {
    /// Whether this artifact traces back to the given content.
    #[must_use]
    pub fn derives_from(&self, digest: &Digest) -> bool {
        self.derived_from.contains(digest)
    }
}
