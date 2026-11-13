//! Publisher keys, signatures, and the store of keys a project trusts.
//!
//! Ed25519 over the manifest digest, with a domain separator so a signature
//! made for a pack cannot be presented as one made for anything else (ADR 0022).

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use serde::{Deserialize, Serialize};

use crate::PackError;

/// The schema of a private key file.
pub const KEY_SCHEMA: &str = "capsulet.pack-key/v1";
/// The schema of `pack.sig`.
pub const SIGNATURE_SCHEMA: &str = "capsulet.pack-signature/v1";
/// The schema of a trust store.
pub const TRUST_SCHEMA: &str = "capsulet.pack-trust/v1";

/// What is signed: the manifest digest, under a separator naming what it is.
fn message(manifest: &str) -> Vec<u8> {
    format!("capsulet.pack/v1 manifest {manifest}").into_bytes()
}

/// A key's id: the digest of its public key bytes.
#[must_use]
pub fn key_id(public_key: &[u8]) -> String {
    capsulet_ir::Digest::of(public_key).to_string()
}

/// A publisher's private key, as `capsulet pack keygen` writes it. It is a
/// secret: it never goes in a pack or a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivateKey {
    pub schema_version: String,
    pub id: String,
    /// The PKCS#8 document, hex-encoded.
    pub pkcs8: String,
}

/// A publisher's public key, as a trust store holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedKey {
    pub id: String,
    /// The 32-byte Ed25519 public key, hex-encoded.
    pub public_key: String,
    /// The pack names this key may sign; `*` for any.
    pub packs: Vec<String>,
}

/// The keys a project trusts, and for which packs.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TrustStore {
    pub schema_version: String,
    pub keys: Vec<TrustedKey>,
}

/// `pack.sig`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    pub schema_version: String,
    /// The id of the key that signed.
    pub key: String,
    /// The manifest digest signed.
    pub manifest: String,
    /// The Ed25519 signature, hex-encoded.
    pub signature: String,
}

/// Makes a new key pair, for a publisher.
///
/// # Errors
///
/// Returns why the system could not make a key.
pub fn generate() -> Result<(PrivateKey, Vec<u8>), PackError> {
    let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| PackError::Key("the system could not make a key".to_string()))?;
    let pair = Ed25519KeyPair::from_pkcs8(document.as_ref())
        .map_err(|_| PackError::Key("a new key did not read back".to_string()))?;
    let public = pair.public_key().as_ref().to_vec();
    Ok((
        PrivateKey {
            schema_version: KEY_SCHEMA.to_string(),
            id: key_id(&public),
            pkcs8: hex::encode(document.as_ref()),
        },
        public,
    ))
}

impl PrivateKey {
    /// The public half, trusted for `packs`.
    ///
    /// # Errors
    ///
    /// Returns why the key does not read.
    pub fn trusted_for(&self, packs: Vec<String>) -> Result<TrustedKey, PackError> {
        let pair = self.pair()?;
        Ok(TrustedKey {
            id: self.id.clone(),
            public_key: hex::encode(pair.public_key().as_ref()),
            packs,
        })
    }

    /// Signs a manifest digest.
    ///
    /// # Errors
    ///
    /// Returns why the key does not read.
    pub fn sign(&self, manifest: &str) -> Result<Signature, PackError> {
        let pair = self.pair()?;
        Ok(Signature {
            schema_version: SIGNATURE_SCHEMA.to_string(),
            key: self.id.clone(),
            manifest: manifest.to_string(),
            signature: hex::encode(pair.sign(&message(manifest)).as_ref()),
        })
    }

    fn pair(&self) -> Result<Ed25519KeyPair, PackError> {
        if self.schema_version != KEY_SCHEMA {
            return Err(PackError::Key(format!(
                "a key file in {}",
                self.schema_version
            )));
        }
        let bytes = hex::decode(&self.pkcs8)
            .map_err(|_| PackError::Key("the key is not hex".to_string()))?;
        let pair = Ed25519KeyPair::from_pkcs8(&bytes)
            .map_err(|_| PackError::Key("the key is not an Ed25519 PKCS#8 document".to_string()))?;
        if key_id(pair.public_key().as_ref()) != self.id {
            return Err(PackError::Key(
                "the key's id is not its public key's digest".to_string(),
            ));
        }
        Ok(pair)
    }
}

impl TrustStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            schema_version: TRUST_SCHEMA.to_string(),
            keys: Vec::new(),
        }
    }

    /// Checks the store is well formed: its schema, and every key's id is the
    /// digest of its public key.
    ///
    /// # Errors
    ///
    /// Returns [`PackError::TrustStore`] naming what is wrong.
    pub fn check(&self) -> Result<(), PackError> {
        if self.schema_version != TRUST_SCHEMA {
            return Err(PackError::TrustStore(format!(
                "a trust store in {}, not {TRUST_SCHEMA}",
                self.schema_version
            )));
        }
        for key in &self.keys {
            let bytes = hex::decode(&key.public_key)
                .map_err(|_| PackError::TrustStore(format!("key {} is not hex", key.id)))?;
            if bytes.len() != 32 || key_id(&bytes) != key.id {
                return Err(PackError::TrustStore(format!(
                    "key {} is not the digest of a 32-byte public key",
                    key.id
                )));
            }
        }
        Ok(())
    }

    /// Verifies `signature` over `manifest` for the pack `name`.
    ///
    /// # Errors
    ///
    /// Returns the first reason the signature does not install the pack: it is
    /// in another schema, signs another manifest, names a key the store does
    /// not hold or holds for other packs, or does not verify.
    pub fn verify(
        &self,
        name: &str,
        manifest: &str,
        signature: &Signature,
    ) -> Result<(), PackError> {
        if signature.schema_version != SIGNATURE_SCHEMA {
            return Err(PackError::SignatureUnreadable(format!(
                "a signature in {}",
                signature.schema_version
            )));
        }
        if signature.manifest != manifest {
            return Err(PackError::SignatureForAnotherManifest {
                signed: signature.manifest.clone(),
                found: manifest.to_string(),
            });
        }
        let Some(key) = self.keys.iter().find(|key| key.id == signature.key) else {
            return Err(PackError::UnknownKey(signature.key.clone()));
        };
        if !key
            .packs
            .iter()
            .any(|pattern| pattern == "*" || pattern == name)
        {
            return Err(PackError::KeyNotTrustedForPack {
                key: key.id.clone(),
                pack: name.to_string(),
            });
        }
        let public = hex::decode(&key.public_key)
            .map_err(|_| PackError::TrustStore(format!("key {} is not hex", key.id)))?;
        let bytes = hex::decode(&signature.signature)
            .map_err(|_| PackError::SignatureUnreadable("the signature is not hex".to_string()))?;
        UnparsedPublicKey::new(&ED25519, &public)
            .verify(&message(manifest), &bytes)
            .map_err(|_| PackError::BadSignature)
    }
}
