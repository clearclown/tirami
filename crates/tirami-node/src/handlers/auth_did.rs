//! Phase 20 Wave 5 — DID-based authentication for autonomous mesh join.
//!
//! Until Wave 5, a human had to pre-share `TIRAMI_API_TOKEN` with
//! every node an agent wanted to use. That made fully-autonomous AI
//! agents structurally impossible — a human bottleneck sat between
//! the agent and the mesh on day one.
//!
//! Wave 5 swaps that for a Sign-In-With-Ed25519 challenge protocol:
//!
//!   1. Agent calls `GET /v1/tirami/auth/challenge` (public, no auth).
//!      Server returns a 32-byte random nonce + an expiry timestamp.
//!   2. Agent signs the raw nonce bytes with its `AgentIdentity`
//!      Ed25519 key (the same key behind its `did:tirami:<hex>`).
//!   3. Agent calls `POST /v1/tirami/auth/verify` (public, no auth)
//!      with `{ did, challenge_hex, signature_hex }`.
//!   4. Server verifies the signature against the DID's embedded
//!      public key, marks the challenge consumed, and issues a
//!      short-lived bearer token via the existing
//!      `Phase 17 Wave 1.5` `TokenStore`.
//!   5. Agent uses that bearer token for all subsequent calls.
//!
//! Properties this primitive guarantees:
//!
//! - **No human-shared secret** is required to join. The agent
//!   onboards purely with cryptographic material it generated
//!   itself in Wave 4.
//! - **Each challenge is single-use** and expires within
//!   `CHALLENGE_TTL_SECS` (default 5 min). A leaked challenge can't
//!   be replayed.
//! - **The issued bearer token's `node_id` is the DID's public key**,
//!   so all `/v1/tirami/trades` / Prometheus metrics that key on
//!   `NodeId` attribute usage to the right agent — not to whatever
//!   shared admin secret the operator happened to issue.
//! - **No new transport** — the existing `Authorization: Bearer
//!   <token>` middleware accepts these tokens unchanged because
//!   they go through the same `TokenStore`.
//!
//! Out of scope for Wave 5 (deferred):
//!
//! - Stake-required mining enforcement. The `can_provide_inference`
//!   function already exists in `tirami-ledger` from Phase 18.2;
//!   Wave 5.5 turns it on in trade paths.
//! - Welcome-loan auto-claim. New agents can today receive a welcome
//!   loan via `POST /v1/tirami/lend`, but the call still requires
//!   admin scope. Wave 5.5 lets a `Bearer` session token issued
//!   under a fresh DID claim its own welcome loan.

use axum::{Json, extract::State, http::StatusCode};
use ed25519_dalek::Signature;
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use tirami_core::NodeId;
use tirami_mind::{AgentIdentity, DID_PREFIX};

use crate::api::{AppState, check_forge_rate_limit, now_millis_pub};
use crate::api_tokens::ApiScope;

/// How long an issued challenge remains valid before it must be
/// re-fetched. 5 minutes is generous enough for a paused-then-resumed
/// agent loop while still small enough that a leaked challenge isn't
/// useful for very long.
pub const CHALLENGE_TTL_SECS: u64 = 300;

/// Session-token lifetime after a successful DID verify. 1 hour —
/// long enough that an agent doesn't have to re-handshake constantly,
/// short enough that revocation propagates quickly.
pub const SESSION_TTL_SECS: u64 = 3_600;

#[derive(Debug, Clone)]
struct ChallengeRecord {
    /// `challenge_hex` doubles as the map key; the raw bytes are kept
    /// alongside to avoid re-decoding on verify.
    challenge_bytes: [u8; 32],
    expires_at_ms: u64,
}

#[derive(Debug, Default)]
pub struct ChallengeStore {
    /// `challenge_hex` → record. Single-use: removing the entry on
    /// verify implements replay protection.
    by_hex: HashMap<String, ChallengeRecord>,
}

impl ChallengeStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mint a fresh challenge. Returns `(challenge_hex, expires_at_ms)`.
    pub fn mint(&mut self, now_ms: u64) -> (String, u64) {
        // Lazy GC: drop anything already past expiry. Bounded work
        // because `by_hex.len()` is at most O(concurrent fresh agents)
        // and old entries leave at the same rate they enter.
        self.by_hex.retain(|_, r| r.expires_at_ms > now_ms);

        let mut buf = [0u8; 32];
        OsRng.fill_bytes(&mut buf);
        let hex = hex::encode(buf);
        let expires_at_ms = now_ms.saturating_add(CHALLENGE_TTL_SECS.saturating_mul(1_000));
        self.by_hex.insert(
            hex.clone(),
            ChallengeRecord {
                challenge_bytes: buf,
                expires_at_ms,
            },
        );
        (hex, expires_at_ms)
    }

    /// Consume a challenge by its hex value. Returns the raw bytes
    /// on success, or an error if the challenge is unknown / expired.
    /// The entry is removed before return so a successful consume
    /// cannot be replayed.
    pub fn consume(&mut self, challenge_hex: &str, now_ms: u64) -> Result<[u8; 32], String> {
        let record = self
            .by_hex
            .remove(challenge_hex)
            .ok_or_else(|| "unknown challenge_hex (expired, already-consumed, or never issued)".to_string())?;
        if record.expires_at_ms <= now_ms {
            return Err("challenge expired".into());
        }
        Ok(record.challenge_bytes)
    }

    pub fn len(&self) -> usize {
        self.by_hex.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_hex.is_empty()
    }
}

pub type ChallengeState = Arc<Mutex<ChallengeStore>>;

// ---------------------------------------------------------------------------
// Request / response shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    pub challenge_hex: String,
    pub expires_at_ms: u64,
    pub server_node_id: String,
    pub ttl_secs: u64,
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub did: String,
    pub challenge_hex: String,
    pub signature_hex: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub did: String,
    pub session_token: String,
    pub expires_at_ms: u64,
    pub scope: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /v1/tirami/auth/challenge` — public, no auth.
pub(crate) async fn issue_challenge(
    State(state): State<AppState>,
) -> Result<Json<ChallengeResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let now = now_millis_pub();
    let mut store = state.auth_challenges.lock().await;
    let (challenge_hex, expires_at_ms) = store.mint(now);
    Ok(Json(ChallengeResponse {
        challenge_hex,
        expires_at_ms,
        server_node_id: hex::encode(state.local_node_id.0),
        ttl_secs: CHALLENGE_TTL_SECS,
    }))
}

/// `POST /v1/tirami/auth/verify` — public, no auth. Body:
/// `{ did, challenge_hex, signature_hex }`. On success returns a
/// short-lived bearer token.
pub(crate) async fn verify_did_signature(
    State(state): State<AppState>,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    // 1. Sanity-check the DID format up front so a malformed input
    //    is rejected before we burn the challenge.
    if !req.did.starts_with(DID_PREFIX) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("did must start with {DID_PREFIX}"),
        ));
    }
    let pk_hex = &req.did[DID_PREFIX.len()..];
    if pk_hex.len() != 64 || !pk_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "did suffix must be 64 hex characters".into(),
        ));
    }

    // 2. Parse the signature hex into an Ed25519 Signature object.
    if req.signature_hex.len() != 128 {
        return Err((
            StatusCode::BAD_REQUEST,
            "signature_hex must be 128 hex characters (64-byte Ed25519 sig)".into(),
        ));
    }
    let sig_bytes = hex::decode(&req.signature_hex)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("signature_hex decode: {e}")))?;
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let sig = Signature::from_bytes(&sig_arr);

    // 3. Consume the challenge (single-use). Doing this BEFORE
    //    verifying the signature is intentional: a replay attempt
    //    with a valid but already-consumed challenge fails here,
    //    and an attacker who guesses challenges burns valid ones
    //    one at a time rather than reusing one indefinitely.
    let now = now_millis_pub();
    let challenge_bytes = {
        let mut store = state.auth_challenges.lock().await;
        store
            .consume(&req.challenge_hex, now)
            .map_err(|e| (StatusCode::UNAUTHORIZED, e))?
    };

    // 4. Verify the signature against the DID's pubkey + the
    //    server-issued challenge bytes.
    AgentIdentity::verify_with_did(&req.did, &challenge_bytes, &sig)
        .map_err(|e| (StatusCode::UNAUTHORIZED, format!("signature invalid: {e}")))?;

    // 5. Issue a short-lived session token. The token's `node_id`
    //    is the DID's public key, so every authenticated trade or
    //    metric this session produces attributes to the right
    //    economic actor.
    let mut pk_bytes = [0u8; 32];
    pk_bytes.copy_from_slice(
        &hex::decode(pk_hex)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("did suffix decode: {e}")))?,
    );
    let did_node_id = NodeId(pk_bytes);
    let scope = ApiScope::Economy;
    let (raw_token, token_record) = {
        let mut store = state.api_tokens.lock().await;
        store.issue(
            did_node_id,
            scope,
            SESSION_TTL_SECS,
            format!("did-auth:{}", &req.did[DID_PREFIX.len()..DID_PREFIX.len() + 12]),
            now,
        )
    };

    Ok(Json(VerifyResponse {
        did: req.did,
        session_token: raw_token,
        expires_at_ms: token_record.expires_at_ms,
        scope: format!("{:?}", scope),
    }))
}

// ---------------------------------------------------------------------------
// Unit tests on the challenge store
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_then_consume_round_trip() {
        let mut store = ChallengeStore::new();
        let (hex_a, exp_a) = store.mint(1_000);
        assert!(exp_a > 1_000);
        let bytes = store.consume(&hex_a, 1_001).expect("consume ok");
        assert_eq!(hex::encode(bytes), hex_a);
    }

    #[test]
    fn consume_is_single_use() {
        let mut store = ChallengeStore::new();
        let (hex_a, _) = store.mint(1_000);
        store.consume(&hex_a, 1_001).expect("first consume ok");
        let err = store.consume(&hex_a, 1_002);
        assert!(err.is_err());
    }

    #[test]
    fn consume_rejects_expired() {
        let mut store = ChallengeStore::new();
        let (hex_a, exp) = store.mint(1_000);
        // Try to consume just after expiry.
        let err = store.consume(&hex_a, exp + 1);
        assert!(err.is_err(), "expired challenge should be rejected");
    }

    #[test]
    fn consume_rejects_unknown() {
        let mut store = ChallengeStore::new();
        let err = store.consume("00".repeat(32).as_str(), 1_000);
        assert!(err.is_err());
    }

    #[test]
    fn mint_gcs_old_entries() {
        let mut store = ChallengeStore::new();
        // Mint at t=0, then mint again well past TTL — old one is GC'd.
        store.mint(0);
        assert_eq!(store.len(), 1);
        store.mint(CHALLENGE_TTL_SECS * 1_000 + 1);
        assert_eq!(store.len(), 1, "old expired entry should have been GC'd");
    }

    #[test]
    fn mint_yields_unique_challenges() {
        let mut store = ChallengeStore::new();
        let (a, _) = store.mint(0);
        let (b, _) = store.mint(0);
        assert_ne!(a, b);
    }
}
