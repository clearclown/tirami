//! Phase 12 Research: BitVM optimistic verification scaffold
//!
//! See docs/bitvm-design.md for the full motivation and architecture.
//!
//! BitVM-style fraud proofs allow Forge's CU claims to be anchored on Bitcoin
//! with genuine dispute resolution: if a staker publishes a false ledger state,
//! any observer can post a FraudProof and slash the stake, without a trusted
//! arbitrator and without any smart-contract chain.
//!
//! This module provides types and trait definitions only. Phase 13+ will plug
//! in the real Bitcoin covenant and verification logic.

use tirami_core::NodeId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── StakedClaim ─────────────────────────────────────────────────────────────

/// An assertion that the Forge trade ledger is in a specific state at a
/// specific Bitcoin block height. Published on-chain (anchored via
/// `anchor::build_anchor_tx_skeleton`) and subject to challenge during the
/// dispute window.
///
/// A StakedClaim pairs a Merkle-root commitment (already provided by
/// `tirami_ledger::anchor`) with an economic stake: the staker loses
/// `stake_sats` if any observer successfully posts a FraudProof during the
/// challenge window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StakedClaim {
    /// Who is staking this claim.
    pub staker: NodeId,
    /// Trade log Merkle root being asserted.
    pub merkle_root: [u8; 32],
    /// Bitcoin block height this claim is anchored at.
    pub bitcoin_height: u64,
    /// Stake amount in satoshis. Lost if a valid FraudProof is posted.
    pub stake_sats: u64,
    /// Challenge window in Bitcoin blocks. Default: 2016 (~14 days).
    pub challenge_window_blocks: u64,
    /// Timestamp of stake creation (ms since epoch).
    pub created_at_ms: u64,
}

impl StakedClaim {
    /// Default challenge window: 2016 Bitcoin blocks ≈ 14 days at 10 min/block.
    pub const DEFAULT_CHALLENGE_WINDOW: u64 = 2016;

    /// Minimum economically meaningful stake: 100,000 sats (0.001 BTC).
    /// Below this, an attacker could profit by false-claiming.
    pub const MIN_STAKE_SATS: u64 = 100_000;

    pub fn new(
        staker: NodeId,
        merkle_root: [u8; 32],
        bitcoin_height: u64,
        stake_sats: u64,
        now_ms: u64,
    ) -> Result<Self, BitVmError> {
        if stake_sats < Self::MIN_STAKE_SATS {
            return Err(BitVmError::InsufficientStake {
                provided: stake_sats,
                required: Self::MIN_STAKE_SATS,
            });
        }
        Ok(Self {
            staker,
            merkle_root,
            bitcoin_height,
            stake_sats,
            challenge_window_blocks: Self::DEFAULT_CHALLENGE_WINDOW,
            created_at_ms: now_ms,
        })
    }

    /// Is this claim still open to challenge at the given Bitcoin height?
    ///
    /// The window is half-open: [bitcoin_height, bitcoin_height +
    /// challenge_window_blocks). Once `current_bitcoin_height` reaches the
    /// upper bound, the claim is considered final.
    pub fn is_challengeable(&self, current_bitcoin_height: u64) -> bool {
        current_bitcoin_height < self.bitcoin_height + self.challenge_window_blocks
    }
}

// ─── FraudProof ──────────────────────────────────────────────────────────────

/// A counter-example proving a StakedClaim is inconsistent with observed
/// evidence. If validated by a FraudProofVerifier, the staker loses their
/// stake.
///
/// The challenger posts this structure on-chain (or to a monitoring relay)
/// during the challenge window. A Bitcoin covenant (Phase 13) enforces the
/// slashing automatically if `FraudProofVerifier::verify` returns `Ok(())`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FraudProof {
    /// The Merkle root of the claim being challenged.
    pub challenged_root: [u8; 32],
    /// The challenger's NodeId.
    pub challenger: NodeId,
    /// Type of fraud being alleged.
    pub fraud_type: FraudType,
    /// Evidence bytes (format depends on fraud_type).
    pub evidence: Vec<u8>,
    /// Timestamp of proof creation (ms since epoch).
    pub created_at_ms: u64,
}

/// The category of inconsistency a FraudProof alleges.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FraudType {
    /// The claimed Merkle root is inconsistent with the inclusion proof for
    /// a specific trade.
    MerkleInclusionMismatch,
    /// A trade record was signed by a NodeId that doesn't match the recorded
    /// signer's public key.
    InvalidSignature,
    /// The same trade_id appears in two different ledger branches (double-spend
    /// of CU).
    DoubleSpend,
    /// A balance update derived from the claimed state doesn't satisfy the
    /// `execute_trade` invariants (e.g., CU conservation law).
    InvalidBalanceUpdate,
}

// ─── FraudProofVerifier ──────────────────────────────────────────────────────

/// Verifies that a FraudProof actually demonstrates fraud against a
/// StakedClaim.
///
/// Returns `Ok(())` if the proof is valid (the claim IS fraudulent).
/// Returns `Err(BitVmError)` if the proof is malformed or doesn't demonstrate
/// fraud.
///
/// Phase 13 will replace MockFraudProofVerifier with a real implementation
/// that validates Bitcoin Script witnesses against the committed Merkle root.
pub trait FraudProofVerifier: Send + Sync {
    /// Human-readable backend name (for logging and API responses).
    fn name(&self) -> &str;

    fn verify(&self, claim: &StakedClaim, proof: &FraudProof) -> Result<(), BitVmError>;
}

// ─── MockFraudProofVerifier ──────────────────────────────────────────────────

/// Mock verifier for testing and development.
///
/// Accepts a fraud proof as valid if and only if:
/// 1. `proof.challenged_root == claim.merkle_root` (targets the right claim)
/// 2. `proof.evidence.len() >= 32` (contains a plausible counter-witness)
/// 3. `proof.evidence[0] != claim.merkle_root[0]` (first byte shows divergence)
///
/// This is intentionally trivial — it exercises the type machinery without
/// implementing real cryptographic verification.
pub struct MockFraudProofVerifier;

impl FraudProofVerifier for MockFraudProofVerifier {
    fn name(&self) -> &str {
        "mock"
    }

    fn verify(&self, claim: &StakedClaim, proof: &FraudProof) -> Result<(), BitVmError> {
        if proof.challenged_root != claim.merkle_root {
            return Err(BitVmError::ProofMismatch);
        }
        if proof.evidence.len() < 32 {
            return Err(BitVmError::MalformedEvidence(
                "evidence must be at least 32 bytes".to_string(),
            ));
        }
        // Mock divergence check: the first byte of evidence must differ from the
        // first byte of the claimed Merkle root.
        if proof.evidence[0] == claim.merkle_root[0] {
            return Err(BitVmError::ProofDoesNotShowFraud);
        }
        Ok(())
    }
}

// ─── BitVmError ──────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq)]
pub enum BitVmError {
    #[error("insufficient stake: {provided} sats < required {required}")]
    InsufficientStake { provided: u64, required: u64 },
    #[error("proof challenged_root does not match claim merkle_root")]
    ProofMismatch,
    #[error("malformed evidence: {0}")]
    MalformedEvidence(String),
    #[error("proof does not demonstrate fraud")]
    ProofDoesNotShowFraud,
    #[error("claim is no longer challengeable (challenge window expired)")]
    ChallengeWindowExpired,
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn node(byte: u8) -> NodeId {
        NodeId([byte; 32])
    }

    fn claim_with_root(root: [u8; 32]) -> StakedClaim {
        StakedClaim::new(node(1), root, 800_000, StakedClaim::MIN_STAKE_SATS, 1000).unwrap()
    }

    // --- StakedClaim construction ---

    #[test]
    fn test_new_claim_rejects_insufficient_stake() {
        let err = StakedClaim::new(node(1), [0u8; 32], 800_000, 99_999, 1000).unwrap_err();
        assert!(matches!(
            err,
            BitVmError::InsufficientStake {
                provided: 99_999,
                required: 100_000
            }
        ));
    }

    #[test]
    fn test_new_claim_accepts_min_stake() {
        let claim =
            StakedClaim::new(node(1), [0u8; 32], 800_000, StakedClaim::MIN_STAKE_SATS, 1000)
                .unwrap();
        assert_eq!(claim.stake_sats, StakedClaim::MIN_STAKE_SATS);
        assert_eq!(claim.challenge_window_blocks, StakedClaim::DEFAULT_CHALLENGE_WINDOW);
        assert_eq!(claim.bitcoin_height, 800_000);
        assert_eq!(claim.created_at_ms, 1000);
    }

    #[test]
    fn test_new_claim_accepts_large_stake() {
        let claim =
            StakedClaim::new(node(2), [0u8; 32], 850_000, 10_000_000, 9999).unwrap();
        assert_eq!(claim.stake_sats, 10_000_000);
    }

    // --- challenge window ---

    #[test]
    fn test_is_challengeable_within_window() {
        let claim = claim_with_root([0u8; 32]);
        // At anchor height: challengeable
        assert!(claim.is_challengeable(800_000));
        // One before the end: still challengeable
        assert!(claim.is_challengeable(800_000 + 2015));
        // At window end: no longer challengeable
        assert!(!claim.is_challengeable(800_000 + 2016));
        // Well after: not challengeable
        assert!(!claim.is_challengeable(810_000));
    }

    // --- MockFraudProofVerifier ---

    #[test]
    fn test_mock_verifier_accepts_valid_proof() {
        let mut root = [0u8; 32];
        root[0] = 0xAA;
        let claim = claim_with_root(root);

        let mut evidence = vec![0u8; 32];
        evidence[0] = 0xBB; // differs from root[0] = 0xAA

        let proof = FraudProof {
            challenged_root: root,
            challenger: node(2),
            fraud_type: FraudType::MerkleInclusionMismatch,
            evidence,
            created_at_ms: 2000,
        };
        assert!(MockFraudProofVerifier.verify(&claim, &proof).is_ok());
    }

    #[test]
    fn test_mock_verifier_rejects_wrong_root() {
        let mut root = [0u8; 32];
        root[0] = 0xAA;
        let claim = claim_with_root(root);

        let mut wrong_root = root;
        wrong_root[1] = 0xFF; // slightly different — claim mismatch

        let proof = FraudProof {
            challenged_root: wrong_root,
            challenger: node(2),
            fraud_type: FraudType::InvalidSignature,
            evidence: vec![0xBB; 32],
            created_at_ms: 2000,
        };
        assert_eq!(
            MockFraudProofVerifier.verify(&claim, &proof),
            Err(BitVmError::ProofMismatch)
        );
    }

    #[test]
    fn test_mock_verifier_rejects_short_evidence() {
        let root = [0xAAu8; 32];
        let claim = claim_with_root(root);

        let proof = FraudProof {
            challenged_root: root,
            challenger: node(2),
            fraud_type: FraudType::DoubleSpend,
            evidence: vec![0xBBu8; 31], // one byte too short
            created_at_ms: 2000,
        };
        assert!(matches!(
            MockFraudProofVerifier.verify(&claim, &proof),
            Err(BitVmError::MalformedEvidence(_))
        ));
    }

    #[test]
    fn test_mock_verifier_rejects_non_fraud_evidence() {
        let mut root = [0u8; 32];
        root[0] = 0xAA;
        let claim = claim_with_root(root);

        let mut evidence = vec![0u8; 32];
        evidence[0] = 0xAA; // same as root[0] → no divergence demonstrated

        let proof = FraudProof {
            challenged_root: root,
            challenger: node(3),
            fraud_type: FraudType::InvalidBalanceUpdate,
            evidence,
            created_at_ms: 3000,
        };
        assert_eq!(
            MockFraudProofVerifier.verify(&claim, &proof),
            Err(BitVmError::ProofDoesNotShowFraud)
        );
    }

    // --- serde round-trip ---

    #[test]
    fn test_all_fraud_types_serialize() {
        for ft in [
            FraudType::MerkleInclusionMismatch,
            FraudType::InvalidSignature,
            FraudType::DoubleSpend,
            FraudType::InvalidBalanceUpdate,
        ] {
            let json = serde_json::to_string(&ft).unwrap();
            let back: FraudType = serde_json::from_str(&json).unwrap();
            assert_eq!(ft, back);
        }
    }

    #[test]
    fn test_staked_claim_serde_round_trip() {
        let claim = claim_with_root([0xDEu8; 32]);
        let json = serde_json::to_string(&claim).unwrap();
        let back: StakedClaim = serde_json::from_str(&json).unwrap();
        assert_eq!(claim, back);
    }

    #[test]
    fn test_fraud_proof_serde_round_trip() {
        let proof = FraudProof {
            challenged_root: [0xABu8; 32],
            challenger: node(5),
            fraud_type: FraudType::DoubleSpend,
            evidence: vec![0u8; 64],
            created_at_ms: 42_000,
        };
        let json = serde_json::to_string(&proof).unwrap();
        let back: FraudProof = serde_json::from_str(&json).unwrap();
        assert_eq!(proof, back);
    }

    // --- error formatting ---

    #[test]
    fn test_error_messages_are_human_readable() {
        let e = BitVmError::InsufficientStake {
            provided: 1000,
            required: 100_000,
        };
        let msg = format!("{e}");
        assert!(msg.contains("1000"));
        assert!(msg.contains("100000"));
    }

    // =========================================================================
    // Security tests: BitVM fraud proof validation
    // =========================================================================

    #[test]
    fn sec_fraud_proof_rejects_wrong_merkle_root() {
        // FraudProof targets root A, but claim has root B → ProofMismatch.
        let root_a = [0xAAu8; 32];
        let mut root_b = root_a;
        root_b[0] = 0xBB; // root B differs by one byte

        // Claim is for root_a.
        let claim = claim_with_root(root_a);

        // Proof targets root_b (wrong claim).
        let proof = FraudProof {
            challenged_root: root_b, // does NOT match claim.merkle_root
            challenger: node(9),
            fraud_type: FraudType::DoubleSpend,
            evidence: vec![0x00u8; 32], // divergent first byte
            created_at_ms: 1000,
        };

        assert_eq!(
            MockFraudProofVerifier.verify(&claim, &proof),
            Err(BitVmError::ProofMismatch),
            "proof targeting wrong root must return ProofMismatch"
        );
    }

    #[test]
    fn sec_staked_claim_expired_window_not_challengeable() {
        // A claim at height 800_000 with default window 2016 becomes final at 802_016.
        let claim = claim_with_root([0xFFu8; 32]);

        // At exactly the window end it is no longer challengeable.
        assert!(
            !claim.is_challengeable(800_000 + 2016),
            "at window end (800_000 + 2016) the claim must not be challengeable"
        );
        // Even further in the future.
        assert!(
            !claim.is_challengeable(802_017),
            "after challenge window expires, claim must not be challengeable"
        );
        // One block before window end — still challengeable.
        assert!(
            claim.is_challengeable(800_000 + 2015),
            "one block before window end must still be challengeable"
        );
    }

    #[test]
    fn sec_fraud_proof_empty_evidence_rejected() {
        // Evidence shorter than 32 bytes must cause MalformedEvidence.
        let root = [0xAAu8; 32];
        let claim = claim_with_root(root);
        let proof = FraudProof {
            challenged_root: root,
            challenger: node(7),
            fraud_type: FraudType::InvalidSignature,
            evidence: vec![], // empty
            created_at_ms: 500,
        };
        assert!(
            matches!(MockFraudProofVerifier.verify(&claim, &proof), Err(BitVmError::MalformedEvidence(_))),
            "empty evidence must return MalformedEvidence"
        );
    }

    #[test]
    fn sec_fraud_proof_single_byte_evidence_rejected() {
        // 1-byte evidence (< 32) must return MalformedEvidence.
        let root = [0xAAu8; 32];
        let claim = claim_with_root(root);
        let proof = FraudProof {
            challenged_root: root,
            challenger: node(7),
            fraud_type: FraudType::MerkleInclusionMismatch,
            evidence: vec![0x01], // 1 byte
            created_at_ms: 500,
        };
        assert!(
            matches!(MockFraudProofVerifier.verify(&claim, &proof), Err(BitVmError::MalformedEvidence(_))),
            "1-byte evidence must return MalformedEvidence"
        );
    }

    #[test]
    fn sec_fraud_proof_non_diverging_evidence_rejected() {
        // Evidence[0] == claim.merkle_root[0] → proof does not demonstrate fraud.
        let root = [0x42u8; 32];
        let claim = claim_with_root(root);

        let mut evidence = vec![0u8; 32];
        evidence[0] = 0x42; // same as root[0]

        let proof = FraudProof {
            challenged_root: root,
            challenger: node(4),
            fraud_type: FraudType::InvalidBalanceUpdate,
            evidence,
            created_at_ms: 1000,
        };
        assert_eq!(
            MockFraudProofVerifier.verify(&claim, &proof),
            Err(BitVmError::ProofDoesNotShowFraud),
            "non-diverging evidence must return ProofDoesNotShowFraud"
        );
    }

    #[test]
    fn sec_staked_claim_below_min_stake_rejected() {
        // Stake of 99_999 sats (below MIN_STAKE_SATS = 100_000) must fail.
        let err = StakedClaim::new(node(1), [0u8; 32], 800_000, 99_999, 1000).unwrap_err();
        assert!(
            matches!(err, BitVmError::InsufficientStake { provided: 99_999, required: 100_000 }),
            "stake below minimum must return InsufficientStake: {err:?}"
        );
    }

    #[test]
    fn sec_staked_claim_zero_stake_rejected() {
        // Zero stake must also be rejected.
        let err = StakedClaim::new(node(1), [0u8; 32], 800_000, 0, 1000).unwrap_err();
        assert!(
            matches!(err, BitVmError::InsufficientStake { provided: 0, .. }),
            "zero stake must return InsufficientStake: {err:?}"
        );
    }
}
