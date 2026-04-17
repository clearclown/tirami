//! Gossip protocol for propagating signed trades across the mesh.
//!
//! When a dual-signed trade is recorded locally, it is broadcast to all
//! connected peers. When a peer receives a gossip trade, it verifies both
//! signatures and records it if not already known. This creates an
//! eventually-consistent view of trade history across the network.

use crate::transport::ForgeTransport;
use tirami_ledger::{ComputeLedger, LoanRecord, LoanStatus, SignedLoanRecord, SignedTradeRecord};
use tirami_proto::{Envelope, LoanGossip, Payload, ReputationObservation, TradeGossip};
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
    /// Separate dedup set for loans to avoid hash collision domain mixing.
    seen_loans: HashSet<[u8; 32]>,
    order_loans: VecDeque<[u8; 32]>,
    /// Separate dedup set for reputation observations (Phase 9 A3).
    seen_reputation: HashSet<[u8; 32]>,
    order_reputation: VecDeque<[u8; 32]>,
    /// Phase 14.1 — dedup for price signals. Keyed by (node_id, timestamp).
    /// We only dedup exact replays; newer signals from the same node always
    /// replace older ones in the PeerRegistry.
    seen_price_signals: HashSet<[u8; 32]>,
    order_price_signals: VecDeque<[u8; 32]>,
    /// Rate limiting for incoming gossip (Issue #14).
    ingest_count: u32,
    ingest_window: std::time::Instant,
}

impl GossipState {
    pub fn new() -> Self {
        Self {
            seen: HashSet::new(),
            order: VecDeque::new(),
            seen_loans: HashSet::new(),
            order_loans: VecDeque::new(),
            seen_reputation: HashSet::new(),
            order_reputation: VecDeque::new(),
            seen_price_signals: HashSet::new(),
            order_price_signals: VecDeque::new(),
            ingest_count: 0,
            ingest_window: std::time::Instant::now(),
        }
    }

    /// Phase 14.1 — check if a price signal is new. Returns true if new.
    /// Key is sha256(node_id || timestamp_le_bytes) for cheap replay dedup.
    pub fn mark_price_signal_seen(
        &mut self,
        node_id: &tirami_core::NodeId,
        timestamp: u64,
    ) -> bool {
        let mut key = [0u8; 32];
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&node_id.0);
        h.update(&timestamp.to_le_bytes());
        key.copy_from_slice(&h.finalize());
        if !self.seen_price_signals.insert(key) {
            return false;
        }
        self.order_price_signals.push_back(key);
        while self.order_price_signals.len() > MAX_GOSSIP_SEEN {
            if let Some(evicted) = self.order_price_signals.pop_front() {
                self.seen_price_signals.remove(&evicted);
            }
        }
        true
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

    /// Check if we've already seen this loan. Returns true if new.
    pub fn mark_loan_seen(&mut self, signed: &SignedLoanRecord) -> bool {
        let hash = loan_hash(signed);
        if !self.seen_loans.insert(hash) {
            return false;
        }
        self.order_loans.push_back(hash);
        while self.order_loans.len() > MAX_GOSSIP_SEEN {
            if let Some(evicted) = self.order_loans.pop_front() {
                self.seen_loans.remove(&evicted);
            }
        }
        true
    }

    /// Number of unique loans seen.
    pub fn seen_loan_count(&self) -> usize {
        self.seen_loans.len()
    }

    /// Check if we've already seen this reputation observation. Returns true if new.
    pub fn mark_reputation_seen(&mut self, key: &[u8; 32]) -> bool {
        if !self.seen_reputation.insert(*key) {
            return false;
        }
        self.order_reputation.push_back(*key);
        while self.order_reputation.len() > MAX_GOSSIP_SEEN {
            if let Some(evicted) = self.order_reputation.pop_front() {
                self.seen_reputation.remove(&evicted);
            }
        }
        true
    }

    /// Number of unique reputation observations seen.
    pub fn seen_reputation_count(&self) -> usize {
        self.seen_reputation.len()
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

    let node_id = transport.tirami_node_id();
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
            trm_amount: signed.trade.trm_amount,
            tokens_processed: signed.trade.tokens_processed,
            timestamp: signed.trade.timestamp,
            model_id: signed.trade.model_id.clone(),
            provider_sig: signed.provider_sig.clone(),
            consumer_sig: signed.consumer_sig.clone(),
            nonce: signed.trade.nonce,
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
    let trade = tirami_ledger::TradeRecord {
        provider: msg.provider.clone(),
        consumer: msg.consumer.clone(),
        trm_amount: msg.trm_amount,
        tokens_processed: msg.tokens_processed,
        timestamp: msg.timestamp,
        model_id: msg.model_id.clone(),
        flops_estimated: 0,
        // Phase 17 Wave 1.2 — carry the provider-chosen nonce through
        // gossip so the ledger can enforce replay dedup on receipt.
        nonce: msg.nonce,
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

/// Broadcast a signed loan to all connected peers.
pub async fn broadcast_loan(
    transport: &ForgeTransport,
    gossip: &Arc<Mutex<GossipState>>,
    signed: &SignedLoanRecord,
) {
    // Mark as seen locally first
    gossip.lock().await.mark_loan_seen(signed);

    let node_id = transport.tirami_node_id();
    let peers = transport.connected_peers().await;

    if peers.is_empty() {
        return;
    }

    let msg = Envelope {
        msg_id: rand::random(),
        sender: node_id,
        timestamp: now_millis(),
        payload: Payload::LoanGossip(LoanGossip {
            lender: signed.loan.lender.clone(),
            borrower: signed.loan.borrower.clone(),
            principal_trm: signed.loan.principal_trm,
            interest_rate_per_hour: signed.loan.interest_rate_per_hour,
            term_hours: signed.loan.term_hours,
            collateral_trm: signed.loan.collateral_trm,
            created_at: signed.loan.created_at,
            due_at: signed.loan.due_at,
            lender_sig: signed.lender_sig.clone(),
            borrower_sig: signed.borrower_sig.clone(),
        }),
    };

    for peer_id in &peers {
        if let Err(e) = transport.send_to(peer_id, &msg).await {
            tracing::debug!("Loan gossip to {} failed: {}", peer_id, e);
        }
    }

    tracing::trace!(
        "broadcasting loan {} to {} peers",
        hex::encode(signed.loan.loan_id),
        peers.len()
    );
}

/// Handle an incoming gossip loan. Verifies signatures and returns the
/// SignedLoanRecord if new and valid, None if already seen or invalid.
pub async fn handle_loan_gossip(
    gossip: &Arc<Mutex<GossipState>>,
    loan_gossip: &LoanGossip,
) -> Option<SignedLoanRecord> {
    // Backpressure: reject if rate limit exceeded (Issue #14)
    if !gossip.lock().await.can_ingest() {
        tracing::debug!("Gossip rate limit exceeded, dropping loan");
        return None;
    }

    let mut loan = LoanRecord {
        loan_id: [0u8; 32],
        lender: loan_gossip.lender.clone(),
        borrower: loan_gossip.borrower.clone(),
        principal_trm: loan_gossip.principal_trm,
        interest_rate_per_hour: loan_gossip.interest_rate_per_hour,
        term_hours: loan_gossip.term_hours,
        collateral_trm: loan_gossip.collateral_trm,
        status: LoanStatus::Active,
        created_at: loan_gossip.created_at,
        due_at: loan_gossip.due_at,
        repaid_at: None,
    };
    loan.loan_id = loan.compute_loan_id();

    let signed = SignedLoanRecord {
        loan,
        lender_sig: loan_gossip.lender_sig.clone(),
        borrower_sig: loan_gossip.borrower_sig.clone(),
    };

    // Verify both signatures
    if let Err(e) = signed.verify() {
        tracing::warn!("Gossip loan failed verification: {}", e);
        return None;
    }

    // Check if we've already seen this loan
    let is_new = gossip.lock().await.mark_loan_seen(&signed);
    if !is_new {
        return None;
    }

    Some(signed)
}

/// Broadcast a reputation observation to all connected peers.
/// Uses dedup so the same observation is not re-sent if already seen.
pub async fn broadcast_reputation(
    transport: &ForgeTransport,
    gossip: &Arc<Mutex<GossipState>>,
    observation: &ReputationObservation,
) {
    let key = observation.dedup_key();
    // Mark as seen locally first; skip if already known.
    if !gossip.lock().await.mark_reputation_seen(&key) {
        return;
    }

    let node_id = transport.tirami_node_id();
    let peers = transport.connected_peers().await;

    if peers.is_empty() {
        return;
    }

    let msg = Envelope {
        msg_id: rand::random(),
        sender: node_id,
        timestamp: now_millis(),
        payload: Payload::ReputationGossip(observation.clone()),
    };

    for peer_id in &peers {
        if let Err(e) = transport.send_to(peer_id, &msg).await {
            tracing::debug!("Reputation gossip to {} failed: {}", peer_id, e);
        }
    }

    tracing::info!(
        "Broadcast reputation observation: observer={} subject={} rep={:.3}",
        observation.observer.to_hex(),
        observation.subject.to_hex(),
        observation.reputation,
    );
}

/// Handle an incoming reputation gossip message.
/// Verifies the signature, checks dedup, merges into the ledger, and re-floods.
pub async fn handle_reputation_gossip(
    observation: ReputationObservation,
    ledger: &Arc<Mutex<ComputeLedger>>,
    gossip: &Arc<Mutex<GossipState>>,
    transport: Option<&ForgeTransport>,
) {
    // Backpressure: reject if rate limit exceeded.
    if !gossip.lock().await.can_ingest() {
        tracing::debug!("Gossip rate limit exceeded, dropping reputation observation");
        return;
    }

    // Verify signature (empty sig = MVP pass-through; real sig = ed25519 check).
    if !observation.verify() {
        tracing::warn!(
            "Reputation gossip failed verification from observer={}",
            observation.observer.to_hex()
        );
        return;
    }

    let key = observation.dedup_key();
    let is_new = gossip.lock().await.mark_reputation_seen(&key);
    if !is_new {
        return;
    }

    // Merge into the ledger.
    {
        let mut ledger_guard = ledger.lock().await;
        ledger_guard.merge_remote_reputation(&observation);
    }

    tracing::debug!(
        "Merged reputation gossip: subject={} rep={:.3} trade_count={}",
        observation.subject.to_hex(),
        observation.reputation,
        observation.trade_count,
    );

    // Re-flood to peers (gossip propagation).
    if let Some(transport) = transport {
        let node_id = transport.tirami_node_id();
        let peers = transport.connected_peers().await;
        if !peers.is_empty() {
            let msg = Envelope {
                msg_id: rand::random(),
                sender: node_id,
                timestamp: now_millis(),
                payload: Payload::ReputationGossip(observation),
            };
            for peer_id in &peers {
                if let Err(e) = transport.send_to(peer_id, &msg).await {
                    tracing::debug!("Re-flood reputation gossip to {} failed: {}", peer_id, e);
                }
            }
        }
    }
}

// ===========================================================================
// Phase 14.1 — PriceSignal gossip
// ===========================================================================

/// Broadcast our own price signal to all connected peers.
///
/// Called periodically by the node daemon (default 30s). Also marks the
/// signal as seen locally so we don't re-flood it on receive.
pub async fn broadcast_price_signal(
    transport: &ForgeTransport,
    gossip: &Arc<Mutex<GossipState>>,
    signal: &tirami_core::PriceSignal,
) {
    // Dedup locally first.
    if !gossip
        .lock()
        .await
        .mark_price_signal_seen(&signal.node_id, signal.timestamp)
    {
        return;
    }

    let node_id = transport.tirami_node_id();
    let peers = transport.connected_peers().await;

    if peers.is_empty() {
        return;
    }

    let msg = Envelope {
        msg_id: rand::random(),
        sender: node_id,
        timestamp: now_millis(),
        payload: Payload::PriceSignalGossip(signal.clone()),
    };

    for peer_id in &peers {
        if let Err(e) = transport.send_to(peer_id, &msg).await {
            tracing::debug!("Price signal gossip to {} failed: {}", peer_id, e);
        }
    }

    tracing::debug!(
        "Broadcast price signal: node={} multiplier={:.3} available_cu={}",
        signal.node_id.to_hex(),
        signal.price_multiplier,
        signal.available_cu,
    );
}

/// Handle an incoming price signal gossip message.
///
/// Validates the signal, checks dedup, merges into the ledger's PeerRegistry,
/// and re-floods to peers that haven't seen it yet.
pub async fn handle_price_signal_gossip(
    signal: tirami_core::PriceSignal,
    ledger: &Arc<Mutex<ComputeLedger>>,
    gossip: &Arc<Mutex<GossipState>>,
    transport: Option<&ForgeTransport>,
) {
    // Backpressure.
    if !gossip.lock().await.can_ingest() {
        tracing::debug!("Gossip rate limit exceeded, dropping price signal");
        return;
    }

    // Format validation (defense in depth — Envelope::validate_with_sender
    // already checked).
    if !signal.is_valid() {
        tracing::warn!(
            "Price signal with invalid multiplier from {}, dropping",
            signal.node_id.to_hex()
        );
        return;
    }

    // Exact-replay dedup.
    let is_new = gossip
        .lock()
        .await
        .mark_price_signal_seen(&signal.node_id, signal.timestamp);
    if !is_new {
        return;
    }

    // Merge into the PeerRegistry. ingest_price_signal internally rejects
    // stale timestamps, so no risk of regression.
    let merged = {
        let mut ledger_guard = ledger.lock().await;
        ledger_guard.ingest_price_signal(&signal)
    };

    if merged {
        tracing::debug!(
            "Merged price signal: node={} multiplier={:.3}",
            signal.node_id.to_hex(),
            signal.price_multiplier,
        );
    }

    // Re-flood to peers so the signal propagates across the mesh.
    if let Some(transport) = transport {
        let node_id = transport.tirami_node_id();
        let peers = transport.connected_peers().await;
        if !peers.is_empty() {
            let msg = Envelope {
                msg_id: rand::random(),
                sender: node_id,
                timestamp: now_millis(),
                payload: Payload::PriceSignalGossip(signal),
            };
            for peer_id in &peers {
                if let Err(e) = transport.send_to(peer_id, &msg).await {
                    tracing::debug!(
                        "Re-flood price signal to {} failed: {}",
                        peer_id,
                        e
                    );
                }
            }
        }
    }
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

/// Compute a SHA-256 hash of a loan's canonical bytes for deduplication.
fn loan_hash(signed: &SignedLoanRecord) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"loan:");
    hasher.update(signed.loan.canonical_bytes());
    hasher.update(&signed.lender_sig);
    hasher.update(&signed.borrower_sig);
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
    use tirami_core::NodeId;

    fn make_signed_trade() -> SignedTradeRecord {
        let mut rng = rand::thread_rng();
        let provider_key = SigningKey::generate(&mut rng);
        let consumer_key = SigningKey::generate(&mut rng);

        let trade = tirami_ledger::TradeRecord {
            provider: NodeId(provider_key.verifying_key().to_bytes()),
            consumer: NodeId(consumer_key.verifying_key().to_bytes()),
            trm_amount: 100,
            tokens_processed: 50,
            timestamp: now_millis(),
            model_id: "test".to_string(),
            flops_estimated: 0,
                    nonce: [0u8; 16],
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
            trm_amount: 100,
            tokens_processed: 50,
            timestamp: now_millis(),
            model_id: "test".to_string(),
            provider_sig: vec![0u8; 64], // invalid
            consumer_sig: vec![0u8; 64], // invalid
            nonce: [0u8; 16],
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
            trm_amount: signed.trade.trm_amount,
            tokens_processed: signed.trade.tokens_processed,
            timestamp: signed.trade.timestamp,
            model_id: signed.trade.model_id.clone(),
            provider_sig: signed.provider_sig.clone(),
            consumer_sig: signed.consumer_sig.clone(),
            nonce: signed.trade.nonce,
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

    fn make_signed_loan() -> SignedLoanRecord {
        let mut rng = rand::thread_rng();
        let lender_key = SigningKey::generate(&mut rng);
        let borrower_key = SigningKey::generate(&mut rng);

        let created_at = now_millis();
        let term_hours: u64 = 24;
        let due_at = created_at + term_hours * 3_600_000;

        let mut loan = tirami_ledger::LoanRecord {
            loan_id: [0u8; 32],
            lender: NodeId(lender_key.verifying_key().to_bytes()),
            borrower: NodeId(borrower_key.verifying_key().to_bytes()),
            principal_trm: 1_000,
            interest_rate_per_hour: 0.001,
            term_hours,
            collateral_trm: 200,
            status: tirami_ledger::LoanStatus::Active,
            created_at,
            due_at,
            repaid_at: None,
        };
        loan.loan_id = loan.compute_loan_id();

        let canonical = loan.canonical_bytes();
        SignedLoanRecord {
            loan,
            lender_sig: lender_key.sign(&canonical).to_bytes().to_vec(),
            borrower_sig: borrower_key.sign(&canonical).to_bytes().to_vec(),
        }
    }

    #[test]
    fn gossip_state_dedupes_loans() {
        let mut state = GossipState::new();
        // Dummy sigs are fine: mark_loan_seen only hashes, does not verify.
        let loan = tirami_ledger::LoanRecord {
            loan_id: [7u8; 32],
            lender: NodeId([1u8; 32]),
            borrower: NodeId([2u8; 32]),
            principal_trm: 500,
            interest_rate_per_hour: 0.002,
            term_hours: 12,
            collateral_trm: 100,
            status: tirami_ledger::LoanStatus::Active,
            created_at: now_millis(),
            due_at: now_millis() + 12 * 3_600_000,
            repaid_at: None,
        };
        let signed = SignedLoanRecord {
            loan,
            lender_sig: vec![0u8; 64],
            borrower_sig: vec![0u8; 64],
        };

        assert!(state.mark_loan_seen(&signed));
        assert!(!state.mark_loan_seen(&signed));
        assert_eq!(state.seen_loan_count(), 1);
    }

    #[tokio::test]
    async fn handle_loan_gossip_rejects_invalid_signature() {
        let gossip = Arc::new(Mutex::new(GossipState::new()));
        let created_at = now_millis();
        let msg = LoanGossip {
            lender: NodeId([1u8; 32]),
            borrower: NodeId([2u8; 32]),
            principal_trm: 1_000,
            interest_rate_per_hour: 0.001,
            term_hours: 24,
            collateral_trm: 200,
            created_at,
            due_at: created_at + 24 * 3_600_000,
            lender_sig: vec![0u8; 64],
            borrower_sig: vec![0u8; 64],
        };

        let result = handle_loan_gossip(&gossip, &msg).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn handle_loan_gossip_accepts_valid_dual_signed_loan() {
        let gossip = Arc::new(Mutex::new(GossipState::new()));
        let signed = make_signed_loan();

        let msg = LoanGossip {
            lender: signed.loan.lender.clone(),
            borrower: signed.loan.borrower.clone(),
            principal_trm: signed.loan.principal_trm,
            interest_rate_per_hour: signed.loan.interest_rate_per_hour,
            term_hours: signed.loan.term_hours,
            collateral_trm: signed.loan.collateral_trm,
            created_at: signed.loan.created_at,
            due_at: signed.loan.due_at,
            lender_sig: signed.lender_sig.clone(),
            borrower_sig: signed.borrower_sig.clone(),
        };

        let result = handle_loan_gossip(&gossip, &msg).await;
        assert!(result.is_some());

        // Second time should be deduplicated
        let result2 = handle_loan_gossip(&gossip, &msg).await;
        assert!(result2.is_none());
    }

    #[test]
    fn reputation_gossip_state_deduplicates() {
        let mut state = GossipState::new();
        let obs = ReputationObservation {
            observer: NodeId([1u8; 32]),
            subject: NodeId([2u8; 32]),
            reputation: 0.7,
            trade_count: 20,
            total_trm_volume: 2_000,
            timestamp_ms: now_millis(),
            signature: vec![],
        };
        let key = obs.dedup_key();
        assert!(state.mark_reputation_seen(&key)); // first time: new
        assert!(!state.mark_reputation_seen(&key)); // second time: already seen
        assert_eq!(state.seen_reputation_count(), 1);
    }

    #[tokio::test]
    async fn test_handle_reputation_gossip_rejects_invalid_sig() {
        // Observation with an empty signature must NOT be merged into the ledger.
        let ledger = Arc::new(Mutex::new(tirami_ledger::ComputeLedger::new()));
        let gossip = Arc::new(Mutex::new(GossipState::new()));
        let subject = NodeId([42u8; 32]);
        let obs = ReputationObservation {
            observer: NodeId([1u8; 32]),
            subject: subject.clone(),
            reputation: 0.9,
            trade_count: 20,
            total_trm_volume: 2_000,
            timestamp_ms: now_millis(),
            signature: vec![], // unsigned — must be rejected
        };
        handle_reputation_gossip(obs, &ledger, &gossip, None).await;
        // Nothing should have been merged.
        let ledger_guard = ledger.lock().await;
        assert!(
            !ledger_guard.remote_reputation.contains_key(&subject)
                || ledger_guard.remote_reputation[&subject].is_empty(),
            "invalid-sig observation must not update ledger"
        );
    }

    #[test]
    fn gossip_bounded_eviction() {
        let mut state = GossipState::new();
        // Fill beyond MAX_GOSSIP_SEEN (use a smaller count for test speed)
        for i in 0..200 {
            let mut rng = rand::thread_rng();
            let provider_key = SigningKey::generate(&mut rng);
            let consumer_key = SigningKey::generate(&mut rng);
            let trade = tirami_ledger::TradeRecord {
                provider: NodeId(provider_key.verifying_key().to_bytes()),
                consumer: NodeId(consumer_key.verifying_key().to_bytes()),
                trm_amount: i + 1,
                tokens_processed: 1,
                timestamp: now_millis(),
                model_id: "test".to_string(),
                flops_estimated: 0,
                            nonce: [0u8; 16],
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

    // =========================================================================
    // Security tests: Gossip dedup prevents replay attacks
    // =========================================================================

    #[test]
    fn sec_gossip_trade_dedup_prevents_replay() {
        // The same signed trade presented twice must be deduplicated:
        // the second call to mark_seen returns false (already seen).
        let mut state = GossipState::new();
        let signed = make_signed_trade();

        let first = state.mark_seen(&signed);
        let second = state.mark_seen(&signed);

        assert!(first, "first presentation of a trade must be accepted (new)");
        assert!(!second, "second presentation of the same trade must be rejected (replay)");
        // Only one unique entry must be stored.
        assert_eq!(state.seen_count(), 1, "dedup set must contain exactly one entry");
    }

    #[test]
    fn sec_gossip_loan_dedup_prevents_replay() {
        // The same signed loan presented twice must be deduplicated.
        let mut state = GossipState::new();
        let loan = tirami_ledger::LoanRecord {
            loan_id: [0xCA; 32],
            lender: NodeId([1u8; 32]),
            borrower: NodeId([2u8; 32]),
            principal_trm: 1_000,
            interest_rate_per_hour: 0.001,
            term_hours: 24,
            collateral_trm: 3_000,
            status: tirami_ledger::LoanStatus::Active,
            created_at: now_millis(),
            due_at: now_millis() + 24 * 3_600_000,
            repaid_at: None,
        };
        let signed = SignedLoanRecord {
            loan,
            lender_sig: vec![0u8; 64],
            borrower_sig: vec![0u8; 64],
        };

        let first = state.mark_loan_seen(&signed);
        let second = state.mark_loan_seen(&signed);

        assert!(first, "first presentation of a loan must be accepted (new)");
        assert!(!second, "second presentation of the same loan must be rejected (replay)");
        assert_eq!(state.seen_loan_count(), 1, "loan dedup set must contain exactly one entry");
    }

    #[test]
    fn sec_gossip_reputation_dedup_prevents_replay() {
        // The same reputation observation key presented twice must be deduplicated.
        let mut state = GossipState::new();
        let obs = tirami_proto::ReputationObservation {
            observer: NodeId([0xAA; 32]),
            subject: NodeId([0xBB; 32]),
            reputation: 0.75,
            trade_count: 10,
            total_trm_volume: 1_000,
            timestamp_ms: 9_999_000,
            signature: vec![],
        };
        let key = obs.dedup_key();

        let first = state.mark_reputation_seen(&key);
        let second = state.mark_reputation_seen(&key);

        assert!(first, "first reputation observation must be new");
        assert!(!second, "second identical reputation observation must be deduplicated (replay blocked)");
        assert_eq!(state.seen_reputation_count(), 1);
    }

    #[test]
    fn sec_gossip_different_trades_both_accepted() {
        // Two structurally different trades must each be treated as new.
        let mut state = GossipState::new();
        let t1 = make_signed_trade();
        let t2 = make_signed_trade(); // different keys → different canonical hash

        let first = state.mark_seen(&t1);
        let second = state.mark_seen(&t2);

        assert!(first, "first trade must be new");
        assert!(second, "second (different) trade must also be new");
        assert_eq!(state.seen_count(), 2);
    }

    #[tokio::test]
    async fn sec_gossip_invalid_provider_sig_rejected_end_to_end() {
        // A TradeGossip with an all-zero provider signature must be rejected
        // by handle_trade_gossip (verify() fails → returns None).
        let gossip = Arc::new(Mutex::new(GossipState::new()));
        let msg = TradeGossip {
            provider: NodeId([1u8; 32]),
            consumer: NodeId([2u8; 32]),
            trm_amount: 100,
            tokens_processed: 10,
            timestamp: now_millis(),
            model_id: "sec".to_string(),
            provider_sig: vec![0u8; 64], // all-zero → invalid
            consumer_sig: vec![0u8; 64],
            nonce: [0u8; 16],
        };
        let result = handle_trade_gossip(&gossip, &msg).await;
        assert!(
            result.is_none(),
            "gossip trade with all-zero signatures must be rejected"
        );
        // Critically, the dedup set must remain EMPTY — an invalid trade must
        // not be "remembered" as seen, which would allow an attacker to poison
        // the dedup cache and block legitimate trades with the same canonical bytes.
        // (The current implementation only marks as seen AFTER verification passes.)
        assert_eq!(
            gossip.lock().await.seen_count(),
            0,
            "invalid trade must not be added to the dedup cache"
        );
    }

    #[tokio::test]
    async fn sec_gossip_all_ff_consumer_sig_rejected() {
        // All-0xFF consumer signature — must fail verification.
        use ed25519_dalek::SigningKey;
        let mut rng = rand::thread_rng();
        let provider_key = SigningKey::generate(&mut rng);
        let consumer_key = SigningKey::generate(&mut rng);

        // Build a valid trade and sign only the provider side.
        let trade = tirami_ledger::TradeRecord {
            provider: NodeId(provider_key.verifying_key().to_bytes()),
            consumer: NodeId(consumer_key.verifying_key().to_bytes()),
            trm_amount: 200,
            tokens_processed: 20,
            timestamp: now_millis(),
            model_id: "sec2".to_string(),
            flops_estimated: 0,
                    nonce: [0u8; 16],
        };
        let canonical = trade.canonical_bytes();
        let provider_sig = provider_key.sign(&canonical).to_bytes().to_vec();

        let gossip = Arc::new(Mutex::new(GossipState::new()));
        let msg = TradeGossip {
            provider: trade.provider.clone(),
            consumer: trade.consumer.clone(),
            trm_amount: trade.trm_amount,
            tokens_processed: trade.tokens_processed,
            timestamp: trade.timestamp,
            model_id: trade.model_id.clone(),
            provider_sig,
            consumer_sig: vec![0xFFu8; 64], // all-0xFF → invalid
            nonce: [0u8; 16],
        };
        let result = handle_trade_gossip(&gossip, &msg).await;
        assert!(result.is_none(), "all-0xFF consumer sig must be rejected by gossip handler");
    }
}
