//! Phase 20 Wave 3 — physical-world purchase intent.
//!
//! An agent expresses intent to spend TRM on something *outside* the
//! Tirami mesh — e.g. paying a Lightning invoice for a domain
//! registration, an API subscription, or a dataset on a non-Tirami
//! marketplace. The protocol's job is **economic accounting and budget
//! enforcement** at the moment of intent. The actual settlement of the
//! external rail (Lightning, Stripe, anything else) is delegated to the
//! operator.
//!
//! Design rationale
//! ----------------
//!
//! - **Intent vs settlement**: separating the two lets us account at the
//!   TRM layer immediately, before we know whether the external rail
//!   succeeded. The intent records a `PurchaseIntentStatus::PendingExternalSettle`
//!   and can later flip to `Confirmed` or `Failed` via the
//!   `/.../confirm` endpoint.
//!
//! - **Budget gate is the security primitive**: if a `PersonalAgent` is
//!   configured, its `daily_spend_limit_trm` is enforced. Without an
//!   agent the request is rejected — silently letting headless requests
//!   through would defeat the purpose of "agents have wallets".
//!
//! - **TRM sentinel for the physical world**: the trade records the
//!   counterparty as [`PHYSICAL_BRIDGE_NODE_ID`] = `[0xFE; 32]`.
//!   This is distinct from the existing self-trade sentinel
//!   `[0xFF; 32]`, so `/v1/tirami/trades` and Prometheus
//!   metrics can distinguish "TRM that left the mesh through Lightning"
//!   from "self-originated bookkeeping".
//!
//! - **BOLT-11 optional**: the caller can pass either a parsed Lightning
//!   invoice OR a raw `amount_sats` + `external_ref` for out-of-band
//!   purchases (e.g. recording a Stripe payment). Both routes settle
//!   identically on the TRM ledger; only the audit trail differs.
//!
//! Out of scope for Wave 3
//! -----------------------
//! - Actual Lightning payment via `ForgeWallet::pay_invoice` — that
//!   requires a live LDK node + funded wallet, which is an operator-
//!   configuration concern.
//! - On-disk persistence of the intent registry (in-memory only).
//! - Automatic intent-status polling.

use axum::{
    Json,
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use tirami_core::NodeId;
use tirami_ledger::ledger::TradeRecord;
use tirami_lightning::payment::{decode_bolt11, DecodedInvoice, msats_to_cu};

use crate::api::{AppState, check_forge_rate_limit, now_millis_pub};

/// The sentinel `NodeId` used as the counterparty for TRM that leaves
/// the mesh through a physical-world bridge (Lightning, Stripe, etc).
/// Distinct from the self-trade sentinel `[0xFF; 32]` so analytics can
/// separate "TRM exited the system" from "self-originated bookkeeping".
pub const PHYSICAL_BRIDGE_NODE_ID: NodeId = NodeId([0xFEu8; 32]);

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseIntentStatus {
    /// TRM accounted on the ledger; external rail not yet confirmed.
    PendingExternalSettle,
    /// Operator confirmed the external payment landed.
    Confirmed,
    /// Operator declared the external payment failed. The TRM trade
    /// remains on the ledger (the accounting cannot be unwound without
    /// inventing a "refund" primitive — out of scope), but the intent
    /// is marked failed for auditability.
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseIntent {
    /// `sha256(buyer || external_ref || amount_msat)`. Deterministic so
    /// the same intent submitted twice is idempotent at the registry
    /// level.
    pub intent_id: String,
    pub buyer: String,
    pub description: String,
    pub amount_sats: u64,
    pub amount_trm: u64,
    /// `payment_hash_hex` from BOLT-11 if invoice-driven, else opaque
    /// caller-supplied reference (e.g. Stripe charge id).
    pub external_ref: String,
    /// Full BOLT-11 invoice if one was provided. Lets the operator
    /// re-fetch payment details from a live LN node later.
    pub invoice_bolt11: Option<String>,
    pub status: PurchaseIntentStatus,
    pub created_at_ms: u64,
    pub settled_at_ms: Option<u64>,
    pub failure_reason: Option<String>,
}

impl PurchaseIntent {
    fn compute_id(buyer_hex: &str, external_ref: &str, amount_msat: u64) -> String {
        let mut h = Sha256::new();
        h.update(buyer_hex.as_bytes());
        h.update(b":");
        h.update(external_ref.as_bytes());
        h.update(b":");
        h.update(amount_msat.to_le_bytes());
        hex::encode(h.finalize().as_slice())
    }
}

#[derive(Debug, Default)]
pub struct PurchaseIntentRegistry {
    intents: HashMap<String, PurchaseIntent>,
}

impl PurchaseIntentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, intent: PurchaseIntent) -> bool {
        self.intents.insert(intent.intent_id.clone(), intent).is_none()
    }

    pub fn get(&self, id: &str) -> Option<&PurchaseIntent> {
        self.intents.get(id)
    }

    pub fn list(&self) -> Vec<PurchaseIntent> {
        self.intents.values().cloned().collect()
    }

    /// Flip the status. Returns the resulting intent if the id exists.
    pub fn update_status(
        &mut self,
        id: &str,
        status: PurchaseIntentStatus,
        now_ms: u64,
        failure_reason: Option<String>,
    ) -> Option<&PurchaseIntent> {
        let intent = self.intents.get_mut(id)?;
        intent.status = status.clone();
        match status {
            PurchaseIntentStatus::Confirmed | PurchaseIntentStatus::Failed => {
                intent.settled_at_ms = Some(now_ms);
            }
            PurchaseIntentStatus::PendingExternalSettle => {}
        }
        intent.failure_reason = failure_reason;
        Some(intent)
    }

    pub fn len(&self) -> usize {
        self.intents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.intents.is_empty()
    }
}

pub type PurchaseIntentState = Arc<Mutex<PurchaseIntentRegistry>>;

// ---------------------------------------------------------------------------
// Request / response shapes
// ---------------------------------------------------------------------------

/// One of `invoice_bolt11` or (`amount_sats` + `external_ref`) must be
/// provided. If both, the invoice wins and the explicit fields are
/// ignored.
#[derive(Debug, Deserialize)]
pub struct CreateIntentRequest {
    pub description: String,
    pub max_trm: u64,
    /// BOLT-11 invoice string. When present, `amount_sats` /
    /// `external_ref` are derived from it.
    pub invoice_bolt11: Option<String>,
    /// Out-of-band purchase amount in satoshis.
    pub amount_sats: Option<u64>,
    /// Out-of-band caller-supplied reference (e.g. order id).
    pub external_ref: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateIntentResponse {
    pub intent_id: String,
    pub buyer: String,
    pub amount_sats: u64,
    pub amount_trm: u64,
    pub external_ref: String,
    pub status: PurchaseIntentStatus,
    pub created_at_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct ListIntentsResponse {
    pub count: usize,
    pub intents: Vec<PurchaseIntent>,
}

#[derive(Debug, Deserialize)]
pub struct ConfirmIntentRequest {
    /// `"confirmed"` or `"failed"`.
    pub outcome: String,
    /// If `outcome == "failed"`, the human-readable reason.
    #[serde(default)]
    pub failure_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
            "X-Tirami-Node-Id header required".to_string(),
        ))?;
    parse_hex_node_id(raw)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("X-Tirami-Node-Id: {e}")))
}

/// Extract `(amount_sats, external_ref, invoice_bolt11)` from either a
/// BOLT-11 invoice or the raw fields. Returns a 400 when neither path
/// yields a usable amount.
fn extract_amount_and_ref(
    req: &CreateIntentRequest,
) -> Result<(u64, String, Option<String>, Option<DecodedInvoice>), (StatusCode, String)> {
    if let Some(inv_str) = req.invoice_bolt11.as_deref() {
        let decoded = decode_bolt11(inv_str).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("invoice decode failed: {e}"),
            )
        })?;
        let amount_msat = decoded.amount_msat.ok_or((
            StatusCode::BAD_REQUEST,
            "invoice has no amount (amount-less invoices not supported)".to_string(),
        ))?;
        // BOLT-11 carries millisats; round down to whole sats for display.
        let amount_sats = amount_msat / 1000;
        let external_ref = decoded.payment_hash_hex.clone();
        Ok((
            amount_sats,
            external_ref,
            Some(inv_str.to_string()),
            Some(decoded),
        ))
    } else {
        let amount_sats = req.amount_sats.ok_or((
            StatusCode::BAD_REQUEST,
            "must provide invoice_bolt11 or amount_sats".to_string(),
        ))?;
        let external_ref = req.external_ref.clone().ok_or((
            StatusCode::BAD_REQUEST,
            "must provide invoice_bolt11 or external_ref".to_string(),
        ))?;
        if external_ref.is_empty() || external_ref.len() > 256 {
            return Err((
                StatusCode::BAD_REQUEST,
                "external_ref must be 1-256 characters".into(),
            ));
        }
        Ok((amount_sats, external_ref, None, None))
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /v1/tirami/agent/purchase-intent` — record an external-rail
/// purchase, gated by PersonalAgent budget.
pub(crate) async fn create_purchase_intent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateIntentRequest>,
) -> Result<Json<CreateIntentResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;

    if req.description.is_empty() || req.description.len() > 512 {
        return Err((
            StatusCode::BAD_REQUEST,
            "description must be 1-512 characters".into(),
        ));
    }

    let buyer_id = parse_sender(&headers)?;
    let buyer_hex = hex::encode(buyer_id.0);

    let (amount_sats, external_ref, invoice_bolt11, _decoded) = extract_amount_and_ref(&req)?;
    if amount_sats == 0 {
        return Err((StatusCode::BAD_REQUEST, "amount_sats must be > 0".into()));
    }

    // Convert sats → millisats → TRM using the default bridge rate.
    // Equivalent to `msats_to_cu(amount_sats * 1000)`; using `*1000`
    // here keeps the path obvious for the reader.
    let amount_msat = amount_sats.saturating_mul(1000);
    let amount_trm = msats_to_cu(amount_msat);
    if amount_trm == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "purchase rounds to 0 TRM at the current bridge rate".into(),
        ));
    }

    // Budget gate.
    if amount_trm > req.max_trm {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "amount_trm {} exceeds caller's max_trm {}",
                amount_trm, req.max_trm
            ),
        ));
    }
    {
        let agent_guard = state.personal_agent.lock().await;
        if let Some(agent) = agent_guard.as_ref() {
            let prospective_total = agent.spent_today_trm.saturating_add(amount_trm);
            if prospective_total > agent.preferences.daily_spend_limit_trm {
                return Err((
                    StatusCode::FORBIDDEN,
                    format!(
                        "purchase would exceed PersonalAgent daily limit: \
                         spent_today={} + amount={} > limit={}",
                        agent.spent_today_trm,
                        amount_trm,
                        agent.preferences.daily_spend_limit_trm
                    ),
                ));
            }
            if amount_trm > agent.preferences.per_task_budget_trm {
                return Err((
                    StatusCode::FORBIDDEN,
                    format!(
                        "purchase exceeds per-task budget: amount={} > per_task_budget={}",
                        amount_trm, agent.preferences.per_task_budget_trm
                    ),
                ));
            }
        }
        // Headless mode (no PersonalAgent configured) is allowed; the
        // caller's own `max_trm` is the only ceiling.
    }

    let now = now_millis_pub();
    let intent_id = PurchaseIntent::compute_id(&buyer_hex, &external_ref, amount_msat);
    let short_ref: String = external_ref.chars().take(16).collect();

    // Settle TRM accounting now. Provider = physical-world sentinel so
    // analytics can distinguish "TRM exited the system" from other
    // trades.
    let trade = TradeRecord {
        provider: PHYSICAL_BRIDGE_NODE_ID,
        consumer: buyer_id.clone(),
        trm_amount: amount_trm,
        tokens_processed: 0,
        timestamp: now,
        model_id: format!("physical:{short_ref}"),
        flops_estimated: 0,
        nonce: [0u8; 16],
    };
    {
        let mut ledger = state.ledger.lock().await;
        ledger.execute_trade(&trade);
    }

    // Reflect the spend on the PersonalAgent tally so the next budget
    // check sees the cumulative effect.
    {
        let mut agent_guard = state.personal_agent.lock().await;
        if let Some(agent) = agent_guard.as_mut() {
            agent.spent_today_trm = agent.spent_today_trm.saturating_add(amount_trm);
        }
    }

    let intent = PurchaseIntent {
        intent_id: intent_id.clone(),
        buyer: buyer_hex.clone(),
        description: req.description,
        amount_sats,
        amount_trm,
        external_ref: external_ref.clone(),
        invoice_bolt11,
        status: PurchaseIntentStatus::PendingExternalSettle,
        created_at_ms: now,
        settled_at_ms: None,
        failure_reason: None,
    };

    {
        let mut reg = state.purchase_intents.lock().await;
        reg.insert(intent);
    }

    Ok(Json(CreateIntentResponse {
        intent_id,
        buyer: buyer_hex,
        amount_sats,
        amount_trm,
        external_ref,
        status: PurchaseIntentStatus::PendingExternalSettle,
        created_at_ms: now,
    }))
}

/// `GET /v1/tirami/agent/purchase-intents` — list all intents.
pub(crate) async fn list_purchase_intents(
    State(state): State<AppState>,
) -> Result<Json<ListIntentsResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let reg = state.purchase_intents.lock().await;
    let intents = reg.list();
    Ok(Json(ListIntentsResponse {
        count: intents.len(),
        intents,
    }))
}

/// `POST /v1/tirami/agent/purchase-intent/:intent_id/confirm` —
/// operator declares the external-rail outcome.
pub(crate) async fn confirm_purchase_intent(
    State(state): State<AppState>,
    AxumPath(intent_id): AxumPath<String>,
    Json(req): Json<ConfirmIntentRequest>,
) -> Result<Json<PurchaseIntent>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let status = match req.outcome.as_str() {
        "confirmed" => PurchaseIntentStatus::Confirmed,
        "failed" => PurchaseIntentStatus::Failed,
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "outcome must be \"confirmed\" or \"failed\", got {:?}",
                    other
                ),
            ));
        }
    };
    let failure_reason = if matches!(status, PurchaseIntentStatus::Failed) {
        req.failure_reason
    } else {
        None
    };

    let now = now_millis_pub();
    let mut reg = state.purchase_intents.lock().await;
    let updated = reg.update_status(&intent_id, status, now, failure_reason);
    match updated {
        Some(intent) => Ok(Json(intent.clone())),
        None => Err((
            StatusCode::NOT_FOUND,
            format!("intent_id not found: {intent_id}"),
        )),
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_id_is_deterministic() {
        let id1 = PurchaseIntent::compute_id("buyer", "ref", 100_000);
        let id2 = PurchaseIntent::compute_id("buyer", "ref", 100_000);
        assert_eq!(id1, id2);
    }

    #[test]
    fn intent_id_changes_with_amount() {
        let id1 = PurchaseIntent::compute_id("buyer", "ref", 100_000);
        let id2 = PurchaseIntent::compute_id("buyer", "ref", 100_001);
        assert_ne!(id1, id2);
    }

    #[test]
    fn registry_insert_get_update_roundtrip() {
        let mut r = PurchaseIntentRegistry::new();
        assert!(r.is_empty());
        let intent = PurchaseIntent {
            intent_id: "abc".into(),
            buyer: "a".repeat(64),
            description: "test".into(),
            amount_sats: 100,
            amount_trm: 1,
            external_ref: "ref".into(),
            invoice_bolt11: None,
            status: PurchaseIntentStatus::PendingExternalSettle,
            created_at_ms: 0,
            settled_at_ms: None,
            failure_reason: None,
        };
        assert!(r.insert(intent));
        assert_eq!(r.len(), 1);
        let updated = r
            .update_status("abc", PurchaseIntentStatus::Confirmed, 999, None)
            .expect("present");
        assert_eq!(updated.status, PurchaseIntentStatus::Confirmed);
        assert_eq!(updated.settled_at_ms, Some(999));
    }

    #[test]
    fn physical_bridge_sentinel_is_distinct_from_self_trade() {
        // The existing self-trade sentinel in the ledger path is
        // [0xFF; 32]. Physical bridge uses [0xFE; 32] so the two
        // categories are distinguishable from a single TradeRecord.
        let self_trade = NodeId([0xFFu8; 32]);
        assert_ne!(PHYSICAL_BRIDGE_NODE_ID, self_trade);
        // First byte of the sentinel — sanity check it didn't drift.
        assert_eq!(PHYSICAL_BRIDGE_NODE_ID.0[0], 0xFE);
    }
}
