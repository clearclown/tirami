//! Phase 20 Wave 2 — priced data offer marketplace.
//!
//! An agent that owns a dataset publishes a `DataOffer` describing
//! what it has, its SHA-256 digest, the TRM price, and an expiry.
//! Buyers see the offer **without** the fetch URL. A buyer who wants
//! it calls `POST /v1/tirami/data/purchase { offer_id }`; the
//! protocol settles TRM seller → buyer via the existing dual-signed
//! trade path, and **then** the fetch URL is revealed in the response.
//!
//! Why this exists (Phase 20 § 1 ❌ gap #3): without a priced data
//! primitive, every agent-to-agent interaction outside chat completions
//! is unbilled. Datasets are a primary non-inference action class an
//! AI agent should be able to pay for.
//!
//! Wave 2 scope:
//! - In-memory `DataOfferRegistry` (per-node, no gossip yet).
//! - HTTP endpoints publish / list / purchase.
//! - `TradeRecord` records the settlement with
//!   `model_id = "data_offer:<offer_id_short>"`, `tokens_processed = 0`,
//!   `flops_estimated = 0`.
//!
//! Out of scope (Wave 2.5+):
//! - Cross-mesh gossip of offers (will reuse PriceSignal channel).
//! - On-disk persistence of the registry.
//! - Dual-signed PurchaseIntent (currently the trade is recorded by
//!   the buyer's node only; gossip will add the seller's countersign).
//! - Actual data delivery — the fetch URL is the seller's
//!   responsibility; this protocol only proves payment.

use axum::{Json, extract::State, http::{HeaderMap, StatusCode}};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use tirami_core::NodeId;
use tirami_ledger::ledger::TradeRecord;

use crate::api::{AppState, check_forge_rate_limit, now_millis_pub};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A signed offer to sell access to a dataset for a flat TRM price.
///
/// `offer_id` is a deterministic hash of the seller + digest + price
/// so the same dataset published twice gets the same id (idempotency).
/// The `fetch_url` is stored locally but never returned to non-buyers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataOffer {
    pub offer_id: String,
    pub seller: String,        // hex-encoded NodeId
    pub description: String,
    pub sha256_digest: String, // hex, 64 chars
    pub price_trm: u64,
    pub expiry_ms: u64,
    pub published_at_ms: u64,
    /// **Never** serialized to non-buyers. Marked `#[serde(skip)]` so
    /// list responses cannot accidentally leak it.
    #[serde(skip)]
    pub fetch_url: String,
}

impl DataOffer {
    fn compute_id(seller: &str, sha256_digest: &str, price_trm: u64) -> String {
        let mut h = Sha256::new();
        h.update(seller.as_bytes());
        h.update(b":");
        h.update(sha256_digest.as_bytes());
        h.update(b":");
        h.update(price_trm.to_le_bytes());
        let digest = h.finalize();
        hex::encode(&digest[..])
    }
}

/// Per-node in-memory store. Wave 2 scope: no persistence, no gossip.
#[derive(Debug, Default)]
pub struct DataOfferRegistry {
    offers: HashMap<String, DataOffer>,
}

impl DataOfferRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, offer: DataOffer) -> bool {
        self.offers.insert(offer.offer_id.clone(), offer).is_none()
    }

    pub fn get(&self, offer_id: &str) -> Option<&DataOffer> {
        self.offers.get(offer_id)
    }

    pub fn remove(&mut self, offer_id: &str) -> Option<DataOffer> {
        self.offers.remove(offer_id)
    }

    pub fn list(&self) -> Vec<DataOffer> {
        self.offers.values().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.offers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.offers.is_empty()
    }
}

pub type DataOfferState = Arc<Mutex<DataOfferRegistry>>;

// ---------------------------------------------------------------------------
// Request / response shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PublishOfferRequest {
    pub description: String,
    pub sha256_digest: String,
    pub price_trm: u64,
    pub expiry_ms: u64,
    pub fetch_url: String,
}

#[derive(Debug, Serialize)]
pub struct PublishOfferResponse {
    pub offer_id: String,
    pub seller: String,
    pub published_at_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct ListOffersResponse {
    pub count: usize,
    pub offers: Vec<DataOffer>,
}

#[derive(Debug, Deserialize)]
pub struct PurchaseRequest {
    pub offer_id: String,
}

#[derive(Debug, Serialize)]
pub struct PurchaseResponse {
    pub offer_id: String,
    pub seller: String,
    pub buyer: String,
    pub trm_paid: u64,
    pub fetch_url: String,
    pub trade_timestamp_ms: u64,
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

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /v1/tirami/data/offer` — seller publishes an offer.
pub(crate) async fn publish_offer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PublishOfferRequest>,
) -> Result<Json<PublishOfferResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let seller_id = parse_sender(&headers)?;
    let seller_hex = hex::encode(seller_id.0);

    // Validate digest format.
    if req.sha256_digest.len() != 64 || !req.sha256_digest.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "sha256_digest must be 64 hex characters".into(),
        ));
    }
    // Price must be strictly positive (no free offers — they would amplify spam).
    if req.price_trm == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "price_trm must be > 0".into(),
        ));
    }
    // Description and URL length caps to prevent registry bloat.
    if req.description.len() > 512 {
        return Err((
            StatusCode::BAD_REQUEST,
            "description must be ≤ 512 characters".into(),
        ));
    }
    if req.fetch_url.is_empty() || req.fetch_url.len() > 2048 {
        return Err((
            StatusCode::BAD_REQUEST,
            "fetch_url must be 1-2048 characters".into(),
        ));
    }
    // Expiry must be in the future.
    let now = now_millis_pub();
    if req.expiry_ms <= now {
        return Err((
            StatusCode::BAD_REQUEST,
            "expiry_ms must be in the future".into(),
        ));
    }

    let offer_id = DataOffer::compute_id(&seller_hex, &req.sha256_digest, req.price_trm);
    let offer = DataOffer {
        offer_id: offer_id.clone(),
        seller: seller_hex.clone(),
        description: req.description,
        sha256_digest: req.sha256_digest,
        price_trm: req.price_trm,
        expiry_ms: req.expiry_ms,
        published_at_ms: now,
        fetch_url: req.fetch_url,
    };

    let mut reg = state.data_offers.lock().await;
    reg.insert(offer);

    Ok(Json(PublishOfferResponse {
        offer_id,
        seller: seller_hex,
        published_at_ms: now,
    }))
}

/// `GET /v1/tirami/data/offers` — public list, fetch_url stripped.
pub(crate) async fn list_offers(
    State(state): State<AppState>,
) -> Result<Json<ListOffersResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let reg = state.data_offers.lock().await;
    // Filter expired offers out of the response (they remain in the
    // registry until purged by some future GC pass).
    let now = now_millis_pub();
    let offers: Vec<DataOffer> = reg
        .list()
        .into_iter()
        .filter(|o| o.expiry_ms > now)
        .collect();
    Ok(Json(ListOffersResponse {
        count: offers.len(),
        offers,
    }))
}

/// `POST /v1/tirami/data/purchase` — buyer settles TRM, receives fetch URL.
pub(crate) async fn purchase_offer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PurchaseRequest>,
) -> Result<Json<PurchaseResponse>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let buyer_id = parse_sender(&headers)?;
    let buyer_hex = hex::encode(buyer_id.0);

    // Snapshot the offer under the registry lock, then drop the lock
    // before settling the ledger trade so we do not hold two locks
    // at once.
    let offer = {
        let reg = state.data_offers.lock().await;
        reg.get(&req.offer_id).cloned().ok_or((
            StatusCode::NOT_FOUND,
            format!("offer_id not found: {}", req.offer_id),
        ))?
    };

    if offer.seller == buyer_hex {
        return Err((
            StatusCode::BAD_REQUEST,
            "cannot purchase your own offer".into(),
        ));
    }
    let now = now_millis_pub();
    if offer.expiry_ms <= now {
        return Err((StatusCode::GONE, "offer has expired".into()));
    }

    // Settle the TRM trade. Identical TradeRecord shape to inference
    // trades, with discriminating model_id so /v1/tirami/trades is
    // queryable by category prefix.
    let seller_node_id = parse_hex_node_id(&offer.seller).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("stored seller is not valid hex: {e}"),
        )
    })?;
    let short_id: String = offer.offer_id.chars().take(16).collect();
    let model_id = format!("data_offer:{short_id}");
    let trade = TradeRecord {
        provider: seller_node_id,    // seller earns
        consumer: buyer_id.clone(),  // buyer pays
        trm_amount: offer.price_trm,
        tokens_processed: 0,
        timestamp: now,
        model_id,
        flops_estimated: 0,
        nonce: [0u8; 16],
    };
    {
        let mut ledger = state.ledger.lock().await;
        ledger.execute_trade(&trade);
    }

    Ok(Json(PurchaseResponse {
        offer_id: offer.offer_id,
        seller: offer.seller,
        buyer: buyer_hex,
        trm_paid: offer.price_trm,
        fetch_url: offer.fetch_url,
        trade_timestamp_ms: now,
    }))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offer_id_is_deterministic_for_same_inputs() {
        let id1 = DataOffer::compute_id("a", "deadbeef", 100);
        let id2 = DataOffer::compute_id("a", "deadbeef", 100);
        assert_eq!(id1, id2);
    }

    #[test]
    fn offer_id_changes_when_price_changes() {
        let id1 = DataOffer::compute_id("a", "deadbeef", 100);
        let id2 = DataOffer::compute_id("a", "deadbeef", 101);
        assert_ne!(id1, id2);
    }

    #[test]
    fn offer_id_changes_when_seller_changes() {
        let id1 = DataOffer::compute_id("a", "deadbeef", 100);
        let id2 = DataOffer::compute_id("b", "deadbeef", 100);
        assert_ne!(id1, id2);
    }

    #[test]
    fn registry_starts_empty_and_inserts_then_gets() {
        let mut r = DataOfferRegistry::new();
        assert!(r.is_empty());
        let o = DataOffer {
            offer_id: "abc".into(),
            seller: "a".repeat(64),
            description: "hi".into(),
            sha256_digest: "d".repeat(64),
            price_trm: 5,
            expiry_ms: 999_999_999,
            published_at_ms: 0,
            fetch_url: "https://example.com".into(),
        };
        assert!(r.insert(o.clone()));
        assert_eq!(r.len(), 1);
        assert_eq!(r.get("abc").map(|x| x.price_trm), Some(5));
    }

    #[test]
    fn serializing_an_offer_strips_the_fetch_url() {
        let o = DataOffer {
            offer_id: "abc".into(),
            seller: "a".repeat(64),
            description: "hi".into(),
            sha256_digest: "d".repeat(64),
            price_trm: 5,
            expiry_ms: 999_999_999,
            published_at_ms: 0,
            fetch_url: "SECRET_URL".into(),
        };
        let s = serde_json::to_string(&o).expect("ok");
        assert!(
            !s.contains("SECRET_URL"),
            "fetch_url leaked into JSON: {s}"
        );
        assert!(!s.contains("fetch_url"), "fetch_url field leaked: {s}");
    }
}
