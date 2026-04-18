//! Governance module: stake-weighted voting on protocol parameters.
//!
//! Spec reference: forge-economics/spec/parameters.md §19
//! - Governance epochs sync with halving epochs.
//! - Minimum reputation 0.7 and minimum stake 1,000 TRM to vote.
//! - Seniority multiplier: 1.0 (0 epochs), 1.5 (1-2 epochs), 2.0 (3+ epochs).
//! - Vote weight = stake * seniority_multiplier.
//! - Proposals pass with >50% weighted approval.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tirami_core::NodeId;

// ---------- Phase 18.1: Tirami Constitution — immutable parameters ----------

/// Phase 18.1 — The list of `ChangeParameter.name` values that
/// governance IS ALLOWED to modify. Anything not on this list is
/// a Constitutional parameter and proposals to change it are
/// rejected at `create_proposal` time.
///
/// Rationale: Bitcoin's 21M supply cap is credible because it is
/// *mathematically* unchangeable (any hard-fork that raises it
/// produces a different coin). Tirami cannot achieve that purely
/// in code because the runtime is upgradeable; what we CAN do
/// is make the governance system refuse to even record an intent
/// to change these parameters, so altering them requires either
/// (a) a hostile software fork that credibly signals "this is no
/// longer Tirami", or (b) a constitutional amendment process
/// that goes through this whitelist itself (meta-change:
/// `CONSTITUTIONAL_AMENDMENT`).
///
/// The whitelist is intentionally short. Each entry has a
/// justification — we've examined the parameter and decided that
/// operational tuning is more valuable than strict immutability.
/// See `docs/constitution.md` for the full justification ledger.
///
/// Adding a new entry here is a constitutional amendment and
/// requires a PR that updates both this array and the Constitution
/// doc.
pub const MUTABLE_GOVERNANCE_PARAMETERS: &[&str] = &[
    // Lending parameters — circuit-breaker tuning is operational.
    "WELCOME_LOAN_AMOUNT",
    "MAX_LTV_RATIO",
    "MIN_RESERVE_RATIO",
    "DEFAULT_RATE_THRESHOLD",
    "VELOCITY_LIMIT_LOANS_PER_MINUTE",
    "MIN_CREDIT_FOR_BORROWING",
    // Market pricing — tuning for supply/demand dynamics.
    "BASE_TRM_PER_TOKEN",
    "TIER_SMALL_CU_PER_TOKEN",
    "TIER_FRONTIER_CU_PER_TOKEN",
    // Sybil / rate-limit knobs — operational DDoS response.
    "WELCOME_LOAN_SYBIL_THRESHOLD",
    "WELCOME_LOAN_PER_BUCKET_CAP",
    "ASN_RATE_LIMIT_PER_SEC",
    "MAX_CONCURRENT_CONNECTIONS",
    // Audit tuning — operational policy, not protocol invariant.
    "AUDIT_SAMPLE_RATE",
    "AUDIT_VALIDATOR_COUNT",
    "HEAVY_AUDIT_SAMPLE_RATE",
    // Reputation / staking bonus curves.
    "STAKE_DURATION_7D_MULTIPLIER",
    "STAKE_DURATION_30D_MULTIPLIER",
    "STAKE_DURATION_90D_MULTIPLIER",
    // Anchor timing — operational cost trade-off.
    "ANCHOR_INTERVAL_SECS",
    "CHECKPOINT_INTERVAL_SECS",
    "CHECKPOINT_RETAIN_SECS",
    "SLASHING_INTERVAL_SECS",
    // Phase 18.2 — stake-required mining tuning.
    // MIN_PROVIDER_STAKE_TRM is mutable ABOVE the constitutional
    // floor (MIN_PROVIDER_STAKE_CONSTITUTIONAL_FLOOR). Governance
    // can raise but not lower below the floor.
    "MIN_PROVIDER_STAKE_TRM",
    "STAKELESS_EARN_CAP_TRM",
    // Phase 18.3 — zkML rollout gate. Mutable UPWARD only
    // (Disabled → Optional → Recommended → Required).
    // The no-downgrade invariant is enforced at execution time
    // in the governance dispatcher (`try_apply_proof_policy`),
    // NOT in the mutable whitelist — whitelisting here means the
    // policy CAN be proposed for change, the downgrade ratchet
    // rejects only the downward direction.
    "PROOF_POLICY",
];

/// Phase 18.1 — parameters explicitly called out as immutable.
/// This list is INFORMATIONAL; the actual enforcement comes from
/// `MUTABLE_GOVERNANCE_PARAMETERS` being a strict whitelist.
/// Having this explicit list serves two purposes:
///   1. Makes the Constitution legible to readers.
///   2. Lets tests assert that no naming collision accidentally
///      whitelisted a Constitutional parameter.
pub const IMMUTABLE_CONSTITUTIONAL_PARAMETERS: &[&str] = &[
    // --- Economic foundation ---
    "TOTAL_TRM_SUPPLY",                // 21 B cap
    "HALVING_EPOCH_FUNCTION",          // 50%/75%/87.5% curve
    "INITIAL_YIELD_RATE",              // 0.1%/hr base yield
    "FLOPS_PER_CU",                    // 10^9 FLOP = 1 TRM (Principle 1)
    // --- Slashing rates (safety floor) ---
    "SLASH_RATE_MINOR",                // 5%
    "SLASH_RATE_MAJOR",                // 20%
    "SLASH_RATE_CRITICAL",             // 50%
    "AUDIT_FAIL_TRUST_PENALTY",        // 0.3 (major)
    // --- Cryptographic invariants ---
    "ED25519_SIGNATURE_REQUIRED",
    "NONCE_REPLAY_DEFENSE_ENABLED",
    "DUAL_SIGNATURE_REQUIRED",
    "CANONICAL_BYTES_V1_FORMAT",
    "CANONICAL_BYTES_V2_FORMAT",
    // --- Trust / identity invariants ---
    "DEFAULT_REPUTATION",              // 0.5 starting point
    "COLD_START_CREDIT",               // 0.3 welcome credit
    "COLLATERAL_BURN_ON_DEFAULT",      // 1.0 full burn
    // --- Governance meta ---
    "GOVERNANCE_MIN_REPUTATION",       // 0.7 threshold
    "GOVERNANCE_MIN_STAKE",            // 1000 TRM
    "GOVERNANCE_WHITELIST_CONTENTS",   // this very list
    // --- Phase 18.2: Stake-required mining invariants ---
    "WELCOME_LOAN_SUNSET_EPOCH",                    // one-way closure
    "MIN_PROVIDER_STAKE_CONSTITUTIONAL_FLOOR",      // 10 TRM floor
    "STAKELESS_EARN_CAP_MAXIMUM",                   // absolute ceiling on faucet
    // --- Phase 18.3: zkML rollout invariants ---
    "PROOF_POLICY_RATCHET",                         // no-downgrade invariant
];

/// True iff `name` is a parameter governance is allowed to change.
pub fn is_mutable_parameter(name: &str) -> bool {
    MUTABLE_GOVERNANCE_PARAMETERS.contains(&name)
}

/// True iff `name` is an explicitly-Constitutional parameter.
/// Note: this is strictly informational; the runtime check is
/// `is_mutable_parameter(name)` returning `false`.
pub fn is_constitutional_parameter(name: &str) -> bool {
    IMMUTABLE_CONSTITUTIONAL_PARAMETERS.contains(&name)
}

// ---------- Constants from parameters.md §19 ----------

/// Governance epochs sync with halving epochs.
pub const GOVERNANCE_EPOCH_SYNC: &str = "halving";

/// Minimum reputation score required to cast a vote.
pub const GOVERNANCE_MIN_REPUTATION: f64 = 0.7;

/// Minimum TRM stake required to cast a vote.
pub const GOVERNANCE_MIN_STAKE: u64 = 1_000;

/// Seniority bonus for 1-2 epochs of participation.
pub const SENIORITY_1_EPOCH_BONUS: f64 = 1.5;

/// Seniority bonus for 3+ epochs of participation.
pub const SENIORITY_3_EPOCH_BONUS: f64 = 2.0;

// ---------- Types ----------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProposalKind {
    ChangeParameter { name: String, new_value: f64 },
    EmergencyPause,
    ProtocolUpgrade { description: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalStatus {
    Active,
    Passed,
    Rejected,
    Executed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: u64,
    pub proposer: NodeId,
    pub kind: ProposalKind,
    pub epoch: u64,
    pub created_at_ms: u64,
    pub deadline_ms: u64,
    pub status: ProposalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub voter: NodeId,
    pub proposal_id: u64,
    pub approve: bool,
    pub stake_weight: f64,
    pub seniority_multiplier: f64,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceState {
    pub proposals: Vec<Proposal>,
    pub votes: HashMap<u64, Vec<Vote>>,
    pub next_proposal_id: u64,
    pub current_epoch: u64,
}

// ---------- Errors ----------

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum GovernanceError {
    #[error("proposal not found: {id}")]
    ProposalNotFound { id: u64 },
    #[error("voter has already voted on proposal {proposal_id}")]
    AlreadyVoted { proposal_id: u64 },
    #[error("insufficient reputation: {reputation} < {minimum}")]
    InsufficientReputation { reputation: f64, minimum: f64 },
    #[error("insufficient stake: {stake} < {minimum}")]
    InsufficientStake { stake: u64, minimum: u64 },
    #[error("proposal {id} has expired")]
    ProposalExpired { id: u64 },
    #[error("proposal {id} is not active")]
    ProposalNotActive { id: u64 },
    /// Phase 18.1 — governance proposed a change to a parameter
    /// that the Tirami Constitution does not allow. The full list
    /// of changeable parameters is `MUTABLE_GOVERNANCE_PARAMETERS`;
    /// Constitutional parameters (`TOTAL_TRM_SUPPLY`,
    /// `SLASH_RATE_*`, `FLOPS_PER_CU`, etc.) are unchangeable.
    #[error(
        "parameter '{name}' is Constitutional and cannot be changed by governance. \
         See docs/constitution.md"
    )]
    ConstitutionalParameter { name: String },
}

// ---------- Seniority ----------

/// Compute seniority multiplier from the number of epochs a node has participated in.
pub fn seniority_multiplier(epochs_participated: u64) -> f64 {
    if epochs_participated >= 3 {
        SENIORITY_3_EPOCH_BONUS
    } else if epochs_participated >= 1 {
        SENIORITY_1_EPOCH_BONUS
    } else {
        1.0
    }
}

// ---------- GovernanceState ----------

impl GovernanceState {
    pub fn new(epoch: u64) -> Self {
        Self {
            proposals: Vec::new(),
            votes: HashMap::new(),
            next_proposal_id: 1,
            current_epoch: epoch,
        }
    }

    /// Create a new proposal. Returns the proposal ID.
    ///
    /// Phase 18.1 — `ChangeParameter` proposals are validated against
    /// `MUTABLE_GOVERNANCE_PARAMETERS`. Attempts to change a
    /// Constitutional parameter return
    /// [`GovernanceError::ConstitutionalParameter`] and the proposal
    /// is NOT recorded (it never reaches `Active` state; voters are
    /// never even offered the option).
    pub fn create_proposal(
        &mut self,
        proposer: NodeId,
        kind: ProposalKind,
        now_ms: u64,
        deadline_ms: u64,
    ) -> Result<u64, GovernanceError> {
        // Phase 18.1 — Constitutional check.
        if let ProposalKind::ChangeParameter { name, .. } = &kind {
            if !is_mutable_parameter(name) {
                return Err(GovernanceError::ConstitutionalParameter {
                    name: name.clone(),
                });
            }
        }
        let id = self.next_proposal_id;
        self.next_proposal_id += 1;
        self.proposals.push(Proposal {
            id,
            proposer,
            kind,
            epoch: self.current_epoch,
            created_at_ms: now_ms,
            deadline_ms,
            status: ProposalStatus::Active,
        });
        self.votes.insert(id, Vec::new());
        Ok(id)
    }

    /// Cast a vote on a proposal.
    ///
    /// Validates minimum reputation, minimum stake, and duplicate-vote prevention.
    /// Calculates the seniority multiplier from `epochs_participated`.
    pub fn cast_vote(
        &mut self,
        voter: NodeId,
        proposal_id: u64,
        approve: bool,
        stake: u64,
        reputation: f64,
        epochs_participated: u64,
    ) -> Result<(), GovernanceError> {
        // Find the proposal
        let proposal = self
            .proposals
            .iter()
            .find(|p| p.id == proposal_id)
            .ok_or(GovernanceError::ProposalNotFound { id: proposal_id })?;

        // Must be active
        if proposal.status != ProposalStatus::Active {
            return Err(GovernanceError::ProposalNotActive { id: proposal_id });
        }

        // Check reputation
        if reputation < GOVERNANCE_MIN_REPUTATION {
            return Err(GovernanceError::InsufficientReputation {
                reputation,
                minimum: GOVERNANCE_MIN_REPUTATION,
            });
        }

        // Check stake
        if stake < GOVERNANCE_MIN_STAKE {
            return Err(GovernanceError::InsufficientStake {
                stake,
                minimum: GOVERNANCE_MIN_STAKE,
            });
        }

        // Check duplicate vote
        let votes = self.votes.entry(proposal_id).or_default();
        if votes.iter().any(|v| v.voter == voter) {
            return Err(GovernanceError::AlreadyVoted { proposal_id });
        }

        let multiplier = seniority_multiplier(epochs_participated);

        votes.push(Vote {
            voter,
            proposal_id,
            approve,
            stake_weight: stake as f64,
            seniority_multiplier: multiplier,
            timestamp_ms: 0, // caller can set if needed
        });

        Ok(())
    }

    /// Tally votes for a proposal. Returns the resulting status.
    ///
    /// Vote weight = stake_weight * seniority_multiplier.
    /// >50% weighted approval => Passed, otherwise Rejected.
    pub fn tally(&mut self, proposal_id: u64) -> Result<ProposalStatus, GovernanceError> {
        let proposal = self
            .proposals
            .iter()
            .find(|p| p.id == proposal_id)
            .ok_or(GovernanceError::ProposalNotFound { id: proposal_id })?;

        if proposal.status != ProposalStatus::Active {
            return Err(GovernanceError::ProposalNotActive { id: proposal_id });
        }

        let votes = self.votes.get(&proposal_id).cloned().unwrap_or_default();

        let mut approve_weight = 0.0_f64;
        let mut reject_weight = 0.0_f64;

        for vote in &votes {
            let w = vote.stake_weight * vote.seniority_multiplier;
            if vote.approve {
                approve_weight += w;
            } else {
                reject_weight += w;
            }
        }

        let total = approve_weight + reject_weight;
        let status = if total > 0.0 && approve_weight / total > 0.5 {
            ProposalStatus::Passed
        } else {
            ProposalStatus::Rejected
        };

        // Update proposal status
        if let Some(p) = self.proposals.iter_mut().find(|p| p.id == proposal_id) {
            p.status = status;
        }

        Ok(status)
    }

    /// Return references to all active proposals.
    pub fn active_proposals(&self) -> Vec<&Proposal> {
        self.proposals
            .iter()
            .filter(|p| p.status == ProposalStatus::Active)
            .collect()
    }

    /// Advance to a new epoch: close and tally any proposals whose deadline has passed.
    pub fn advance_epoch(&mut self, new_epoch: u64) {
        self.current_epoch = new_epoch;

        // Collect IDs of expired-but-still-active proposals.
        // We use epoch comparison: any active proposal from a previous epoch is expired.
        let expired_ids: Vec<u64> = self
            .proposals
            .iter()
            .filter(|p| p.status == ProposalStatus::Active && p.epoch < new_epoch)
            .map(|p| p.id)
            .collect();

        for id in expired_ids {
            // tally will set it to Passed or Rejected
            let _ = self.tally(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(seed: u8) -> NodeId {
        NodeId([seed; 32])
    }

    const NOW: u64 = 1_000_000_000;
    const DEADLINE: u64 = 2_000_000_000;

    // --- Proposal creation ---

    #[test]
    fn test_create_proposal_returns_id() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(
                node(1),
                ProposalKind::EmergencyPause,
                NOW,
                DEADLINE,
            )
            .unwrap();
        assert_eq!(id, 1);
        assert_eq!(gov.proposals.len(), 1);
        assert_eq!(gov.proposals[0].status, ProposalStatus::Active);
        assert_eq!(gov.proposals[0].epoch, 1);
    }

    #[test]
    fn test_create_proposal_increments_id() {
        let mut gov = GovernanceState::new(1);
        let id1 = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        let id2 = gov
            .create_proposal(
                node(2),
                // Phase 18.1: must be a whitelisted mutable parameter.
                ProposalKind::ChangeParameter {
                    name: "WELCOME_LOAN_AMOUNT".into(),
                    new_value: 500.0,
                },
                NOW,
                DEADLINE,
            )
            .unwrap();
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    // --- Voting ---

    #[test]
    fn test_cast_vote_success() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        let result = gov.cast_vote(node(2), id, true, 5_000, 0.9, 2);
        assert!(result.is_ok());
        assert_eq!(gov.votes[&id].len(), 1);
        assert_eq!(gov.votes[&id][0].seniority_multiplier, 1.5);
    }

    #[test]
    fn test_reject_vote_insufficient_reputation() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        let err = gov.cast_vote(node(2), id, true, 5_000, 0.5, 0).unwrap_err();
        assert_eq!(
            err,
            GovernanceError::InsufficientReputation {
                reputation: 0.5,
                minimum: GOVERNANCE_MIN_REPUTATION,
            }
        );
    }

    #[test]
    fn test_reject_vote_insufficient_stake() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        let err = gov.cast_vote(node(2), id, true, 500, 0.9, 0).unwrap_err();
        assert_eq!(
            err,
            GovernanceError::InsufficientStake {
                stake: 500,
                minimum: GOVERNANCE_MIN_STAKE,
            }
        );
    }

    #[test]
    fn test_reject_duplicate_vote() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        gov.cast_vote(node(2), id, true, 5_000, 0.9, 0).unwrap();
        let err = gov.cast_vote(node(2), id, false, 5_000, 0.9, 0).unwrap_err();
        assert_eq!(err, GovernanceError::AlreadyVoted { proposal_id: id });
    }

    // --- Tally ---

    #[test]
    fn test_tally_single_approve() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        gov.cast_vote(node(2), id, true, 5_000, 0.9, 0).unwrap();
        let status = gov.tally(id).unwrap();
        assert_eq!(status, ProposalStatus::Passed);
    }

    #[test]
    fn test_tally_single_reject() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        gov.cast_vote(node(2), id, false, 5_000, 0.9, 0).unwrap();
        let status = gov.tally(id).unwrap();
        assert_eq!(status, ProposalStatus::Rejected);
    }

    #[test]
    fn test_tally_mixed_votes_weight_based() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        // Voter A: 10,000 stake, 3 epochs (2.0x) => weight 20,000, approve
        gov.cast_vote(node(2), id, true, 10_000, 0.9, 3).unwrap();
        // Voter B: 5,000 stake, 0 epochs (1.0x) => weight 5,000, reject
        gov.cast_vote(node(3), id, false, 5_000, 0.8, 0).unwrap();
        // Total: 20,000 approve vs 5,000 reject => 80% approve => Passed
        let status = gov.tally(id).unwrap();
        assert_eq!(status, ProposalStatus::Passed);
    }

    #[test]
    fn test_tally_mixed_votes_reject_wins() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        // Voter A: 2,000 stake, 0 epochs (1.0x) => weight 2,000, approve
        gov.cast_vote(node(2), id, true, 2_000, 0.9, 0).unwrap();
        // Voter B: 10,000 stake, 3 epochs (2.0x) => weight 20,000, reject
        gov.cast_vote(node(3), id, false, 10_000, 0.8, 3).unwrap();
        let status = gov.tally(id).unwrap();
        assert_eq!(status, ProposalStatus::Rejected);
    }

    #[test]
    fn test_tally_no_votes_rejects() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        // No votes cast — total is 0, so it rejects
        let status = gov.tally(id).unwrap();
        assert_eq!(status, ProposalStatus::Rejected);
    }

    // --- Seniority ---

    #[test]
    fn test_seniority_0_epochs() {
        assert_eq!(seniority_multiplier(0), 1.0);
    }

    #[test]
    fn test_seniority_1_epoch() {
        assert_eq!(seniority_multiplier(1), SENIORITY_1_EPOCH_BONUS);
    }

    #[test]
    fn test_seniority_2_epochs() {
        assert_eq!(seniority_multiplier(2), SENIORITY_1_EPOCH_BONUS);
    }

    #[test]
    fn test_seniority_3_plus_epochs() {
        assert_eq!(seniority_multiplier(3), SENIORITY_3_EPOCH_BONUS);
        assert_eq!(seniority_multiplier(10), SENIORITY_3_EPOCH_BONUS);
    }

    // --- Active proposals ---

    #[test]
    fn test_active_proposals_listing() {
        let mut gov = GovernanceState::new(1);
        let id1 = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        let _id2 = gov
            .create_proposal(
                node(2),
                ProposalKind::ProtocolUpgrade {
                    description: "v2".into(),
                },
                NOW,
                DEADLINE,
            )
            .unwrap();

        assert_eq!(gov.active_proposals().len(), 2);

        // Tally the first one (no votes => Rejected)
        gov.tally(id1).unwrap();
        assert_eq!(gov.active_proposals().len(), 1);
    }

    // --- Advance epoch ---

    #[test]
    fn test_advance_epoch_closes_expired_proposals() {
        let mut gov = GovernanceState::new(1);
        // Proposal in epoch 1
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        gov.cast_vote(node(2), id, true, 5_000, 0.9, 0).unwrap();

        // Advance to epoch 2 — proposal from epoch 1 should be tallied
        gov.advance_epoch(2);
        assert_eq!(gov.current_epoch, 2);
        let proposal = gov.proposals.iter().find(|p| p.id == id).unwrap();
        assert_eq!(proposal.status, ProposalStatus::Passed);
    }

    #[test]
    fn test_advance_epoch_keeps_current_epoch_proposals_active() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        // Advance within the same epoch (1 -> 1) — nothing should close
        gov.advance_epoch(1);
        let proposal = gov.proposals.iter().find(|p| p.id == id).unwrap();
        assert_eq!(proposal.status, ProposalStatus::Active);
    }

    // --- Vote on non-active proposal ---

    #[test]
    fn test_vote_on_non_active_proposal_fails() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        // Tally immediately (no votes => Rejected)
        gov.tally(id).unwrap();

        let err = gov.cast_vote(node(2), id, true, 5_000, 0.9, 0).unwrap_err();
        assert_eq!(err, GovernanceError::ProposalNotActive { id });
    }

    // --- Multiple concurrent proposals ---

    #[test]
    fn test_multiple_concurrent_proposals() {
        let mut gov = GovernanceState::new(1);
        let id1 = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        let id2 = gov
            .create_proposal(
                node(2),
                // Phase 18.1: whitelisted mutable parameter.
                ProposalKind::ChangeParameter {
                    name: "ANCHOR_INTERVAL_SECS".into(),
                    new_value: 900.0,
                },
                NOW,
                DEADLINE,
            )
            .unwrap();

        // Vote on both
        gov.cast_vote(node(3), id1, true, 5_000, 0.9, 0).unwrap();
        gov.cast_vote(node(3), id2, false, 5_000, 0.9, 0).unwrap();

        let s1 = gov.tally(id1).unwrap();
        let s2 = gov.tally(id2).unwrap();
        assert_eq!(s1, ProposalStatus::Passed);
        assert_eq!(s2, ProposalStatus::Rejected);
    }

    // --- ProposalNotFound ---

    #[test]
    fn test_proposal_not_found() {
        let mut gov = GovernanceState::new(1);
        let err = gov.cast_vote(node(1), 999, true, 5_000, 0.9, 0).unwrap_err();
        assert_eq!(err, GovernanceError::ProposalNotFound { id: 999 });
    }

    #[test]
    fn test_tally_proposal_not_found() {
        let mut gov = GovernanceState::new(1);
        let err = gov.tally(999).unwrap_err();
        assert_eq!(err, GovernanceError::ProposalNotFound { id: 999 });
    }

    // --- Edge cases ---

    #[test]
    fn test_exact_boundary_reputation() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        // Exactly 0.7 should be accepted
        let result = gov.cast_vote(node(2), id, true, 1_000, 0.7, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_exact_boundary_stake() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        // Exactly 1,000 should be accepted
        let result = gov.cast_vote(node(2), id, true, 1_000, 0.9, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_vote_weight_calculation() {
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
        gov.cast_vote(node(2), id, true, 5_000, 0.9, 2).unwrap();
        let vote = &gov.votes[&id][0];
        // stake_weight = 5000, seniority = 1.5 for 2 epochs
        assert_eq!(vote.stake_weight, 5_000.0);
        assert_eq!(vote.seniority_multiplier, 1.5);
        // effective weight = 5000 * 1.5 = 7500
        assert_eq!(vote.stake_weight * vote.seniority_multiplier, 7_500.0);
    }

    // -----------------------------------------------------------------
    // Phase 18.1 — Tirami Constitution invariants
    //
    // These tests enforce that governance can modify ONLY the
    // parameters whitelisted in `MUTABLE_GOVERNANCE_PARAMETERS`.
    // Every other parameter — notably `TOTAL_TRM_SUPPLY`,
    // `FLOPS_PER_CU`, slash rates, signature-required invariants —
    // is Constitutional and governance proposals to change it are
    // rejected at `create_proposal` time.
    // -----------------------------------------------------------------

    #[test]
    fn constitutional_total_trm_supply_change_rejected() {
        // The 21 B cap is the most important number in the system.
        // Governance MUST NOT be able to even *record* an intent
        // to change it.
        let mut gov = GovernanceState::new(1);
        let err = gov
            .create_proposal(
                node(1),
                ProposalKind::ChangeParameter {
                    name: "TOTAL_TRM_SUPPLY".to_string(),
                    new_value: 210_000_000_000.0,
                },
                NOW,
                DEADLINE,
            )
            .unwrap_err();
        assert_eq!(
            err,
            GovernanceError::ConstitutionalParameter {
                name: "TOTAL_TRM_SUPPLY".into()
            }
        );
        // Proposal not recorded.
        assert!(gov.proposals.is_empty());
    }

    #[test]
    fn constitutional_flops_per_cu_change_rejected() {
        // The "1 TRM = 10^9 FLOP" relation is Principle 1 of the
        // Tirami economy. Changing it would break the FLOP-backed
        // scarcity claim entirely.
        let mut gov = GovernanceState::new(1);
        let err = gov
            .create_proposal(
                node(1),
                ProposalKind::ChangeParameter {
                    name: "FLOPS_PER_CU".to_string(),
                    new_value: 1_000_000.0, // 1e6 instead of 1e9 — would inflate
                },
                NOW,
                DEADLINE,
            )
            .unwrap_err();
        assert!(matches!(err, GovernanceError::ConstitutionalParameter { .. }));
    }

    #[test]
    fn constitutional_slash_rates_change_rejected() {
        let mut gov = GovernanceState::new(1);
        for name in ["SLASH_RATE_MINOR", "SLASH_RATE_MAJOR", "SLASH_RATE_CRITICAL"] {
            let err = gov
                .create_proposal(
                    node(1),
                    ProposalKind::ChangeParameter {
                        name: name.to_string(),
                        new_value: 0.0, // disable slashing entirely
                    },
                    NOW,
                    DEADLINE,
                )
                .unwrap_err();
            assert!(
                matches!(err, GovernanceError::ConstitutionalParameter { .. }),
                "expected ConstitutionalParameter for {}, got {:?}",
                name,
                err
            );
        }
    }

    #[test]
    fn constitutional_canonical_bytes_format_change_rejected() {
        // If governance could change the signature canonical byte
        // layout, every historical signature would retroactively
        // fail verification. Absolutely not.
        let mut gov = GovernanceState::new(1);
        let err = gov
            .create_proposal(
                node(1),
                ProposalKind::ChangeParameter {
                    name: "CANONICAL_BYTES_V1_FORMAT".to_string(),
                    new_value: 0.0,
                },
                NOW,
                DEADLINE,
            )
            .unwrap_err();
        assert!(matches!(err, GovernanceError::ConstitutionalParameter { .. }));
    }

    #[test]
    fn mutable_welcome_loan_amount_change_accepted() {
        // Bootstrap tuning MUST stay adjustable — this is
        // operational, not Constitutional.
        let mut gov = GovernanceState::new(1);
        let id = gov
            .create_proposal(
                node(1),
                ProposalKind::ChangeParameter {
                    name: "WELCOME_LOAN_AMOUNT".to_string(),
                    new_value: 500.0,
                },
                NOW,
                DEADLINE,
            )
            .unwrap();
        assert_eq!(id, 1);
        assert_eq!(gov.proposals.len(), 1);
    }

    #[test]
    fn mutable_max_ltv_ratio_change_accepted() {
        let mut gov = GovernanceState::new(1);
        gov.create_proposal(
            node(1),
            ProposalKind::ChangeParameter {
                name: "MAX_LTV_RATIO".to_string(),
                new_value: 2.5, // tighten from 3.0 → 2.5
            },
            NOW,
            DEADLINE,
        )
        .unwrap();
    }

    #[test]
    fn emergency_pause_always_allowed() {
        // Emergency pause is NOT a parameter change; the
        // Constitutional check must never gate it.
        let mut gov = GovernanceState::new(1);
        gov.create_proposal(node(1), ProposalKind::EmergencyPause, NOW, DEADLINE)
            .unwrap();
    }

    #[test]
    fn protocol_upgrade_always_allowed() {
        // `ProtocolUpgrade` is a coordination signal for a
        // software migration. It does not itself alter a
        // parameter — operators decide whether to adopt the new
        // software. Must not be gated.
        let mut gov = GovernanceState::new(1);
        gov.create_proposal(
            node(1),
            ProposalKind::ProtocolUpgrade {
                description: "Phase 20 migration".into(),
            },
            NOW,
            DEADLINE,
        )
        .unwrap();
    }

    #[test]
    fn unknown_parameter_name_is_rejected() {
        // Any name not on the whitelist is Constitutional by
        // default. This closes the "just make up a new name" bypass.
        let mut gov = GovernanceState::new(1);
        let err = gov
            .create_proposal(
                node(1),
                ProposalKind::ChangeParameter {
                    name: "TOTALLY_MADE_UP_PARAMETER".to_string(),
                    new_value: 1.0,
                },
                NOW,
                DEADLINE,
            )
            .unwrap_err();
        assert!(matches!(err, GovernanceError::ConstitutionalParameter { .. }));
    }

    #[test]
    fn mutable_and_immutable_lists_are_disjoint() {
        // No entry should appear in both lists. Overlap would be a
        // bug: the "immutable" list is informational, and if an
        // entry ALSO appears in the mutable whitelist it would
        // silently become mutable in practice.
        for mutable in MUTABLE_GOVERNANCE_PARAMETERS {
            assert!(
                !IMMUTABLE_CONSTITUTIONAL_PARAMETERS.contains(mutable),
                "{} appears in both lists — Constitutional violation",
                mutable
            );
        }
    }

    #[test]
    fn immutable_list_has_core_principles() {
        // Regression guard — if someone accidentally removes a
        // Constitutional parameter from the immutable list, this
        // test catches it. The list itself is the Constitution.
        for required in [
            "TOTAL_TRM_SUPPLY",
            "FLOPS_PER_CU",
            "SLASH_RATE_MINOR",
            "SLASH_RATE_MAJOR",
            "SLASH_RATE_CRITICAL",
            "DUAL_SIGNATURE_REQUIRED",
            "NONCE_REPLAY_DEFENSE_ENABLED",
            "GOVERNANCE_WHITELIST_CONTENTS",
        ] {
            assert!(
                IMMUTABLE_CONSTITUTIONAL_PARAMETERS.contains(&required),
                "Constitutional parameter {} missing from immutable list",
                required
            );
        }
    }

    #[test]
    fn is_mutable_parameter_helpers_match_list() {
        assert!(is_mutable_parameter("WELCOME_LOAN_AMOUNT"));
        assert!(!is_mutable_parameter("TOTAL_TRM_SUPPLY"));
        assert!(!is_mutable_parameter("NOT_ANY_PARAMETER"));

        assert!(is_constitutional_parameter("TOTAL_TRM_SUPPLY"));
        assert!(is_constitutional_parameter("FLOPS_PER_CU"));
        assert!(!is_constitutional_parameter("WELCOME_LOAN_AMOUNT"));
    }
}
