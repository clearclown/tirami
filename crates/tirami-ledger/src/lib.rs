pub mod agentnet;
pub mod agora;
pub mod agora_relay;
pub mod anchor;
pub mod bitvm;
pub mod collusion;
pub mod ledger;
pub mod lending;
pub mod metrics;
pub mod safety;
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
pub use safety::{BudgetPolicy, KillSwitch, SafetyController, SafetyStatus, SpendDenied};
pub use agentnet::{AgentNet, AgentPost, AgentProfile};
pub use agora::{AgoraError, JobRequest, JobResult, Nip90Publisher, ProviderAdvertisement};
// Re-export ReputationObservation from forge-proto for convenience.
pub use tirami_proto::ReputationObservation;
pub use bitvm::{
    BitVmError, FraudProof, FraudProofVerifier, FraudType, MockFraudProofVerifier,
    StakedClaim,
};
pub use zk::{MockVerifier, ProofOfInference, ProofVerifier, VerifierRegistry, ZkError};
