//! NIP-90 (Data Vending Machines) compatibility layer for Forge.
//!
//! This module defines the event types and publishing interface for
//! announcing inference capabilities and CU pricing to the Nostr network.
//!
//! # Status: Stub
//!
//! This is a minimum-viable scaffold. It builds well-formed NIP-90 event
//! JSON that can be submitted to any Nostr relay, but does not yet
//! implement the actual relay connection. A future revision will wire
//! this into the mesh-llm Nostr transport.
//!
//! # NIP-90 reference
//!
//! - Job request: kind 5050 (customer asks for inference)
//! - Job result: kind 6050 (provider delivers response)
//! - Handler advertisement: kind 31990 (provider announces capabilities)
//!
//! See <https://github.com/nostr-protocol/nips/blob/master/90.md>
//!
//! # Example (planned)
//!
//! ```no_run
//! use tirami_ledger::agora::{Nip90Publisher, ProviderAdvertisement};
//! use tirami_ledger::lending::ModelTier;
//!
//! let ad = ProviderAdvertisement {
//!     node_pubkey_hex: "abc...".to_string(),
//!     models: vec!["qwen3-8b".to_string()],
//!     tier: ModelTier::Medium,
//!     trm_per_token: 3,
//!     reputation: 0.85,
//!     accepted_payment: vec!["cu".into(), "lightning".into()],
//!     relays: vec!["wss://relay.damus.io".into()],
//! };
//!
//! let event_json = Nip90Publisher::build_handler_event(&ad)?;
//! // ... submit event_json to a Nostr relay via an external client
//! # Ok::<_, tirami_ledger::agora::AgoraError>(())
//! ```

use serde::{Deserialize, Serialize};

use crate::lending::ModelTier;

/// NIP-90 event kinds used by Forge.
///
/// All kinds come from the [NIP-90 specification](https://github.com/nostr-protocol/nips/blob/master/90.md).
pub mod kinds {
    /// Text generation job request (customer → provider).
    pub const JOB_REQUEST_TEXT: u16 = 5050;
    /// Text generation job result (provider → customer).
    pub const JOB_RESULT_TEXT: u16 = 6050;
    /// Replaceable handler advertisement (provider → relay).
    ///
    /// This is the "I am open for business" event.
    pub const HANDLER_ADVERTISEMENT: u16 = 31990;
}

/// A provider's capability advertisement.
///
/// Published as a NIP-90 kind 31990 event so consumers can discover this
/// node's models, pricing, and reputation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderAdvertisement {
    /// Nostr pubkey (hex-encoded, 64 chars). Typically the Forge node's
    /// Ed25519 public key reused as the Nostr identity.
    pub node_pubkey_hex: String,
    /// Human-readable model identifiers this node serves.
    pub models: Vec<String>,
    /// Pricing tier (Small / Medium / Large / Frontier).
    pub tier: ModelTier,
    /// Current base CU per token at this node.
    pub trm_per_token: u64,
    /// Reputation score (0.0-1.0).
    pub reputation: f64,
    /// Accepted payment methods. Typically ["cu"] or ["cu", "lightning"].
    pub accepted_payment: Vec<String>,
    /// Nostr relay URLs this node listens on.
    pub relays: Vec<String>,
}

/// A job request wrapping an inference task and its CU budget.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobRequest {
    /// Nostr event id the customer generated for this request.
    pub event_id_hex: String,
    /// Customer's Nostr pubkey.
    pub customer_pubkey_hex: String,
    /// Optional target provider (omit for any).
    pub target_provider_hex: Option<String>,
    /// Model requested.
    pub model: String,
    /// Prompt.
    pub prompt: String,
    /// Maximum CU the customer will pay.
    pub max_cu: u64,
    /// Max output tokens.
    pub max_tokens: u64,
    /// Creation timestamp (seconds since Unix epoch).
    pub created_at: u64,
}

/// A job result returned by a provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobResult {
    /// The request event id this result replies to.
    pub request_event_id_hex: String,
    /// Provider's Nostr pubkey.
    pub provider_pubkey_hex: String,
    /// The inference output text.
    pub output: String,
    /// Number of tokens actually generated.
    pub tokens_produced: u64,
    /// CU charged (may be less than max_cu).
    pub cu_charged: u64,
    /// Creation timestamp (seconds since Unix epoch).
    pub created_at: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum AgoraError {
    #[error("invalid pubkey hex")]
    InvalidPubkey,
    #[error("serialization failed: {0}")]
    Serialization(String),
    #[error("nostr relay error: {0}")]
    RelayError(String),
}

/// Builds NIP-90 event payloads for publication to Nostr relays.
///
/// This struct is currently a namespace; it holds no state. A future
/// version will carry a Nostr relay client handle.
pub struct Nip90Publisher;

impl Nip90Publisher {
    /// Build a NIP-90 kind 31990 handler advertisement event.
    ///
    /// Returns a JSON string that conforms to the NIP-01 event format. The
    /// caller is responsible for signing and publishing to a relay.
    ///
    /// The returned JSON has the shape:
    /// ```json
    /// {
    ///   "kind": 31990,
    ///   "pubkey": "...",
    ///   "created_at": 1234567890,
    ///   "tags": [
    ///     ["d", "forge-handler"],
    ///     ["k", "5050"],
    ///     ["model", "qwen3-8b"],
    ///     ["trm_per_token", "3"],
    ///     ["reputation", "0.85"]
    ///   ],
    ///   "content": "{\"tier\":\"medium\",\"accepted_payment\":[\"cu\",\"lightning\"]}"
    /// }
    /// ```
    pub fn build_handler_event(ad: &ProviderAdvertisement) -> Result<String, AgoraError> {
        if ad.node_pubkey_hex.len() != 64 {
            return Err(AgoraError::InvalidPubkey);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut tags: Vec<Vec<String>> = vec![
            vec!["d".into(), "forge-handler".into()],
            vec!["k".into(), kinds::JOB_REQUEST_TEXT.to_string()],
            vec!["trm_per_token".into(), ad.trm_per_token.to_string()],
            vec!["reputation".into(), format!("{:.3}", ad.reputation)],
        ];
        for model in &ad.models {
            tags.push(vec!["model".into(), model.clone()]);
        }
        for relay in &ad.relays {
            tags.push(vec!["relay".into(), relay.clone()]);
        }

        let content = serde_json::json!({
            "tier": tier_label(ad.tier),
            "accepted_payment": ad.accepted_payment,
        });

        let event = serde_json::json!({
            "kind": kinds::HANDLER_ADVERTISEMENT,
            "pubkey": ad.node_pubkey_hex,
            "created_at": now,
            "tags": tags,
            "content": content.to_string(),
        });

        serde_json::to_string(&event).map_err(|e| AgoraError::Serialization(e.to_string()))
    }

    /// Build a NIP-90 kind 31990 handler advertisement event as a `serde_json::Value`.
    ///
    /// This is the typed-value variant used by `agora_relay::publish_event`. The event
    /// is unsigned — callers are responsible for adding `id`, `pubkey`, and `sig` before
    /// publishing if Nostr signature verification is required by the relay.
    pub fn build_advertisement_event(
        &self,
        ad: &ProviderAdvertisement,
    ) -> Result<serde_json::Value, AgoraError> {
        if ad.node_pubkey_hex.len() != 64 {
            return Err(AgoraError::InvalidPubkey);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut tags: Vec<Vec<String>> = vec![
            vec!["d".into(), "forge-handler".into()],
            vec!["k".into(), kinds::JOB_REQUEST_TEXT.to_string()],
            vec!["trm_per_token".into(), ad.trm_per_token.to_string()],
            vec!["reputation".into(), format!("{:.3}", ad.reputation)],
        ];
        for model in &ad.models {
            tags.push(vec!["model".into(), model.clone()]);
        }
        for relay in &ad.relays {
            tags.push(vec!["relay".into(), relay.clone()]);
        }

        let content = serde_json::json!({
            "tier": tier_label(ad.tier),
            "accepted_payment": ad.accepted_payment,
        });

        Ok(serde_json::json!({
            "kind": kinds::HANDLER_ADVERTISEMENT,
            "pubkey": ad.node_pubkey_hex,
            "created_at": now,
            "tags": tags,
            "content": content.to_string(),
        }))
    }

    /// Build a NIP-90 kind 31990 advertisement event and publish it to a Nostr relay.
    ///
    /// `relay_url` defaults to `agora_relay::DEFAULT_RELAY_URL` if `None`.
    /// Returns `Ok(())` on relay acceptance, or `Err(AgoraError::RelayError(...))` on
    /// connection failure, timeout, or relay rejection.
    pub async fn publish_advertisement(
        &self,
        advertisement: &ProviderAdvertisement,
        relay_url: Option<&str>,
        timeout_sec: u64,
    ) -> Result<(), AgoraError> {
        let event = self.build_advertisement_event(advertisement)?;
        let url = relay_url.unwrap_or(crate::agora_relay::DEFAULT_RELAY_URL);
        crate::agora_relay::publish_event(url, &event, timeout_sec).await
    }

    /// Build a NIP-90 kind 5050 job request event.
    pub fn build_job_request_event(req: &JobRequest) -> Result<String, AgoraError> {
        if req.customer_pubkey_hex.len() != 64 {
            return Err(AgoraError::InvalidPubkey);
        }
        let mut tags: Vec<Vec<String>> = vec![
            vec!["i".into(), req.prompt.clone(), "text".into()],
            vec!["param".into(), "model".into(), req.model.clone()],
            vec!["param".into(), "max_tokens".into(), req.max_tokens.to_string()],
            vec!["bid".into(), req.max_cu.to_string(), "cu".into()],
        ];
        if let Some(target) = &req.target_provider_hex {
            tags.push(vec!["p".into(), target.clone()]);
        }

        let event = serde_json::json!({
            "kind": kinds::JOB_REQUEST_TEXT,
            "pubkey": req.customer_pubkey_hex,
            "created_at": req.created_at,
            "tags": tags,
            "content": "",
        });
        serde_json::to_string(&event).map_err(|e| AgoraError::Serialization(e.to_string()))
    }

    /// Build a NIP-90 kind 6050 job result event.
    pub fn build_job_result_event(res: &JobResult) -> Result<String, AgoraError> {
        if res.provider_pubkey_hex.len() != 64 {
            return Err(AgoraError::InvalidPubkey);
        }
        let tags = vec![
            vec!["e".into(), res.request_event_id_hex.clone()],
            vec!["tokens".into(), res.tokens_produced.to_string()],
            vec!["cu_charged".into(), res.cu_charged.to_string()],
        ];

        let event = serde_json::json!({
            "kind": kinds::JOB_RESULT_TEXT,
            "pubkey": res.provider_pubkey_hex,
            "created_at": res.created_at,
            "tags": tags,
            "content": res.output,
        });
        serde_json::to_string(&event).map_err(|e| AgoraError::Serialization(e.to_string()))
    }
}

fn tier_label(tier: ModelTier) -> &'static str {
    match tier {
        ModelTier::Small => "small",
        ModelTier::Medium => "medium",
        ModelTier::Large => "large",
        ModelTier::Frontier => "frontier",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_pubkey() -> String {
        // 64 hex chars = 32 bytes
        "a".repeat(64)
    }

    #[test]
    fn handler_event_has_correct_kind() {
        let ad = ProviderAdvertisement {
            node_pubkey_hex: example_pubkey(),
            models: vec!["qwen3-8b".into()],
            tier: ModelTier::Medium,
            trm_per_token: 3,
            reputation: 0.85,
            accepted_payment: vec!["cu".into()],
            relays: vec!["wss://relay.test".into()],
        };
        let json = Nip90Publisher::build_handler_event(&ad).unwrap();
        assert!(json.contains("\"kind\":31990"));
    }

    #[test]
    fn handler_event_includes_model_tags() {
        let ad = ProviderAdvertisement {
            node_pubkey_hex: example_pubkey(),
            models: vec!["a".into(), "b".into()],
            tier: ModelTier::Small,
            trm_per_token: 1,
            reputation: 0.5,
            accepted_payment: vec!["cu".into()],
            relays: vec![],
        };
        let json = Nip90Publisher::build_handler_event(&ad).unwrap();
        assert!(json.contains("\"model\""));
    }

    #[test]
    fn invalid_pubkey_is_rejected() {
        let ad = ProviderAdvertisement {
            node_pubkey_hex: "too-short".into(),
            models: vec![],
            tier: ModelTier::Small,
            trm_per_token: 1,
            reputation: 0.0,
            accepted_payment: vec![],
            relays: vec![],
        };
        assert!(matches!(
            Nip90Publisher::build_handler_event(&ad),
            Err(AgoraError::InvalidPubkey)
        ));
    }

    #[test]
    fn job_request_event_has_correct_kind() {
        let req = JobRequest {
            event_id_hex: "b".repeat(64),
            customer_pubkey_hex: example_pubkey(),
            target_provider_hex: None,
            model: "qwen3-8b".into(),
            prompt: "hello".into(),
            max_cu: 100,
            max_tokens: 256,
            created_at: 1_700_000_000,
        };
        let json = Nip90Publisher::build_job_request_event(&req).unwrap();
        assert!(json.contains("\"kind\":5050"));
        assert!(json.contains("hello"));
        assert!(json.contains("bid"));
    }

    #[test]
    fn job_result_event_has_correct_kind() {
        let res = JobResult {
            request_event_id_hex: "c".repeat(64),
            provider_pubkey_hex: example_pubkey(),
            output: "greeting".into(),
            tokens_produced: 42,
            cu_charged: 42,
            created_at: 1_700_000_001,
        };
        let json = Nip90Publisher::build_job_result_event(&res).unwrap();
        assert!(json.contains("\"kind\":6050"));
        assert!(json.contains("greeting"));
    }

    #[test]
    fn kind_constants_match_nip90() {
        assert_eq!(kinds::JOB_REQUEST_TEXT, 5050);
        assert_eq!(kinds::JOB_RESULT_TEXT, 6050);
        assert_eq!(kinds::HANDLER_ADVERTISEMENT, 31990);
    }

    #[test]
    fn tier_label_round_trip() {
        assert_eq!(tier_label(ModelTier::Small), "small");
        assert_eq!(tier_label(ModelTier::Medium), "medium");
        assert_eq!(tier_label(ModelTier::Large), "large");
        assert_eq!(tier_label(ModelTier::Frontier), "frontier");
    }
}
