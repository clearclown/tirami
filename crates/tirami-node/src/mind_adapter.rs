//! Adapter from forge-mind proposals to forge-ledger CU consumption.
//!
//! forge-mind stays forge-ledger-independent. CU recording is done here
//! in forge-node after improve() returns.

use tirami_core::NodeId;
use tirami_ledger::{ComputeLedger, TradeRecord};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Deterministic NodeId representing a frontier model "provider".
/// Hash of "frontier:<model_id>" so the same model always gets the same NodeId.
pub fn frontier_node_id(model_id: &str) -> NodeId {
    let mut h = Sha256::new();
    h.update(b"frontier:");
    h.update(model_id.as_bytes());
    let digest = h.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&digest);
    NodeId(bytes)
}

/// Record a frontier API call as a TradeRecord in the ledger.
///
/// The frontier model is the "provider" (earns CU), the forge-mind agent
/// (local node) is the "consumer" (spends CU).
pub async fn record_frontier_consumption(
    ledger: &Arc<Mutex<ComputeLedger>>,
    consumer: &NodeId,
    model_id: &str,
    tokens: u64,
    trm_amount: u64,
) {
    if trm_amount == 0 {
        return;
    }
    // Phase 17 Wave 4.5 — self-originated bookkeeping trade for a
    // frontier-API call. See api::record_api_trade for the full
    // rationale on why this stays unsigned. TL;DR: never gossiped,
    // signing provides no defense against a compromised host.
    let trade = TradeRecord {
        provider: frontier_node_id(model_id),
        consumer: consumer.clone(),
        trm_amount,
        tokens_processed: tokens,
        timestamp: crate::api::now_millis_pub(),
        model_id: model_id.to_string(),
        flops_estimated: 0,
        nonce: [0u8; 16],
    };
    let mut l = ledger.lock().await;
    l.execute_trade(&trade);
}
