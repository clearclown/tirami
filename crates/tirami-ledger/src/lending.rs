//! Lending parameters and core lending types for Forge CU economy.
//!
//! This module is the **single source of truth** for all numeric constants
//! used in CU lending, credit scoring, multi-model pricing, and circuit
//! breakers. Values are mirrored from `forge-economics/spec/parameters.md`.
//!
//! # Cross-reference
//!
//! Every constant here corresponds to an entry in
//! <https://github.com/clearclown/forge-economics/blob/main/spec/parameters.md>.
//! Values MUST match that file. When changing a value, update both sides.

use serde::{Deserialize, Serialize};

use tirami_core::NodeId;

// ===========================================================================
// §1 — Welcome loan (parameters.md §3)
// ===========================================================================

/// New node welcome loan principal (CU).
pub const WELCOME_LOAN_AMOUNT: u64 = 1_000;
/// Welcome loan interest rate (0%).
pub const WELCOME_LOAN_INTEREST: f64 = 0.0;
/// Welcome loan repayment term (hours).
pub const WELCOME_LOAN_TERM_HOURS: u64 = 72;
/// Sybil threshold: if more than this many unknown nodes exist, reject new welcome loans.
pub const WELCOME_LOAN_SYBIL_THRESHOLD: usize = 100;
/// Credit score bonus awarded when welcome loan is repaid on time.
pub const WELCOME_LOAN_CREDIT_BONUS: f64 = 0.1;

// ===========================================================================
// §1.5 — Phase 18.2: Stake-required mining
// ===========================================================================

/// Phase 18.2 — Minimum TRM stake a provider must hold to receive
/// paid inference requests. Below this threshold, the provider is
/// treated as un-staked and the pipeline refuses to settle trades
/// in their favor. This closes the "free rider" path where a node
/// burns electricity with no skin in the game.
///
/// This is a mutable parameter (operational tuning) but it has a
/// Constitutional floor: the Constitution forbids setting it below
/// `MIN_PROVIDER_STAKE_CONSTITUTIONAL_FLOOR`. Phase 18.1 enshrines
/// the floor in `governance.rs`.
///
/// Initial value: 100 TRM. Chosen so the Phase 5.5 welcome loan
/// (1 000 TRM at 0 %) is enough to cover it 10× over — a new
/// node can stake from their welcome loan and still have 900 TRM
/// of working capital. Once welcome loans sunset (Phase 18.2b),
/// new entrants must earn their first 100 TRM through off-protocol
/// means (exchange purchase, gift from existing staker) OR via
/// the time-limited stakeless earn-cap (see below).
pub const MIN_PROVIDER_STAKE_TRM: u64 = 100;

/// Phase 18.2 — Constitutional floor for `MIN_PROVIDER_STAKE_TRM`.
/// Governance can raise the effective minimum above this but never
/// below it. A value of 0 would revert to "anyone can provide
/// without skin in the game" — the exact Sybil vector we're
/// closing.
pub const MIN_PROVIDER_STAKE_CONSTITUTIONAL_FLOOR: u64 = 10;

/// Phase 18.2 — Stakeless earn cap. A node with zero stake may
/// still earn up to this amount of TRM, after which further
/// inference is refused until they stake. Bootstraps new nodes
/// WITHOUT giving them unbounded free-rider capacity.
///
/// Intuition: the first 10 TRM is the "faucet". Run 10⁹ FLOP × 10
/// = 10¹⁰ FLOP (≈ 0.01 H100-seconds) to earn it, then stake and
/// become a real provider. Similar to Bitcoin's early CPU-mining
/// window but bounded.
pub const STAKELESS_EARN_CAP_TRM: u64 = 10;

/// Phase 18.2 — Welcome loan sunset epoch. Once
/// `ComputeLedger::current_epoch() >= WELCOME_LOAN_SUNSET_EPOCH`,
/// new welcome loans are refused. Chosen as epoch 2 = after 75%
/// of the TRM supply has been minted; beyond that point the
/// bootstrap incentive has served its purpose and new entrants
/// must use the stakeless-earn path.
///
/// This is **Constitutional**: re-opening welcome loans would
/// re-open the Sybil vector that Phase 2.8 + 4.1 + 18.2 closed.
/// See `docs/constitution.md` Article XI.
pub const WELCOME_LOAN_SUNSET_EPOCH: u64 = 2;

// ===========================================================================
// §2 — Credit score (parameters.md §4)
// ===========================================================================

/// Weight of trade history in credit score.
pub const WEIGHT_TRADE: f64 = 0.3;
/// Weight of repayment history in credit score.
pub const WEIGHT_REPAYMENT: f64 = 0.4;
/// Weight of uptime in credit score.
pub const WEIGHT_UPTIME: f64 = 0.2;
/// Weight of account age in credit score.
pub const WEIGHT_AGE: f64 = 0.1;

/// Minimum credit score required to borrow.
pub const MIN_CREDIT_FOR_BORROWING: f64 = 0.2;
/// Credit score assigned to new nodes (cold start).
pub const COLD_START_CREDIT: f64 = 0.3;
/// Credit score target after repaying welcome loan.
pub const TARGET_CREDIT_AFTER_REPAY: f64 = 0.4;
/// Neutral repayment score for nodes with no loan history.
pub const NEUTRAL_REPAYMENT_SCORE: f64 = 0.5;

/// Default reputation for a new node (spec §7, parameters.md).
/// Used by `NodeBalance::new_with_reputation()` and yield calculation.
pub const DEFAULT_REPUTATION: f64 = 0.5;

/// EMA smoothing factor for market price supply/demand updates (spec §2).
/// 0.3 = moderate responsiveness; spec says 30-minute half-life, this alpha
/// approximates that under typical update cadence.
pub const EMA_ALPHA: f64 = 0.3;

/// Trade volume cap for trade_score = 1.0 (CU).
pub const TRADE_SCORE_CAP_CU: u64 = 100_000;
/// Account age cap for age_score = 1.0 (days).
pub const AGE_SCORE_CAP_DAYS: u64 = 90;

// ===========================================================================
// §3 — Lending pool (parameters.md §5)
// ===========================================================================

/// Minimum reserve ratio (30% of pool stays unlent).
pub const MIN_RESERVE_RATIO: f64 = 0.30;
/// Maximum loan-to-value ratio (loan : collateral).
pub const MAX_LTV_RATIO: f64 = 3.0;
/// Maximum single loan as fraction of total pool.
pub const MAX_SINGLE_LOAN_POOL_PCT: f64 = 0.20;
/// Maximum loan term (hours).
pub const MAX_LOAN_TERM_HOURS: u64 = 168;
/// Maximum new loans per minute (velocity limit).
pub const MAX_LENDING_VELOCITY: usize = 10;

// ===========================================================================
// §4 — Default and circuit breakers (parameters.md §6)
// ===========================================================================

/// Hourly default rate that trips the global lending circuit breaker.
pub const DEFAULT_CIRCUIT_BREAKER_THRESHOLD: f64 = 0.10;
/// Fraction of collateral burned on default (the rest goes to lender).
pub const COLLATERAL_BURN_ON_DEFAULT: f64 = 0.10;
/// Velocity circuit breaker observation window (seconds).
pub const VELOCITY_CB_WINDOW_SECS: u64 = 3_600;
/// Velocity circuit breaker trip threshold: if this fraction of pool is lent per window, suspend.
pub const VELOCITY_CB_THRESHOLD: f64 = 0.50;

// ===========================================================================
// §5 — Interest rate model (parameters.md §4 / banking.md §5.3)
// ===========================================================================

/// Base interest rate (fraction per hour).
pub const BASE_INTEREST_RATE_PER_HOUR: f64 = 0.001;
/// Maximum risk premium added for lowest credit score.
pub const RISK_PREMIUM_PER_HOUR: f64 = 0.005;

// ===========================================================================
// §6 — Multi-model pricing tiers (parameters.md §2)
// ===========================================================================

/// Base CU/token for small models (<3B parameters).
pub const TIER_SMALL_CU_PER_TOKEN: u64 = 1;
/// Base CU/token for medium models (3B-14B parameters).
pub const TIER_MEDIUM_CU_PER_TOKEN: u64 = 3;
/// Base CU/token for large models (14B-70B parameters).
pub const TIER_LARGE_CU_PER_TOKEN: u64 = 8;
/// Base CU/token for frontier models (>70B parameters).
pub const TIER_FRONTIER_CU_PER_TOKEN: u64 = 20;

/// Small-tier parameter upper bound.
pub const TIER_SMALL_MAX_PARAMS: u64 = 3_000_000_000;
/// Medium-tier parameter upper bound.
pub const TIER_MEDIUM_MAX_PARAMS: u64 = 14_000_000_000;
/// Large-tier parameter upper bound.
pub const TIER_LARGE_MAX_PARAMS: u64 = 70_000_000_000;

// ===========================================================================
// §7 — Inactivity (parameters.md §7)
// ===========================================================================

/// Days without activity before uptime score starts decaying.
pub const INACTIVITY_DECAY_THRESHOLD_DAYS: u64 = 7;
/// Daily uptime-score decay rate once inactive.
pub const INACTIVITY_DECAY_PER_DAY: f64 = 0.01;
/// Days offline before CU starts being burned.
pub const INACTIVITY_BURN_THRESHOLD_DAYS: u64 = 90;
/// Monthly CU burn rate for long-inactive nodes.
pub const INACTIVITY_BURN_PER_MONTH: f64 = 0.01;

// ===========================================================================
// §8 — Yield (parameters.md §7)
// ===========================================================================

/// Availability yield rate (fraction per hour, multiplied by reputation).
pub const AVAILABILITY_YIELD_RATE_PER_HOUR: f64 = 0.001;

// ===========================================================================
// §9 — Reputation gossip (parameters.md §9 / Phase 9 A3)
// ===========================================================================

/// Maximum remote observations retained per subject for consensus reputation.
/// Oldest observations are evicted when this limit is exceeded.
pub const MAX_REMOTE_OBSERVATIONS_PER_NODE: usize = 32;

/// Minimum trade_count for an observation to influence the weighted median.
/// Observers with fewer than this many trades involving the subject are ignored
/// to prevent low-confidence noise from swaying the score.
pub const MIN_OBSERVATION_WEIGHT: u64 = 5;

// ===========================================================================
// LoanRecord types
// ===========================================================================

/// Status of a loan in its lifecycle.
///
/// See `forge-economics/docs/05-banking.md` §5.2 for the state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoanStatus {
    /// Loan is outstanding and within term.
    Active,
    /// Loan has been fully repaid.
    Repaid,
    /// Loan has passed its due date without repayment.
    Defaulted,
}

/// Bilateral loan agreement between a lender and borrower.
///
/// Every loan requires dual Ed25519 signatures. LoanRecords are gossip-synced
/// across the mesh like `TradeRecord`, enabling trustless verification.
///
/// Mirrors the specification in `forge-economics/spec/forge-economics-spec-v0.2.md`
/// Part 5 (Banking).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoanRecord {
    /// Unique loan identifier (SHA-256 of canonical bytes).
    pub loan_id: [u8; 32],
    /// Node providing CU.
    pub lender: NodeId,
    /// Node receiving CU.
    pub borrower: NodeId,
    /// Principal amount in CU.
    pub principal_trm: u64,
    /// Interest rate per hour (fraction, e.g., 0.001 = 0.1%/hr).
    pub interest_rate_per_hour: f64,
    /// Loan term in hours.
    pub term_hours: u64,
    /// CU locked as collateral from the borrower.
    pub collateral_trm: u64,
    /// Current lifecycle state.
    pub status: LoanStatus,
    /// Creation timestamp (milliseconds since epoch).
    pub created_at: u64,
    /// Due timestamp (= created_at + term_hours * 3_600_000).
    pub due_at: u64,
    /// Actual repayment timestamp, if repaid.
    #[serde(default)]
    pub repaid_at: Option<u64>,
}

impl LoanRecord {
    /// Maximum age in milliseconds beyond which a proposed loan is rejected.
    pub const MAX_PROPOSAL_AGE_MS: u64 = 3_600_000;

    /// Derive a deterministic loan_id by hashing the canonical bytes (pre-signature).
    pub fn compute_loan_id(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.canonical_bytes());
        let result = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&result);
        out
    }

    /// Deterministic binary representation used for signing and hashing.
    ///
    /// Field order: lender(32) + borrower(32) + principal_trm(8) +
    /// interest_rate_per_hour(8, IEEE754 BE) + term_hours(8) +
    /// collateral_trm(8) + created_at(8) + due_at(8).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(32 + 32 + 8 + 8 + 8 + 8 + 8 + 8);
        bytes.extend_from_slice(&self.lender.0);
        bytes.extend_from_slice(&self.borrower.0);
        bytes.extend_from_slice(&self.principal_trm.to_be_bytes());
        bytes.extend_from_slice(&self.interest_rate_per_hour.to_be_bytes());
        bytes.extend_from_slice(&self.term_hours.to_be_bytes());
        bytes.extend_from_slice(&self.collateral_trm.to_be_bytes());
        bytes.extend_from_slice(&self.created_at.to_be_bytes());
        bytes.extend_from_slice(&self.due_at.to_be_bytes());
        bytes
    }

    /// Interest accrued over the full term (CU).
    pub fn total_interest(&self) -> u64 {
        (self.principal_trm as f64 * self.interest_rate_per_hour * self.term_hours as f64) as u64
    }

    /// Total amount borrower owes to clear the loan.
    pub fn total_due(&self) -> u64 {
        self.principal_trm.saturating_add(self.total_interest())
    }
}

/// Dual-signed loan record. Mirrors `SignedTradeRecord`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedLoanRecord {
    pub loan: LoanRecord,
    /// Ed25519 signature from the lender (64 bytes).
    pub lender_sig: Vec<u8>,
    /// Ed25519 signature from the borrower (64 bytes).
    pub borrower_sig: Vec<u8>,
}

/// Signature verification errors for loans.
#[derive(Debug, thiserror::Error)]
pub enum LoanSignatureError {
    #[error("lender public key is invalid")]
    InvalidLenderKey,
    #[error("borrower public key is invalid")]
    InvalidBorrowerKey,
    #[error("lender signature is invalid")]
    InvalidLenderSignature,
    #[error("borrower signature is invalid")]
    InvalidBorrowerSignature,
    #[error("loan proposal is too old")]
    ProposalExpired,
}

impl SignedLoanRecord {
    /// Verify both signatures and proposal freshness.
    pub fn verify(&self) -> Result<(), LoanSignatureError> {
        use ed25519_dalek::{Signature, VerifyingKey};

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if now > self.loan.created_at + LoanRecord::MAX_PROPOSAL_AGE_MS {
            return Err(LoanSignatureError::ProposalExpired);
        }

        let canonical = self.loan.canonical_bytes();

        let lender_key = VerifyingKey::from_bytes(&self.loan.lender.0)
            .map_err(|_| LoanSignatureError::InvalidLenderKey)?;
        let lender_sig: [u8; 64] = self
            .lender_sig
            .as_slice()
            .try_into()
            .map_err(|_| LoanSignatureError::InvalidLenderSignature)?;
        lender_key
            .verify_strict(&canonical, &Signature::from_bytes(&lender_sig))
            .map_err(|_| LoanSignatureError::InvalidLenderSignature)?;

        let borrower_key = VerifyingKey::from_bytes(&self.loan.borrower.0)
            .map_err(|_| LoanSignatureError::InvalidBorrowerKey)?;
        let borrower_sig: [u8; 64] = self
            .borrower_sig
            .as_slice()
            .try_into()
            .map_err(|_| LoanSignatureError::InvalidBorrowerSignature)?;
        borrower_key
            .verify_strict(&canonical, &Signature::from_bytes(&borrower_sig))
            .map_err(|_| LoanSignatureError::InvalidBorrowerSignature)?;

        Ok(())
    }
}

// ===========================================================================
// Credit score helpers
// ===========================================================================

/// Compute credit score from component sub-scores using the canonical weights.
///
/// All inputs are clamped to [0.0, 1.0] before combination.
pub fn compute_credit_score_from_components(
    trade_score: f64,
    repayment_score: f64,
    uptime_score: f64,
    age_score: f64,
) -> f64 {
    let clamp = |v: f64| v.clamp(0.0, 1.0);
    WEIGHT_TRADE * clamp(trade_score)
        + WEIGHT_REPAYMENT * clamp(repayment_score)
        + WEIGHT_UPTIME * clamp(uptime_score)
        + WEIGHT_AGE * clamp(age_score)
}

/// Derive the trade sub-score from lifetime CU volume.
pub fn trade_score_from_volume(total_trm: u64) -> f64 {
    (total_trm as f64 / TRADE_SCORE_CAP_CU as f64).min(1.0)
}

/// Derive the age sub-score from days since joining.
pub fn age_score_from_days(days: u64) -> f64 {
    (days as f64 / AGE_SCORE_CAP_DAYS as f64).min(1.0)
}

/// Compute the interest rate offered to a borrower given their credit score.
///
/// `offered_rate = base_rate + (1.0 - credit_score) * risk_premium`
pub fn offered_interest_rate(credit_score: f64) -> f64 {
    let credit = credit_score.clamp(0.0, 1.0);
    BASE_INTEREST_RATE_PER_HOUR + (1.0 - credit) * RISK_PREMIUM_PER_HOUR
}

/// Maximum CU that a borrower with the given credit score can borrow from a pool.
///
/// `max_borrow = credit^2 * pool_available * 0.2`
pub fn max_borrowable(credit_score: f64, pool_available: u64) -> u64 {
    let credit = credit_score.clamp(0.0, 1.0);
    (credit * credit * pool_available as f64 * MAX_SINGLE_LOAN_POOL_PCT) as u64
}

// ===========================================================================
// Model tier helpers
// ===========================================================================

/// Four-tier classification for LLMs by active parameter count.
///
/// Mirrors `spec/parameters.md` §2 (dynamic pricing) and drives multi-model CU
/// pricing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    /// < 3B active parameters.
    Small,
    /// 3B - 14B active parameters.
    Medium,
    /// 14B - 70B active parameters.
    Large,
    /// > 70B active parameters.
    Frontier,
}

impl ModelTier {
    /// Classify a model by its (active) parameter count.
    ///
    /// For MoE models, pass the number of **active** parameters per token.
    pub fn from_active_params(params: u64) -> Self {
        if params < TIER_SMALL_MAX_PARAMS {
            ModelTier::Small
        } else if params < TIER_MEDIUM_MAX_PARAMS {
            ModelTier::Medium
        } else if params < TIER_LARGE_MAX_PARAMS {
            ModelTier::Large
        } else {
            ModelTier::Frontier
        }
    }

    /// Base CU/token price for this tier.
    pub fn base_trm_per_token(self) -> u64 {
        match self {
            ModelTier::Small => TIER_SMALL_CU_PER_TOKEN,
            ModelTier::Medium => TIER_MEDIUM_CU_PER_TOKEN,
            ModelTier::Large => TIER_LARGE_CU_PER_TOKEN,
            ModelTier::Frontier => TIER_FRONTIER_CU_PER_TOKEN,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credit_score_weights_sum_to_one() {
        let sum = WEIGHT_TRADE + WEIGHT_REPAYMENT + WEIGHT_UPTIME + WEIGHT_AGE;
        assert!((sum - 1.0).abs() < 1e-9, "weights must sum to 1.0, got {}", sum);
    }

    #[test]
    fn cold_start_credit_is_between_min_and_target() {
        assert!(MIN_CREDIT_FOR_BORROWING < COLD_START_CREDIT);
        assert!(COLD_START_CREDIT < TARGET_CREDIT_AFTER_REPAY);
    }

    #[test]
    fn perfect_credit_gets_base_rate() {
        let rate = offered_interest_rate(1.0);
        assert!((rate - BASE_INTEREST_RATE_PER_HOUR).abs() < 1e-9);
    }

    #[test]
    fn zero_credit_gets_full_risk_premium() {
        let rate = offered_interest_rate(0.0);
        assert!((rate - (BASE_INTEREST_RATE_PER_HOUR + RISK_PREMIUM_PER_HOUR)).abs() < 1e-9);
    }

    #[test]
    fn max_borrowable_is_quadratic_in_credit() {
        let half = max_borrowable(0.5, 1_000_000);
        let full = max_borrowable(1.0, 1_000_000);
        // 0.5^2 * 0.2 * 1M = 50_000 ; 1^2 * 0.2 * 1M = 200_000
        assert_eq!(half, 50_000);
        assert_eq!(full, 200_000);
    }

    #[test]
    fn compute_credit_score_from_all_ones_equals_one() {
        let score = compute_credit_score_from_components(1.0, 1.0, 1.0, 1.0);
        assert!((score - 1.0).abs() < 1e-9);
    }

    #[test]
    fn trade_score_saturates_at_cap() {
        assert_eq!(trade_score_from_volume(TRADE_SCORE_CAP_CU * 10), 1.0);
        assert_eq!(trade_score_from_volume(TRADE_SCORE_CAP_CU / 2), 0.5);
    }

    #[test]
    fn age_score_saturates_at_cap() {
        assert_eq!(age_score_from_days(AGE_SCORE_CAP_DAYS * 2), 1.0);
        assert_eq!(age_score_from_days(AGE_SCORE_CAP_DAYS / 2), 0.5);
    }

    #[test]
    fn model_tier_classification() {
        assert_eq!(ModelTier::from_active_params(500_000_000), ModelTier::Small);
        assert_eq!(ModelTier::from_active_params(8_000_000_000), ModelTier::Medium);
        assert_eq!(ModelTier::from_active_params(32_000_000_000), ModelTier::Large);
        assert_eq!(ModelTier::from_active_params(405_000_000_000), ModelTier::Frontier);
    }

    #[test]
    fn model_tier_base_prices_match_spec() {
        assert_eq!(ModelTier::Small.base_trm_per_token(), 1);
        assert_eq!(ModelTier::Medium.base_trm_per_token(), 3);
        assert_eq!(ModelTier::Large.base_trm_per_token(), 8);
        assert_eq!(ModelTier::Frontier.base_trm_per_token(), 20);
    }

    #[test]
    fn loan_total_interest_matches_formula() {
        let loan = LoanRecord {
            loan_id: [0u8; 32],
            lender: NodeId([1u8; 32]),
            borrower: NodeId([2u8; 32]),
            principal_trm: 10_000,
            interest_rate_per_hour: 0.001,
            term_hours: 100,
            collateral_trm: 30_000,
            status: LoanStatus::Active,
            created_at: 0,
            due_at: 100 * 3_600_000,
            repaid_at: None,
        };
        // 10_000 * 0.001 * 100 = 1_000
        assert_eq!(loan.total_interest(), 1_000);
        assert_eq!(loan.total_due(), 11_000);
    }

    #[test]
    fn loan_canonical_bytes_are_deterministic() {
        let loan = LoanRecord {
            loan_id: [0u8; 32],
            lender: NodeId([1u8; 32]),
            borrower: NodeId([2u8; 32]),
            principal_trm: 1_000,
            interest_rate_per_hour: 0.001,
            term_hours: 24,
            collateral_trm: 3_000,
            status: LoanStatus::Active,
            created_at: 1_700_000_000_000,
            due_at: 1_700_000_000_000 + 24 * 3_600_000,
            repaid_at: None,
        };
        let a = loan.canonical_bytes();
        let b = loan.canonical_bytes();
        assert_eq!(a, b);
        assert_eq!(a.len(), 32 + 32 + 8 + 8 + 8 + 8 + 8 + 8);
    }

    #[test]
    fn loan_id_is_deterministic() {
        let loan = LoanRecord {
            loan_id: [0u8; 32],
            lender: NodeId([3u8; 32]),
            borrower: NodeId([4u8; 32]),
            principal_trm: 500,
            interest_rate_per_hour: 0.002,
            term_hours: 48,
            collateral_trm: 1_500,
            status: LoanStatus::Active,
            created_at: 123,
            due_at: 123 + 48 * 3_600_000,
            repaid_at: None,
        };
        assert_eq!(loan.compute_loan_id(), loan.compute_loan_id());
    }

    #[test]
    fn signed_loan_round_trips() {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::thread_rng;

        let mut rng = thread_rng();
        let lender_key = SigningKey::generate(&mut rng);
        let borrower_key = SigningKey::generate(&mut rng);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let loan = LoanRecord {
            loan_id: [0u8; 32],
            lender: NodeId(lender_key.verifying_key().to_bytes()),
            borrower: NodeId(borrower_key.verifying_key().to_bytes()),
            principal_trm: 2_000,
            interest_rate_per_hour: 0.001,
            term_hours: 72,
            collateral_trm: 6_000,
            status: LoanStatus::Active,
            created_at: now,
            due_at: now + 72 * 3_600_000,
            repaid_at: None,
        };
        let canonical = loan.canonical_bytes();
        let lender_sig = lender_key.sign(&canonical).to_bytes().to_vec();
        let borrower_sig = borrower_key.sign(&canonical).to_bytes().to_vec();

        let signed = SignedLoanRecord {
            loan,
            lender_sig,
            borrower_sig,
        };
        signed.verify().expect("valid loan must verify");
    }
}
