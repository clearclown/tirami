pub mod agentnet;
pub mod agora;
pub mod agora_relay;
pub mod anchor;
pub mod bitvm;
pub mod collusion;
pub mod governance;
pub mod ledger;
pub mod lending;
pub mod audit;
pub mod metrics;
pub mod peer_registry;
pub mod referral;
pub mod safety;
pub mod staking;
pub mod tokenomics;
pub mod zk;

pub use collusion::{CollusionDetector, CollusionReport};
pub use ledger::{
    ComputeLedger, MarketPrice, NetworkStats, SettlementNode, SettlementStatement,
    SignatureError, SignedTradeRecord, TradeRecord,
};
pub use lending::{
    LoanRecord, LoanSignatureError, LoanStatus, ModelTier, SignedLoanRecord,
    compute_credit_score_from_components, max_borrowable, offered_interest_rate,
};
pub use referral::{ReferralError, ReferralRecord, ReferralTracker};
pub use safety::{BudgetPolicy, KillSwitch, SafetyController, SafetyStatus, SpendDenied};
pub use agentnet::{AgentNet, AgentPost, AgentProfile};
pub use agora::{AgoraError, JobRequest, JobResult, Nip90Publisher, ProviderAdvertisement};
// Re-export ReputationObservation from forge-proto for convenience.
pub use tirami_proto::ReputationObservation;
pub use bitvm::{
    BitVmError, FraudProof, FraudProofVerifier, FraudType, MockFraudProofVerifier,
    StakedClaim,
};
pub use governance::{
    GovernanceError, GovernanceState, Proposal, ProposalKind, ProposalStatus, Vote,
};
pub use staking::{Stake, StakeDuration, StakingError, StakingPool};
pub use tokenomics::{
    effective_mint_rate, epoch_yield_rate, supply_factor, transaction_fee,
    FEE_ACTIVATION_THRESHOLD, INITIAL_YIELD_RATE, RARITY_COMMON, RARITY_LEGENDARY,
    RARITY_RARE, RARITY_UNCOMMON, TOTAL_TRM_SUPPLY, TRANSACTION_FEE_RATE,
};
pub use zk::{MockVerifier, ProofOfInference, ProofVerifier, VerifierRegistry, ZkError};
pub use peer_registry::{PeerRegistry, PeerState};
pub use audit::{AuditTracker, AuditVerdict, PendingChallenge, AUDIT_TIMEOUT_MS};
