//! Domain packs (ADR 0022).
//!
//! A pack is a directory: `pack.json`, the [`Manifest`] naming the pack, its
//! version, its contracts, its validators and every file by digest, and
//! `pack.sig`, an Ed25519 [`Signature`] over the manifest digest. [`admit`]
//! decides whether a project may install one:
//!
//! - every file is listed and is the bytes the manifest says;
//! - the signature verifies, under a key the project trusts for this pack's name;
//! - the version is not a downgrade, and not a reused version with other content;
//! - every contract parses and compiles, and every validator registration and
//!   cases file reads.
//!
//! [`test`] runs the validators against their cases through the verifier's
//! harness, and [`refresh`] lists a directory's files in its manifest, which is
//! what an author does before signing.

pub mod keys;
pub mod manifest;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use capsulet_core::{MemoryContract, MemoryContractId};
use capsulet_verifier::Registration;
use capsulet_verifier::harness::{CaseReport, Cases, run_cases};
use thiserror::Error;

pub use keys::{PrivateKey, Signature, TrustStore, TrustedKey, generate};
pub use manifest::{FileEntry, Manifest, PACK_SCHEMA, ValidatorEntry, is_pack_name, is_pack_path};

/// The manifest's file name.
pub const MANIFEST_FILE: &str = "pack.json";
/// The signature's file name.
pub const SIGNATURE_FILE: &str = "pack.sig";

/// Why a pack cannot be read, signed, or installed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PackError {
    #[error("the pack cannot be read: {0}")]
    Unreadable(String),
    #[error("the manifest is in {found}, not {PACK_SCHEMA}")]
    UnsupportedSchema { found: String },
    #[error(
        "{0:?} is not a pack name: lower-case letters, digits and hyphens, starting with a letter"
    )]
    BadName(String),
    #[error("{0:?} is not a semantic version")]
    BadVersion(String),
    #[error("{0:?} is not a path inside the pack")]
    UnsafePath(String),
    #[error("{0} is in the pack and not in its manifest")]
    UnlistedFile(String),
    #[error("{0} is in the manifest and not in the pack")]
    MissingFile(String),
    #[error("{0} is not the bytes the manifest says it is")]
    TamperedFile(String),
    #[error("the pack is not signed")]
    Unsigned,
    #[error("the signature cannot be read: {0}")]
    SignatureUnreadable(String),
    #[error("the signature is over manifest {signed}, and this manifest is {found}")]
    SignatureForAnotherManifest { signed: String, found: String },
    #[error("the pack is signed by {0}, which the trust store does not hold")]
    UnknownKey(String),
    #[error("{key} is trusted, but not for pack {pack}")]
    KeyNotTrustedForPack { key: String, pack: String },
    #[error("the signature does not verify")]
    BadSignature,
    #[error("version {offered} is older than {installed}, which is installed")]
    Downgrade { installed: String, offered: String },
    #[error("version {version} is installed with other content; a version does not change")]
    VersionReused { version: String },
    #[error("contract {path} does not compile: {detail}")]
    BadContract { path: String, detail: String },
    #[error("validator {path} cannot be registered: {detail}")]
    BadValidator { path: String, detail: String },
    #[error("cases {path} cannot be read: {detail}")]
    BadCases { path: String, detail: String },
    #[error("the trust store cannot be used: {0}")]
    TrustStore(String),
    #[error("the key cannot be used: {0}")]
    Key(String),
}

impl PackError {
    /// A short name clients can route on.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Unreadable(_) => "unreadable",
            Self::UnsupportedSchema { .. } => "unsupported_schema",
            Self::BadName(_) => "bad_name",
            Self::BadVersion(_) => "bad_version",
            Self::UnsafePath(_) => "unsafe_path",
            Self::UnlistedFile(_) => "unlisted_file",
            Self::MissingFile(_) => "missing_file",
            Self::TamperedFile(_) => "tampered_file",
            Self::Unsigned => "unsigned",
            Self::SignatureUnreadable(_) => "signature_unreadable",
            Self::SignatureForAnotherManifest { .. } => "signature_for_another_manifest",
            Self::UnknownKey(_) => "unknown_key",
            Self::KeyNotTrustedForPack { .. } => "key_not_trusted_for_pack",
            Self::BadSignature => "bad_signature",
            Self::Downgrade { .. } => "downgrade",
            Self::VersionReused { .. } => "version_reused",
            Self::BadContract { .. } => "bad_contract",
            Self::BadValidator { .. } => "bad_validator",
            Self::BadCases { .. } => "bad_cases",
            Self::TrustStore(_) => "trust_store",
            Self::Key(_) => "key",
        }
    }
}

/// A pack version a project already holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    pub version: String,
    pub manifest: String,
}

/// A pack a project may install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admitted {
    pub name: String,
    pub version: String,
    /// The manifest digest: what a definition pins and a certificate names.
    pub manifest: String,
    /// The key that signed it.
    pub key: String,
    /// Whether this exact pack was already installed, so installing it again
    /// changes nothing.
    pub already_installed: bool,
}

/// A pack read from a directory.
#[derive(Debug, Clone)]
pub struct Pack {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub signature: Option<Signature>,
}

fn unreadable(error: impl std::fmt::Display) -> PackError {
    PackError::Unreadable(error.to_string())
}

/// Reads a pack directory: its manifest and, when present, its signature.
///
/// # Errors
///
/// Returns [`PackError::Unreadable`] when `pack.json` or `pack.sig` is missing
/// or not JSON of the right shape.
pub fn read(root: &Path) -> Result<Pack, PackError> {
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(root.join(MANIFEST_FILE)).map_err(unreadable)?)
            .map_err(unreadable)?;
    let signature_path = root.join(SIGNATURE_FILE);
    let signature = if signature_path.exists() {
        Some(
            serde_json::from_slice(&fs::read(&signature_path).map_err(unreadable)?)
                .map_err(|error| PackError::SignatureUnreadable(error.to_string()))?,
        )
    } else {
        None
    };
    Ok(Pack {
        root: root.to_path_buf(),
        manifest,
        signature,
    })
}

/// Every file under `root` but the manifest and the signature, with its digest,
/// sorted by path.
///
/// # Errors
///
/// Returns [`PackError::Unreadable`] when the directory cannot be walked.
pub fn files_of(root: &Path) -> Result<Vec<FileEntry>, PackError> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(unreadable)? {
            let entry = entry.map_err(unreadable)?;
            let kind = entry.file_type().map_err(unreadable)?;
            if kind.is_dir() {
                pending.push(entry.path());
                continue;
            }
            let path = entry
                .path()
                .strip_prefix(root)
                .map_err(unreadable)?
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            if path == MANIFEST_FILE || path == SIGNATURE_FILE {
                continue;
            }
            let bytes = fs::read(entry.path()).map_err(unreadable)?;
            found.push(FileEntry {
                path,
                digest: capsulet_ir::Digest::of(&bytes).to_string(),
            });
        }
    }
    found.sort();
    Ok(found)
}

/// Lists every file in the directory in its manifest, as an author does before
/// signing, and writes the manifest back. Returns the manifest digest.
///
/// # Errors
///
/// Returns why the pack cannot be read or written.
pub fn refresh(root: &Path) -> Result<String, PackError> {
    let mut pack = read(root)?;
    pack.manifest.files = files_of(root)?;
    let text = serde_json::to_string_pretty(&pack.manifest).map_err(unreadable)?;
    fs::write(root.join(MANIFEST_FILE), text + "\n").map_err(unreadable)?;
    pack.manifest
        .digest()
        .map(|digest| digest.to_string())
        .map_err(PackError::Unreadable)
}

/// Refreshes the manifest and signs it with `key`, writing `pack.sig`. Returns
/// the manifest digest signed.
///
/// # Errors
///
/// Returns why the pack cannot be read or written, or the key cannot sign.
pub fn sign(root: &Path, key: &PrivateKey) -> Result<String, PackError> {
    let manifest = refresh(root)?;
    let signature = key.sign(&manifest)?;
    let text = serde_json::to_string_pretty(&signature).map_err(unreadable)?;
    fs::write(root.join(SIGNATURE_FILE), text + "\n").map_err(unreadable)?;
    Ok(manifest)
}

/// Decides whether a project holding `installed` may install the pack, under
/// the keys it trusts.
///
/// # Errors
///
/// Returns the first reason it may not, in the order a reviewer would want them:
/// what the pack is, whether its files are what it says, who signed it, whether
/// its version moves forward, and whether its content reads.
pub fn admit(
    pack: &Pack,
    trust: &TrustStore,
    installed: &[Installed],
) -> Result<Admitted, PackError> {
    let manifest = &pack.manifest;
    if manifest.schema_version != PACK_SCHEMA {
        return Err(PackError::UnsupportedSchema {
            found: manifest.schema_version.clone(),
        });
    }
    if !is_pack_name(&manifest.name) {
        return Err(PackError::BadName(manifest.name.clone()));
    }
    let version = semver::Version::parse(&manifest.version)
        .map_err(|_| PackError::BadVersion(manifest.version.clone()))?;
    check_files(pack)?;

    trust.check()?;
    let digest = manifest
        .digest()
        .map_err(PackError::Unreadable)?
        .to_string();
    let Some(signature) = &pack.signature else {
        return Err(PackError::Unsigned);
    };
    trust.verify(&manifest.name, &digest, signature)?;

    let already_installed = check_version(manifest, &version, &digest, installed)?;
    check_content(pack)?;
    Ok(Admitted {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        manifest: digest,
        key: signature.key.clone(),
        already_installed,
    })
}

/// Every file on disk is listed, every listed file is on disk, and each is the
/// bytes its digest names.
fn check_files(pack: &Pack) -> Result<(), PackError> {
    let listed: BTreeMap<&str, &str> = pack
        .manifest
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.digest.as_str()))
        .collect();
    for path in listed.keys() {
        if !is_pack_path(path) {
            return Err(PackError::UnsafePath((*path).to_string()));
        }
    }
    let present = files_of(&pack.root)?;
    let on_disk: BTreeSet<&str> = present.iter().map(|file| file.path.as_str()).collect();
    for file in &present {
        match listed.get(file.path.as_str()) {
            None => return Err(PackError::UnlistedFile(file.path.clone())),
            Some(digest) if *digest != file.digest => {
                return Err(PackError::TamperedFile(file.path.clone()));
            }
            Some(_) => {}
        }
    }
    if let Some(missing) = listed.keys().find(|path| !on_disk.contains(*path)) {
        return Err(PackError::MissingFile((*missing).to_string()));
    }
    let named = pack.manifest.contracts.iter().chain(
        pack.manifest
            .validators
            .iter()
            .flat_map(|entry| [&entry.registration, &entry.cases]),
    );
    for path in named {
        if !listed.contains_key(path.as_str()) {
            return Err(PackError::MissingFile(path.clone()));
        }
    }
    Ok(())
}

/// Whether installing this version moves the pack forward. `Ok(true)` when it
/// is exactly a pack already installed.
fn check_version(
    manifest: &Manifest,
    version: &semver::Version,
    digest: &str,
    installed: &[Installed],
) -> Result<bool, PackError> {
    let mut already = false;
    for held in installed.iter().filter(|held| held.name == manifest.name) {
        let Ok(held_version) = semver::Version::parse(&held.version) else {
            continue;
        };
        if &held_version == version {
            if held.manifest != digest {
                return Err(PackError::VersionReused {
                    version: manifest.version.clone(),
                });
            }
            already = true;
        } else if &held_version > version {
            return Err(PackError::Downgrade {
                installed: held.version.clone(),
                offered: manifest.version.clone(),
            });
        }
    }
    Ok(already)
}

/// Every contract compiles, and every validator registration and cases file
/// reads.
fn check_content(pack: &Pack) -> Result<(), PackError> {
    for path in &pack.manifest.contracts {
        let source = fs::read_to_string(pack.root.join(path)).map_err(unreadable)?;
        let bad = |detail: String| PackError::BadContract {
            path: path.clone(),
            detail,
        };
        let id = MemoryContractId::new(format!("{}/{path}", pack.manifest.name)).map_err(bad)?;
        let contract = MemoryContract::parse(id, &pack.manifest.name, &source)
            .map_err(|error| bad(error.to_string()))?;
        contract.compile().map_err(|error| bad(error.to_string()))?;
    }
    for entry in &pack.manifest.validators {
        registration_of(pack, entry)?;
        cases_of(pack, entry)?;
    }
    Ok(())
}

fn registration_of(pack: &Pack, entry: &ValidatorEntry) -> Result<Registration, PackError> {
    let bad = |detail: String| PackError::BadValidator {
        path: entry.registration.clone(),
        detail,
    };
    let bytes =
        fs::read(pack.root.join(&entry.registration)).map_err(|error| bad(error.to_string()))?;
    let registration: Registration =
        serde_json::from_slice(&bytes).map_err(|error| bad(error.to_string()))?;
    registration
        .validate()
        .map_err(|error| bad(error.to_string()))?;
    Ok(registration)
}

fn cases_of(pack: &Pack, entry: &ValidatorEntry) -> Result<Cases, PackError> {
    let bad = |detail: String| PackError::BadCases {
        path: entry.cases.clone(),
        detail,
    };
    let bytes = fs::read(pack.root.join(&entry.cases)).map_err(|error| bad(error.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|error| bad(error.to_string()))
}

/// One validator's cases, and how they came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatorReport {
    pub registration: String,
    pub cases: Vec<CaseReport>,
}

/// Runs every validator in the pack against its cases. Paths in a cases file
/// resolve beside it.
///
/// # Errors
///
/// Returns why a registration or a cases file does not read.
pub fn test(pack: &Pack) -> Result<Vec<ValidatorReport>, PackError> {
    pack.manifest
        .validators
        .iter()
        .map(|entry| {
            let registration = registration_of(pack, entry)?;
            let cases = cases_of(pack, entry)?;
            let cases_path = pack.root.join(&entry.cases);
            let base = cases_path.parent().unwrap_or(&pack.root);
            Ok(ValidatorReport {
                registration: entry.registration.clone(),
                cases: run_cases(&registration, &cases, base),
            })
        })
        .collect()
}
