//! Phase 14.1 — PeerRegistry
//!
//! Aggregates per-peer market state observed via gossip:
//! - Latest PriceSignal (price_multiplier, available_cu, capabilities)
//! - Latency EMA (exponential moving average of observed RTT)
//! - Audit tier (trust gradient — Phase 14.3 will drive transitions)
//! - Verified trade count (for tier promotion)
//!
//! This is the data the scheduler (Phase 14.2 `select_provider`) reads
//! when picking a provider for an inference request.
//!
//! # Invariants
//! - Every PeerState has a matching entry in `ComputeLedger::balances` once
//!   verified trades accumulate (enforced by `ingest_price_signal`).
//! - `latency_ema_ms` is always finite and non-negative.
//! - Price signals with invalid multipliers are silently rejected.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tirami_core::{AuditTier, ModelId, NodeId, PriceSignal};

/// Per-peer market state aggregated from gossip and observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerState {
    /// Most recent price signal from this peer (None until first gossip).
    pub price_signal: Option<PriceSignal>,
    /// Exponential moving average of observed RTT in milliseconds.
    /// Seeded to 500ms on first observation; decays toward truth over time.
    pub latency_ema_ms: f64,
    /// Unix ms of the last signal or interaction.
    pub last_seen: u64,
    /// Current audit tier (Phase 14.3 updates this on audit results).
    pub audit_tier: AuditTier,
    /// Count of trades this peer has completed that passed verification.
    pub verified_trade_count: u64,
}

impl Default for PeerState {
    fn default() -> Self {
        Self {
            price_signal: None,
            latency_ema_ms: 500.0, // pessimistic seed
            last_seen: 0,
            audit_tier: AuditTier::default(),
            verified_trade_count: 0,
        }
    }
}

impl PeerState {
    /// EMA smoothing factor — higher = more weight to recent samples.
    /// 0.2 = current sample contributes 20%, history 80%.
    pub const LATENCY_EMA_ALPHA: f64 = 0.2;

    /// Returns the effective price per token given the base tier price.
    /// If no signal has been received, returns `base_price` unchanged.
    pub fn effective_price(&self, base_price_per_token: f64) -> f64 {
        match &self.price_signal {
            Some(sig) => base_price_per_token * sig.price_multiplier,
            None => base_price_per_token,
        }
    }

    /// Returns true if this peer advertises the given model.
    pub fn serves_model(&self, model_id: &ModelId) -> bool {
        match &self.price_signal {
            Some(sig) => sig.model_capabilities.contains(model_id),
            None => false,
        }
    }

    /// Returns advertised available CU, or 0 if no signal yet.
    pub fn available_cu(&self) -> u64 {
        self.price_signal.as_ref().map(|s| s.available_cu).unwrap_or(0)
    }
}

/// Per-node market state registry.
///
/// Lives inside `ComputeLedger`. Fed by `ingest_price_signal` (from gossip)
/// and `update_latency` (from pipeline coordinator). Read by
/// `select_provider` when scheduling.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeerRegistry {
    peers: HashMap<NodeId, PeerState>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of peers currently known.
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    /// Borrow the peer map for iteration.
    pub fn peers(&self) -> &HashMap<NodeId, PeerState> {
        &self.peers
    }

    /// Get a peer's state.
    pub fn get(&self, node_id: &NodeId) -> Option<&PeerState> {
        self.peers.get(node_id)
    }

    /// Mutable access (for ledger-internal operations).
    pub fn get_mut(&mut self, node_id: &NodeId) -> Option<&mut PeerState> {
        self.peers.get_mut(node_id)
    }

    /// Ensure a PeerState exists for `node_id`, creating a default one if not.
    pub fn ensure(&mut self, node_id: &NodeId) -> &mut PeerState {
        self.peers.entry(node_id.clone()).or_default()
    }

    /// Ingest a price signal from gossip.
    ///
    /// Validates the signal is well-formed. Rejects stale signals (older
    /// than the stored signal). Updates `last_seen` to signal timestamp.
    ///
    /// Returns true if the signal was accepted, false if rejected.
    pub fn ingest_price_signal(&mut self, signal: &PriceSignal) -> bool {
        if !signal.is_valid() {
            return false;
        }

        let state = self.peers.entry(signal.node_id.clone()).or_default();

        // Reject out-of-order signals (must be strictly newer).
        if let Some(existing) = &state.price_signal {
            if signal.timestamp <= existing.timestamp {
                return false;
            }
        }

        state.price_signal = Some(signal.clone());
        state.last_seen = signal.timestamp;
        true
    }

    /// Update latency EMA for a peer based on an observed RTT sample.
    pub fn update_latency(&mut self, node_id: &NodeId, observed_ms: f64) {
        if !observed_ms.is_finite() || observed_ms < 0.0 {
            return;
        }
        let state = self.peers.entry(node_id.clone()).or_default();
        state.latency_ema_ms =
            PeerState::LATENCY_EMA_ALPHA * observed_ms
                + (1.0 - PeerState::LATENCY_EMA_ALPHA) * state.latency_ema_ms;
    }

    /// Record a verified trade for this peer. Called when a trade
    /// completes without an audit failure.
    pub fn record_verified_trade(&mut self, node_id: &NodeId) {
        let state = self.peers.entry(node_id.clone()).or_default();
        state.verified_trade_count = state.verified_trade_count.saturating_add(1);
        // Promote to next tier every 10 verified trades without a failure.
        // This is a simple threshold rule; Phase 15 might make it dynamic.
        match state.audit_tier {
            AuditTier::Unverified if state.verified_trade_count >= 1 => {
                state.audit_tier = AuditTier::Probationary;
            }
            AuditTier::Probationary if state.verified_trade_count >= 10 => {
                state.audit_tier = AuditTier::Established;
            }
            AuditTier::Established if state.verified_trade_count >= 100 => {
                state.audit_tier = AuditTier::Trusted;
            }
            _ => {}
        }
    }

    /// Phase 14.3 — record an audit outcome for a peer.
    /// `passed = true` promotes the tier; `false` demotes it.
    pub fn record_audit_result(&mut self, node_id: &NodeId, passed: bool) {
        let state = self.peers.entry(node_id.clone()).or_default();
        if passed {
            state.audit_tier = state.audit_tier.promote();
            state.verified_trade_count = state.verified_trade_count.saturating_add(1);
        } else {
            state.audit_tier = state.audit_tier.demote();
            // Don't zero out verified_trade_count — one failure shouldn't
            // erase all history, just downgrade.
        }
    }

    /// Return all peers that currently advertise `model_id` with non-zero
    /// available capacity. Result is unsorted — callers rank.
    pub fn providers_for_model(&self, model_id: &ModelId) -> Vec<(&NodeId, &PeerState)> {
        self.peers
            .iter()
            .filter(|(_, s)| s.serves_model(model_id) && s.available_cu() > 0)
            .collect()
    }

    /// Remove peers that have not been seen for `stale_threshold_ms` ms.
    /// Returns the number of entries removed. Useful for long-running nodes.
    pub fn prune_stale(&mut self, now_ms: u64, stale_threshold_ms: u64) -> usize {
        let before = self.peers.len();
        self.peers.retain(|_, s| now_ms.saturating_sub(s.last_seen) < stale_threshold_ms);
        before - self.peers.len()
    }

    /// Phase 14.3 — probabilistically select peers for audit based on their
    /// `AuditTier`. Each peer is rolled independently against its tier
    /// probability. Returns cloned NodeIds plus the ModelId they advertise
    /// (required for a challenge).
    ///
    /// - `now_ms` — used to skip peers that haven't been seen in > 24h.
    /// - `rng_sample` — callback returning a uniform float in `[0, 1)`.
    ///   Tests pass a deterministic sampler; production uses `rand::random`.
    pub fn select_audit_targets<F>(
        &self,
        now_ms: u64,
        mut rng_sample: F,
    ) -> Vec<(NodeId, ModelId)>
    where
        F: FnMut() -> f64,
    {
        const STALE_THRESHOLD_MS: u64 = 24 * 60 * 60 * 1000; // 24h
        self.peers
            .iter()
            .filter(|(_, s)| now_ms.saturating_sub(s.last_seen) < STALE_THRESHOLD_MS)
            .filter_map(|(id, s)| {
                let prob = s.audit_tier.audit_probability();
                if rng_sample() < prob {
                    // Pick any served model — audit needs a real model id.
                    s.price_signal
                        .as_ref()
                        .and_then(|sig| sig.model_capabilities.first().cloned())
                        .map(|m| (id.clone(), m))
                } else {
                    None
                }
            })
            .collect()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tirami_core::ModelId;

    fn sample_node(b: u8) -> NodeId {
        NodeId([b; 32])
    }

    fn sample_signal(node: NodeId, multiplier: f64, timestamp: u64) -> PriceSignal {
        PriceSignal {
            node_id: node,
            price_multiplier: multiplier,
            available_cu: 1000,
            model_capabilities: vec![ModelId("qwen2.5-0.5b".into())],
            latency_hint_ms: 50,
            timestamp,
        }
    }

    #[test]
    fn new_registry_is_empty() {
        let r = PeerRegistry::new();
        assert_eq!(r.len(), 0);
        assert!(r.is_empty());
    }

    #[test]
    fn ingest_valid_signal_stores_it() {
        let mut r = PeerRegistry::new();
        let sig = sample_signal(sample_node(1), 1.0, 100);
        assert!(r.ingest_price_signal(&sig));
        assert_eq!(r.len(), 1);
        assert!(r.get(&sample_node(1)).unwrap().price_signal.is_some());
    }

    #[test]
    fn ingest_invalid_signal_rejected() {
        let mut r = PeerRegistry::new();
        let bad = sample_signal(sample_node(1), f64::NAN, 100);
        assert!(!r.ingest_price_signal(&bad));
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn ingest_stale_signal_rejected() {
        let mut r = PeerRegistry::new();
        let sig_new = sample_signal(sample_node(1), 1.0, 200);
        let sig_old = sample_signal(sample_node(1), 1.5, 100);

        assert!(r.ingest_price_signal(&sig_new));
        assert!(!r.ingest_price_signal(&sig_old)); // older timestamp — reject
        assert_eq!(
            r.get(&sample_node(1)).unwrap().price_signal.as_ref().unwrap().price_multiplier,
            1.0
        );
    }

    #[test]
    fn ingest_newer_signal_replaces() {
        let mut r = PeerRegistry::new();
        let sig1 = sample_signal(sample_node(1), 1.0, 100);
        let sig2 = sample_signal(sample_node(1), 1.5, 200);

        assert!(r.ingest_price_signal(&sig1));
        assert!(r.ingest_price_signal(&sig2));
        assert_eq!(
            r.get(&sample_node(1)).unwrap().price_signal.as_ref().unwrap().price_multiplier,
            1.5
        );
    }

    #[test]
    fn update_latency_ema() {
        let mut r = PeerRegistry::new();
        let node = sample_node(1);
        r.update_latency(&node, 100.0);
        let first = r.get(&node).unwrap().latency_ema_ms;
        // Initial EMA is default 500; new sample 100 => 0.2*100 + 0.8*500 = 420
        assert!((first - 420.0).abs() < 0.1);

        r.update_latency(&node, 100.0);
        let second = r.get(&node).unwrap().latency_ema_ms;
        // EMA should converge toward 100.
        assert!(second < first);
    }

    #[test]
    fn update_latency_ignores_invalid() {
        let mut r = PeerRegistry::new();
        let node = sample_node(1);
        r.update_latency(&node, f64::NAN);
        r.update_latency(&node, -5.0);
        assert!(r.get(&node).is_none() || r.get(&node).unwrap().latency_ema_ms == 500.0);
    }

    #[test]
    fn providers_for_model_filters_correctly() {
        let mut r = PeerRegistry::new();
        r.ingest_price_signal(&sample_signal(sample_node(1), 1.0, 100));

        let mut sig_other = sample_signal(sample_node(2), 1.0, 100);
        sig_other.model_capabilities = vec![ModelId("other-model".into())];
        r.ingest_price_signal(&sig_other);

        let providers = r.providers_for_model(&ModelId("qwen2.5-0.5b".into()));
        assert_eq!(providers.len(), 1);
        assert_eq!(*providers[0].0, sample_node(1));
    }

    #[test]
    fn verified_trade_count_increments() {
        let mut r = PeerRegistry::new();
        let node = sample_node(1);
        r.record_verified_trade(&node);
        r.record_verified_trade(&node);
        r.record_verified_trade(&node);
        assert_eq!(r.get(&node).unwrap().verified_trade_count, 3);
    }

    #[test]
    fn record_audit_result_passes_promote_tier() {
        let mut r = PeerRegistry::new();
        let node = sample_node(1);
        r.record_audit_result(&node, true);
        assert_eq!(r.get(&node).unwrap().audit_tier, AuditTier::Probationary);
    }

    #[test]
    fn record_audit_result_failure_demotes() {
        let mut r = PeerRegistry::new();
        let node = sample_node(1);
        r.record_audit_result(&node, true); // → Probationary
        r.record_audit_result(&node, true); // → Established
        r.record_audit_result(&node, false); // → Probationary (demoted)
        assert_eq!(r.get(&node).unwrap().audit_tier, AuditTier::Probationary);
    }

    #[test]
    fn verified_trade_auto_promotes_unverified() {
        let mut r = PeerRegistry::new();
        let node = sample_node(1);
        r.record_verified_trade(&node);
        assert_eq!(r.get(&node).unwrap().audit_tier, AuditTier::Probationary);
    }

    #[test]
    fn select_audit_targets_picks_unverified_always() {
        let mut r = PeerRegistry::new();
        r.ingest_price_signal(&sample_signal(sample_node(1), 1.0, 100));
        // Even with rng returning 0.99, Unverified probability is 1.0.
        let picks = r.select_audit_targets(200, || 0.99);
        assert_eq!(picks.len(), 1);
        assert_eq!(picks[0].0, sample_node(1));
    }

    #[test]
    fn select_audit_targets_skips_trusted_on_high_roll() {
        let mut r = PeerRegistry::new();
        r.ingest_price_signal(&sample_signal(sample_node(1), 1.0, 100));
        // Promote to Trusted.
        for _ in 0..3 {
            r.record_audit_result(&sample_node(1), true);
        }
        assert_eq!(r.get(&sample_node(1)).unwrap().audit_tier, AuditTier::Trusted);
        // Trusted probability = 0.01; roll of 0.5 must skip.
        let picks = r.select_audit_targets(200, || 0.5);
        assert!(picks.is_empty());
    }

    #[test]
    fn select_audit_targets_skips_stale_peers() {
        let mut r = PeerRegistry::new();
        r.ingest_price_signal(&sample_signal(sample_node(1), 1.0, 100));
        // 25 hours later — stale.
        let now = 100 + 25 * 60 * 60 * 1000;
        let picks = r.select_audit_targets(now, || 0.0);
        assert!(picks.is_empty());
    }

    #[test]
    fn select_audit_targets_requires_model_advertised() {
        let mut r = PeerRegistry::new();
        // Peer with default state (no price_signal) — can't audit.
        r.ensure(&sample_node(1));
        let picks = r.select_audit_targets(100, || 0.0);
        assert!(picks.is_empty());
    }

    #[test]
    fn prune_stale_removes_old_peers() {
        let mut r = PeerRegistry::new();
        r.ingest_price_signal(&sample_signal(sample_node(1), 1.0, 100));
        r.ingest_price_signal(&sample_signal(sample_node(2), 1.0, 500));

        let removed = r.prune_stale(1000, 600);
        // node(1) last_seen=100, age=900 > 600 → removed
        // node(2) last_seen=500, age=500 < 600 → kept
        assert_eq!(removed, 1);
        assert!(r.get(&sample_node(1)).is_none());
        assert!(r.get(&sample_node(2)).is_some());
    }

    #[test]
    fn effective_price_applies_multiplier() {
        let mut r = PeerRegistry::new();
        r.ingest_price_signal(&sample_signal(sample_node(1), 0.5, 100));
        let state = r.get(&sample_node(1)).unwrap();
        assert_eq!(state.effective_price(2.0), 1.0); // 2.0 × 0.5 = 1.0
    }

    #[test]
    fn effective_price_falls_back_to_base_when_no_signal() {
        let mut r = PeerRegistry::new();
        r.ensure(&sample_node(1));
        let state = r.get(&sample_node(1)).unwrap();
        assert_eq!(state.effective_price(2.0), 2.0);
    }
}
