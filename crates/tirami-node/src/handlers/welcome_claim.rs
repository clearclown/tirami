//! Phase 21 Wave 2 — autonomous welcome-loan claim.
//!
//! `POST /v1/tirami/agent/claim-welcome`
//!
//! An autonomous agent that has just authenticated via Phase 20 Wave 5
//! (DID-signed challenge → bearer token) can claim its own welcome
//! loan with this endpoint — no admin scope required, no human in
//! the loop. The handler trusts the `X-Tirami-Node-Id` header for
//! the borrower identity (same convention as `agent_message` /
//! `data_offer` / `purchase_intent`); a stake-required mining gate
//! could later be wired to require the header to match the bearer
//! token's `node_id`, but Wave 2 keeps it as a discoverable surface
//! consistent with the rest of the API.
//!
//! On success the response carries the full [`WelcomeLoanGrant`] so
//! the agent can plan around the 72-hour expiry.

use axum::{Json, extract::State, http::{HeaderMap, StatusCode}};
use serde::{Deserialize, Serialize};

use tirami_core::NodeId;
use tirami_ledger::{WelcomeLoanError, WelcomeLoanGrant};

use crate::api::{AppState, check_forge_rate_limit, now_millis_pub};

#[derive(Debug, Deserialize, Default)]
pub struct ClaimWelcomeRequest {
    /// Operator-supplied bucket key (typically the requester's ASN
    /// string, e.g. `"AS16509"` for AWS). Required for Phase 17
    /// Wave 4.1 per-bucket rate limiting. Empty string disables
    /// bucket-level rate limiting for this request.
    #[serde(default)]
    pub bucket: String,
}

#[derive(Debug, Serialize)]
pub struct ClaimWelcomeResponse {
    pub node_id: String,
    pub principal_trm: u64,
    pub granted_at_ms: u64,
    pub expires_at_ms: u64,
}

impl From<WelcomeLoanGrant> for ClaimWelcomeResponse {
    fn from(grant: WelcomeLoanGrant) -> Self {
        Self {
            node_id: hex::encode(grant.node_id.0),
            principal_trm: grant.principal_trm,
            granted_at_ms: grant.granted_at_ms,
            expires_at_ms: grant.expires_at_ms,
        }
    }
}

fn parse_hex_node_id(hex: &str) -> Result<NodeId, String> {
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "node id must be exactly 64 hex characters, got {}",
            hex.len()
        ));
    }
    let bytes = hex::decode(hex).map_err(|e| format!("hex decode failed: {e}"))?;
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(NodeId(arr))
}

fn parse_sender(headers: &HeaderMap) -> Result<NodeId, (StatusCode, String)> {
    let raw = headers
        .get("X-Tirami-Node-Id")
        .and_then(|v| v.to_str().ok())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "X-Tirami-Node-Id header required (claimant node id)".into(),
        ))?;
    parse_hex_node_id(raw)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("X-Tirami-Node-Id: {e}")))
}

/// `POST /v1/tirami/agent/claim-welcome`
pub(crate) async fn claim_welcome_loan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ClaimWelcomeRequest>,
) -> Result<Json<ClaimWelcomeResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let node_id = parse_sender(&headers)?;
    let now = now_millis_pub();
    let mut ledger = state.ledger.lock().await;
    let grant = ledger
        .grant_welcome_loan(node_id, &req.bucket, now)
        .map_err(|e| {
            let status = match e {
                WelcomeLoanError::AlreadyHasBalance => StatusCode::CONFLICT,
                WelcomeLoanError::SunsetReached => StatusCode::GONE,
                WelcomeLoanError::SybilCeiling => StatusCode::TOO_MANY_REQUESTS,
            };
            let code = match e {
                WelcomeLoanError::AlreadyHasBalance => "already_has_balance",
                WelcomeLoanError::SunsetReached => "sunset_reached",
                WelcomeLoanError::SybilCeiling => "sybil_ceiling",
            };
            (
                status,
                serde_json::json!({
                    "error": {
                        "type": "welcome_loan_denied",
                        "code": code,
                        "message": e.to_string(),
                    }
                })
                .to_string(),
            )
        })?;
    Ok(Json(ClaimWelcomeResponse::from(grant)))
}
