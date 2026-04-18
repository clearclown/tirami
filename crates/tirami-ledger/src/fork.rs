//! Phase 17 Wave 2.5 — fork detection + fraud-proof primitives.
//!
//! # Problem
//!
//! Independently operating nodes with local trade logs can diverge.
//! Two causes:
//! 1. Benign: different gossip arrival orders, dropped messages, clock
//!    skew. The ledgers stabilize once gossip catches up.
//! 2. Malicious: a node rewrites its own trade log, or a dishonest
//!    provider double-signs a `TradeRecord` — same `(provider, nonce)`
//!    pair, different economic payload. The replay cache (Wave 1.2)
//!    blocks double-spends of the same record, but doesn't prevent
//!    the provider from issuing *two distinct* records bound to the
//!    same nonce, each to a different consumer.
//!
//! # Solution
//!
//! This wave delivers the detection + verdict primitives:
//!
//! * [`ForkDetector`] collects Merkle-root observations from peers
//!   (see Wave 2.4 `trades_merkle_root`). When the local root
//!   disagrees with a strict majority of peers, the local node is
//!   in a minority fork and should initiate resync.
//! * [`NonceFraudProof`] captures the "one provider, one nonce, two
//!   signed records" pattern in a single broadcastable type. Any
//!   node that sees two conflicting `SignedTradeRecord` with the
//!   same `(provider, nonce)` can construct a `NonceFraudProof` and
//!   gossip it; receivers verify both signatures and the nonce
//!   equality themselves.
//! * [`detect_nonce_conflict`] is the scan over a trade slice that
//!   produces the fraud proof, or `None` if nothing suspicious.
//!
//! # Deferred
//!
//! Full resync (request 1 000 trades from a majority peer, diff
//! against local, apply the missing, re-verify Merkle root) needs
//! new wire messages and retry semantics; it's the natural
//! Wave-2.5-part-2. The verdict types here are the contract that
//! the resync layer will consume.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::SignedTradeRecord;
use tirami_core::NodeId;

// ---------------------------------------------------------------------------
// ForkDetector
// ---------------------------------------------------------------------------

/// Outcome of a fork-detection round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkVerdict {
    /// The local root matches the strict majority of observed peers.
    /// No action needed.
    Converged {
        agreed_root: [u8; 32],
        /// How many peers were in the majority bucket (including local).
        majority_size: usize,
        /// Total peers observed this round.
        total_observed: usize,
    },
    /// A different root is held by a strict majority of peers.
    /// The local node is on a minority fork and should resync.
    InMinority {
        local_root: [u8; 32],
        majority_root: [u8; 32],
        majority_size: usize,
        minority_size: usize,
    },
    /// No strict majority — multiple competing roots, or the
    /// observation set is too small to produce a verdict. Policy:
    /// do NOT resync on ambiguous evidence; re-roll next window.
    NoQuorum { total_observed: usize },
}

/// Collects peer Merkle-root observations for a single round and
/// produces a [`ForkVerdict`].
#[derive(Debug, Clone, Default)]
pub struct ForkDetector {
    observations: HashMap<NodeId, [u8; 32]>,
}

impl ForkDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `peer`'s observed Merkle root. Idempotent — a second
    /// observation from the same peer overwrites the first.
    pub fn observe(&mut self, peer: NodeId, root: [u8; 32]) {
        self.observations.insert(peer, root);
    }

    /// Number of distinct peers observed this round.
    pub fn len(&self) -> usize {
        self.observations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.observations.is_empty()
    }

    /// Reset the detector for a fresh round.
    pub fn reset(&mut self) {
        self.observations.clear();
    }

    /// Compute the verdict given the local root. `min_observations`
    /// is the threshold below which we refuse to make a call (returns
    /// `NoQuorum`); typical value is 3 so a single noisy peer can't
    /// flip the outcome.
    pub fn verdict(&self, local_root: [u8; 32], min_observations: usize) -> ForkVerdict {
        let mut all: HashMap<[u8; 32], Vec<NodeId>> = HashMap::new();
        for (peer, root) in &self.observations {
            all.entry(*root).or_default().push(peer.clone());
        }
        // Always fold the local node into its own bucket.
        all.entry(local_root)
            .or_default()
            .push(NodeId([0u8; 32]));

        let total_observed = all.values().map(|v| v.len()).sum::<usize>();
        if total_observed < min_observations.max(2) {
            return ForkVerdict::NoQuorum { total_observed };
        }

        // Find the largest bucket. Tie → NoQuorum.
        let mut best_root: [u8; 32] = [0u8; 32];
        let mut best_size: usize = 0;
        let mut tie = false;
        for (root, members) in &all {
            if members.len() > best_size {
                best_size = members.len();
                best_root = *root;
                tie = false;
            } else if members.len() == best_size {
                tie = true;
            }
        }
        if tie {
            return ForkVerdict::NoQuorum { total_observed };
        }
        // Strict majority: > half.
        if best_size * 2 <= total_observed {
            return ForkVerdict::NoQuorum { total_observed };
        }
        if best_root == local_root {
            ForkVerdict::Converged {
                agreed_root: best_root,
                majority_size: best_size,
                total_observed,
            }
        } else {
            ForkVerdict::InMinority {
                local_root,
                majority_root: best_root,
                majority_size: best_size,
                minority_size: total_observed - best_size,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// NonceFraudProof — double-signed nonce collision
// ---------------------------------------------------------------------------

/// Evidence that a provider issued two distinct `SignedTradeRecord`
/// instances sharing the same `(provider, nonce)` pair but
/// disagreeing on at least one economic field.
///
/// Broadcast as-is via gossip; receivers re-verify both signatures
/// and the nonce equality before slashing the offending provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NonceFraudProof {
    /// The first of the two conflicting records, in reception order.
    pub record_a: SignedTradeRecord,
    /// The second conflicting record.
    pub record_b: SignedTradeRecord,
}

impl PartialEq for NonceFraudProof {
    fn eq(&self, other: &Self) -> bool {
        (self.record_a.trade.provider == other.record_a.trade.provider
            && self.record_a.trade.nonce == other.record_a.trade.nonce
            && self.record_a.trade.canonical_bytes() == other.record_a.trade.canonical_bytes()
            && self.record_b.trade.canonical_bytes() == other.record_b.trade.canonical_bytes())
            || (self.record_a.trade.canonical_bytes() == other.record_b.trade.canonical_bytes()
                && self.record_b.trade.canonical_bytes() == other.record_a.trade.canonical_bytes())
    }
}

/// Errors verifying a [`NonceFraudProof`] against local state.
#[derive(Debug, Clone, thiserror::Error)]
pub enum NonceFraudProofError {
    #[error("records have different providers — not a single-provider conflict")]
    ProviderMismatch,
    #[error("records have different nonces — not a nonce collision")]
    NonceMismatch,
    #[error("records are byte-identical — not two distinct trades")]
    NotDistinct,
    #[error("record A carries a zero nonce; legacy v1 records cannot form a fraud proof")]
    LegacyV1A,
    #[error("record B carries a zero nonce; legacy v1 records cannot form a fraud proof")]
    LegacyV1B,
    #[error("record A signature verification failed: {0}")]
    InvalidSignatureA(crate::SignatureError),
    #[error("record B signature verification failed: {0}")]
    InvalidSignatureB(crate::SignatureError),
}

impl NonceFraudProof {
    /// Validate the fraud proof structurally and cryptographically.
    /// `Ok(())` means the proof is sound; the receiver should slash
    /// `record_a.trade.provider`.
    pub fn verify(&self) -> Result<(), NonceFraudProofError> {
        if self.record_a.trade.provider != self.record_b.trade.provider {
            return Err(NonceFraudProofError::ProviderMismatch);
        }
        if self.record_a.trade.nonce != self.record_b.trade.nonce {
            return Err(NonceFraudProofError::NonceMismatch);
        }
        // A v1 (zero-nonce) record is exempt from the dedup contract,
        // so it's not fraud evidence — a provider can legitimately
        // issue many v1 trades. Only v2 collisions are actionable.
        if !self.record_a.trade.has_nonce() {
            return Err(NonceFraudProofError::LegacyV1A);
        }
        if !self.record_b.trade.has_nonce() {
            return Err(NonceFraudProofError::LegacyV1B);
        }
        if self.record_a.trade.canonical_bytes() == self.record_b.trade.canonical_bytes() {
            return Err(NonceFraudProofError::NotDistinct);
        }
        self.record_a
            .verify()
            .map_err(NonceFraudProofError::InvalidSignatureA)?;
        self.record_b
            .verify()
            .map_err(NonceFraudProofError::InvalidSignatureB)?;
        Ok(())
    }

    /// Convenience: the provider this proof targets.
    pub fn accused(&self) -> &NodeId {
        &self.record_a.trade.provider
    }
}

/// Scan a slice of signed trades for `(provider, nonce)` collisions
/// and return the first one found as a [`NonceFraudProof`], or `None`.
/// Intended for use on batches of records freshly received via
/// gossip before they are applied to the ledger.
///
/// v1 (zero-nonce) records are ignored — see [`NonceFraudProofError::LegacyV1A`].
pub fn detect_nonce_conflict(trades: &[SignedTradeRecord]) -> Option<NonceFraudProof> {
    use std::collections::HashMap as Map;
    let mut seen: Map<(NodeId, [u8; 16]), &SignedTradeRecord> = Map::new();
    for st in trades {
        if !st.trade.has_nonce() {
            continue;
        }
        let key = (st.trade.provider.clone(), st.trade.nonce);
        if let Some(prev) = seen.get(&key) {
            // Same trade bytes → duplicate, not a conflict.
            if prev.trade.canonical_bytes() == st.trade.canonical_bytes() {
                continue;
            }
            return Some(NonceFraudProof {
                record_a: (*prev).clone(),
                record_b: st.clone(),
            });
        }
        seen.insert(key, st);
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TradeRecord;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    fn root(b: u8) -> [u8; 32] {
        [b; 32]
    }

    fn peer(b: u8) -> NodeId {
        NodeId([b; 32])
    }

    // --- ForkDetector ---

    #[test]
    fn empty_detector_with_local_is_no_quorum_below_threshold() {
        let d = ForkDetector::new();
        assert_eq!(
            d.verdict(root(0xAA), 3),
            ForkVerdict::NoQuorum { total_observed: 1 }
        );
    }

    #[test]
    fn converged_when_majority_matches_local() {
        let mut d = ForkDetector::new();
        d.observe(peer(1), root(0xAA));
        d.observe(peer(2), root(0xAA));
        d.observe(peer(3), root(0xBB));
        let v = d.verdict(root(0xAA), 3);
        match v {
            ForkVerdict::Converged {
                agreed_root,
                majority_size,
                total_observed,
            } => {
                assert_eq!(agreed_root, root(0xAA));
                assert!(majority_size >= 3); // local + 2 peers
                assert_eq!(total_observed, 4);
            }
            other => panic!("expected Converged, got {:?}", other),
        }
    }

    #[test]
    fn in_minority_when_majority_differs() {
        let mut d = ForkDetector::new();
        d.observe(peer(1), root(0xAA));
        d.observe(peer(2), root(0xAA));
        d.observe(peer(3), root(0xAA));
        let v = d.verdict(root(0xBB), 3);
        match v {
            ForkVerdict::InMinority {
                local_root,
                majority_root,
                majority_size,
                minority_size,
            } => {
                assert_eq!(local_root, root(0xBB));
                assert_eq!(majority_root, root(0xAA));
                assert_eq!(majority_size, 3);
                assert_eq!(minority_size, 1);
            }
            other => panic!("expected InMinority, got {:?}", other),
        }
    }

    #[test]
    fn tie_is_no_quorum() {
        let mut d = ForkDetector::new();
        d.observe(peer(1), root(0xAA));
        d.observe(peer(2), root(0xBB));
        // Local adds to 0xCC bucket → three 1-peer buckets, no majority.
        let v = d.verdict(root(0xCC), 2);
        assert!(matches!(v, ForkVerdict::NoQuorum { total_observed: 3 }));
    }

    #[test]
    fn reset_clears_observations() {
        let mut d = ForkDetector::new();
        d.observe(peer(1), root(0xAA));
        assert_eq!(d.len(), 1);
        d.reset();
        assert!(d.is_empty());
    }

    #[test]
    fn re_observation_overwrites_previous() {
        let mut d = ForkDetector::new();
        d.observe(peer(1), root(0xAA));
        d.observe(peer(1), root(0xBB));
        assert_eq!(d.len(), 1);
        // Only one observation (the new root) + local.
        let v = d.verdict(root(0xBB), 2);
        assert!(matches!(v, ForkVerdict::Converged { .. }));
    }

    // --- NonceFraudProof ---

    fn sign_with(provider: &SigningKey, consumer: &SigningKey, trade: TradeRecord) -> SignedTradeRecord {
        let canonical = trade.canonical_bytes();
        SignedTradeRecord {
            trade,
            provider_sig: provider.sign(&canonical).to_bytes().to_vec(),
            consumer_sig: consumer.sign(&canonical).to_bytes().to_vec(),
        }
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    fn two_conflicting_trades() -> (SignedTradeRecord, SignedTradeRecord) {
        let mut rng = OsRng;
        let provider_key = SigningKey::generate(&mut rng);
        let consumer_key_a = SigningKey::generate(&mut rng);
        let consumer_key_b = SigningKey::generate(&mut rng);
        let nonce = [0x77u8; 16];
        let provider = NodeId(provider_key.verifying_key().to_bytes());
        // Use a current timestamp so SignedTradeRecord::verify() doesn't
        // reject with TimestampExpired.
        let ts = now_ms();
        let trade_a = TradeRecord {
            provider: provider.clone(),
            consumer: NodeId(consumer_key_a.verifying_key().to_bytes()),
            trm_amount: 100,
            tokens_processed: 10,
            timestamp: ts,
            model_id: "m".into(),
            flops_estimated: 0,
            nonce,
        };
        let trade_b = TradeRecord {
            // Same provider + nonce, different consumer + amount.
            provider,
            consumer: NodeId(consumer_key_b.verifying_key().to_bytes()),
            trm_amount: 200,
            tokens_processed: 20,
            timestamp: ts,
            model_id: "m".into(),
            flops_estimated: 0,
            nonce,
        };
        let sa = sign_with(&provider_key, &consumer_key_a, trade_a);
        let sb = sign_with(&provider_key, &consumer_key_b, trade_b);
        (sa, sb)
    }

    #[test]
    fn fraud_proof_accepts_valid_nonce_collision() {
        let (a, b) = two_conflicting_trades();
        let proof = NonceFraudProof {
            record_a: a,
            record_b: b,
        };
        assert!(proof.verify().is_ok());
    }

    #[test]
    fn fraud_proof_rejects_identical_records() {
        let (a, _) = two_conflicting_trades();
        let proof = NonceFraudProof {
            record_a: a.clone(),
            record_b: a,
        };
        assert!(matches!(proof.verify(), Err(NonceFraudProofError::NotDistinct)));
    }

    #[test]
    fn fraud_proof_rejects_provider_mismatch() {
        let (mut a, b) = two_conflicting_trades();
        // Replace record_a's provider — breaks the "same provider" invariant.
        a.trade.provider = NodeId([0xFFu8; 32]);
        let proof = NonceFraudProof {
            record_a: a,
            record_b: b,
        };
        assert!(matches!(
            proof.verify(),
            Err(NonceFraudProofError::ProviderMismatch)
        ));
    }

    #[test]
    fn fraud_proof_rejects_nonce_mismatch() {
        let (a, mut b) = two_conflicting_trades();
        b.trade.nonce = [0x88u8; 16];
        let proof = NonceFraudProof {
            record_a: a,
            record_b: b,
        };
        assert!(matches!(
            proof.verify(),
            Err(NonceFraudProofError::NonceMismatch)
        ));
    }

    #[test]
    fn fraud_proof_rejects_legacy_v1_records() {
        // Synthesize two distinct v1 trades with matching (provider, nonce=0)
        // — v1 is exempt by design, so this must be rejected.
        let mut rng = OsRng;
        let p = SigningKey::generate(&mut rng);
        let c = SigningKey::generate(&mut rng);
        let make = |amt: u64| {
            let trade = TradeRecord {
                provider: NodeId(p.verifying_key().to_bytes()),
                consumer: NodeId(c.verifying_key().to_bytes()),
                trm_amount: amt,
                tokens_processed: 10,
                timestamp: 1_000,
                model_id: "m".into(),
                flops_estimated: 0,
                nonce: [0u8; 16],
            };
            sign_with(&p, &c, trade)
        };
        let proof = NonceFraudProof {
            record_a: make(100),
            record_b: make(200),
        };
        assert!(matches!(proof.verify(), Err(NonceFraudProofError::LegacyV1A)));
    }

    #[test]
    fn fraud_proof_rejects_bad_signature_a() {
        let (mut a, b) = two_conflicting_trades();
        a.provider_sig[0] ^= 0xFF;
        let proof = NonceFraudProof {
            record_a: a,
            record_b: b,
        };
        assert!(matches!(
            proof.verify(),
            Err(NonceFraudProofError::InvalidSignatureA(_))
        ));
    }

    #[test]
    fn detect_nonce_conflict_finds_collision_in_batch() {
        let (a, b) = two_conflicting_trades();
        let proof = detect_nonce_conflict(&[a.clone(), b.clone()]).unwrap();
        assert!(proof.verify().is_ok());
        assert_eq!(proof.accused(), &a.trade.provider);
    }

    #[test]
    fn detect_nonce_conflict_ignores_duplicates() {
        // Same record presented twice is NOT a conflict.
        let (a, _) = two_conflicting_trades();
        assert!(detect_nonce_conflict(&[a.clone(), a]).is_none());
    }

    #[test]
    fn detect_nonce_conflict_ignores_v1_records() {
        // Two v1 trades (zero nonce) from the same provider are legitimate
        // back-to-back trades (no dedup promised).
        let mut rng = OsRng;
        let p = SigningKey::generate(&mut rng);
        let c = SigningKey::generate(&mut rng);
        let make = |amt: u64| {
            let trade = TradeRecord {
                provider: NodeId(p.verifying_key().to_bytes()),
                consumer: NodeId(c.verifying_key().to_bytes()),
                trm_amount: amt,
                tokens_processed: 10,
                timestamp: 1_000,
                model_id: "m".into(),
                flops_estimated: 0,
                nonce: [0u8; 16],
            };
            sign_with(&p, &c, trade)
        };
        assert!(detect_nonce_conflict(&[make(100), make(200)]).is_none());
    }

    #[test]
    fn detect_nonce_conflict_empty_batch_is_none() {
        assert!(detect_nonce_conflict(&[]).is_none());
    }

    #[test]
    fn fraud_proof_equality_is_symmetric_over_record_order() {
        let (a, b) = two_conflicting_trades();
        let p1 = NonceFraudProof {
            record_a: a.clone(),
            record_b: b.clone(),
        };
        let p2 = NonceFraudProof {
            record_a: b,
            record_b: a,
        };
        assert_eq!(p1, p2);
    }
}
