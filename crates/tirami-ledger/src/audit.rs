//! Phase 14.3 — Audit protocol state.
//!
//! Tracks pending audit challenges the local node has issued. When the
//! matching `AuditResponse` arrives we look up the expected hash here,
//! compare, and call `PeerRegistry::record_audit_result`.
//!
//! The tracker is intentionally in-memory only: audit state is ephemeral,
//! short-lived (5-minute timeout), and rebuilt from scratch on node restart.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tirami_core::{ModelId, NodeId};

/// Default challenge timeout in milliseconds. Matches the proto-layer cap.
pub const AUDIT_TIMEOUT_MS: u64 = 5 * 60 * 1000;

/// A challenge the local node has issued and is waiting on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingChallenge {
    pub challenge_id: u64,
    pub target: NodeId,
    pub model_id: ModelId,
    pub expected_hash: [u8; 32],
    pub issued_at_ms: u64,
}

impl PendingChallenge {
    pub fn is_expired(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.issued_at_ms) > AUDIT_TIMEOUT_MS
    }
}

/// Verdict returned by `AuditTracker::resolve` once a response arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditVerdict {
    /// Target's hash matches — promotes AuditTier.
    Passed,
    /// Target's hash differs — demotes AuditTier.
    Failed,
    /// No matching challenge id exists, or it has already expired.
    Unknown,
}

/// Tracks in-flight audit challenges issued by the local node.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AuditTracker {
    pending: HashMap<u64, PendingChallenge>,
    /// Monotonic id generator. Challengers increment this per issued challenge.
    next_id: u64,
}

impl AuditTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a fresh challenge id + remember the expected hash.
    pub fn issue_challenge(
        &mut self,
        target: NodeId,
        model_id: ModelId,
        expected_hash: [u8; 32],
        now_ms: u64,
    ) -> PendingChallenge {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let challenge = PendingChallenge {
            challenge_id: id,
            target,
            model_id,
            expected_hash,
            issued_at_ms: now_ms,
        };
        self.pending.insert(id, challenge.clone());
        challenge
    }

    /// Resolve an incoming response: match hash, remove the challenge,
    /// and return the verdict.
    pub fn resolve(
        &mut self,
        challenge_id: u64,
        target: &NodeId,
        output_hash: &[u8; 32],
        now_ms: u64,
    ) -> AuditVerdict {
        let Some(c) = self.pending.remove(&challenge_id) else {
            return AuditVerdict::Unknown;
        };
        if c.is_expired(now_ms) {
            return AuditVerdict::Unknown;
        }
        if c.target != *target {
            return AuditVerdict::Unknown;
        }
        if c.expected_hash == *output_hash {
            AuditVerdict::Passed
        } else {
            AuditVerdict::Failed
        }
    }

    /// Number of challenges currently awaiting responses.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Drop any challenges older than `AUDIT_TIMEOUT_MS`. Returns the
    /// number evicted. Called periodically by the daemon audit loop.
    pub fn prune_expired(&mut self, now_ms: u64) -> usize {
        let before = self.pending.len();
        self.pending.retain(|_, c| !c.is_expired(now_ms));
        before - self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(b: u8) -> NodeId {
        NodeId([b; 32])
    }

    #[test]
    fn issue_allocates_monotonic_ids() {
        let mut t = AuditTracker::new();
        let a = t.issue_challenge(node(1), ModelId("m".into()), [0; 32], 0);
        let b = t.issue_challenge(node(1), ModelId("m".into()), [0; 32], 0);
        assert!(b.challenge_id > a.challenge_id);
    }

    #[test]
    fn resolve_passed_on_matching_hash() {
        let mut t = AuditTracker::new();
        let c = t.issue_challenge(node(1), ModelId("m".into()), [7; 32], 0);
        let v = t.resolve(c.challenge_id, &node(1), &[7; 32], 100);
        assert_eq!(v, AuditVerdict::Passed);
        assert_eq!(t.pending_count(), 0);
    }

    #[test]
    fn resolve_failed_on_mismatched_hash() {
        let mut t = AuditTracker::new();
        let c = t.issue_challenge(node(1), ModelId("m".into()), [7; 32], 0);
        let v = t.resolve(c.challenge_id, &node(1), &[8; 32], 100);
        assert_eq!(v, AuditVerdict::Failed);
    }

    #[test]
    fn resolve_unknown_on_wrong_target() {
        let mut t = AuditTracker::new();
        let c = t.issue_challenge(node(1), ModelId("m".into()), [7; 32], 0);
        let v = t.resolve(c.challenge_id, &node(2), &[7; 32], 100);
        assert_eq!(v, AuditVerdict::Unknown);
    }

    #[test]
    fn resolve_unknown_on_missing_id() {
        let mut t = AuditTracker::new();
        let v = t.resolve(9999, &node(1), &[0; 32], 0);
        assert_eq!(v, AuditVerdict::Unknown);
    }

    #[test]
    fn resolve_unknown_on_expired() {
        let mut t = AuditTracker::new();
        let c = t.issue_challenge(node(1), ModelId("m".into()), [7; 32], 0);
        let later = AUDIT_TIMEOUT_MS + 1_000;
        let v = t.resolve(c.challenge_id, &node(1), &[7; 32], later);
        assert_eq!(v, AuditVerdict::Unknown);
    }

    #[test]
    fn prune_expired_removes_old_entries() {
        let mut t = AuditTracker::new();
        t.issue_challenge(node(1), ModelId("m".into()), [0; 32], 0);
        t.issue_challenge(node(2), ModelId("m".into()), [0; 32], AUDIT_TIMEOUT_MS * 2);
        let removed = t.prune_expired(AUDIT_TIMEOUT_MS * 2);
        assert_eq!(removed, 1);
        assert_eq!(t.pending_count(), 1);
    }
}
