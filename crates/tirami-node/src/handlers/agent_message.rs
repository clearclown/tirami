//! Phase 20 Wave 1 — typed agent-to-agent message economy.
//!
//! `POST /v1/tirami/agent/message`
//!
//! An agent on this node sends a typed message to another agent on the
//! mesh. The protocol charges a flat TRM fee from sender to receiver
//! and records it as a `TradeRecord` with `flops_estimated = 0` and
//! `model_id = "agent_message:<kind>"`. This is the **economic primitive**
//! — actual P2P delivery of the message body is out of scope for Wave 1
//! (it can layer on top of the existing gossip / agora paths).
//!
//! Rationale: without this primitive, every agent-to-agent interaction
//! that isn't an LLM inference is unbilled. Phase 20's thesis is that
//! TRM should denominate **every** agent action, not just chat
//! completions; this is the first step.
//!
//! Pricing model (Wave 1): flat `MESSAGE_BASE_TRM = 1` per message.
//! Future waves can switch to dynamic pricing per kind, but the simple
//! constant lets us iterate without changing the semantic shape.

use axum::{Json, extract::State, http::{HeaderMap, StatusCode}};
use serde::{Deserialize, Serialize};
use tirami_core::NodeId;
use tirami_ledger::ledger::TradeRecord;

use crate::api::{AppState, check_forge_rate_limit, now_millis_pub};

/// Wave 1 flat fee. Justified as "pure protocol overhead; no compute happened."
/// Sized so 10 messages cost less than a single token's worth of inference.
const MESSAGE_BASE_TRM: u64 = 1;

/// Allowed message kinds. Strict allow-list — unknown kinds are rejected.
const ALLOWED_KINDS: &[&str] = &["request_action", "request_data", "broadcast"];

#[derive(Debug, Deserialize)]
pub struct AgentMessageRequest {
    /// Recipient NodeId, hex-encoded (64 chars).
    pub to: String,
    /// Message kind. Must be one of [`ALLOWED_KINDS`].
    pub kind: String,
    /// Opaque payload. Schema is up to the application layer; Tirami
    /// only records that the message was paid for, not its contents.
    /// (Privacy: store nothing identifying in here that you wouldn't
    /// want appearing in `model_id` debug output.)
    #[serde(default)]
    pub body: serde_json::Value,
    /// Sender's price ceiling. The actual fee must be ≤ this value or
    /// the request is rejected. Set to 1 to require Wave-1 flat pricing.
    pub max_trm: u64,
}

#[derive(Debug, Serialize)]
pub struct AgentMessageResponse {
    /// Trade ID = "msg:<kind>:<timestamp_ms>". Both sender and receiver
    /// can locate this on the ledger via `/v1/tirami/trades`.
    pub message_id: String,
    pub from: String,
    pub to: String,
    pub kind: String,
    pub trm_cost: u64,
    pub timestamp_ms: u64,
}

fn parse_hex_node_id(hex: &str) -> Result<NodeId, String> {
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("node id must be exactly 64 hex characters, got {}", hex.len()));
    }
    let bytes = hex::decode(hex).map_err(|e| format!("hex decode failed: {e}"))?;
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(NodeId(arr))
}

/// Sender identification.
///
/// Convention matches `/v1/chat/completions` and `/v1/tirami/agent/task`:
/// the `X-Tirami-Node-Id` header attributes the request to a specific
/// agent / consumer. If the header is missing, the request is rejected
/// (we never silently bill the local node — that would be free
/// self-dealing).
fn parse_sender(headers: &HeaderMap) -> Result<NodeId, (StatusCode, String)> {
    let raw = headers
        .get("X-Tirami-Node-Id")
        .and_then(|v| v.to_str().ok())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "X-Tirami-Node-Id header required (sender attribution)".to_string(),
        ))?;
    parse_hex_node_id(raw).map_err(|e| (StatusCode::BAD_REQUEST, format!("X-Tirami-Node-Id: {e}")))
}

pub(crate) async fn agent_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AgentMessageRequest>,
) -> Result<Json<AgentMessageResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    // Validate kind against allow-list.
    if !ALLOWED_KINDS.contains(&req.kind.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "kind must be one of {:?}, got {:?}",
                ALLOWED_KINDS, req.kind
            ),
        ));
    }

    // Parse + validate addresses.
    let to = parse_hex_node_id(&req.to)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("to: {e}")))?;
    let from = parse_sender(&headers)?;

    if from == to {
        return Err((
            StatusCode::BAD_REQUEST,
            "self-message not allowed (sender == recipient)".to_string(),
        ));
    }

    // Price gate.
    let trm_cost = MESSAGE_BASE_TRM;
    if trm_cost > req.max_trm {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "fee {} TRM exceeds sender's max_trm {}",
                trm_cost, req.max_trm
            ),
        ));
    }

    // Body size cap (Wave 1: keep small; this is the payment primitive,
    // not a bulk-transfer mechanism). 4 KB is generous for a request_action
    // payload and protects the ledger from amplification.
    let body_bytes = serde_json::to_vec(&req.body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("body serialize failed: {e}")))?;
    if body_bytes.len() > 4096 {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "message body {} bytes exceeds 4096-byte cap",
                body_bytes.len()
            ),
        ));
    }

    // Record as a TradeRecord. flops_estimated = 0 (no compute happened),
    // tokens_processed = 0 (this is not a token-priced inference),
    // model_id discriminates the action class.
    let timestamp = now_millis_pub();
    let model_id = format!("agent_message:{}", req.kind);
    let trade = TradeRecord {
        provider: to.clone(), // receiver earns
        consumer: from.clone(), // sender spends
        trm_amount: trm_cost,
        tokens_processed: 0,
        timestamp,
        model_id: model_id.clone(),
        flops_estimated: 0,
        nonce: [0u8; 16],
    };
    {
        let mut ledger = state.ledger.lock().await;
        ledger.execute_trade(&trade);
    }

    Ok(Json(AgentMessageResponse {
        message_id: format!("msg:{}:{}", req.kind, timestamp),
        from: hex::encode(from.0),
        to: hex::encode(to.0),
        kind: req.kind,
        trm_cost,
        timestamp_ms: timestamp,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_node_id_accepts_64_hex() {
        let valid = "a".repeat(64);
        assert!(parse_hex_node_id(&valid).is_ok());
    }

    #[test]
    fn parse_hex_node_id_rejects_short() {
        assert!(parse_hex_node_id("abc").is_err());
    }

    #[test]
    fn parse_hex_node_id_rejects_non_hex() {
        let bad = "g".repeat(64);
        assert!(parse_hex_node_id(&bad).is_err());
    }

    #[test]
    fn allowed_kinds_includes_canonical_three() {
        assert!(ALLOWED_KINDS.contains(&"request_action"));
        assert!(ALLOWED_KINDS.contains(&"request_data"));
        assert!(ALLOWED_KINDS.contains(&"broadcast"));
    }

    #[test]
    fn message_base_trm_is_strictly_positive() {
        // Fee must be > 0 — a free message means an agent can broadcast
        // infinitely cheap spam.
        assert!(MESSAGE_BASE_TRM >= 1);
    }
}
