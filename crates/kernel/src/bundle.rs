//! Certificate bundles: a certificate plus every byte it cites.
//!
//! A certificate on its own is a set of claims about evidence. A bundle is the
//! evidence, so someone who has never seen this installation can check the
//! claims. That is the difference between "trust us" and "here, look".
//!
//! Two rules keep a bundle honest. It must contain every piece of evidence the
//! certificate refers to, so replay cannot be defeated by leaving out the
//! inconvenient log. And it must contain nothing else, so a bundle cannot
//! quietly become a place to ship unrelated material.
//!
//! Bytes are carried base64-encoded inside the canonical document rather than
//! in a container format, which keeps the whole bundle one deterministic file:
//! the same certificate and the same evidence always produce the same bytes.

use std::collections::BTreeMap;

use capsulet_ir::correctness::certificate::Certificate;
use capsulet_ir::digest::Digest;
use capsulet_ir::version::{BUNDLE_SCHEMA_VERSION, SchemaVersion, SchemaVersionError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::replay::EvidenceMap;

/// Why a bundle was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BundleError {
    #[error("the bundle is missing evidence `{digest}`, which the certificate cites")]
    MissingEvidence { digest: Digest },
    #[error(
        "the bundle carries `{digest}`, which the certificate does not cite; a bundle is the \
         evidence for one certificate, not a place to ship anything else"
    )]
    UnreferencedBlob { digest: Digest },
    #[error("blob `{recorded}` contains bytes that digest to `{found}`")]
    BlobDigestMismatch { recorded: Digest, found: Digest },
    #[error("blob `{digest}` is not valid base64")]
    MalformedBlob { digest: Digest },
    #[error("the bundle is not valid JSON: {detail}")]
    Malformed { detail: String },
    #[error("the bundle manifest is not readable by this build: {source}")]
    SchemaVersion {
        #[source]
        source: SchemaVersionError,
    },
}

/// A certificate and the evidence it cites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bundle {
    pub schema_version: SchemaVersion,
    pub certificate: Certificate,
    /// Evidence bytes, keyed by digest and base64-encoded.
    blobs: BTreeMap<String, String>,
}

impl Bundle {
    /// Builds a bundle from a certificate and a source of evidence.
    ///
    /// # Errors
    ///
    /// Returns [`BundleError::MissingEvidence`] when the source cannot supply
    /// something the certificate cites.
    pub fn build(certificate: Certificate, evidence: &EvidenceMap) -> Result<Self, BundleError> {
        let mut blobs = BTreeMap::new();
        for reference in &certificate.body().evidence {
            let bytes = crate::replay::EvidenceSource::bytes(evidence, &reference.content).ok_or(
                BundleError::MissingEvidence {
                    digest: reference.content,
                },
            )?;
            blobs.insert(reference.content.to_string(), base64_encode(bytes));
        }

        Ok(Self {
            schema_version: BUNDLE_SCHEMA_VERSION,
            certificate,
            blobs,
        })
    }

    /// The bundle's canonical bytes.
    ///
    /// # Errors
    ///
    /// Returns [`BundleError::Malformed`] when the bundle cannot be encoded,
    /// which would mean a value in it has no canonical form.
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, BundleError> {
        capsulet_ir::to_canonical_bytes(self).map_err(|source| BundleError::Malformed {
            detail: source.to_string(),
        })
    }

    /// Reads a bundle and checks it is complete, exact, and nothing more.
    ///
    /// # Errors
    ///
    /// Returns [`BundleError`] when the document does not parse, its schema
    /// major is unknown, a blob does not match its digest, a cited piece of
    /// evidence is absent, or a blob is carried that nothing cites.
    pub fn read(bytes: &[u8]) -> Result<Self, BundleError> {
        let bundle: Self =
            serde_json::from_slice(bytes).map_err(|source| BundleError::Malformed {
                detail: source.to_string(),
            })?;

        capsulet_ir::version::read_compatible(
            &bundle.schema_version.to_string(),
            &BUNDLE_SCHEMA_VERSION,
        )
        .map_err(|source| BundleError::SchemaVersion { source })?;

        // Completeness only. A blob whose bytes do not match its digest is left
        // for replay to report: that is a tampering finding, and burying it in
        // a container parse error would hide the one signal a reader most needs
        // to see.
        bundle.check_completeness()?;
        Ok(bundle)
    }

    /// Checks the bundle carries exactly what the certificate cites: nothing
    /// missing, nothing extra.
    ///
    /// # Errors
    ///
    /// Returns [`BundleError`] for the first problem found.
    pub fn check_completeness(&self) -> Result<(), BundleError> {
        let cited = self.cited();

        for key in self.blobs.keys() {
            let recorded: Digest = key.parse().map_err(|_| BundleError::MalformedBlob {
                digest: Digest::of(key.as_bytes()),
            })?;
            if !cited.contains(&recorded) {
                return Err(BundleError::UnreferencedBlob { digest: recorded });
            }
        }

        for digest in cited {
            if !self.blobs.contains_key(&digest.to_string()) {
                return Err(BundleError::MissingEvidence { digest });
            }
        }
        Ok(())
    }

    /// Checks completeness, and additionally that every blob really is the
    /// bytes its key names.
    ///
    /// Callers who want to know whether a bundle is intact before replaying it
    /// use this; replay itself re-checks the digests regardless, so a bundle
    /// that skips this check is not trusted, only reported on later.
    ///
    /// # Errors
    ///
    /// Returns [`BundleError`] for the first problem found.
    pub fn check(&self) -> Result<(), BundleError> {
        self.check_completeness()?;
        for (key, encoded) in &self.blobs {
            let recorded: Digest = key.parse().map_err(|_| BundleError::MalformedBlob {
                digest: Digest::of(key.as_bytes()),
            })?;
            let bytes =
                base64_decode(encoded).ok_or(BundleError::MalformedBlob { digest: recorded })?;
            let found = Digest::of(&bytes);
            if found != recorded {
                return Err(BundleError::BlobDigestMismatch { recorded, found });
            }
        }
        Ok(())
    }

    fn cited(&self) -> Vec<Digest> {
        self.certificate
            .body()
            .evidence
            .iter()
            .map(|reference| reference.content)
            .collect()
    }

    /// The evidence this bundle carries, ready for replay.
    ///
    /// # Errors
    ///
    /// Returns [`BundleError`] when a blob does not decode.
    pub fn evidence(&self) -> Result<EvidenceMap, BundleError> {
        let mut map = EvidenceMap::new();
        for (key, encoded) in &self.blobs {
            let recorded: Digest = key.parse().map_err(|_| BundleError::MalformedBlob {
                digest: Digest::of(key.as_bytes()),
            })?;
            let bytes =
                base64_decode(encoded).ok_or(BundleError::MalformedBlob { digest: recorded })?;
            // Insert under the recorded key, not the computed one, so a bundle
            // whose bytes do not match their digest is caught by replay rather
            // than silently corrected here.
            map.insert_as(recorded, bytes);
        }
        Ok(map)
    }

    /// How many blobs the bundle carries.
    #[must_use]
    pub fn blob_count(&self) -> usize {
        self.blobs.len()
    }

    /// Replaces a blob's bytes, leaving its key alone.
    ///
    /// Only useful for building the tampered bundle a replay gate has to
    /// detect. Shipping it here means the check is exercised the same way in
    /// tests and in the verifier.
    pub fn tamper_with(&mut self, digest: &Digest, bytes: &[u8]) {
        self.blobs.insert(digest.to_string(), base64_encode(bytes));
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut buffer = 0_u32;
    let mut bits = 0_u32;

    for character in text.bytes() {
        if character == b'=' {
            break;
        }
        let position = ALPHABET.iter().position(|entry| *entry == character)?;
        let value = u32::try_from(position).ok()?;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((buffer >> bits) & 0xff).ok()?);
        }
    }
    Some(out)
}
