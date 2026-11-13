//! `pack.json`: what a pack is, by name, version and content.

use serde::{Deserialize, Serialize};

/// The manifest schema this build reads and writes.
pub const PACK_SCHEMA: &str = "capsulet.pack/v1";

/// A pack's manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: String,
    /// Lower-case letters, digits and hyphens.
    pub name: String,
    /// A semantic version.
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// Memory contract DSL files, by path.
    #[serde(default)]
    pub contracts: Vec<String>,
    #[serde(default)]
    pub validators: Vec<ValidatorEntry>,
    /// The policy defaults for the pack's obligations, by path: each file is an
    /// IR contract whose obligations say which kinds of checker may discharge
    /// them and the authority a person needs to discharge one by review. A
    /// definition that uses the pack declares these contracts as they are.
    /// Absent from manifests written before packs carried any, which digest as
    /// they always did.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub obligations: Vec<String>,
    /// Every file in the pack but `pack.json` and `pack.sig`, sorted by path.
    #[serde(default)]
    pub files: Vec<FileEntry>,
}

/// A validator the pack registers, and the cases that show it works.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatorEntry {
    /// A `capsulet.verifier/v1` registration, by path.
    pub registration: String,
    /// A harness cases file, by path.
    pub cases: String,
}

/// One file, by path and the digest of its bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub digest: String,
}

impl Manifest {
    /// The digest over the manifest's canonical bytes: the pack's identity.
    ///
    /// # Errors
    ///
    /// Returns why the manifest has no canonical encoding.
    pub fn digest(&self) -> Result<capsulet_ir::Digest, String> {
        capsulet_ir::digest_of(self).map_err(|error| error.to_string())
    }
}

/// Whether `name` is a pack name, by the rule a definition's pin is held to.
pub use capsulet_ir::is_pack_name;

/// Whether `path` stays inside the pack: relative, `/`-separated, with no
/// empty, `.` or `..` segment, and no drive or backslash.
#[must_use]
pub fn is_pack_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_name_and_a_pack_path_are_narrow() {
        assert!(is_pack_name("contract-clauses"));
        for name in ["", "Contract", "1pack", "pack_name", "pack.name"] {
            assert!(!is_pack_name(name), "{name:?}");
        }
        assert!(is_pack_path("validators/parser.json"));
        for path in [
            "",
            "/etc/passwd",
            "../outside",
            "a/../b",
            "a//b",
            "c:\\x",
            "a\\b",
            "./a",
        ] {
            assert!(!is_pack_path(path), "{path:?}");
        }
    }
}
