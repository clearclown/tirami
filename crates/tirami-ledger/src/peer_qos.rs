//! Phase 25 C8 — per-peer connection QoS primitive.
//!
//! # Problem
//!
//! `Config.max_concurrent_connections` (default 1,000) caps the
//! transport globally. A single misbehaving peer can monopolise
//! the entire connection budget by opening + abandoning hundreds
//! of streams, starving honest peers.
//!
//! # Solution
//!
//! `PerPeerConnectionTracker` tracks per-peer open-connection
//! counts + a temp-ban list. The transport accept path consults
//! it and either:
//!   - allows the connection and increments the per-peer count, or
//!   - rejects + records a reject (potentially escalating into a
//!     temp ban) when the per-peer cap is exceeded.
//!
//! This module is intentionally a pure data-structure primitive
//! (no async, no transport-specific types) so the transport
//! integration is the only place that needs Phase 25 C8 thinking.
//! That keeps the migration surface bounded.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use tirami_core::NodeId;

/// Phase 25 C8 — default maximum connections one peer may hold
/// open. Operators tune via `Config.max_connections_per_peer`.
/// 5 is generous (enough for multiple model streams from one
/// agent) but well below the global cap so one peer can't
/// monopolise the budget.
pub const DEFAULT_MAX_CONNS_PER_PEER: u32 = 5;

/// Phase 25 C8 — default temp-ban duration after exceeding the
/// per-peer cap repeatedly. 5 minutes is enough to discourage
/// flapping clients without permanently blacklisting a legitimate
/// peer that hit a momentary anomaly.
pub const DEFAULT_TEMP_BAN_SECS: u64 = 300;

/// Number of consecutive over-cap events before a peer is
/// temp-banned.
pub const REJECT_TO_BAN_THRESHOLD: u32 = 3;

/// Configuration knobs for the tracker. Operator-tunable so
/// experimentation in production doesn't require a code patch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerPeerQosConfig {
    pub max_conns_per_peer: u32,
    pub temp_ban_secs: u64,
    pub reject_to_ban_threshold: u32,
}

impl Default for PerPeerQosConfig {
    fn default() -> Self {
        Self {
            max_conns_per_peer: DEFAULT_MAX_CONNS_PER_PEER,
            temp_ban_secs: DEFAULT_TEMP_BAN_SECS,
            reject_to_ban_threshold: REJECT_TO_BAN_THRESHOLD,
        }
    }
}

/// Per-peer state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PeerState {
    open_count: u32,
    /// Consecutive over-cap rejects since the last successful
    /// open. Reset to 0 on a successful open or after the ban
    /// expires.
    reject_streak: u32,
    /// `Some(unix_ms)` when the peer is currently temp-banned.
    /// `None` otherwise.
    banned_until_ms: Option<u64>,
}

/// Pure data-structure tracker. No async, no transport types.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerPeerConnectionTracker {
    cfg: PerPeerQosConfig,
    peers: HashMap<NodeId, PeerState>,
    /// Bounded ring of recent reject events so we can serve a
    /// /metrics counter without unbounded growth.
    recent_rejects: VecDeque<(NodeId, u64)>,
}

impl PerPeerConnectionTracker {
    pub fn new() -> Self {
        Self::with_config(PerPeerQosConfig::default())
    }

    pub fn with_config(cfg: PerPeerQosConfig) -> Self {
        Self {
            cfg,
            peers: HashMap::new(),
            recent_rejects: VecDeque::new(),
        }
    }

    pub fn config(&self) -> &PerPeerQosConfig {
        &self.cfg
    }

    /// Result of an `accept_or_reject` call.
    pub fn can_accept(&self, peer: &NodeId, now_ms: u64) -> AcceptVerdict {
        if let Some(state) = self.peers.get(peer) {
            if let Some(banned_until) = state.banned_until_ms {
                if now_ms < banned_until {
                    return AcceptVerdict::TempBanned { until_ms: banned_until };
                }
            }
            if state.open_count >= self.cfg.max_conns_per_peer {
                return AcceptVerdict::OverCap {
                    open: state.open_count,
                    cap: self.cfg.max_conns_per_peer,
                };
            }
        }
        AcceptVerdict::Allow
    }

    /// Record a new accepted connection from `peer`.
    pub fn record_open(&mut self, peer: NodeId) {
        let entry = self.peers.entry(peer).or_default();
        entry.open_count = entry.open_count.saturating_add(1);
        entry.reject_streak = 0;
    }

    /// Record a connection close from `peer` — decrement the count
    /// (saturating at 0; double-close is idempotent).
    pub fn record_close(&mut self, peer: &NodeId) {
        if let Some(state) = self.peers.get_mut(peer) {
            state.open_count = state.open_count.saturating_sub(1);
        }
    }

    /// Record an over-cap reject. After `reject_to_ban_threshold`
    /// consecutive rejects without a successful open, the peer is
    /// temp-banned for `temp_ban_secs`.
    pub fn record_reject(&mut self, peer: NodeId, now_ms: u64) -> RejectOutcome {
        let cfg = self.cfg.clone();
        let entry = self.peers.entry(peer.clone()).or_default();
        entry.reject_streak = entry.reject_streak.saturating_add(1);
        let banned = entry.reject_streak >= cfg.reject_to_ban_threshold;
        if banned {
            let until = now_ms.saturating_add(cfg.temp_ban_secs.saturating_mul(1_000));
            entry.banned_until_ms = Some(until);
            entry.reject_streak = 0;
        }
        // Track recent rejects (ring-buffered).
        self.recent_rejects.push_back((peer, now_ms));
        while self.recent_rejects.len() > 1024 {
            self.recent_rejects.pop_front();
        }
        if banned {
            RejectOutcome::Banned
        } else {
            RejectOutcome::CountedTowardBan
        }
    }

    /// Number of distinct peers currently tracked.
    pub fn tracked_peers(&self) -> usize {
        self.peers.len()
    }

    /// Open-connection count for a specific peer.
    pub fn open_count(&self, peer: &NodeId) -> u32 {
        self.peers.get(peer).map(|p| p.open_count).unwrap_or(0)
    }

    /// `true` iff the peer is currently temp-banned.
    pub fn is_banned(&self, peer: &NodeId, now_ms: u64) -> bool {
        self.peers
            .get(peer)
            .and_then(|p| p.banned_until_ms)
            .is_some_and(|until| now_ms < until)
    }

    /// Number of rejects across all peers in the recent ring.
    pub fn recent_reject_count(&self) -> usize {
        self.recent_rejects.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptVerdict {
    Allow,
    OverCap { open: u32, cap: u32 },
    TempBanned { until_ms: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectOutcome {
    CountedTowardBan,
    Banned,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn nid(seed: u8) -> NodeId {
        NodeId([seed; 32])
    }

    #[test]
    fn fresh_peer_can_accept() {
        let t = PerPeerConnectionTracker::new();
        assert_eq!(t.can_accept(&nid(1), 0), AcceptVerdict::Allow);
    }

    #[test]
    fn open_increments_count_and_reset_streak() {
        let mut t = PerPeerConnectionTracker::new();
        t.record_open(nid(1));
        assert_eq!(t.open_count(&nid(1)), 1);
        t.record_open(nid(1));
        assert_eq!(t.open_count(&nid(1)), 2);
    }

    #[test]
    fn close_decrements_count_and_saturates_at_zero() {
        let mut t = PerPeerConnectionTracker::new();
        t.record_open(nid(1));
        t.record_close(&nid(1));
        assert_eq!(t.open_count(&nid(1)), 0);
        // Double-close is idempotent.
        t.record_close(&nid(1));
        assert_eq!(t.open_count(&nid(1)), 0);
    }

    #[test]
    fn over_cap_returns_over_cap_verdict() {
        let mut t = PerPeerConnectionTracker::new();
        for _ in 0..DEFAULT_MAX_CONNS_PER_PEER {
            t.record_open(nid(1));
        }
        match t.can_accept(&nid(1), 0) {
            AcceptVerdict::OverCap { open, cap } => {
                assert_eq!(open, DEFAULT_MAX_CONNS_PER_PEER);
                assert_eq!(cap, DEFAULT_MAX_CONNS_PER_PEER);
            }
            other => panic!("expected OverCap, got {other:?}"),
        }
    }

    #[test]
    fn other_peers_unaffected_by_one_over_cap_peer() {
        let mut t = PerPeerConnectionTracker::new();
        for _ in 0..DEFAULT_MAX_CONNS_PER_PEER {
            t.record_open(nid(1));
        }
        assert_eq!(t.can_accept(&nid(2), 0), AcceptVerdict::Allow);
    }

    #[test]
    fn rejects_below_threshold_count_toward_ban() {
        let mut t = PerPeerConnectionTracker::new();
        for _ in 0..(REJECT_TO_BAN_THRESHOLD - 1) {
            let outcome = t.record_reject(nid(1), 1_000);
            assert_eq!(outcome, RejectOutcome::CountedTowardBan);
        }
        assert!(!t.is_banned(&nid(1), 1_000));
    }

    #[test]
    fn reaching_threshold_triggers_temp_ban() {
        let mut t = PerPeerConnectionTracker::new();
        let mut last = RejectOutcome::CountedTowardBan;
        for _ in 0..REJECT_TO_BAN_THRESHOLD {
            last = t.record_reject(nid(1), 1_000);
        }
        assert_eq!(last, RejectOutcome::Banned);
        assert!(t.is_banned(&nid(1), 1_000));
    }

    #[test]
    fn ban_expires_after_temp_ban_secs() {
        let mut t = PerPeerConnectionTracker::new();
        for _ in 0..REJECT_TO_BAN_THRESHOLD {
            t.record_reject(nid(1), 1_000);
        }
        // Still banned mid-window.
        assert!(t.is_banned(&nid(1), 1_000 + 60_000));
        // Past the window — ban expired.
        let after_ban = 1_000 + (DEFAULT_TEMP_BAN_SECS * 1_000) + 1;
        assert!(!t.is_banned(&nid(1), after_ban));
    }

    #[test]
    fn can_accept_during_ban_returns_temp_banned() {
        let mut t = PerPeerConnectionTracker::new();
        for _ in 0..REJECT_TO_BAN_THRESHOLD {
            t.record_reject(nid(1), 1_000);
        }
        let now = 1_000 + 30_000;
        match t.can_accept(&nid(1), now) {
            AcceptVerdict::TempBanned { until_ms } => {
                assert!(until_ms > now);
            }
            other => panic!("expected TempBanned, got {other:?}"),
        }
    }

    #[test]
    fn successful_open_resets_reject_streak() {
        let mut t = PerPeerConnectionTracker::new();
        t.record_reject(nid(1), 1_000);
        t.record_open(nid(1));
        // Next reject is a fresh streak count of 1, not 2.
        let outcome = t.record_reject(nid(1), 1_000);
        assert_eq!(outcome, RejectOutcome::CountedTowardBan);
        assert!(!t.is_banned(&nid(1), 1_000));
    }

    #[test]
    fn recent_rejects_ring_is_bounded() {
        let mut t = PerPeerConnectionTracker::new();
        for i in 0..2000 {
            t.record_reject(nid((i % 250) as u8), 1_000);
        }
        assert!(t.recent_reject_count() <= 1024);
    }

    #[test]
    fn config_serde_roundtrips() {
        let cfg = PerPeerQosConfig::default();
        let s = serde_json::to_string(&cfg).unwrap();
        let back: PerPeerQosConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(back.max_conns_per_peer, cfg.max_conns_per_peer);
        assert_eq!(back.temp_ban_secs, cfg.temp_ban_secs);
        assert_eq!(back.reject_to_ban_threshold, cfg.reject_to_ban_threshold);
    }
}
