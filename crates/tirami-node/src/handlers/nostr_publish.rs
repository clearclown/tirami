//! Phase 22 Wave 3 — HTTP surface for NIP-90 publishing.
//!
//! Phase 22 Wave 1 shipped the cryptographic primitives (BIP-340
//! Schnorr signing, NIP-01 event id canonicalisation) and a one-call
//! library wrapper `Nip90Publisher::publish_signed_advertisement`,
//! but no HTTP surface to drive them. Wave 3 closes the loop: an
//! autonomous agent that has authenticated via Phase 20 Wave 5
//! (DID-signed bearer token) can now bootstrap a Nostr identity,
//! sign events, and publish to a real Nostr relay — all without
//! human-shared secrets.
//!
//! Endpoints (all auth-required, gated by the existing bearer
//! middleware):
//!
//! - `POST /v1/tirami/agora/nostr/init` — idempotent bootstrap of a
//!   per-node [`NostrIdentity`]. Returns the x-only pubkey.
//! - `GET  /v1/tirami/agora/nostr` — current pubkey + bootstrap state.
//! - `POST /v1/tirami/agora/nostr/sign-event` — sign an arbitrary
//!   partially-built NIP-01 event. Useful for testing and for clients
//!   that want to ship events on a relay we don't know about.
//! - `POST /v1/tirami/agora/publish` — build a kind-31990 handler
//!   advertisement from a [`ProviderAdvertisement`] payload, sign it,
//!   and optionally publish to a relay. With `dry_run = true` the
//!   relay is skipped and the signed event is returned for inspection —
//!   this is what tests use to verify the build+sign path without
//!   needing a live Nostr relay.
//!
//! Out of scope for Wave 3:
//!
//! - On-disk persistence of the Nostr keypair. In-memory only. A
//!   restart drops it; the next `init` call gets a new keypair. A
//!   follow-up wave can add encrypted persistence à la Phase 20
//!   Wave 4's `AgentIdentityBundle`.
//! - Subscribing to incoming events. We only publish in Wave 3.

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use tirami_ledger::{
    AgoraError, JobRequest, ModelTier, Nip90Publisher, NostrIdentity, ProviderAdvertisement,
};

use crate::api::{AppState, check_forge_rate_limit};

pub type NostrIdentityState = Arc<Mutex<Option<NostrIdentity>>>;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct NostrIdentityView {
    pub initialized: bool,
    pub pubkey_hex: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct InitRequest {}

#[derive(Debug, Deserialize)]
pub struct SignEventRequest {
    /// Partially-built event JSON. Must contain `kind`, `created_at`,
    /// `tags`, `content`. Caller can omit `pubkey`, `id`, `sig` — the
    /// signer overrides them. Any extra fields are preserved.
    pub event: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct SignEventResponse {
    pub event: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct PublishAdvertisementRequest {
    /// What the node is advertising on Nostr. Must carry a 64-char
    /// hex `node_pubkey_hex` (typically the Ed25519 node identity).
    pub advertisement: ProviderAdvertisement,
    /// Optional Nostr relay URL. Defaults to the library default
    /// (`wss://relay.damus.io` as of writing). Ignored when
    /// `dry_run = true`.
    #[serde(default)]
    pub relay_url: Option<String>,
    /// Timeout in seconds for the relay handshake + send + ack.
    /// Defaults to 5 s. Ignored when `dry_run = true`.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// When `true`, build and sign the event but **do not** open a
    /// WebSocket connection to a relay. Returns the signed event so
    /// the caller can ship it on their own transport (or inspect it
    /// in tests). Default `false` — actual publish is the common
    /// case.
    #[serde(default)]
    pub dry_run: bool,
}

fn default_timeout_secs() -> u64 {
    5
}

#[derive(Debug, Serialize)]
pub struct PublishAdvertisementResponse {
    /// The signed event that was published (or would have been, in
    /// `dry_run` mode). Caller can verify or re-ship it.
    pub event: serde_json::Value,
    /// `true` if the relay returned OK; `false` if `dry_run`
    /// suppressed the publish.
    pub relay_accepted: bool,
    /// Echo of the relay URL used (after default resolution).
    pub relay_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

fn map_agora_error(e: AgoraError) -> (StatusCode, String) {
    match e {
        AgoraError::InvalidPubkey => (StatusCode::BAD_REQUEST, e.to_string()),
        AgoraError::Serialization(_) => (StatusCode::BAD_REQUEST, e.to_string()),
        AgoraError::RelayError(_) => (StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

/// `POST /v1/tirami/agora/nostr/init` — bootstrap, idempotent.
pub(crate) async fn nostr_init(
    State(state): State<AppState>,
    Json(_req): Json<InitRequest>,
) -> Result<Json<NostrIdentityView>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let mut guard = state.nostr_identity.lock().await;
    if let Some(existing) = guard.as_ref() {
        return Ok(Json(NostrIdentityView {
            initialized: true,
            pubkey_hex: Some(existing.pubkey_hex()),
        }));
    }
    let id = NostrIdentity::generate();
    let view = NostrIdentityView {
        initialized: true,
        pubkey_hex: Some(id.pubkey_hex()),
    };
    *guard = Some(id);
    Ok(Json(view))
}

/// `GET /v1/tirami/agora/nostr` — current pubkey + state.
pub(crate) async fn nostr_status(
    State(state): State<AppState>,
) -> Result<Json<NostrIdentityView>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let guard = state.nostr_identity.lock().await;
    Ok(Json(NostrIdentityView {
        initialized: guard.is_some(),
        pubkey_hex: guard.as_ref().map(|id| id.pubkey_hex()),
    }))
}

/// `POST /v1/tirami/agora/nostr/sign-event` — sign an arbitrary
/// partially-built event. Useful when the caller has its own opinion
/// about the event shape that doesn't fit the kind-31990 advertisement
/// flow.
pub(crate) async fn nostr_sign_event(
    State(state): State<AppState>,
    Json(req): Json<SignEventRequest>,
) -> Result<Json<SignEventResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let guard = state.nostr_identity.lock().await;
    let id = guard.as_ref().ok_or((
        StatusCode::PRECONDITION_FAILED,
        "no NostrIdentity bootstrapped; POST /v1/tirami/agora/nostr/init first".to_string(),
    ))?;
    let signed = id.sign_event(req.event).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("sign_event failed: {e}"),
        )
    })?;
    Ok(Json(SignEventResponse { event: signed }))
}

/// `POST /v1/tirami/agora/publish` — build, sign, optionally publish.
pub(crate) async fn agora_publish(
    State(state): State<AppState>,
    Json(req): Json<PublishAdvertisementRequest>,
) -> Result<Json<PublishAdvertisementResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let guard = state.nostr_identity.lock().await;
    let id = guard.as_ref().ok_or((
        StatusCode::PRECONDITION_FAILED,
        "no NostrIdentity bootstrapped; POST /v1/tirami/agora/nostr/init first".to_string(),
    ))?;

    let publisher = Nip90Publisher;
    let unsigned = publisher
        .build_advertisement_event(&req.advertisement)
        .map_err(map_agora_error)?;
    let signed = id
        .sign_event(unsigned)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("sign_event failed: {e}")))?;

    if req.dry_run {
        return Ok(Json(PublishAdvertisementResponse {
            event: signed,
            relay_accepted: false,
            relay_url: None,
        }));
    }
    // Resolve the relay URL, then drop the lock before the WebSocket
    // round-trip so other requests can still hit the Nostr state.
    let url = req
        .relay_url
        .clone()
        .unwrap_or_else(|| tirami_ledger::agora_relay::DEFAULT_RELAY_URL.to_string());
    drop(guard);
    tirami_ledger::agora_relay::publish_event(&url, &signed, req.timeout_secs)
        .await
        .map_err(map_agora_error)?;
    Ok(Json(PublishAdvertisementResponse {
        event: signed,
        relay_accepted: true,
        relay_url: Some(url),
    }))
}

// ---------------------------------------------------------------------------
// Unit tests on the handler types
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_timeout_is_5_seconds() {
        assert_eq!(default_timeout_secs(), 5);
    }

    #[test]
    fn nostr_identity_view_serialises_with_initialized_and_pubkey() {
        let v = NostrIdentityView {
            initialized: true,
            pubkey_hex: Some("aa".repeat(32)),
        };
        let s = serde_json::to_string(&v).unwrap();
        assert!(s.contains("\"initialized\":true"));
        assert!(s.contains("\"pubkey_hex\":\""));
    }

    #[test]
    fn publish_request_dry_run_default_is_false() {
        let body: PublishAdvertisementRequest =
            serde_json::from_value(json!({
                "advertisement": {
                    "node_pubkey_hex": "a".repeat(64),
                    "models": ["m"],
                    "tier": "small",
                    "trm_per_token": 1u64,
                    "reputation": 0.5f64,
                    "accepted_payment": ["cu"],
                    "relays": []
                }
            }))
            .unwrap();
        assert!(!body.dry_run);
    }

    #[test]
    fn publish_request_dry_run_can_be_set_to_true() {
        let body: PublishAdvertisementRequest =
            serde_json::from_value(json!({
                "advertisement": {
                    "node_pubkey_hex": "a".repeat(64),
                    "models": ["m"],
                    "tier": "small",
                    "trm_per_token": 1u64,
                    "reputation": 0.5f64,
                    "accepted_payment": ["cu"],
                    "relays": []
                },
                "dry_run": true
            }))
            .unwrap();
        assert!(body.dry_run);
    }
}

// Silence the unused-import warning for `JobRequest`/`ModelTier` — they
// are part of the public agora surface and re-exported for handler
// consumers that build job_request events through the same identity.
#[allow(dead_code)]
fn _marker(_: JobRequest, _: ModelTier) {}
