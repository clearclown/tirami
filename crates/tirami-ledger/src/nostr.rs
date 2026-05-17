//! Phase 22 Wave 1 — NIP-01 Schnorr event signing.
//!
//! Nostr (NIP-01) requires events to be signed with **Schnorr / BIP-340**
//! over secp256k1, not the Ed25519 Tirami uses everywhere else for its
//! own signed trades. This module ships the minimum primitive needed to
//! make `Nip90Publisher::build_advertisement_event` produce events that
//! a real Nostr relay will accept.
//!
//! Surface:
//!
//! - [`NostrIdentity`] — a secp256k1 keypair distinct from any
//!   Ed25519 NodeId / DID. Exposes the BIP-340 x-only pubkey (32 bytes
//!   of hex) that goes into the `pubkey` field of every NIP-01 event.
//! - [`NostrIdentity::sign_event`] — takes a partially-built event
//!   (with `kind`, `created_at`, `tags`, `content` set, but no `id`,
//!   `pubkey`, or `sig`), computes the NIP-01 canonical event id,
//!   signs it, and returns a complete JSON object.
//! - [`NostrError`] — typed failure surface.
//!
//! What this module deliberately does NOT do:
//!
//! - It does not generate, persist, or migrate `NostrIdentity` keys
//!   across nodes. Wave 1 builds the cryptographic primitive; an
//!   identity-management surface (similar to Phase 20 Wave 4's
//!   `AgentIdentity`) is a follow-up.
//! - It does not implement Nostr verification of incoming events
//!   beyond what `secp256k1::Secp256k1::verify_schnorr` provides.

use secp256k1::rand::rngs::OsRng;
use secp256k1::schnorr::Signature;
use secp256k1::{Keypair, Message, Secp256k1, SecretKey, XOnlyPublicKey};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Errors from [`NostrIdentity`] operations.
#[derive(Debug, Error)]
pub enum NostrError {
    #[error("malformed event JSON: {0}")]
    EventShape(&'static str),
    #[error("BIP-340 signature error: {0}")]
    Signature(String),
    #[error("BIP-340 verify failed: {0}")]
    Verify(String),
    #[error("hex decode failed: {0}")]
    Hex(String),
}

/// A secp256k1 keypair scoped to Nostr publishing.
///
/// Separate from the Tirami node's Ed25519 [`NodeId`](crate::NodeId) and
/// from any [`AgentIdentity`](https://docs.rs/tirami-mind) DID. A node
/// that wants to advertise on Nostr generates one of these; the rest
/// of the protocol (trades, gossip, etc.) is unaffected.
pub struct NostrIdentity {
    keypair: Keypair,
}

impl NostrIdentity {
    /// Generate a fresh keypair using `OsRng`.
    pub fn generate() -> Self {
        let secp = Secp256k1::new();
        let keypair = Keypair::new(&secp, &mut OsRng);
        Self { keypair }
    }

    /// Reconstruct from a 32-byte secret key.
    pub fn from_secret_bytes(secret: &[u8; 32]) -> Result<Self, NostrError> {
        let secp = Secp256k1::new();
        let sk = SecretKey::from_slice(secret)
            .map_err(|e| NostrError::Signature(e.to_string()))?;
        let keypair = Keypair::from_secret_key(&secp, &sk);
        Ok(Self { keypair })
    }

    /// 32-byte BIP-340 x-only public key.
    pub fn x_only_pubkey(&self) -> XOnlyPublicKey {
        self.keypair.x_only_public_key().0
    }

    /// Hex-encoded x-only pubkey — this is what shows up in the
    /// `pubkey` field of every signed Nostr event published by this
    /// identity.
    pub fn pubkey_hex(&self) -> String {
        hex::encode(self.x_only_pubkey().serialize())
    }

    /// 32-byte secret key (sensitive — do not log).
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.keypair.secret_bytes()
    }

    /// Sign a partially-built Nostr event.
    ///
    /// The input must already have `kind`, `created_at`, `tags`, and
    /// `content` set. This method computes the canonical
    /// [NIP-01](https://github.com/nostr-protocol/nips/blob/master/01.md)
    /// event id, signs it with BIP-340 Schnorr, and returns a complete
    /// event object (`id`, `pubkey`, `created_at`, `kind`, `tags`,
    /// `content`, `sig`) ready to ship over `agora_relay::publish_event`.
    pub fn sign_event(&self, mut event: Value) -> Result<Value, NostrError> {
        // 1. Replace / set the `pubkey` field with our x-only pubkey.
        let pubkey_hex = self.pubkey_hex();
        let obj = event
            .as_object_mut()
            .ok_or(NostrError::EventShape("event must be a JSON object"))?;
        obj.insert("pubkey".into(), Value::String(pubkey_hex.clone()));

        // 2. Compute the canonical event id per NIP-01: sha256 of
        //    the JSON serialisation of the array
        //    [0, pubkey, created_at, kind, tags, content].
        let created_at = obj
            .get("created_at")
            .and_then(|v| v.as_u64())
            .ok_or(NostrError::EventShape("missing or non-u64 created_at"))?;
        let kind = obj
            .get("kind")
            .and_then(|v| v.as_u64())
            .ok_or(NostrError::EventShape("missing or non-u64 kind"))?;
        let tags = obj
            .get("tags")
            .cloned()
            .ok_or(NostrError::EventShape("missing tags"))?;
        let content = obj
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or(NostrError::EventShape("missing or non-string content"))?
            .to_string();

        let serialized = Value::Array(vec![
            Value::Number(0u64.into()),
            Value::String(pubkey_hex.clone()),
            Value::Number(created_at.into()),
            Value::Number(kind.into()),
            tags,
            Value::String(content),
        ]);
        let canonical = serde_json::to_string(&serialized)
            .map_err(|e| NostrError::EventShape(Box::leak(e.to_string().into_boxed_str())))?;
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        let id_bytes: [u8; 32] = hasher.finalize().into();
        let id_hex = hex::encode(id_bytes);
        obj.insert("id".into(), Value::String(id_hex.clone()));

        // 3. Sign the id with BIP-340 Schnorr.
        let secp = Secp256k1::new();
        let msg = Message::from_digest(id_bytes);
        let sig = secp.sign_schnorr_no_aux_rand(&msg, &self.keypair);
        obj.insert(
            "sig".into(),
            Value::String(hex::encode(sig.serialize())),
        );

        Ok(event)
    }
}

/// Verify a signed Nostr event in isolation. Returns `Ok(())` on
/// success or [`NostrError::Verify`] on any failure (malformed
/// pubkey, id mismatch, signature mismatch).
///
/// Provided as a free function so verifiers don't need to hold a
/// [`NostrIdentity`].
pub fn verify_event(event: &Value) -> Result<(), NostrError> {
    let obj = event
        .as_object()
        .ok_or(NostrError::EventShape("event must be a JSON object"))?;

    let pubkey_hex = obj
        .get("pubkey")
        .and_then(|v| v.as_str())
        .ok_or(NostrError::EventShape("missing pubkey"))?;
    let id_hex = obj
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or(NostrError::EventShape("missing id"))?;
    let sig_hex = obj
        .get("sig")
        .and_then(|v| v.as_str())
        .ok_or(NostrError::EventShape("missing sig"))?;

    // Recompute the id from the event content and compare to the
    // advertised id — catches tampering with the body that left
    // the sig untouched.
    let created_at = obj
        .get("created_at")
        .and_then(|v| v.as_u64())
        .ok_or(NostrError::EventShape("missing or non-u64 created_at"))?;
    let kind = obj
        .get("kind")
        .and_then(|v| v.as_u64())
        .ok_or(NostrError::EventShape("missing or non-u64 kind"))?;
    let tags = obj
        .get("tags")
        .cloned()
        .ok_or(NostrError::EventShape("missing tags"))?;
    let content = obj
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or(NostrError::EventShape("missing or non-string content"))?
        .to_string();

    let serialized = Value::Array(vec![
        Value::Number(0u64.into()),
        Value::String(pubkey_hex.to_string()),
        Value::Number(created_at.into()),
        Value::Number(kind.into()),
        tags,
        Value::String(content),
    ]);
    let canonical = serde_json::to_string(&serialized)
        .map_err(|_| NostrError::EventShape("event reserialise failed"))?;
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let computed_id: [u8; 32] = hasher.finalize().into();
    let computed_id_hex = hex::encode(computed_id);
    if computed_id_hex != id_hex {
        return Err(NostrError::Verify(format!(
            "id mismatch (advertised {} vs recomputed {})",
            id_hex, computed_id_hex
        )));
    }

    // Decode the pubkey and signature, then verify.
    let pk_bytes =
        hex::decode(pubkey_hex).map_err(|e| NostrError::Hex(format!("pubkey: {e}")))?;
    let pk = XOnlyPublicKey::from_slice(&pk_bytes)
        .map_err(|e| NostrError::Verify(format!("pubkey decode: {e}")))?;
    let sig_bytes = hex::decode(sig_hex).map_err(|e| NostrError::Hex(format!("sig: {e}")))?;
    if sig_bytes.len() != 64 {
        return Err(NostrError::Verify(format!(
            "sig must be 64 bytes, got {}",
            sig_bytes.len()
        )));
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let sig = Signature::from_slice(&sig_arr)
        .map_err(|e| NostrError::Verify(format!("sig parse: {e}")))?;

    let secp = Secp256k1::verification_only();
    let msg = Message::from_digest(computed_id);
    secp.verify_schnorr(&sig, &msg, &pk)
        .map_err(|e| NostrError::Verify(format!("schnorr verify: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_event() -> Value {
        json!({
            "kind": 31990u64,
            "created_at": 1_700_000_000u64,
            "tags": [
                ["d", "forge-handler"],
                ["model", "qwen2.5:0.5b"]
            ],
            "content": "{\"tier\":\"small\"}",
        })
    }

    #[test]
    fn pubkey_is_32_byte_hex() {
        let id = NostrIdentity::generate();
        let hex = id.pubkey_hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn sign_event_attaches_id_pubkey_sig() {
        let id = NostrIdentity::generate();
        let signed = id.sign_event(sample_event()).expect("sign ok");
        let obj = signed.as_object().expect("obj");
        let id_field = obj["id"].as_str().expect("id");
        assert_eq!(id_field.len(), 64);
        assert_eq!(obj["pubkey"].as_str().expect("pubkey").len(), 64);
        assert_eq!(obj["sig"].as_str().expect("sig").len(), 128);
    }

    #[test]
    fn sign_then_verify_round_trip() {
        let id = NostrIdentity::generate();
        let signed = id.sign_event(sample_event()).expect("sign ok");
        verify_event(&signed).expect("verify ok");
    }

    #[test]
    fn tampered_content_fails_verification() {
        let id = NostrIdentity::generate();
        let mut signed = id.sign_event(sample_event()).expect("sign ok");
        // Mutate the content field without re-signing.
        signed["content"] = Value::String("evil-payload".into());
        let err = verify_event(&signed).expect_err("must fail");
        // Either the recomputed id doesn't match (caught early) OR
        // the schnorr verify fails outright; both manifest as
        // `NostrError::Verify`.
        assert!(matches!(err, NostrError::Verify(_)));
    }

    #[test]
    fn tampered_signature_fails_verification() {
        let id = NostrIdentity::generate();
        let mut signed = id.sign_event(sample_event()).expect("sign ok");
        // Flip the first byte of the sig.
        let sig_str = signed["sig"].as_str().unwrap().to_string();
        let mut bytes = hex::decode(&sig_str).unwrap();
        bytes[0] ^= 0x01;
        signed["sig"] = Value::String(hex::encode(bytes));
        let err = verify_event(&signed).expect_err("must fail");
        assert!(matches!(err, NostrError::Verify(_)));
    }

    #[test]
    fn substituted_pubkey_fails_verification() {
        // Build a valid event with identity A, then swap its pubkey
        // for identity B's. The id derives from pubkey, so the
        // recomputed id won't match; verification must fail.
        let alice = NostrIdentity::generate();
        let bob = NostrIdentity::generate();
        let mut signed = alice.sign_event(sample_event()).expect("sign ok");
        signed["pubkey"] = Value::String(bob.pubkey_hex());
        let err = verify_event(&signed).expect_err("must fail");
        assert!(matches!(err, NostrError::Verify(_)));
    }

    #[test]
    fn round_trip_from_secret_bytes_preserves_pubkey() {
        let original = NostrIdentity::generate();
        let secret = original.secret_bytes();
        let restored = NostrIdentity::from_secret_bytes(&secret).expect("ok");
        assert_eq!(original.pubkey_hex(), restored.pubkey_hex());
    }

    /// Two different identities must produce different ids for the
    /// same event body — the pubkey is part of the id pre-image, so
    /// any divergence in identity propagates into the id.
    #[test]
    fn different_identities_produce_different_event_ids() {
        let a = NostrIdentity::generate();
        let b = NostrIdentity::generate();
        let sa = a.sign_event(sample_event()).expect("ok");
        let sb = b.sign_event(sample_event()).expect("ok");
        assert_ne!(sa["id"], sb["id"]);
    }
}
