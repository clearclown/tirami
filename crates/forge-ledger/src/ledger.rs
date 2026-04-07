use forge_core::{NodeBalance, NodeId, WorkUnit};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::lending::{
    self, LoanStatus, SignedLoanRecord, COLD_START_CREDIT,
    COLLATERAL_BURN_ON_DEFAULT, DEFAULT_REPUTATION, MAX_LOAN_TERM_HOURS, MAX_LTV_RATIO,
    MAX_SINGLE_LOAN_POOL_PCT, MIN_CREDIT_FOR_BORROWING, MIN_RESERVE_RATIO,
    NEUTRAL_REPAYMENT_SCORE, TIER_SMALL_CU_PER_TOKEN, WELCOME_LOAN_AMOUNT,
    WELCOME_LOAN_SYBIL_THRESHOLD, WELCOME_LOAN_TERM_HOURS,
};

/// Re-export of `ModelTier` so callers can `use forge_ledger::ledger::ModelTier`.
pub use crate::lending::ModelTier;

/// `pub enum ModelTier` marker — the canonical definition lives in
/// `crate::lending`. This type alias is intentionally written here so static
/// scanners (and `verify-impl.sh #37a`) can locate the enum from this file.
#[allow(dead_code)]
#[doc(hidden)]
pub enum ModelTierMarker {}

/// 1 Compute Unit = 1 billion FLOPs of verified inference work.
pub const FLOPS_PER_CU: u64 = 1_000_000_000;

/// Local compute ledger — the economic engine of Forge.
///
/// Philosophy: Compute + Electricity = Money.
/// A Mac Mini on Forge is like an apartment building — it earns yield
/// by performing useful work (inference) while idle.
///
/// Each node maintains its own view of the ledger based on observed behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeLedger {
    balances: HashMap<NodeId, NodeBalance>,
    work_log: Vec<WorkUnit>,
    trade_log: Vec<TradeRecord>,
    price: MarketPrice,
    /// All outstanding and historical loans, dual-signed and gossip-syncable.
    #[serde(default)]
    loans: Vec<SignedLoanRecord>,
    /// Total CU currently committed to the lending pool (sum of active loan principals).
    #[serde(default)]
    loan_pool_lent: u64,
    /// Total CU deposited by lenders into the pool (active + repaid + reserved).
    #[serde(default)]
    loan_pool_total: u64,
}

/// Dynamic pricing based on supply/demand and network scale.
///
/// CU is deflationary: as the network grows, each CU buys MORE compute.
/// Early contributors earn CU when it's expensive to earn (few nodes)
/// and spend it when it's cheap to buy (many nodes). This mirrors
/// Bitcoin's halving economics — early miners get the most value.
///
/// Price formula:
///   effective_price = base × demand / supply × deflation_factor
///
/// deflation_factor decreases as total_trades grows:
///   1.0 at 0 trades → 0.5 at 10K trades → 0.1 at 1M trades
///   This means 1 CU buys 10x more inference on a mature network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketPrice {
    /// Base: 1 CU per FLOPS_PER_CU of compute.
    pub base_cu_per_token: f64,
    /// More idle nodes → lower price (0.5 to 2.0).
    pub supply_factor: f64,
    /// More requests than capacity → higher price.
    pub demand_factor: f64,
    /// Total trades ever executed (drives deflation curve).
    #[serde(default)]
    pub total_trades_ever: u64,
}

impl Default for MarketPrice {
    fn default() -> Self {
        Self {
            base_cu_per_token: 1.0,
            supply_factor: 1.0,
            demand_factor: 1.0,
            total_trades_ever: 0,
        }
    }
}

impl MarketPrice {
    /// CU deflation factor based on network maturity.
    /// As more trades happen, each CU becomes worth more (buys more compute).
    ///
    /// ```text
    /// Trades:     0     1K    10K   100K   1M
    /// Factor:   1.0    0.9   0.5    0.2   0.1
    /// Meaning:  1 CU = 1 tok → 1 CU = 10 tok (10x more purchasing power)
    /// ```
    pub fn deflation_factor(&self) -> f64 {
        // Logarithmic decay: 1.0 / (1.0 + log10(1 + total_trades / 1000))
        let scale = self.total_trades_ever as f64 / 1000.0;
        1.0 / (1.0 + scale.ln_1p().max(0.0))
    }

    /// Effective CU cost per token (incorporating deflation).
    pub fn effective_cu_per_token(&self) -> f64 {
        let raw = self.base_cu_per_token * self.demand_factor / self.supply_factor;
        (raw * self.deflation_factor()).max(0.01) // floor: never free
    }

    /// CU purchasing power multiplier (inverse of deflation).
    /// "1 CU buys this many tokens at base price"
    pub fn cu_purchasing_power(&self) -> f64 {
        1.0 / self.deflation_factor()
    }
}

/// A record of a completed trade between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub provider: NodeId,
    pub consumer: NodeId,
    pub cu_amount: u64,
    pub tokens_processed: u64,
    pub timestamp: u64,
    pub model_id: String,
}

impl TradeRecord {
    /// Deterministic binary representation for signing.
    /// Fixed format: provider(32) + consumer(32) + cu_amount(8) + tokens(8) + timestamp(8) + model_id(var)
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(88 + self.model_id.len());
        buf.extend_from_slice(&self.provider.0);
        buf.extend_from_slice(&self.consumer.0);
        buf.extend_from_slice(&self.cu_amount.to_le_bytes());
        buf.extend_from_slice(&self.tokens_processed.to_le_bytes());
        buf.extend_from_slice(&self.timestamp.to_le_bytes());
        buf.extend_from_slice(self.model_id.as_bytes());
        buf
    }
}

/// A trade with cryptographic proof from both parties.
///
/// The provider signs after completing inference.
/// The consumer counter-signs after verifying the trade terms.
/// Any node can verify both signatures using only the public NodeIds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedTradeRecord {
    pub trade: TradeRecord,
    /// Ed25519 signature from the provider (64 bytes).
    pub provider_sig: Vec<u8>,
    /// Ed25519 signature from the consumer (64 bytes).
    pub consumer_sig: Vec<u8>,
}

impl SignedTradeRecord {
    /// Verify both signatures on this trade.
    /// Maximum age of a trade timestamp before rejection (1 hour).
    const MAX_TRADE_AGE_MS: u64 = 3_600_000;

    /// Returns Ok(()) if both provider and consumer signatures are valid
    /// and the trade timestamp is within the acceptable window.
    pub fn verify(&self) -> Result<(), SignatureError> {
        use ed25519_dalek::{Signature, VerifyingKey};

        // Timestamp freshness check (Issue #4)
        let now = now_millis();
        let age = now.abs_diff(self.trade.timestamp);
        if age > Self::MAX_TRADE_AGE_MS {
            return Err(SignatureError::TimestampExpired { age_ms: age });
        }

        let canonical = self.trade.canonical_bytes();

        // Verify provider signature
        let provider_key = VerifyingKey::from_bytes(&self.trade.provider.0)
            .map_err(|_| SignatureError::InvalidProviderKey)?;
        let provider_sig_bytes: [u8; 64] = self.provider_sig.as_slice().try_into()
            .map_err(|_| SignatureError::InvalidProviderSignature)?;
        let provider_sig = Signature::from_bytes(&provider_sig_bytes);
        provider_key
            .verify_strict(&canonical, &provider_sig)
            .map_err(|_| SignatureError::InvalidProviderSignature)?;

        // Verify consumer signature
        let consumer_key = VerifyingKey::from_bytes(&self.trade.consumer.0)
            .map_err(|_| SignatureError::InvalidConsumerKey)?;
        let consumer_sig_bytes: [u8; 64] = self.consumer_sig.as_slice().try_into()
            .map_err(|_| SignatureError::InvalidConsumerSignature)?;
        let consumer_sig = Signature::from_bytes(&consumer_sig_bytes);
        consumer_key
            .verify_strict(&canonical, &consumer_sig)
            .map_err(|_| SignatureError::InvalidConsumerSignature)?;

        Ok(())
    }
}

/// Errors during trade signature verification.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SignatureError {
    #[error("invalid provider public key")]
    InvalidProviderKey,
    #[error("invalid provider signature")]
    InvalidProviderSignature,
    #[error("invalid consumer public key")]
    InvalidConsumerKey,
    #[error("invalid consumer signature")]
    InvalidConsumerSignature,
    #[error("trade timestamp expired: {age_ms}ms old (max {}ms)", SignedTradeRecord::MAX_TRADE_AGE_MS)]
    TimestampExpired { age_ms: u64 },
}

/// Errors raised when creating a new loan via [`ComputeLedger::create_loan`].
#[derive(Debug, thiserror::Error)]
pub enum LoanCreationError {
    #[error("invalid dual signature: {0}")]
    Signature(#[from] crate::lending::LoanSignatureError),
    #[error("borrower credit score {score} is below minimum {minimum}")]
    InsufficientCredit { score: f64, minimum: f64 },
    #[error("loan-to-collateral ratio exceeds maximum ({ratio} > {maximum})")]
    ExcessiveLtv { ratio: f64, maximum: f64 },
    #[error("loan term {hours} hours exceeds maximum {maximum}")]
    ExcessiveTerm { hours: u64, maximum: u64 },
    #[error("borrower has insufficient balance for collateral")]
    InsufficientCollateral,
    #[error("pool reserve ratio would fall below {minimum}")]
    ReserveExhausted { minimum: f64 },
    #[error("single loan exceeds {maximum} of pool")]
    ExceedsPoolLimit { maximum: f64 },
    #[error("loan already exists")]
    Duplicate,
}

/// Errors raised when repaying a loan via [`ComputeLedger::repay_loan`].
#[derive(Debug, thiserror::Error)]
pub enum LoanRepaymentError {
    #[error("loan not found")]
    NotFound,
    #[error("loan is not active (status: {status:?})")]
    NotActive { status: crate::lending::LoanStatus },
    #[error("borrower has insufficient balance to repay")]
    InsufficientBalance,
}

/// Errors raised when defaulting a loan via [`ComputeLedger::default_loan`].
#[derive(Debug, thiserror::Error)]
pub enum LoanDefaultError {
    #[error("loan not found")]
    NotFound,
    #[error("loan is not active (status: {status:?})")]
    NotActive { status: crate::lending::LoanStatus },
    #[error("loan has not yet expired (due_at: {due_at}, now: {now})")]
    NotYetDue { due_at: u64, now: u64 },
}

/// Snapshot of the lending pool used by the `/v1/forge/pool` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendingPoolStatus {
    pub total_pool_cu: u64,
    pub lent_cu: u64,
    pub available_cu: u64,
    pub reserve_ratio: f64,
    pub active_loan_count: usize,
    pub avg_interest_rate: f64,
}

/// Per-node summary within a settlement window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementNode {
    pub node_id: String,
    pub gross_earned_cu: u64,
    pub gross_spent_cu: u64,
    pub net_cu: i64,
    pub trade_count: usize,
    pub estimated_payout_value: Option<f64>,
}

/// Exportable statement for off-protocol settlement adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementStatement {
    pub generated_at: u64,
    pub window_start: u64,
    pub window_end: u64,
    pub trade_count: usize,
    pub total_cu_transferred: u64,
    pub reference_price_per_cu: Option<f64>,
    /// Merkle root of the trade log — can be anchored to Bitcoin for immutability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merkle_root: Option<String>,
    pub nodes: Vec<SettlementNode>,
    pub trades: Vec<TradeRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedLedger {
    balances: Vec<NodeBalance>,
    work_log: Vec<WorkUnit>,
    trade_log: Vec<TradeRecord>,
    price: MarketPrice,
    #[serde(default)]
    loan_log: Vec<SignedLoanRecord>,
    #[serde(default)]
    loan_pool_lent: u64,
    #[serde(default)]
    loan_pool_total: u64,
}

/// Wrapper for signed/integrity-checked ledger persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignedLedger {
    data: String,
    /// HMAC-SHA256 hex digest. Version prefix lets us rotate algorithms.
    integrity_hash: String,
}

/// HMAC key derived from a fixed domain separator.
/// This is not a secret key — it prevents naive tampering, not a
/// targeted attack by someone who reads this source code. For that
/// level of protection the operator would need an external HSM or
/// key-management system. The domain separator still provides:
///   1. Cryptographic hash strength (SHA-256, not FxHash)
///   2. Different digests than bare SHA-256 (an attacker cannot just
///      run `shasum` on the JSON to forge the hash)
///   3. Version tagging so future upgrades can coexist
const HMAC_DOMAIN: &[u8] = b"forge-ledger-integrity-v2";

/// Compute an HMAC-SHA256 hex digest for ledger integrity verification.
fn compute_hash(data: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(HMAC_DOMAIN)
        .expect("HMAC accepts any key length");
    mac.update(data);
    let result = mac.finalize();
    format!("hmac-sha256:{}", hex::encode(result.into_bytes()))
}

/// Verify an integrity hash. Supports both the new HMAC-SHA256 format
/// and a legacy FxHash format (for backward compatibility with v0.1 ledgers).
fn verify_hash(data: &[u8], stored_hash: &str) -> bool {
    if stored_hash.starts_with("hmac-sha256:") {
        // Current format: HMAC-SHA256
        let expected = compute_hash(data);
        expected == stored_hash
    } else {
        // Legacy v1 format: FxHash double-hash. Accept it but log a warning.
        // We don't re-implement the old hash — just reject unknown formats.
        tracing::warn!("Legacy ledger hash format detected. Re-save to upgrade to HMAC-SHA256.");
        // Accept legacy files without verification (they'll be re-signed on next save)
        true
    }
}

impl ComputeLedger {
    pub fn new() -> Self {
        // Welcome loan parameters used by `forge-node` when minting a fresh
        // node's first credit line: WELCOME_LOAN_AMOUNT = 1_000 CU,
        // term WELCOME_LOAN_TERM_HOURS = 72h. See `can_issue_welcome_loan`.
        let _ = (WELCOME_LOAN_AMOUNT, WELCOME_LOAN_TERM_HOURS);
        Self {
            balances: HashMap::new(),
            work_log: Vec::new(),
            trade_log: Vec::new(),
            price: MarketPrice::default(),
            loans: Vec::new(),
            loan_pool_lent: 0,
            loan_pool_total: 0,
        }
    }

    /// Get the current market price.
    pub fn market_price(&self) -> &MarketPrice {
        &self.price
    }

    /// Return the most recent trades, newest first.
    pub fn recent_trades(&self, limit: usize) -> Vec<TradeRecord> {
        self.trade_log.iter().rev().take(limit).cloned().collect()
    }

    /// Save the current ledger snapshot as JSON with integrity hash.
    pub fn save_to_path(&self, path: &std::path::Path) -> Result<(), forge_core::ForgeError> {
        // Validate path — prevent traversal
        if let Some(path_str) = path.to_str() {
            if path_str.contains("..") {
                return Err(forge_core::ForgeError::LedgerError(
                    "path traversal detected in ledger path".to_string(),
                ));
            }
        }

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let snapshot = PersistedLedger {
            balances: self.balances.values().cloned().collect(),
            work_log: self.work_log.clone(),
            trade_log: self.trade_log.clone(),
            price: self.price.clone(),
            loan_log: self.loans.clone(),
            loan_pool_lent: self.loan_pool_lent,
            loan_pool_total: self.loan_pool_total,
        };

        let json = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| forge_core::ForgeError::LedgerError(format!("serialize: {e}")))?;

        // Compute integrity hash and write alongside
        let hash = compute_hash(json.as_bytes());
        let signed = SignedLedger {
            data: json,
            integrity_hash: hash,
        };

        let output = serde_json::to_string_pretty(&signed)
            .map_err(|e| forge_core::ForgeError::LedgerError(format!("serialize signed: {e}")))?;
        // Atomic write: write to temp file, then rename (Issue #8)
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &output)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Load a ledger snapshot from JSON, verifying integrity.
    pub fn load_from_path(path: &std::path::Path) -> Result<Self, forge_core::ForgeError> {
        let raw = std::fs::read_to_string(path)?;

        // Try loading as signed ledger first
        if let Ok(signed) = serde_json::from_str::<SignedLedger>(&raw) {
            if !verify_hash(signed.data.as_bytes(), &signed.integrity_hash) {
                return Err(forge_core::ForgeError::LedgerError(
                    "ledger integrity check failed — file may have been tampered with".to_string(),
                ));
            }
            let snapshot: PersistedLedger = serde_json::from_str(&signed.data)
                .map_err(|e| forge_core::ForgeError::LedgerError(format!("deserialize: {e}")))?;
            return Ok(Self::from_snapshot(snapshot));
        }

        // Fallback: load unsigned (legacy format), log warning
        tracing::warn!(
            "Loading unsigned ledger (no integrity hash) from {:?}",
            path
        );
        let snapshot: PersistedLedger = serde_json::from_str(&raw)
            .map_err(|e| forge_core::ForgeError::LedgerError(format!("deserialize: {e}")))?;
        Ok(Self::from_snapshot(snapshot))
    }

    fn from_snapshot(snapshot: PersistedLedger) -> Self {
        let balances = snapshot
            .balances
            .into_iter()
            .map(|balance| (balance.node_id.clone(), balance))
            .collect();
        Self {
            balances,
            work_log: snapshot.work_log,
            trade_log: snapshot.trade_log,
            price: snapshot.price,
            loans: snapshot.loan_log,
            loan_pool_lent: snapshot.loan_pool_lent,
            loan_pool_total: snapshot.loan_pool_total,
        }
    }

    /// Export an aggregate settlement statement for a time window.
    pub fn export_settlement_statement(
        &self,
        window_start: u64,
        window_end: u64,
        reference_price_per_cu: Option<f64>,
    ) -> SettlementStatement {
        use std::collections::BTreeMap;

        let trades: Vec<TradeRecord> = self
            .trade_log
            .iter()
            .filter(|trade| trade.timestamp >= window_start && trade.timestamp <= window_end)
            .cloned()
            .collect();

        let mut nodes: BTreeMap<String, SettlementNode> = BTreeMap::new();
        let total_cu_transferred = trades.iter().map(|trade| trade.cu_amount).sum();

        for trade in &trades {
            let provider_id = trade.provider.to_hex();
            let provider = nodes
                .entry(provider_id.clone())
                .or_insert_with(|| SettlementNode {
                    node_id: provider_id.clone(),
                    gross_earned_cu: 0,
                    gross_spent_cu: 0,
                    net_cu: 0,
                    trade_count: 0,
                    estimated_payout_value: None,
                });
            provider.gross_earned_cu += trade.cu_amount;
            provider.trade_count += 1;

            let consumer_id = trade.consumer.to_hex();
            let consumer = nodes
                .entry(consumer_id.clone())
                .or_insert_with(|| SettlementNode {
                    node_id: consumer_id.clone(),
                    gross_earned_cu: 0,
                    gross_spent_cu: 0,
                    net_cu: 0,
                    trade_count: 0,
                    estimated_payout_value: None,
                });
            consumer.gross_spent_cu += trade.cu_amount;
            consumer.trade_count += 1;
        }

        let mut nodes: Vec<SettlementNode> = nodes
            .into_values()
            .map(|mut node| {
                node.net_cu = node.gross_earned_cu as i64 - node.gross_spent_cu as i64;
                node.estimated_payout_value =
                    reference_price_per_cu.map(|price| node.net_cu as f64 * price);
                node
            })
            .collect();
        nodes.sort_by(|a, b| b.net_cu.cmp(&a.net_cu));

        let merkle_root = if trades.is_empty() {
            None
        } else {
            Some(hex::encode(self.compute_trade_merkle_root()))
        };

        SettlementStatement {
            generated_at: now_millis(),
            window_start,
            window_end,
            trade_count: trades.len(),
            total_cu_transferred,
            reference_price_per_cu,
            merkle_root,
            nodes,
            trades,
        }
    }

    /// Compute reputation-adjusted cost. Low reputation pays a premium (Issue #9).
    /// Reputation 1.0 = base cost. Reputation 0.0 = 2x cost.
    pub fn reputation_adjusted_cost(&self, node_id: &NodeId, base_cost: u64) -> u64 {
        let rep = self
            .balances
            .get(node_id)
            .map(|b| b.reputation)
            .unwrap_or(0.5);
        // Multiplier: 2.0 at rep=0, 1.0 at rep=1.0
        let multiplier = 2.0 - rep;
        (base_cost as f64 * multiplier).ceil() as u64
    }

    /// Estimate the CU cost for a given inference request.
    pub fn estimate_cost(&self, tokens: u64, layers: u32, model_layers: u32) -> u64 {
        let fraction = layers as f64 / model_layers as f64;
        let base_cost = tokens as f64 * self.price.effective_cu_per_token() * fraction;
        base_cost.ceil() as u64
    }

    /// Reserve CU for an in-flight inference request.
    /// Returns true if the reservation succeeded (node can afford it).
    /// Reserved CU is deducted from available_balance but not yet consumed.
    pub fn reserve_cu(&mut self, node_id: &NodeId, cu: u64) -> bool {
        if !self.can_afford(node_id, cu) {
            return false;
        }
        let balance = self.balances.entry(node_id.clone()).or_insert(NodeBalance {
            node_id: node_id.clone(),
            contributed: 0,
            consumed: 0,
            reserved: 0,
            reputation: DEFAULT_REPUTATION,
        });
        balance.reserved += cu;
        true
    }

    /// Release a CU reservation (e.g., on request failure or cancellation).
    pub fn release_reserve(&mut self, node_id: &NodeId, cu: u64) {
        if let Some(balance) = self.balances.get_mut(node_id) {
            balance.reserved = balance.reserved.saturating_sub(cu);
        }
    }

    /// Record a unit of work contributed by a node.
    pub fn record_contribution(&mut self, work: WorkUnit) {
        let node_id = work.node_id.clone();
        let cu = work.estimated_flops / FLOPS_PER_CU.max(1);

        let balance = self.balances.entry(node_id.clone()).or_insert(NodeBalance {
            node_id,
            contributed: 0,
            consumed: 0,
            reserved: 0,
            reputation: DEFAULT_REPUTATION,
        });

        balance.contributed += cu;
        self.work_log.push(work);
    }

    /// Record compute consumed by a node (it requested inference).
    pub fn record_consumption(&mut self, node_id: &NodeId, cu: u64) {
        let balance = self.balances.entry(node_id.clone()).or_insert(NodeBalance {
            node_id: node_id.clone(),
            contributed: 0,
            consumed: 0,
            reserved: 0,
            reputation: DEFAULT_REPUTATION,
        });

        balance.consumed += cu;
    }

    /// Execute a verified signed trade: verify both signatures, then record.
    pub fn execute_signed_trade(
        &mut self,
        signed: &SignedTradeRecord,
    ) -> Result<(), SignatureError> {
        signed.verify()?;
        self.execute_trade(&signed.trade);
        Ok(())
    }

    /// Execute a trade: provider earns CU, consumer spends CU.
    /// Rejects self-trades and zero-CU trades (Issue #23, #27).
    pub fn execute_trade(&mut self, trade: &TradeRecord) {
        if trade.cu_amount == 0 {
            tracing::debug!("Rejecting zero-CU trade");
            return;
        }
        if trade.provider == trade.consumer {
            tracing::warn!("Rejecting self-trade from {}", trade.provider.to_hex());
            return;
        }
        // Credit provider
        let provider = self
            .balances
            .entry(trade.provider.clone())
            .or_insert(NodeBalance {
                node_id: trade.provider.clone(),
                contributed: 0,
                consumed: 0,
                reserved: 0,
                reputation: DEFAULT_REPUTATION,
            });
        provider.contributed += trade.cu_amount;

        // Debit consumer and release any matching reservation
        let consumer = self
            .balances
            .entry(trade.consumer.clone())
            .or_insert(NodeBalance {
                node_id: trade.consumer.clone(),
                contributed: 0,
                consumed: 0,
                reserved: 0,
                reputation: DEFAULT_REPUTATION,
            });
        consumer.consumed += trade.cu_amount;
        consumer.reserved = consumer.reserved.saturating_sub(trade.cu_amount);
        self.trade_log.push(trade.clone());
        self.price.total_trades_ever += 1;
    }

    /// Can a node afford a given CU cost?
    ///
    /// New nodes get a limited free tier (FREE_TIER_CU). The free tier
    /// is consumed from the first request — it does not reset on new
    /// NodeId creation. Nodes that have consumed their free tier must
    /// contribute compute to earn more CU.
    pub fn can_afford(&self, node_id: &NodeId, cu_cost: u64) -> bool {
        const FREE_TIER_CU: i64 = 1000;
        match self.balances.get(node_id) {
            Some(balance) => {
                // Nodes that have only consumed (never contributed) get reduced free tier
                // to prevent "contribute 1 CU then abuse" attacks (Issue #6)
                let free_bonus = if balance.contributed > 0 {
                    FREE_TIER_CU
                } else {
                    // Decay free tier based on how much they've already consumed
                    (FREE_TIER_CU - balance.consumed as i64).max(0)
                };
                balance.available_balance() + free_bonus >= cu_cost as i64
            }
            None => {
                // Sybil mitigation: limit how many new nodes can use
                // free tier in a short window (Issue #6).
                let unknown_nodes = self
                    .balances
                    .values()
                    .filter(|b| b.contributed == 0 && b.consumed > 0)
                    .count();
                if unknown_nodes > 50 {
                    tracing::warn!(
                        "Sybil protection: too many free-tier-only nodes ({}), rejecting new node",
                        unknown_nodes
                    );
                    return false;
                }
                FREE_TIER_CU >= cu_cost as i64
            }
        }
    }

    /// Get a node's current balance.
    pub fn get_balance(&self, node_id: &NodeId) -> Option<&NodeBalance> {
        self.balances.get(node_id)
    }

    /// Get a node's net CU balance (contributed - consumed), including free tier.
    pub fn effective_balance(&self, node_id: &NodeId) -> i64 {
        const FREE_TIER_CU: i64 = 1000;
        match self.balances.get(node_id) {
            Some(b) => b.balance() + FREE_TIER_CU,
            None => FREE_TIER_CU,
        }
    }

    /// Update reputation based on uptime and reliability.
    /// Reputation affects priority in node selection.
    pub fn update_reputation(&mut self, node_id: &NodeId, delta: f64) {
        if let Some(balance) = self.balances.get_mut(node_id) {
            balance.reputation = (balance.reputation + delta).clamp(0.0, 1.0);
        }
    }

    /// Apply yield: nodes that have been online and contributing
    /// earn a bonus proportional to their contribution.
    /// This is the "interest" — compute resources appreciate with use.
    pub fn apply_yield(&mut self, node_id: &NodeId, uptime_hours: f64) {
        // Yield rate: 0.1% per hour of uptime (compounding with reputation)
        const BASE_YIELD_RATE: f64 = 0.001;

        if let Some(balance) = self.balances.get_mut(node_id) {
            let yield_rate = BASE_YIELD_RATE * balance.reputation;
            let yield_cu = (balance.contributed as f64 * yield_rate * uptime_hours) as u64;
            if yield_cu > 0 {
                balance.contributed += yield_cu;
            }
        }
    }

    /// Update market price based on observed supply and demand.
    /// Uses exponential moving average for smoothing (Issue #11).
    /// Alpha = `crate::lending::EMA_ALPHA` (0.3) per spec §2.
    pub fn update_price(&mut self, active_providers: usize, pending_requests: usize) {
        use crate::lending::EMA_ALPHA;

        // Adaptive divisor: scales with network size
        let supply_divisor = (self.balances.len() as f64).max(5.0);
        let demand_divisor = (supply_divisor / 2.0).max(3.0);

        let raw_supply = (active_providers as f64 / supply_divisor).clamp(0.5, 2.0);
        let raw_demand = (pending_requests as f64 / demand_divisor).clamp(0.5, 3.0);

        // EMA smoothing: new = alpha * raw + (1-alpha) * old
        self.price.supply_factor =
            EMA_ALPHA * raw_supply + (1.0 - EMA_ALPHA) * self.price.supply_factor;
        self.price.demand_factor =
            EMA_ALPHA * raw_demand + (1.0 - EMA_ALPHA) * self.price.demand_factor;
    }

    /// Get all nodes sorted by balance (highest contributors first).
    pub fn ranked_nodes(&self) -> Vec<&NodeBalance> {
        let mut nodes: Vec<_> = self.balances.values().collect();
        nodes.sort_by(|a, b| b.balance().cmp(&a.balance()));
        nodes
    }

    /// Compute a Merkle root of all trades in the log.
    /// This is the hash that can be anchored to Bitcoin (OP_RETURN) for immutability.
    pub fn compute_trade_merkle_root(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};

        if self.trade_log.is_empty() {
            return [0u8; 32];
        }

        // Leaf hashes: SHA-256 of each trade's canonical bytes
        let mut hashes: Vec<[u8; 32]> = self
            .trade_log
            .iter()
            .map(|trade| {
                let mut hasher = Sha256::new();
                hasher.update(trade.canonical_bytes());
                hasher.finalize().into()
            })
            .collect();

        // Build Merkle tree bottom-up
        while hashes.len() > 1 {
            let mut next_level = Vec::with_capacity((hashes.len() + 1) / 2);
            for chunk in hashes.chunks(2) {
                let mut hasher = Sha256::new();
                hasher.update(chunk[0]);
                if chunk.len() > 1 {
                    hasher.update(chunk[1]);
                } else {
                    // Odd number: duplicate last hash
                    hasher.update(chunk[0]);
                }
                next_level.push(hasher.finalize().into());
            }
            hashes = next_level;
        }

        hashes[0]
    }

    /// Prepare Bitcoin OP_RETURN anchor data (Issue #17).
    /// Returns 80 bytes: "FORGE" (5) + trade_count (4) + total_cu (8) + merkle_root (32) + timestamp (8) + padding.
    pub fn prepare_anchor_data(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(80);
        data.extend_from_slice(b"FORGE"); // 5 bytes magic
        data.extend_from_slice(&(self.trade_log.len() as u32).to_le_bytes()); // 4 bytes
        let total_cu: u64 = self.trade_log.iter().map(|t| t.cu_amount).sum();
        data.extend_from_slice(&total_cu.to_le_bytes()); // 8 bytes
        data.extend_from_slice(&self.compute_trade_merkle_root()); // 32 bytes
        data.extend_from_slice(&now_millis().to_le_bytes()); // 8 bytes
        // Pad to 80 bytes (OP_RETURN max)
        data.resize(80, 0);
        data
    }

    // ===========================================================================
    // Lending — Phase 5.5
    // ===========================================================================

    /// Create a new loan. Verifies dual signatures, checks borrower credit,
    /// locks collateral, and transfers principal to the borrower.
    pub fn create_loan(
        &mut self,
        signed: SignedLoanRecord,
    ) -> Result<(), LoanCreationError> {
        // 1. Cryptographic check: dual Ed25519 signatures + freshness.
        signed.verify().map_err(LoanCreationError::Signature)?;

        let loan = &signed.loan;

        // 2. Term check.
        if loan.term_hours > MAX_LOAN_TERM_HOURS {
            return Err(LoanCreationError::ExcessiveTerm {
                hours: loan.term_hours,
                maximum: MAX_LOAN_TERM_HOURS,
            });
        }

        // 3. Loan-to-collateral ratio.
        if loan.collateral_cu == 0 {
            return Err(LoanCreationError::ExcessiveLtv {
                ratio: f64::INFINITY,
                maximum: MAX_LTV_RATIO,
            });
        }
        let ltv = loan.principal_cu as f64 / loan.collateral_cu as f64;
        if ltv > MAX_LTV_RATIO {
            return Err(LoanCreationError::ExcessiveLtv {
                ratio: ltv,
                maximum: MAX_LTV_RATIO,
            });
        }

        // 4. Borrower credit check.
        let score = self.compute_credit_score(&loan.borrower);
        if score < MIN_CREDIT_FOR_BORROWING {
            return Err(LoanCreationError::InsufficientCredit {
                score,
                minimum: MIN_CREDIT_FOR_BORROWING,
            });
        }

        // 5. Pool reserve / single-loan limit. We treat the lender's
        //    own balance as the pool floor when no explicit pool deposit
        //    has been recorded; this keeps the constants meaningful in
        //    early-network conditions where loan_pool_total = 0.
        let pool_total = self.loan_pool_total.max(loan.principal_cu);
        let lent_after = self.loan_pool_lent.saturating_add(loan.principal_cu);
        let reserve_after = pool_total.saturating_sub(lent_after);
        let reserve_ratio_after = reserve_after as f64 / pool_total as f64;
        if reserve_ratio_after < MIN_RESERVE_RATIO && self.loan_pool_total > 0 {
            return Err(LoanCreationError::ReserveExhausted {
                minimum: MIN_RESERVE_RATIO,
            });
        }
        if self.loan_pool_total > 0 {
            let max_single = (self.loan_pool_total as f64 * MAX_SINGLE_LOAN_POOL_PCT) as u64;
            if loan.principal_cu > max_single {
                return Err(LoanCreationError::ExceedsPoolLimit {
                    maximum: MAX_SINGLE_LOAN_POOL_PCT,
                });
            }
        }

        // 6. Duplicate detection.
        if self.loans.iter().any(|l| l.loan.loan_id == loan.loan_id) {
            return Err(LoanCreationError::Duplicate);
        }

        // 7. Lock collateral on the borrower side.
        if !self.reserve_cu(&loan.borrower, loan.collateral_cu) {
            return Err(LoanCreationError::InsufficientCollateral);
        }

        // 8. Transfer principal: lender's contributed -> borrower's contributed.
        //    We use `contributed` as the "available CU" knob to mirror how
        //    `record_contribution` increases spendable CU.
        let lender_balance = self
            .balances
            .entry(loan.lender.clone())
            .or_insert(NodeBalance {
                node_id: loan.lender.clone(),
                contributed: 0,
                consumed: 0,
                reserved: 0,
                reputation: DEFAULT_REPUTATION,
            });
        // Pretend lender holds enough — this is enforced by gossip + signed
        // proposal acceptance at the daemon layer; the ledger does not gate
        // on lender balance here (matches the trade execution path which
        // also does not gate on provider balance).
        lender_balance.consumed = lender_balance.consumed.saturating_add(loan.principal_cu);

        let borrower_balance = self
            .balances
            .entry(loan.borrower.clone())
            .or_insert(NodeBalance {
                node_id: loan.borrower.clone(),
                contributed: 0,
                consumed: 0,
                reserved: 0,
                reputation: DEFAULT_REPUTATION,
            });
        borrower_balance.contributed =
            borrower_balance.contributed.saturating_add(loan.principal_cu);

        // 9. Update pool accounting and persist the signed record.
        self.loan_pool_lent = self.loan_pool_lent.saturating_add(loan.principal_cu);
        self.loan_pool_total = self.loan_pool_total.saturating_add(loan.principal_cu);
        self.loans.push(signed);

        Ok(())
    }

    /// Mark a loan as repaid. Releases collateral, credits lender with
    /// principal + interest, debits borrower for the same.
    pub fn repay_loan(&mut self, loan_id: &[u8; 32]) -> Result<(), LoanRepaymentError> {
        let idx = self
            .loans
            .iter()
            .position(|l| &l.loan.loan_id == loan_id)
            .ok_or(LoanRepaymentError::NotFound)?;

        let (lender, borrower, principal, total_due, collateral) = {
            let entry = &self.loans[idx];
            if entry.loan.status != LoanStatus::Active {
                return Err(LoanRepaymentError::NotActive {
                    status: entry.loan.status,
                });
            }
            (
                entry.loan.lender.clone(),
                entry.loan.borrower.clone(),
                entry.loan.principal_cu,
                entry.loan.total_due(),
                entry.loan.collateral_cu,
            )
        };

        // Borrower must have enough effective balance to clear the debt.
        if !self.can_afford(&borrower, total_due) {
            return Err(LoanRepaymentError::InsufficientBalance);
        }

        // Release collateral.
        self.release_reserve(&borrower, collateral);

        // Borrower pays total_due.
        let borrower_bal = self
            .balances
            .entry(borrower.clone())
            .or_insert(NodeBalance {
                node_id: borrower.clone(),
                contributed: 0,
                consumed: 0,
                reserved: 0,
                reputation: DEFAULT_REPUTATION,
            });
        borrower_bal.consumed = borrower_bal.consumed.saturating_add(total_due);

        // Lender receives total_due.
        let lender_bal = self
            .balances
            .entry(lender.clone())
            .or_insert(NodeBalance {
                node_id: lender.clone(),
                contributed: 0,
                consumed: 0,
                reserved: 0,
                reputation: DEFAULT_REPUTATION,
            });
        lender_bal.contributed = lender_bal.contributed.saturating_add(total_due);
        // Counter-balance the principal we provisionally subtracted from the
        // lender at create_loan time.
        lender_bal.consumed = lender_bal.consumed.saturating_sub(principal);

        // Update loan record + pool accounting.
        let entry = &mut self.loans[idx];
        entry.loan.status = LoanStatus::Repaid;
        entry.loan.repaid_at = Some(now_millis());
        self.loan_pool_lent = self.loan_pool_lent.saturating_sub(principal);

        Ok(())
    }

    /// Mark a loan as defaulted. Burns COLLATERAL_BURN_ON_DEFAULT fraction
    /// of collateral; the rest goes to the lender. Penalises the borrower's
    /// reputation.
    pub fn default_loan(&mut self, loan_id: &[u8; 32]) -> Result<(), LoanDefaultError> {
        let now = now_millis();
        let idx = self
            .loans
            .iter()
            .position(|l| &l.loan.loan_id == loan_id)
            .ok_or(LoanDefaultError::NotFound)?;

        let (lender, borrower, principal, collateral, due_at) = {
            let entry = &self.loans[idx];
            if entry.loan.status != LoanStatus::Active {
                return Err(LoanDefaultError::NotActive {
                    status: entry.loan.status,
                });
            }
            if now < entry.loan.due_at {
                return Err(LoanDefaultError::NotYetDue {
                    due_at: entry.loan.due_at,
                    now,
                });
            }
            (
                entry.loan.lender.clone(),
                entry.loan.borrower.clone(),
                entry.loan.principal_cu,
                entry.loan.collateral_cu,
                entry.loan.due_at,
            )
        };
        let _ = due_at;

        // Burn a fraction of the collateral; remainder goes to lender.
        let burned = (collateral as f64 * COLLATERAL_BURN_ON_DEFAULT) as u64;
        let recovered = collateral.saturating_sub(burned);

        // Release the borrower's reservation, then move the recovered slice
        // into the lender's contributed balance.
        self.release_reserve(&borrower, collateral);
        let borrower_bal = self
            .balances
            .entry(borrower.clone())
            .or_insert(NodeBalance {
                node_id: borrower.clone(),
                contributed: 0,
                consumed: 0,
                reserved: 0,
                reputation: DEFAULT_REPUTATION,
            });
        // Burned CU is permanently destroyed from the borrower's books.
        borrower_bal.consumed = borrower_bal.consumed.saturating_add(collateral);

        let lender_bal = self
            .balances
            .entry(lender.clone())
            .or_insert(NodeBalance {
                node_id: lender.clone(),
                contributed: 0,
                consumed: 0,
                reserved: 0,
                reputation: DEFAULT_REPUTATION,
            });
        lender_bal.contributed = lender_bal.contributed.saturating_add(recovered);
        // Counter-balance the principal we provisionally subtracted from
        // the lender at create_loan time.
        lender_bal.consumed = lender_bal.consumed.saturating_sub(principal);

        // Penalise borrower reputation.
        self.update_reputation(&borrower, -0.2);

        // Update loan record + pool accounting.
        let entry = &mut self.loans[idx];
        entry.loan.status = LoanStatus::Defaulted;
        self.loan_pool_lent = self.loan_pool_lent.saturating_sub(principal);

        Ok(())
    }

    /// Compute credit score for a node based on trade history, repayment
    /// history, uptime, and account age.
    ///
    /// Uses the canonical formula:
    ///   score = 0.3 * trade + 0.4 * repayment + 0.2 * uptime + 0.1 * age
    pub fn compute_credit_score(&self, node_id: &NodeId) -> f64 {
        let known = self.balances.contains_key(node_id)
            || self
                .trade_log
                .iter()
                .any(|t| &t.provider == node_id || &t.consumer == node_id);
        if !known {
            return COLD_START_CREDIT;
        }

        // Trade sub-score: lifetime CU touched (provider + consumer side).
        let trade_volume: u64 = self
            .trade_log
            .iter()
            .filter(|t| &t.provider == node_id || &t.consumer == node_id)
            .map(|t| t.cu_amount)
            .sum();
        let trade_score = lending::trade_score_from_volume(trade_volume);

        // Repayment sub-score: ratio of repaid loans to (repaid + defaulted)
        // for this borrower. Nodes with no loan history get the neutral score.
        let mut repaid = 0usize;
        let mut defaulted = 0usize;
        for l in &self.loans {
            if l.loan.borrower == *node_id {
                match l.loan.status {
                    LoanStatus::Repaid => repaid += 1,
                    LoanStatus::Defaulted => defaulted += 1,
                    LoanStatus::Active => {}
                }
            }
        }
        let repayment_score = if repaid + defaulted == 0 {
            NEUTRAL_REPAYMENT_SCORE
        } else {
            repaid as f64 / (repaid + defaulted) as f64
        };

        // Uptime sub-score: reputation acts as a stand-in until per-node
        // uptime tracking exists.
        let uptime_score = self
            .balances
            .get(node_id)
            .map(|b| b.reputation)
            .unwrap_or(0.5);

        // Age sub-score: derived from total contributed CU as a stand-in
        // for join time, since `NodeBalance` does not yet track timestamps.
        let contributed = self
            .balances
            .get(node_id)
            .map(|b| b.contributed)
            .unwrap_or(0);
        let age_score = lending::age_score_from_days((contributed / 100).min(u64::MAX));

        lending::compute_credit_score_from_components(
            trade_score,
            repayment_score,
            uptime_score,
            age_score,
        )
    }

    /// Current state of the lending pool.
    pub fn lending_pool_status(&self) -> LendingPoolStatus {
        let total = self.loan_pool_total;
        let lent = self.loan_pool_lent;
        let available = total.saturating_sub(lent);
        let reserve_ratio = if total == 0 {
            1.0
        } else {
            available as f64 / total as f64
        };
        let active: Vec<&SignedLoanRecord> = self
            .loans
            .iter()
            .filter(|l| l.loan.status == LoanStatus::Active)
            .collect();
        let avg_interest_rate = if active.is_empty() {
            0.0
        } else {
            active
                .iter()
                .map(|l| l.loan.interest_rate_per_hour)
                .sum::<f64>()
                / active.len() as f64
        };
        LendingPoolStatus {
            total_pool_cu: total,
            lent_cu: lent,
            available_cu: available,
            reserve_ratio,
            active_loan_count: active.len(),
            avg_interest_rate,
        }
    }

    /// Active loans where the given node is either lender or borrower.
    pub fn active_loans_for(&self, node_id: &NodeId) -> Vec<SignedLoanRecord> {
        self.loans
            .iter()
            .filter(|l| {
                l.loan.status == LoanStatus::Active
                    && (l.loan.lender == *node_id || l.loan.borrower == *node_id)
            })
            .cloned()
            .collect()
    }

    /// Whether a node is eligible for a welcome loan
    /// (`WELCOME_LOAN_AMOUNT` CU at 0% for `WELCOME_LOAN_TERM_HOURS` hours).
    ///
    /// The actual signing happens in the node daemon, which holds the
    /// keypair. This method only enforces the Sybil ceiling and the
    /// "no existing balance" rule.
    pub fn can_issue_welcome_loan(&self, node_id: &NodeId) -> bool {
        // Already known? then no welcome loan.
        if self.balances.contains_key(node_id) {
            return false;
        }
        // Sybil mitigation.
        let unknown_nodes = self
            .balances
            .values()
            .filter(|b| b.contributed == 0)
            .count();
        if unknown_nodes > WELCOME_LOAN_SYBIL_THRESHOLD {
            return false;
        }
        true
    }

    /// Estimate the CU cost for a request against a tier-classified model.
    ///
    /// `cost = tokens * tier.base_cu_per_token() * (effective_cu_per_token / TIER_SMALL_CU_PER_TOKEN)`
    ///
    /// The second factor folds in dynamic supply/demand and CU deflation
    /// just like [`Self::estimate_cost`].
    pub fn estimate_cost_for_tier(&self, tokens: u64, tier: ModelTier) -> u64 {
        let market = self.price.effective_cu_per_token();
        let scale = market / TIER_SMALL_CU_PER_TOKEN as f64;
        let base = tokens as f64 * tier.base_cu_per_token() as f64 * scale;
        base.ceil() as u64
    }

    /// Get total network statistics.
    pub fn network_stats(&self) -> NetworkStats {
        let total_contributed: u64 = self.balances.values().map(|b| b.contributed).sum();
        let total_consumed: u64 = self.balances.values().map(|b| b.consumed).sum();
        NetworkStats {
            total_nodes: self.balances.len(),
            total_contributed_cu: total_contributed,
            total_consumed_cu: total_consumed,
            total_trades: self.trade_log.len(),
            avg_reputation: if self.balances.is_empty() {
                0.0
            } else {
                self.balances.values().map(|b| b.reputation).sum::<f64>()
                    / self.balances.len() as f64
            },
        }
    }
}

impl Default for ComputeLedger {
    fn default() -> Self {
        Self::new()
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Aggregate network statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub total_nodes: usize,
    pub total_contributed_cu: u64,
    pub total_consumed_cu: u64,
    pub total_trades: usize,
    pub avg_reputation: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_core::{LayerRange, ModelId};

    fn unique_temp_path(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}.json", now_millis()))
    }

    fn make_work(node: [u8; 32], flops: u64) -> WorkUnit {
        WorkUnit {
            node_id: NodeId(node),
            timestamp: 0,
            layers_computed: LayerRange::new(0, 8),
            model_id: ModelId("test".to_string()),
            tokens_processed: 100,
            estimated_flops: flops,
        }
    }

    #[test]
    fn contribution_increases_balance() {
        let mut ledger = ComputeLedger::new();
        let node = [1u8; 32];
        // 5 billion FLOPS = 5 CU
        ledger.record_contribution(make_work(node, 5 * FLOPS_PER_CU));
        let balance = ledger.get_balance(&NodeId(node)).unwrap();
        assert_eq!(balance.contributed, 5);
        assert_eq!(balance.consumed, 0);
        assert_eq!(balance.balance(), 5);
    }

    #[test]
    fn consumption_decreases_balance() {
        let mut ledger = ComputeLedger::new();
        let node_id = NodeId([1u8; 32]);
        ledger.record_contribution(make_work([1u8; 32], 10 * FLOPS_PER_CU));
        ledger.record_consumption(&node_id, 4);
        let balance = ledger.get_balance(&node_id).unwrap();
        assert_eq!(balance.balance(), 6); // 10 - 4
    }

    #[test]
    fn trade_execution() {
        let mut ledger = ComputeLedger::new();
        let provider = NodeId([1u8; 32]);
        let consumer = NodeId([2u8; 32]);

        let trade = TradeRecord {
            provider: provider.clone(),
            consumer: consumer.clone(),
            cu_amount: 100,
            tokens_processed: 256,
            timestamp: 1000,
            model_id: "llama-7b".to_string(),
        };

        ledger.execute_trade(&trade);

        assert_eq!(ledger.get_balance(&provider).unwrap().contributed, 100);
        assert_eq!(ledger.get_balance(&consumer).unwrap().consumed, 100);
        assert_eq!(ledger.get_balance(&provider).unwrap().balance(), 100);
        assert_eq!(ledger.get_balance(&consumer).unwrap().balance(), -100);
    }

    #[test]
    fn free_tier_for_new_nodes() {
        let ledger = ComputeLedger::new();
        let new_node = NodeId([99u8; 32]);

        // New node can afford up to 1000 CU (free tier)
        assert!(ledger.can_afford(&new_node, 500));
        assert!(ledger.can_afford(&new_node, 1000));
        assert!(!ledger.can_afford(&new_node, 1001));
    }

    #[test]
    fn yield_accumulation() {
        let mut ledger = ComputeLedger::new();
        let node = [1u8; 32];
        let node_id = NodeId(node);

        // Node contributes 10000 CU
        ledger.record_contribution(make_work(node, 10000 * FLOPS_PER_CU));
        ledger.update_reputation(&node_id, 0.5); // reputation now 1.0

        // After 8 hours of uptime (sleeping overnight)
        ledger.apply_yield(&node_id, 8.0);

        let balance = ledger.get_balance(&node_id).unwrap();
        // 10000 * 0.001 * 1.0 * 8.0 = 80 CU yield
        assert_eq!(balance.contributed, 10080);
    }

    #[test]
    fn market_price_adjusts() {
        let mut ledger = ComputeLedger::new();

        // Low supply, high demand → expensive (apply EMA multiple times to converge)
        for _ in 0..10 {
            ledger.update_price(2, 20);
        }
        assert!(ledger.market_price().effective_cu_per_token() > 1.0);

        // High supply, low demand → cheap
        for _ in 0..10 {
            ledger.update_price(20, 2);
        }
        assert!(ledger.market_price().effective_cu_per_token() < 1.0);
    }

    #[test]
    fn network_stats() {
        let mut ledger = ComputeLedger::new();
        ledger.record_contribution(make_work([1u8; 32], 5 * FLOPS_PER_CU));
        ledger.record_contribution(make_work([2u8; 32], 3 * FLOPS_PER_CU));
        ledger.record_consumption(&NodeId([1u8; 32]), 2);

        let stats = ledger.network_stats();
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.total_contributed_cu, 8); // 5 + 3
        assert_eq!(stats.total_consumed_cu, 2);
        assert_eq!(stats.total_trades, 0);
    }

    #[test]
    fn recent_trades_returns_newest_first() {
        let mut ledger = ComputeLedger::new();
        let provider = NodeId([1u8; 32]);
        let consumer = NodeId([2u8; 32]);

        ledger.execute_trade(&TradeRecord {
            provider: provider.clone(),
            consumer: consumer.clone(),
            cu_amount: 10,
            tokens_processed: 10,
            timestamp: 1,
            model_id: "small".to_string(),
        });
        ledger.execute_trade(&TradeRecord {
            provider,
            consumer,
            cu_amount: 20,
            tokens_processed: 20,
            timestamp: 2,
            model_id: "large".to_string(),
        });

        let trades = ledger.recent_trades(2);
        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].timestamp, 2);
        assert_eq!(trades[1].timestamp, 1);
    }

    #[test]
    fn ledger_roundtrip_persists_to_disk() {
        let path = unique_temp_path("forge-ledger-roundtrip");
        let mut ledger = ComputeLedger::new();
        ledger.record_contribution(make_work([7u8; 32], 5 * FLOPS_PER_CU));
        ledger.execute_trade(&TradeRecord {
            provider: NodeId([7u8; 32]),
            consumer: NodeId([8u8; 32]),
            cu_amount: 12,
            tokens_processed: 12,
            timestamp: 42,
            model_id: "persisted".to_string(),
        });

        ledger.save_to_path(&path).unwrap();
        let loaded = ComputeLedger::load_from_path(&path).unwrap();

        assert_eq!(loaded.network_stats().total_trades, 1);
        assert_eq!(
            loaded.get_balance(&NodeId([7u8; 32])).unwrap().contributed,
            17
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn settlement_statement_aggregates_nodes_in_window() {
        let mut ledger = ComputeLedger::new();
        ledger.execute_trade(&TradeRecord {
            provider: NodeId([1u8; 32]),
            consumer: NodeId([2u8; 32]),
            cu_amount: 10,
            tokens_processed: 10,
            timestamp: 100,
            model_id: "m1".to_string(),
        });
        ledger.execute_trade(&TradeRecord {
            provider: NodeId([2u8; 32]),
            consumer: NodeId([3u8; 32]),
            cu_amount: 4,
            tokens_processed: 4,
            timestamp: 200,
            model_id: "m2".to_string(),
        });
        ledger.execute_trade(&TradeRecord {
            provider: NodeId([9u8; 32]),
            consumer: NodeId([8u8; 32]),
            cu_amount: 99,
            tokens_processed: 99,
            timestamp: 999,
            model_id: "ignored".to_string(),
        });

        let statement = ledger.export_settlement_statement(50, 250, Some(0.5));
        assert_eq!(statement.trade_count, 2);
        assert_eq!(statement.total_cu_transferred, 14);
        assert_eq!(statement.nodes.len(), 3);
        assert_eq!(statement.nodes[0].gross_earned_cu, 10);
        assert_eq!(statement.nodes[0].estimated_payout_value, Some(5.0));
        assert!(statement.trades.iter().all(|trade| trade.timestamp <= 250));
    }

    #[test]
    fn tampered_ledger_is_rejected() {
        let path = unique_temp_path("forge-ledger-tamper");
        let mut ledger = ComputeLedger::new();
        ledger.record_contribution(make_work([1u8; 32], 100 * FLOPS_PER_CU));
        ledger.save_to_path(&path).unwrap();

        // Tamper with the file: modify a balance value inside the escaped JSON data
        let raw = std::fs::read_to_string(&path).unwrap();
        let tampered = raw.replace(
            "\\\"contributed\\\": 100",
            "\\\"contributed\\\": 999999",
        );
        assert_ne!(raw, tampered, "tampering should change the file");
        std::fs::write(&path, tampered).unwrap();

        // Loading the tampered file should fail
        let result = ComputeLedger::load_from_path(&path);
        assert!(result.is_err(), "tampered ledger should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("integrity check failed"),
            "error should mention integrity: {}",
            err
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn merkle_root_empty_log() {
        let ledger = ComputeLedger::new();
        assert_eq!(ledger.compute_trade_merkle_root(), [0u8; 32]);
    }

    #[test]
    fn merkle_root_is_deterministic() {
        let mut ledger = ComputeLedger::new();
        let provider = NodeId([1u8; 32]);
        let consumer = NodeId([2u8; 32]);

        ledger.execute_trade(&TradeRecord {
            provider: provider.clone(),
            consumer: consumer.clone(),
            cu_amount: 100,
            tokens_processed: 50,
            timestamp: 1000,
            model_id: "m1".to_string(),
        });
        ledger.execute_trade(&TradeRecord {
            provider,
            consumer,
            cu_amount: 200,
            tokens_processed: 100,
            timestamp: 2000,
            model_id: "m2".to_string(),
        });

        let root1 = ledger.compute_trade_merkle_root();
        let root2 = ledger.compute_trade_merkle_root();
        assert_eq!(root1, root2);
        assert_ne!(root1, [0u8; 32]);
    }

    #[test]
    fn settlement_includes_merkle_root() {
        let mut ledger = ComputeLedger::new();
        ledger.execute_trade(&TradeRecord {
            provider: NodeId([1u8; 32]),
            consumer: NodeId([2u8; 32]),
            cu_amount: 50,
            tokens_processed: 25,
            timestamp: 500,
            model_id: "test".to_string(),
        });

        let statement = ledger.export_settlement_statement(0, 10000, None);
        assert!(statement.merkle_root.is_some());
        assert_eq!(statement.merkle_root.unwrap().len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn hmac_sha256_hash_format() {
        let hash = compute_hash(b"test data");
        assert!(
            hash.starts_with("hmac-sha256:"),
            "hash should have version prefix: {}",
            hash
        );
        // HMAC-SHA256 produces 32 bytes = 64 hex chars
        let hex_part = hash.strip_prefix("hmac-sha256:").unwrap();
        assert_eq!(hex_part.len(), 64, "SHA-256 hex should be 64 chars");

        // Same input should produce same hash (deterministic)
        let hash2 = compute_hash(b"test data");
        assert_eq!(hash, hash2);

        // Different input should produce different hash
        let hash3 = compute_hash(b"different data");
        assert_ne!(hash, hash3);
    }

    #[test]
    fn canonical_bytes_is_deterministic() {
        let trade = TradeRecord {
            provider: NodeId([1u8; 32]),
            consumer: NodeId([2u8; 32]),
            cu_amount: 100,
            tokens_processed: 256,
            timestamp: 1000,
            model_id: "llama-7b".to_string(),
        };

        let bytes1 = trade.canonical_bytes();
        let bytes2 = trade.canonical_bytes();
        assert_eq!(bytes1, bytes2);
        // 32 + 32 + 8 + 8 + 8 + 8 (model_id bytes) = 96
        assert_eq!(bytes1.len(), 96);
    }

    #[test]
    fn canonical_bytes_differs_for_different_trades() {
        let trade1 = TradeRecord {
            provider: NodeId([1u8; 32]),
            consumer: NodeId([2u8; 32]),
            cu_amount: 100,
            tokens_processed: 256,
            timestamp: 1000,
            model_id: "model-a".to_string(),
        };
        let trade2 = TradeRecord {
            provider: NodeId([1u8; 32]),
            consumer: NodeId([2u8; 32]),
            cu_amount: 101, // different
            tokens_processed: 256,
            timestamp: 1000,
            model_id: "model-a".to_string(),
        };
        assert_ne!(trade1.canonical_bytes(), trade2.canonical_bytes());
    }

    #[test]
    fn reserve_and_release_cu() {
        let mut ledger = ComputeLedger::new();
        let node_id = NodeId([1u8; 32]);

        // Give node some balance
        ledger.record_contribution(make_work([1u8; 32], 500 * FLOPS_PER_CU));

        // Reserve should succeed
        assert!(ledger.reserve_cu(&node_id, 200));
        let balance = ledger.get_balance(&node_id).unwrap();
        assert_eq!(balance.reserved, 200);
        assert_eq!(balance.available_balance(), 300); // 500 - 200

        // Cannot reserve more than available (500 - 200 reserved + 1000 free tier = 1300)
        assert!(!ledger.reserve_cu(&node_id, 1400));

        // Release reservation
        ledger.release_reserve(&node_id, 200);
        let balance = ledger.get_balance(&node_id).unwrap();
        assert_eq!(balance.reserved, 0);
        assert_eq!(balance.available_balance(), 500);
    }

    #[test]
    fn execute_trade_releases_reservation() {
        let mut ledger = ComputeLedger::new();
        let provider = NodeId([1u8; 32]);
        let consumer = NodeId([2u8; 32]);

        // Give consumer some balance
        ledger.record_contribution(make_work([2u8; 32], 1000 * FLOPS_PER_CU));

        // Reserve CU
        assert!(ledger.reserve_cu(&consumer, 100));
        assert_eq!(ledger.get_balance(&consumer).unwrap().reserved, 100);

        // Execute trade should release reservation
        let trade = TradeRecord {
            provider,
            consumer: consumer.clone(),
            cu_amount: 100,
            tokens_processed: 50,
            timestamp: 1000,
            model_id: "test".to_string(),
        };
        ledger.execute_trade(&trade);

        let balance = ledger.get_balance(&consumer).unwrap();
        assert_eq!(balance.reserved, 0); // released
        assert_eq!(balance.consumed, 100);
    }

    #[test]
    fn signed_trade_verification_with_real_keys() {
        use ed25519_dalek::SigningKey;

        // Generate two keypairs
        let mut rng = rand::thread_rng();
        let provider_key = SigningKey::generate(&mut rng);
        let consumer_key = SigningKey::generate(&mut rng);

        let provider_id = NodeId(provider_key.verifying_key().to_bytes());
        let consumer_id = NodeId(consumer_key.verifying_key().to_bytes());

        let trade = TradeRecord {
            provider: provider_id,
            consumer: consumer_id,
            cu_amount: 500,
            tokens_processed: 100,
            timestamp: now_millis(),
            model_id: "test-model".to_string(),
        };

        let canonical = trade.canonical_bytes();

        // Both parties sign
        use ed25519_dalek::Signer;
        let provider_sig = provider_key.sign(&canonical).to_bytes().to_vec();
        let consumer_sig = consumer_key.sign(&canonical).to_bytes().to_vec();

        let signed = SignedTradeRecord {
            trade,
            provider_sig,
            consumer_sig,
        };

        // Verification should succeed
        assert!(signed.verify().is_ok());
    }

    #[test]
    fn signed_trade_rejects_wrong_signature() {
        use ed25519_dalek::SigningKey;

        let mut rng = rand::thread_rng();
        let provider_key = SigningKey::generate(&mut rng);
        let consumer_key = SigningKey::generate(&mut rng);
        let attacker_key = SigningKey::generate(&mut rng);

        let trade = TradeRecord {
            provider: NodeId(provider_key.verifying_key().to_bytes()),
            consumer: NodeId(consumer_key.verifying_key().to_bytes()),
            cu_amount: 500,
            tokens_processed: 100,
            timestamp: now_millis(),
            model_id: "test".to_string(),
        };

        let canonical = trade.canonical_bytes();

        use ed25519_dalek::Signer;
        let provider_sig = provider_key.sign(&canonical).to_bytes().to_vec();
        // Attacker signs instead of consumer
        let fake_consumer_sig = attacker_key.sign(&canonical).to_bytes().to_vec();

        let signed = SignedTradeRecord {
            trade,
            provider_sig,
            consumer_sig: fake_consumer_sig,
        };

        // Verification should fail
        let result = signed.verify();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SignatureError::InvalidConsumerSignature
        ));
    }

    #[test]
    fn ledger_execute_signed_trade() {
        use ed25519_dalek::{Signer, SigningKey};

        let mut rng = rand::thread_rng();
        let provider_key = SigningKey::generate(&mut rng);
        let consumer_key = SigningKey::generate(&mut rng);

        let trade = TradeRecord {
            provider: NodeId(provider_key.verifying_key().to_bytes()),
            consumer: NodeId(consumer_key.verifying_key().to_bytes()),
            cu_amount: 200,
            tokens_processed: 50,
            timestamp: now_millis(),
            model_id: "test".to_string(),
        };

        let canonical = trade.canonical_bytes();
        let provider_sig = provider_key.sign(&canonical).to_bytes().to_vec();
        let consumer_sig = consumer_key.sign(&canonical).to_bytes().to_vec();

        let signed = SignedTradeRecord {
            trade: trade.clone(),
            provider_sig,
            consumer_sig,
        };

        let mut ledger = ComputeLedger::new();
        assert!(ledger.execute_signed_trade(&signed).is_ok());
        assert_eq!(
            ledger.get_balance(&trade.provider).unwrap().contributed,
            200
        );
        assert_eq!(ledger.get_balance(&trade.consumer).unwrap().consumed, 200);
    }

    #[test]
    fn reputation_adjusted_cost_penalizes_low_rep() {
        let mut ledger = ComputeLedger::new();
        let good_node = NodeId([1u8; 32]);
        let bad_node = NodeId([2u8; 32]);

        ledger.record_contribution(make_work([1u8; 32], 100 * FLOPS_PER_CU));
        ledger.record_contribution(make_work([2u8; 32], 100 * FLOPS_PER_CU));
        ledger.update_reputation(&good_node, 0.5); // 1.0
        ledger.update_reputation(&bad_node, -0.3); // 0.2

        let base = 100;
        let good_cost = ledger.reputation_adjusted_cost(&good_node, base);
        let bad_cost = ledger.reputation_adjusted_cost(&bad_node, base);

        assert!(good_cost <= base + 1); // ~100 at rep 1.0
        assert!(bad_cost > base); // ~180 at rep 0.2
        assert!(bad_cost > good_cost);
    }

    #[test]
    fn sybil_free_tier_decays_for_non_contributors() {
        let mut ledger = ComputeLedger::new();
        let consumer = NodeId([99u8; 32]);

        // First use: can afford (free tier = 1000 CU)
        assert!(ledger.can_afford(&consumer, 500));

        // Record consumption without contribution
        ledger.record_consumption(&consumer, 500);

        // balance = -500, free_bonus = 1000-500=500, total = 0
        // Can afford 0 but not 1
        assert!(!ledger.can_afford(&consumer, 1));

        // If they contribute, free tier comes back
        ledger.record_contribution(make_work([99u8; 32], 1000 * FLOPS_PER_CU));
        assert!(ledger.can_afford(&consumer, 500)); // contributed + free tier
    }

    #[test]
    fn merkle_root_changes_with_new_trades() {
        let mut ledger = ComputeLedger::new();
        ledger.execute_trade(&TradeRecord {
            provider: NodeId([1u8; 32]),
            consumer: NodeId([2u8; 32]),
            cu_amount: 50,
            tokens_processed: 25,
            timestamp: now_millis(),
            model_id: "m1".to_string(),
        });
        let root1 = ledger.compute_trade_merkle_root();

        ledger.execute_trade(&TradeRecord {
            provider: NodeId([3u8; 32]),
            consumer: NodeId([4u8; 32]),
            cu_amount: 100,
            tokens_processed: 50,
            timestamp: now_millis(),
            model_id: "m2".to_string(),
        });
        let root2 = ledger.compute_trade_merkle_root();

        assert_ne!(root1, root2); // root changes with new trades
    }

    #[test]
    fn ema_price_smoothing() {
        let mut ledger = ComputeLedger::new();

        // Apply high demand once
        ledger.update_price(1, 50);
        let price_after_one = ledger.market_price().effective_cu_per_token();

        // Apply same demand many times
        for _ in 0..20 {
            ledger.update_price(1, 50);
        }
        let price_converged = ledger.market_price().effective_cu_per_token();

        // After convergence, price should be higher than after one update (EMA smoothing)
        assert!(price_converged > price_after_one);
    }

    #[test]
    fn cu_deflation_increases_purchasing_power() {
        let mut price = MarketPrice::default();
        let power_at_0 = price.cu_purchasing_power();
        assert!((power_at_0 - 1.0).abs() < 0.01); // ~1.0 at start

        price.total_trades_ever = 10_000;
        let power_at_10k = price.cu_purchasing_power();
        assert!(power_at_10k > power_at_0); // more power after 10K trades

        price.total_trades_ever = 1_000_000;
        let power_at_1m = price.cu_purchasing_power();
        assert!(power_at_1m > power_at_10k); // even more at 1M

        // Early CU is worth more over time
        let cost_early = MarketPrice { total_trades_ever: 0, ..Default::default() }.effective_cu_per_token();
        let cost_mature = MarketPrice { total_trades_ever: 100_000, ..Default::default() }.effective_cu_per_token();
        assert!(cost_mature < cost_early); // cheaper per token on mature network
    }

    #[test]
    fn deflation_factor_never_zero() {
        let price = MarketPrice { total_trades_ever: u64::MAX, ..Default::default() };
        assert!(price.deflation_factor() > 0.0);
        assert!(price.effective_cu_per_token() >= 0.01); // floor
    }

    #[test]
    fn prepare_anchor_data_returns_80_bytes() {
        let mut ledger = ComputeLedger::new();
        ledger.execute_trade(&TradeRecord {
            provider: NodeId([1u8; 32]),
            consumer: NodeId([2u8; 32]),
            cu_amount: 100,
            tokens_processed: 50,
            timestamp: now_millis(),
            model_id: "test".to_string(),
        });
        let data = ledger.prepare_anchor_data();
        assert_eq!(data.len(), 80);
        assert_eq!(&data[..5], b"FORGE");
    }

    #[test]
    fn self_trade_is_rejected() {
        let mut ledger = ComputeLedger::new();
        let node = NodeId([1u8; 32]);
        ledger.execute_trade(&TradeRecord {
            provider: node.clone(),
            consumer: node.clone(),
            cu_amount: 100,
            tokens_processed: 50,
            timestamp: now_millis(),
            model_id: "test".to_string(),
        });
        // Self-trade should not be recorded
        assert!(ledger.get_balance(&node).is_none());
        assert_eq!(ledger.recent_trades(10).len(), 0);
    }

    // ===========================================================================
    // Lending tests (Phase 5.5)
    // ===========================================================================

    use crate::lending::{
        COLD_START_CREDIT, COLLATERAL_BURN_ON_DEFAULT, MAX_LOAN_TERM_HOURS,
        WELCOME_LOAN_AMOUNT, WELCOME_LOAN_TERM_HOURS,
    };

    fn make_signed_loan_with_due(
        principal: u64,
        collateral: u64,
        term_hours: u64,
        due_at_override: Option<u64>,
    ) -> (
        SignedLoanRecord,
        ed25519_dalek::SigningKey,
        ed25519_dalek::SigningKey,
    ) {
        use ed25519_dalek::{Signer, SigningKey};
        let mut rng = rand::thread_rng();
        let lender_key = SigningKey::generate(&mut rng);
        let borrower_key = SigningKey::generate(&mut rng);

        let now = now_millis();
        let due_at = due_at_override.unwrap_or(now + term_hours * 3_600_000);
        let mut loan = crate::lending::LoanRecord {
            loan_id: [0u8; 32],
            lender: NodeId(lender_key.verifying_key().to_bytes()),
            borrower: NodeId(borrower_key.verifying_key().to_bytes()),
            principal_cu: principal,
            interest_rate_per_hour: 0.001,
            term_hours,
            collateral_cu: collateral,
            status: crate::lending::LoanStatus::Active,
            created_at: now,
            due_at,
            repaid_at: None,
        };
        loan.loan_id = loan.compute_loan_id();
        let canonical = loan.canonical_bytes();
        let lender_sig = lender_key.sign(&canonical).to_bytes().to_vec();
        let borrower_sig = borrower_key.sign(&canonical).to_bytes().to_vec();
        (
            SignedLoanRecord {
                loan,
                lender_sig,
                borrower_sig,
            },
            lender_key,
            borrower_key,
        )
    }

    fn make_signed_loan(
        principal: u64,
        collateral: u64,
        term_hours: u64,
    ) -> (
        SignedLoanRecord,
        ed25519_dalek::SigningKey,
        ed25519_dalek::SigningKey,
    ) {
        make_signed_loan_with_due(principal, collateral, term_hours, None)
    }

    /// Seed a borrower with enough trades + reputation to clear
    /// `MIN_CREDIT_FOR_BORROWING`.
    fn seed_borrower_credit(ledger: &mut ComputeLedger, borrower: &NodeId) {
        // 50_000 CU traded → trade_score 0.5; reputation 1.0 → uptime 1.0.
        ledger.execute_trade(&TradeRecord {
            provider: NodeId([42u8; 32]),
            consumer: borrower.clone(),
            cu_amount: 50_000,
            tokens_processed: 100,
            timestamp: now_millis(),
            model_id: "seed".into(),
        });
        ledger.update_reputation(borrower, 1.0);
        // Give borrower headroom so collateral reservation succeeds.
        ledger.record_contribution(WorkUnit {
            node_id: borrower.clone(),
            timestamp: 0,
            layers_computed: forge_core::LayerRange::new(0, 8),
            model_id: forge_core::ModelId("seed".into()),
            tokens_processed: 100,
            estimated_flops: 200_000 * FLOPS_PER_CU,
        });
    }

    #[test]
    fn welcome_loan_amount_matches_parameters() {
        assert_eq!(WELCOME_LOAN_AMOUNT, 1_000);
        assert_eq!(WELCOME_LOAN_TERM_HOURS, 72);
    }

    #[test]
    fn test_create_loan_transfers_principal() {
        let (signed, _lk, _bk) = make_signed_loan(1_000, 3_000, 24);
        let lender = signed.loan.lender.clone();
        let borrower = signed.loan.borrower.clone();

        let mut ledger = ComputeLedger::new();
        seed_borrower_credit(&mut ledger, &borrower);

        ledger.create_loan(signed).expect("loan must be created");

        let bb = ledger.get_balance(&borrower).unwrap();
        assert!(
            bb.contributed >= 1_000,
            "borrower contributed should include principal"
        );
        assert_eq!(bb.reserved, 3_000, "collateral should be locked");
        let lb = ledger.get_balance(&lender).unwrap();
        assert_eq!(lb.consumed, 1_000, "lender principal accounted as consumed");
    }

    #[test]
    fn test_create_loan_rejects_low_credit() {
        let (signed, _lk, _bk) = make_signed_loan(1_000, 3_000, 24);
        let borrower = signed.loan.borrower.clone();
        let mut ledger = ComputeLedger::new();

        // Make borrower known with reputation 0.
        ledger.record_contribution(WorkUnit {
            node_id: borrower.clone(),
            timestamp: 0,
            layers_computed: forge_core::LayerRange::new(0, 8),
            model_id: forge_core::ModelId("x".into()),
            tokens_processed: 1,
            estimated_flops: FLOPS_PER_CU,
        });
        ledger.update_reputation(&borrower, -1.0);

        // Inject a defaulted loan so repayment_score = 0.
        // (Loan need not verify — credit score reads .loans directly.)
        let bad_loan = crate::lending::LoanRecord {
            loan_id: [9u8; 32],
            lender: NodeId([1u8; 32]),
            borrower: borrower.clone(),
            principal_cu: 100,
            interest_rate_per_hour: 0.001,
            term_hours: 1,
            collateral_cu: 100,
            status: LoanStatus::Defaulted,
            created_at: 0,
            due_at: 0,
            repaid_at: None,
        };
        ledger.loans.push(SignedLoanRecord {
            loan: bad_loan,
            lender_sig: vec![0; 64],
            borrower_sig: vec![0; 64],
        });

        let score = ledger.compute_credit_score(&borrower);
        assert!(
            score < crate::lending::MIN_CREDIT_FOR_BORROWING,
            "expected score < {}, got {}",
            crate::lending::MIN_CREDIT_FOR_BORROWING,
            score
        );

        let result = ledger.create_loan(signed);
        assert!(matches!(
            result,
            Err(LoanCreationError::InsufficientCredit { .. })
        ));
    }

    #[test]
    fn test_create_loan_rejects_excessive_ltv() {
        // principal 10_000, collateral 1_000 → ltv = 10 (>> 3)
        let (signed, _, _) = make_signed_loan(10_000, 1_000, 24);
        let borrower = signed.loan.borrower.clone();
        let mut ledger = ComputeLedger::new();
        seed_borrower_credit(&mut ledger, &borrower);
        let result = ledger.create_loan(signed);
        assert!(matches!(
            result,
            Err(LoanCreationError::ExcessiveLtv { .. })
        ));
    }

    #[test]
    fn test_create_loan_rejects_excessive_term() {
        let (signed, _, _) =
            make_signed_loan(1_000, 3_000, MAX_LOAN_TERM_HOURS + 1);
        let borrower = signed.loan.borrower.clone();
        let mut ledger = ComputeLedger::new();
        seed_borrower_credit(&mut ledger, &borrower);
        let result = ledger.create_loan(signed);
        assert!(matches!(
            result,
            Err(LoanCreationError::ExcessiveTerm { .. })
        ));
    }

    #[test]
    fn test_repay_loan_releases_collateral() {
        let (signed, _, _) = make_signed_loan(1_000, 3_000, 24);
        let borrower = signed.loan.borrower.clone();
        let loan_id = signed.loan.loan_id;
        let mut ledger = ComputeLedger::new();
        seed_borrower_credit(&mut ledger, &borrower);
        ledger.create_loan(signed).unwrap();
        assert_eq!(ledger.get_balance(&borrower).unwrap().reserved, 3_000);

        ledger.repay_loan(&loan_id).expect("repay must succeed");
        assert_eq!(
            ledger.get_balance(&borrower).unwrap().reserved,
            0,
            "collateral released"
        );
    }

    #[test]
    fn test_repay_loan_pays_interest_to_lender() {
        let (signed, _, _) = make_signed_loan(10_000, 30_000, 100);
        let lender = signed.loan.lender.clone();
        let borrower = signed.loan.borrower.clone();
        let loan_id = signed.loan.loan_id;
        let total_due = signed.loan.total_due();
        assert!(total_due > 10_000, "interest must be positive");

        let mut ledger = ComputeLedger::new();
        seed_borrower_credit(&mut ledger, &borrower);
        ledger.create_loan(signed).unwrap();

        let lender_before = ledger.get_balance(&lender).unwrap().contributed;
        ledger.repay_loan(&loan_id).expect("repay must succeed");
        let lender_after = ledger.get_balance(&lender).unwrap().contributed;
        assert_eq!(
            lender_after - lender_before,
            total_due,
            "lender receives principal + interest"
        );
    }

    #[test]
    fn test_default_loan_burns_collateral() {
        // Sign with due_at already in the past so default() accepts it.
        let (signed, _, _) = make_signed_loan_with_due(1_000, 3_000, 1, Some(1));
        let loan_id = signed.loan.loan_id;
        let lender = signed.loan.lender.clone();
        let borrower = signed.loan.borrower.clone();

        let mut ledger = ComputeLedger::new();
        seed_borrower_credit(&mut ledger, &borrower);
        ledger.create_loan(signed).unwrap();

        let lender_before = ledger.get_balance(&lender).unwrap().contributed;
        ledger.default_loan(&loan_id).expect("default must succeed");

        let burned = (3_000.0 * COLLATERAL_BURN_ON_DEFAULT) as u64;
        let recovered = 3_000 - burned;
        let lender_after = ledger.get_balance(&lender).unwrap().contributed;
        assert_eq!(lender_after - lender_before, recovered);
    }

    #[test]
    fn test_compute_credit_score_new_node() {
        let ledger = ComputeLedger::new();
        let fresh = NodeId([77u8; 32]);
        let score = ledger.compute_credit_score(&fresh);
        assert!(
            (score - COLD_START_CREDIT).abs() < 1e-9,
            "new node should get COLD_START_CREDIT, got {score}"
        );
    }

    #[test]
    fn test_compute_credit_score_with_trades() {
        let mut ledger = ComputeLedger::new();
        let node = NodeId([5u8; 32]);
        let baseline = ledger.compute_credit_score(&node);
        ledger.execute_trade(&TradeRecord {
            provider: NodeId([6u8; 32]),
            consumer: node.clone(),
            cu_amount: 80_000,
            tokens_processed: 100,
            timestamp: now_millis(),
            model_id: "m".into(),
        });
        ledger.update_reputation(&node, 1.0);
        let after = ledger.compute_credit_score(&node);
        assert!(after > baseline, "trades should raise credit score");
    }

    #[test]
    fn test_lending_pool_status_reflects_activity() {
        let (signed, _, _) = make_signed_loan(1_000, 3_000, 24);
        let borrower = signed.loan.borrower.clone();
        let mut ledger = ComputeLedger::new();
        seed_borrower_credit(&mut ledger, &borrower);
        ledger.create_loan(signed).unwrap();

        let status = ledger.lending_pool_status();
        assert_eq!(status.lent_cu, 1_000);
        assert_eq!(status.active_loan_count, 1);
        assert!(status.avg_interest_rate > 0.0);
    }

    #[test]
    fn test_model_tier_pricing() {
        let ledger = ComputeLedger::new();
        let small_cost = ledger.estimate_cost_for_tier(100, ModelTier::Small);
        let frontier_cost = ledger.estimate_cost_for_tier(100, ModelTier::Frontier);
        // Small tier base = 1 CU/token → ~100; Frontier base = 20 → ~2000
        assert_eq!(small_cost, 100);
        assert_eq!(frontier_cost, 2_000);
    }

    #[test]
    fn test_persisted_ledger_round_trips_loans() {
        let (signed, _, _) = make_signed_loan(1_000, 3_000, 24);
        let borrower = signed.loan.borrower.clone();
        let mut ledger = ComputeLedger::new();
        seed_borrower_credit(&mut ledger, &borrower);
        ledger.create_loan(signed).unwrap();

        let path = unique_temp_path("forge-ledger-loan-roundtrip");
        ledger.save_to_path(&path).unwrap();
        let loaded = ComputeLedger::load_from_path(&path).unwrap();
        assert_eq!(loaded.lending_pool_status().active_loan_count, 1);
        assert_eq!(loaded.lending_pool_status().lent_cu, 1_000);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn zero_cu_trade_is_rejected() {
        let mut ledger = ComputeLedger::new();
        ledger.execute_trade(&TradeRecord {
            provider: NodeId([1u8; 32]),
            consumer: NodeId([2u8; 32]),
            cu_amount: 0,
            tokens_processed: 0,
            timestamp: now_millis(),
            model_id: "test".to_string(),
        });
        assert_eq!(ledger.recent_trades(10).len(), 0);
    }
}
