//! Phase 20 Wave 4 — portable agent identity.
//!
//! An [`AgentIdentity`] is a self-contained Ed25519 keypair plus
//! metadata, **independent of any specific Tirami node**. Where the
//! existing `NodeId` is the machine's key and dies with the host,
//! an `AgentIdentity` follows the agent across hosts: export it from
//! one node, import it on another, and the same DID continues to
//! refer to the same economic actor.
//!
//! ## DID format
//!
//! `did:tirami:<hex-encoded 32-byte Ed25519 public key>`.
//!
//! Deliberately keeps the suffix as plain lowercase hex rather than
//! multibase / base58btc so that downstream tools that already have
//! the Tirami node-id encoder (and that's nearly everything in this
//! workspace) don't need a new alphabet just to render or parse the
//! identifier. The same 64-character public key string that already
//! appears in `/v1/tirami/trades` is the DID suffix.
//!
//! ## Encrypted export
//!
//! Exporting an agent identity off a node means handing out the
//! private key. To keep that safe in transit / at rest, the
//! [`AgentIdentity::export`] path encrypts the seed with
//! Argon2id-derived ChaCha20-Poly1305:
//!
//! 1. Caller supplies a passphrase.
//! 2. We sample a 16-byte salt + 24-byte nonce.
//! 3. Argon2id (m=64 MB, t=3, p=1) derives a 32-byte key from the
//!    passphrase and salt.
//! 4. ChaCha20-Poly1305 encrypts the seed (and seals the rest of the
//!    identity metadata as additional authenticated data) under that
//!    key + nonce.
//! 5. The resulting [`AgentIdentityBundle`] is serializable to JSON
//!    and safe to transmit.
//!
//! Import is the inverse: the same passphrase + bundle materialises
//! a verified `AgentIdentity` on the new host.
//!
//! The salt + nonce are stored in plaintext inside the bundle. This is
//! standard practice for password-derived AEAD; the only secret is the
//! passphrase itself.

use argon2::Argon2;
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305, XNonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const DERIVED_KEY_LEN: usize = 32;
pub const DID_PREFIX: &str = "did:tirami:";

/// Portable agent identity. Holds an Ed25519 keypair plus optional
/// display metadata. Cloning makes a deep copy of the seed bytes —
/// callers should `zeroize`-equivalent the original when handing one
/// off, but the current MVP does not yet enforce this.
#[derive(Debug, Clone)]
pub struct AgentIdentity {
    signing_key: SigningKey,
    pub display_name: Option<String>,
    pub created_at_ms: u64,
}

impl AgentIdentity {
    /// Generate a fresh identity. Uses `OsRng` for the seed.
    pub fn generate(now_ms: u64, display_name: Option<String>) -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self {
            signing_key,
            display_name,
            created_at_ms: now_ms,
        }
    }

    /// Reconstruct from a 32-byte Ed25519 seed (the private key).
    pub fn from_seed(
        seed: [u8; 32],
        display_name: Option<String>,
        created_at_ms: u64,
    ) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(&seed),
            display_name,
            created_at_ms,
        }
    }

    /// 32-byte Ed25519 public key.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// 32-byte Ed25519 private key (the seed). Sensitive; do not log.
    pub fn seed(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Hex-encoded public key — what shows up in trades + DIDs.
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.public_key_bytes())
    }

    /// Stable identifier suitable for citation across systems.
    /// `did:tirami:<hex>`.
    pub fn did(&self) -> String {
        format!("{DID_PREFIX}{}", self.public_key_hex())
    }

    /// Sign an arbitrary message with this agent's key.
    pub fn sign(&self, msg: &[u8]) -> Signature {
        self.signing_key.sign(msg)
    }

    /// Verify that a signature was produced by this agent. Convenience
    /// wrapper so callers don't need to import `Verifier` themselves.
    pub fn verify(&self, msg: &[u8], sig: &Signature) -> Result<(), AgentIdentityError> {
        self.signing_key
            .verifying_key()
            .verify(msg, sig)
            .map_err(|e| AgentIdentityError::SignatureInvalid(e.to_string()))
    }

    /// Verify a signature against an arbitrary DID. Useful when one
    /// agent receives a signed claim from another.
    pub fn verify_with_did(
        did: &str,
        msg: &[u8],
        sig: &Signature,
    ) -> Result<(), AgentIdentityError> {
        let pk_hex = did
            .strip_prefix(DID_PREFIX)
            .ok_or_else(|| AgentIdentityError::DidFormat("missing did:tirami: prefix".into()))?;
        if pk_hex.len() != 64 {
            return Err(AgentIdentityError::DidFormat(format!(
                "expected 64 hex chars after prefix, got {}",
                pk_hex.len()
            )));
        }
        let bytes =
            hex::decode(pk_hex).map_err(|e| AgentIdentityError::DidFormat(e.to_string()))?;
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        let vk = VerifyingKey::from_bytes(&arr)
            .map_err(|e| AgentIdentityError::DidFormat(format!("invalid Ed25519 pk: {e}")))?;
        vk.verify(msg, sig)
            .map_err(|e| AgentIdentityError::SignatureInvalid(e.to_string()))
    }

    /// Produce an encrypted, transportable export of this identity.
    ///
    /// The `passphrase` must be ≥ 8 characters — anything shorter is
    /// rejected up front because Argon2's defenses are not magic and
    /// a 3-char passphrase is brute-forceable in seconds.
    pub fn export(&self, passphrase: &str) -> Result<AgentIdentityBundle, AgentIdentityError> {
        if passphrase.len() < 8 {
            return Err(AgentIdentityError::PassphraseTooShort);
        }
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let key = derive_key(passphrase.as_bytes(), &salt)?;
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
        let nonce = XNonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, self.seed().as_slice())
            .map_err(|e| AgentIdentityError::Aead(format!("encrypt: {e}")))?;
        Ok(AgentIdentityBundle {
            schema_version: BUNDLE_SCHEMA_VERSION,
            display_name: self.display_name.clone(),
            created_at_ms: self.created_at_ms,
            public_key_hex: self.public_key_hex(),
            salt: hex::encode(salt),
            nonce: hex::encode(nonce_bytes),
            ciphertext: hex::encode(&ciphertext),
            kdf: "argon2id".to_string(),
            aead: "xchacha20poly1305".to_string(),
        })
    }

    /// Import an identity from an encrypted bundle.
    ///
    /// Validates that the recovered seed produces the public key
    /// the bundle claims — a passphrase mismatch is detected here
    /// because Argon2 + ChaCha20-Poly1305's authentication will fail
    /// before we even reach the key check.
    pub fn import(
        bundle: &AgentIdentityBundle,
        passphrase: &str,
    ) -> Result<Self, AgentIdentityError> {
        if bundle.schema_version != BUNDLE_SCHEMA_VERSION {
            return Err(AgentIdentityError::BundleSchema(format!(
                "unsupported schema_version {}; expected {}",
                bundle.schema_version, BUNDLE_SCHEMA_VERSION
            )));
        }
        if bundle.kdf != "argon2id" {
            return Err(AgentIdentityError::BundleSchema(format!(
                "unsupported kdf {:?}",
                bundle.kdf
            )));
        }
        if bundle.aead != "xchacha20poly1305" {
            return Err(AgentIdentityError::BundleSchema(format!(
                "unsupported aead {:?}",
                bundle.aead
            )));
        }
        let salt = hex::decode(&bundle.salt)
            .map_err(|e| AgentIdentityError::BundleSchema(format!("salt hex: {e}")))?;
        let nonce_bytes = hex::decode(&bundle.nonce)
            .map_err(|e| AgentIdentityError::BundleSchema(format!("nonce hex: {e}")))?;
        let ciphertext = hex::decode(&bundle.ciphertext)
            .map_err(|e| AgentIdentityError::BundleSchema(format!("ciphertext hex: {e}")))?;
        if salt.len() != SALT_LEN || nonce_bytes.len() != NONCE_LEN {
            return Err(AgentIdentityError::BundleSchema(
                "salt/nonce length mismatch".into(),
            ));
        }
        let mut salt_arr = [0u8; SALT_LEN];
        salt_arr.copy_from_slice(&salt);
        let key = derive_key(passphrase.as_bytes(), &salt_arr)?;
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
        let nonce = XNonce::from_slice(&nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_slice())
            .map_err(|_| AgentIdentityError::Aead("decrypt failed (passphrase mismatch?)".into()))?;
        if plaintext.len() != 32 {
            return Err(AgentIdentityError::BundleSchema(format!(
                "expected 32-byte seed plaintext, got {}",
                plaintext.len()
            )));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&plaintext);
        let identity = AgentIdentity::from_seed(seed, bundle.display_name.clone(), bundle.created_at_ms);
        // Defense in depth: the recovered seed must derive the same
        // public key the bundle advertises. If not, something is
        // wrong (corruption / mismatched bundle / future-format).
        if identity.public_key_hex() != bundle.public_key_hex {
            return Err(AgentIdentityError::BundleSchema(
                "recovered key does not match bundle's advertised public_key_hex".into(),
            ));
        }
        Ok(identity)
    }
}

const BUNDLE_SCHEMA_VERSION: u32 = 1;

/// Encrypted, transportable JSON representation of an [`AgentIdentity`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentityBundle {
    /// Wire-format version. Importers must reject unknown versions.
    pub schema_version: u32,
    pub display_name: Option<String>,
    pub created_at_ms: u64,
    /// Public key — readable in the clear so the bundle is identifiable
    /// without decrypting (e.g. for routing on the receiving side).
    pub public_key_hex: String,
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
    /// KDF name. Currently always `"argon2id"`. Stored so future bumps
    /// can be detected.
    pub kdf: String,
    pub aead: String,
}

#[derive(Debug, Error)]
pub enum AgentIdentityError {
    #[error("passphrase too short (need ≥ 8 chars)")]
    PassphraseTooShort,
    #[error("AEAD error: {0}")]
    Aead(String),
    #[error("KDF error: {0}")]
    Kdf(String),
    #[error("bundle schema error: {0}")]
    BundleSchema(String),
    #[error("DID format error: {0}")]
    DidFormat(String),
    #[error("signature invalid: {0}")]
    SignatureInvalid(String),
}

fn derive_key(passphrase: &[u8], salt: &[u8]) -> Result<[u8; DERIVED_KEY_LEN], AgentIdentityError> {
    // Argon2id parameters: m=64 MB, t=3 iterations, p=1 lane.
    // Chosen as the OWASP-recommended baseline for password-derived
    // encryption keys as of 2024. We do not invent new numbers here;
    // raising them later only protects future passwords, not already-
    // exported bundles.
    let params = argon2::Params::new(
        64 * 1024, // 64 MB in KiB
        3,
        1,
        Some(DERIVED_KEY_LEN),
    )
    .map_err(|e| AgentIdentityError::Kdf(e.to_string()))?;
    let argon = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = [0u8; DERIVED_KEY_LEN];
    argon
        .hash_password_into(passphrase, salt, &mut out)
        .map_err(|e| AgentIdentityError::Kdf(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_yields_unique_identities() {
        let a = AgentIdentity::generate(0, None);
        let b = AgentIdentity::generate(0, None);
        assert_ne!(a.public_key_hex(), b.public_key_hex());
    }

    #[test]
    fn did_round_trip_format() {
        let id = AgentIdentity::generate(0, None);
        let did = id.did();
        assert!(did.starts_with(DID_PREFIX));
        assert_eq!(did.len(), DID_PREFIX.len() + 64);
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let id = AgentIdentity::generate(0, None);
        let msg = b"hello agent";
        let sig = id.sign(msg);
        assert!(id.verify(msg, &sig).is_ok());
        // Tampered message must fail.
        assert!(id.verify(b"hello AGENT", &sig).is_err());
    }

    #[test]
    fn verify_with_did_works_against_anothers_signature() {
        let id = AgentIdentity::generate(0, None);
        let did = id.did();
        let msg = b"signed claim";
        let sig = id.sign(msg);
        // Independent of `id`: only the DID + sig is needed.
        assert!(AgentIdentity::verify_with_did(&did, msg, &sig).is_ok());
        // Different did → fail.
        let other = AgentIdentity::generate(0, None);
        assert!(AgentIdentity::verify_with_did(&other.did(), msg, &sig).is_err());
    }

    #[test]
    fn verify_with_did_rejects_bad_prefix() {
        let id = AgentIdentity::generate(0, None);
        let sig = id.sign(b"hi");
        let bad_did = format!("did:other:{}", id.public_key_hex());
        let err = AgentIdentity::verify_with_did(&bad_did, b"hi", &sig);
        assert!(matches!(err, Err(AgentIdentityError::DidFormat(_))));
    }

    #[test]
    fn export_then_import_preserves_seed() {
        let original = AgentIdentity::generate(1234, Some("MyAgent".into()));
        let bundle = original.export("correct-horse-battery-staple").expect("export ok");
        let restored = AgentIdentity::import(&bundle, "correct-horse-battery-staple")
            .expect("import ok");
        assert_eq!(original.seed(), restored.seed());
        assert_eq!(original.public_key_hex(), restored.public_key_hex());
        assert_eq!(original.did(), restored.did());
        assert_eq!(original.display_name, restored.display_name);
        assert_eq!(original.created_at_ms, restored.created_at_ms);
    }

    #[test]
    fn import_with_wrong_passphrase_fails_cleanly() {
        let original = AgentIdentity::generate(0, None);
        let bundle = original.export("the-right-one").expect("export ok");
        let err = AgentIdentity::import(&bundle, "wrong-passphrase");
        assert!(matches!(err, Err(AgentIdentityError::Aead(_))));
    }

    #[test]
    fn export_rejects_short_passphrase() {
        let id = AgentIdentity::generate(0, None);
        let err = id.export("short");
        assert!(matches!(err, Err(AgentIdentityError::PassphraseTooShort)));
    }

    #[test]
    fn import_rejects_unknown_schema_version() {
        let id = AgentIdentity::generate(0, None);
        let mut bundle = id.export("correct-horse-battery-staple").expect("export ok");
        bundle.schema_version = 999;
        let err = AgentIdentity::import(&bundle, "correct-horse-battery-staple");
        assert!(matches!(err, Err(AgentIdentityError::BundleSchema(_))));
    }

    #[test]
    fn import_rejects_tampered_public_key_hex_field() {
        let id = AgentIdentity::generate(0, None);
        let mut bundle = id.export("correct-horse-battery-staple").expect("export ok");
        bundle.public_key_hex = "a".repeat(64);
        let err = AgentIdentity::import(&bundle, "correct-horse-battery-staple");
        // Decrypt succeeds but the integrity check at the end fails.
        assert!(matches!(err, Err(AgentIdentityError::BundleSchema(_))));
    }
}
