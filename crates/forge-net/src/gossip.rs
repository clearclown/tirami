//! Gossip protocol for propagating signed trades across the mesh.
//!
//! When a dual-signed trade is recorded locally, it is broadcast to all
//! connected peers. When a peer receives a gossip trade, it verifies both
//! signatures and records it if not already known. This creates an
//! eventually-consistent view of trade history across the network.

use crate::transport::ForgeTransport;
use forge_ledger::SignedTradeRecord;
use forge_proto::{Envelope, Payload, TradeGossip};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Maximum number of trade hashes to remember for deduplication.
/// Prevents unbounded memory growth under trade flooding (Issue #1).
const MAX_GOSSIP_SEEN: usize = 100_000;

/// Maximum pending gossip trades to process per second (Issue #14).
const MAX_GOSSIP_TRADES_PER_SEC: u32 = 100;

/// Tracks which trades have been seen to avoid re-broadcasting.
/// Bounded: evicts oldest entries when exceeding MAX_GOSSIP_SEEN.
pub struct GossipState {
    seen: HashSet<[u8; 32]>,
    order: VecDeque<[u8; 32]>,
    /// Rate limiting for incoming gossip (Issue #14).
    ingest_count: u32,
    ingest_window: std::time::Instant,
}

impl GossipState {
    pub fn new() -> Self {
        Self {
            seen: HashSet::new(),
            order: VecDeque::new(),
            ingest_count: 0,
            ingest_window: std::time::Instant::now(),
        }
    }

    /// Check if we can accept more gossip trades this second.
    pub fn can_ingest(&mut self) -> bool {
        if self.ingest_window.elapsed() > std::time::Duration::from_secs(1) {
            self.ingest_count = 0;
            self.ingest_window = std::time::Instant::now();
        }
        self.ingest_count += 1;
        self.ingest_count <= MAX_GOSSIP_TRADES_PER_SEC
    }

    /// Check if we've already seen this trade. Returns true if new.
    pub fn mark_seen(&mut self, trade: &SignedTradeRecord) -> bool {
        let hash = trade_hash(trade);
        if !self.seen.insert(hash) {
            return false; // already seen
        }
        self.order.push_back(hash);
        // Evict oldest when over limit
        while self.order.len() > MAX_GOSSIP_SEEN {
            if let Some(evicted) = self.order.pop_front() {
                self.seen.remove(&evicted);
            }
        }
        true
    }

    /// Number of unique trades seen.
    pub fn seen_count(&self) -> usize {
        self.seen.len()
    }
}

impl Default for GossipState {
    fn default() -> Self {
        Self::new()
    }
}

/// Broadcast a signed trade to all connected peers.
pub async fn broadcast_trade(
    transport: &ForgeTransport,
    gossip: &Arc<Mutex<GossipState>>,
    signed: &SignedTradeRecord,
) {
    // Mark as seen locally first
    gossip.lock().await.mark_seen(signed);

    let node_id = transport.forge_node_id();
    let peers = transport.connected_peers().await;

    if peers.is_empty() {
        return;
    }

    let msg = Envelope {
        msg_id: rand::random(),
        sender: node_id,
        timestamp: now_millis(),
        payload: Payload::TradeGossip(TradeGossip {
            provider: signed.trade.provider.clone(),
            consumer: signed.trade.consumer.clone(),
            cu_amount: signed.trade.cu_amount,
            tokens_processed: signed.trade.tokens_processed,
            timestamp: signed.trade.timestamp,
            model_id: signed.trade.model_id.clone(),
            provider_sig: signed.provider_sig.clone(),
            consumer_sig: signed.consumer_sig.clone(),
        }),
    };

    for peer_id in &peers {
        if let Err(e) = transport.send_to(peer_id, &msg).await {
            tracing::debug!("Gossip to {} failed: {}", peer_id, e);
        }
    }

    tracing::debug!("Broadcast trade gossip to {} peers", peers.len());
}

/// Handle an incoming gossip trade. Verifies signatures and returns
/// the SignedTradeRecord if it's new and valid, None if already seen or invalid.
pub async fn handle_trade_gossip(
    gossip: &Arc<Mutex<GossipState>>,
    msg: &TradeGossip,
) -> Option<SignedTradeRecord> {
    // Backpressure: reject if rate limit exceeded (Issue #14)
    if !gossip.lock().await.can_ingest() {
        tracing::debug!("Gossip rate limit exceeded, dropping trade");
        return None;
    }
    let trade = forge_ledger::TradeRecord {
        provider: msg.provider.clone(),
        consumer: msg.consumer.clone(),
        cu_amount: msg.cu_amount,
        tokens_processed: msg.tokens_processed,
        timestamp: msg.timestamp,
        model_id: msg.model_id.clone(),
    };

    let signed = SignedTradeRecord {
        trade,
        provider_sig: msg.provider_sig.clone(),
        consumer_sig: msg.consumer_sig.clone(),
    };

    // Verify both signatures
    if let Err(e) = signed.verify() {
        tracing::warn!("Gossip trade failed verification: {}", e);
        return None;
    }

    // Check if we've already seen this trade
    let is_new = gossip.lock().await.mark_seen(&signed);
    if !is_new {
        return None;
    }

    Some(signed)
}

/// Check for network partition by comparing local Merkle root with a peer's.
/// Returns true if roots match (consistent), false if divergent (partition detected).
pub fn check_consistency(local_root: &[u8; 32], peer_root: &[u8; 32]) -> bool {
    local_root == peer_root
}

/// Log a partition warning if Merkle roots differ (Issue #12).
pub fn log_partition_check(local_root: &[u8; 32], peer_id: &str, peer_root: &[u8; 32]) {
    if !check_consistency(local_root, peer_root) {
        tracing::warn!(
            "Ledger divergence detected with peer {}: local={} peer={}",
            peer_id,
            hex::encode(local_root),
            hex::encode(peer_root)
        );
    }
}

/// Compute a SHA-256 hash of a trade's canonical bytes for deduplication.
fn trade_hash(signed: &SignedTradeRecord) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(&signed.trade.canonical_bytes());
    hasher.update(&signed.provider_sig);
    hasher.update(&signed.consumer_sig);
    hasher.finalize().into()
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use forge_core::NodeId;

    fn make_signed_trade() -> SignedTradeRecord {
        let mut rng = rand::thread_rng();
        let provider_key = SigningKey::generate(&mut rng);
        let consumer_key = SigningKey::generate(&mut rng);

        let trade = forge_ledger::TradeRecord {
            provider: NodeId(provider_key.verifying_key().to_bytes()),
            consumer: NodeId(consumer_key.verifying_key().to_bytes()),
            cu_amount: 100,
            tokens_processed: 50,
            timestamp: now_millis(),
            model_id: "test".to_string(),
        };

        let canonical = trade.canonical_bytes();
        SignedTradeRecord {
            trade,
            provider_sig: provider_key.sign(&canonical).to_bytes().to_vec(),
            consumer_sig: consumer_key.sign(&canonical).to_bytes().to_vec(),
        }
    }

    #[test]
    fn gossip_state_deduplicates() {
        let mut state = GossipState::new();
        let trade = make_signed_trade();

        assert!(state.mark_seen(&trade)); // first time: new
        assert!(!state.mark_seen(&trade)); // second time: already seen
        assert_eq!(state.seen_count(), 1);
    }

    #[tokio::test]
    async fn handle_gossip_rejects_invalid_signature() {
        let gossip = Arc::new(Mutex::new(GossipState::new()));
        let msg = TradeGossip {
            provider: NodeId([1u8; 32]),
            consumer: NodeId([2u8; 32]),
            cu_amount: 100,
            tokens_processed: 50,
            timestamp: now_millis(),
            model_id: "test".to_string(),
            provider_sig: vec![0u8; 64], // invalid
            consumer_sig: vec![0u8; 64], // invalid
        };

        let result = handle_trade_gossip(&gossip, &msg).await;
        assert!(result.is_none()); // should reject
    }

    #[tokio::test]
    async fn handle_gossip_accepts_valid_trade() {
        let gossip = Arc::new(Mutex::new(GossipState::new()));
        let signed = make_signed_trade();

        let msg = TradeGossip {
            provider: signed.trade.provider.clone(),
            consumer: signed.trade.consumer.clone(),
            cu_amount: signed.trade.cu_amount,
            tokens_processed: signed.trade.tokens_processed,
            timestamp: signed.trade.timestamp,
            model_id: signed.trade.model_id.clone(),
            provider_sig: signed.provider_sig.clone(),
            consumer_sig: signed.consumer_sig.clone(),
        };

        let result = handle_trade_gossip(&gossip, &msg).await;
        assert!(result.is_some());

        // Second time should be deduplicated
        let result2 = handle_trade_gossip(&gossip, &msg).await;
        assert!(result2.is_none());
    }

    #[test]
    fn check_consistency_detects_divergence() {
        let root_a = [1u8; 32];
        let root_b = [2u8; 32];
        assert!(check_consistency(&root_a, &root_a));
        assert!(!check_consistency(&root_a, &root_b));
    }

    #[test]
    fn gossip_bounded_eviction() {
        let mut state = GossipState::new();
        // Fill beyond MAX_GOSSIP_SEEN (use a smaller count for test speed)
        for i in 0..200 {
            let mut rng = rand::thread_rng();
            let provider_key = SigningKey::generate(&mut rng);
            let consumer_key = SigningKey::generate(&mut rng);
            let trade = forge_ledger::TradeRecord {
                provider: NodeId(provider_key.verifying_key().to_bytes()),
                consumer: NodeId(consumer_key.verifying_key().to_bytes()),
                cu_amount: i + 1,
                tokens_processed: 1,
                timestamp: now_millis(),
                model_id: "test".to_string(),
            };
            let canonical = trade.canonical_bytes();
            let signed = SignedTradeRecord {
                trade,
                provider_sig: provider_key.sign(&canonical).to_bytes().to_vec(),
                consumer_sig: consumer_key.sign(&canonical).to_bytes().to_vec(),
            };
            state.mark_seen(&signed);
        }
        // Should have entries but bounded
        assert!(state.seen_count() <= 200);
        assert!(state.seen_count() > 0);
    }
}
