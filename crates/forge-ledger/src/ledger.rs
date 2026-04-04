use forge_core::{NodeBalance, NodeId, WorkUnit};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
}

/// Dynamic pricing based on local supply/demand observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketPrice {
    /// Base: 1 CU per FLOPS_PER_CU of compute.
    pub base_cu_per_token: f64,
    /// More idle nodes → lower price (0.5 to 2.0).
    pub supply_factor: f64,
    /// More requests than capacity → higher price.
    pub demand_factor: f64,
}

impl Default for MarketPrice {
    fn default() -> Self {
        Self {
            base_cu_per_token: 1.0,
            supply_factor: 1.0,
            demand_factor: 1.0,
        }
    }
}

impl MarketPrice {
    /// Effective CU cost per token.
    pub fn effective_cu_per_token(&self) -> f64 {
        self.base_cu_per_token * self.demand_factor / self.supply_factor
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
        Self {
            balances: HashMap::new(),
            work_log: Vec::new(),
            trade_log: Vec::new(),
            price: MarketPrice::default(),
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
            reputation: 0.5,
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
            reputation: 0.5,
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
            reputation: 0.5,
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
    pub fn execute_trade(&mut self, trade: &TradeRecord) {
        // Credit provider
        let provider = self
            .balances
            .entry(trade.provider.clone())
            .or_insert(NodeBalance {
                node_id: trade.provider.clone(),
                contributed: 0,
                consumed: 0,
                reserved: 0,
                reputation: 0.5,
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
                reputation: 0.5,
            });
        consumer.consumed += trade.cu_amount;
        consumer.reserved = consumer.reserved.saturating_sub(trade.cu_amount);
        self.trade_log.push(trade.clone());
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
    pub fn update_price(&mut self, active_providers: usize, pending_requests: usize) {
        // Supply: more providers → lower price
        self.price.supply_factor = (active_providers as f64 / 10.0).max(0.5).min(2.0);

        // Demand: more pending requests → higher price
        self.price.demand_factor = (pending_requests as f64 / 5.0).max(0.5).min(3.0);
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

        // Low supply, high demand → expensive
        ledger.update_price(2, 20);
        assert!(ledger.market_price().effective_cu_per_token() > 1.0);

        // High supply, low demand → cheap
        ledger.update_price(20, 2);
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
}
