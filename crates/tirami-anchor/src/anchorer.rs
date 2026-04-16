//! The periodic anchoring task.
//!
//! Runs inside `TiramiNode`. Every `interval` seconds it:
//! 1. Reads the ledger's current trade log
//! 2. Computes BatchDeltas (net TRM per node + FLOP total + Merkle root)
//! 3. Submits via the configured ChainClient
//!
//! Runs forever until the owning node shuts down; errors are logged but
//! don't crash the node (chain RPC can flap, inference must continue).

use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use tirami_core::NodeId;
use tirami_ledger::ComputeLedger;

use crate::client::{BatchSubmission, ChainClient, ChainError};
use crate::proof::{BatchDeltas, NodeDelta};

/// Configuration for the anchor loop.
#[derive(Debug, Clone)]
pub struct AnchorerConfig {
    /// Interval between anchor attempts.
    pub interval: Duration,
    /// Maximum trades to summarize per batch. If the trade log grows beyond
    /// this we close the batch early and roll the remainder into the next.
    pub max_trades_per_batch: usize,
    /// Identity of this node (used as `submitter` on submissions).
    pub node_id: NodeId,
}

impl AnchorerConfig {
    /// Default anchor interval: 10 minutes (matches §14 parameters.md).
    pub fn ten_minutes(node_id: NodeId) -> Self {
        Self {
            interval: Duration::from_secs(600),
            max_trades_per_batch: 10_000,
            node_id,
        }
    }

    /// Fast cadence for tests and local dev.
    pub fn fast_test(node_id: NodeId) -> Self {
        Self {
            interval: Duration::from_millis(100),
            max_trades_per_batch: 100,
            node_id,
        }
    }
}

#[derive(Debug, Error)]
pub enum AnchoringError {
    #[error(transparent)]
    Chain(#[from] ChainError),
}

/// Periodic anchor task.
pub struct Anchorer<C: ChainClient> {
    config: AnchorerConfig,
    ledger: Arc<Mutex<ComputeLedger>>,
    chain: Arc<C>,
    /// Monotonic batch id counter. Persisted between anchor calls.
    next_batch_id: Mutex<u64>,
    /// Index into the trade log up to which the previous anchor covered.
    last_anchored_idx: Mutex<usize>,
}

impl<C: ChainClient> Anchorer<C> {
    pub fn new(
        config: AnchorerConfig,
        ledger: Arc<Mutex<ComputeLedger>>,
        chain: Arc<C>,
    ) -> Self {
        Self {
            config,
            ledger,
            chain,
            next_batch_id: Mutex::new(0),
            last_anchored_idx: Mutex::new(0),
        }
    }

    /// Build (but do not submit) the BatchDeltas for the current trade-log
    /// tail. Caller-visible for tests.
    pub async fn build_batch(&self) -> Option<BatchDeltas> {
        let ledger = self.ledger.lock().await;
        let trades = ledger.recent_trades(self.config.max_trades_per_batch);
        if trades.is_empty() {
            return None;
        }

        // Aggregate per-node deltas.
        use std::collections::HashMap;
        let mut per_node: HashMap<NodeId, (u64, u64, u64, u32)> = HashMap::new();
        let mut flops_total: u64 = 0;
        for t in &trades {
            let prov = per_node.entry(t.provider.clone()).or_default();
            prov.0 = prov.0.saturating_add(t.trm_amount);
            prov.2 = prov.2.saturating_add(t.flops_estimated);
            prov.3 = prov.3.saturating_add(1);

            let cons = per_node.entry(t.consumer.clone()).or_default();
            cons.1 = cons.1.saturating_add(t.trm_amount);
            cons.3 = cons.3.saturating_add(1);

            flops_total = flops_total.saturating_add(t.flops_estimated);
        }

        let node_deltas: Vec<NodeDelta> = per_node
            .into_iter()
            .map(|(node_id, (c, s, f, n))| NodeDelta {
                node_id,
                contributed_delta: c,
                consumed_delta: s,
                flops_in_batch: f,
                trade_count: n,
            })
            .collect();

        let merkle_root = ledger.compute_trade_merkle_root();

        let batch_id = {
            let mut g = self.next_batch_id.lock().await;
            let id = *g;
            *g = g.saturating_add(1);
            id
        };

        Some(BatchDeltas {
            batch_id,
            batch_closed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            node_deltas,
            trade_merkle_root: merkle_root,
            trade_count_total: trades.len() as u64,
            flops_total,
        })
    }

    /// Execute one anchoring cycle: build batch, submit to chain.
    pub async fn tick(&self) -> Result<Option<BatchSubmission>, AnchoringError> {
        let Some(deltas) = self.build_batch().await else {
            debug!("anchor tick: no trades to anchor");
            return Ok(None);
        };
        if !deltas.is_nonempty() {
            debug!("anchor tick: empty batch");
            return Ok(None);
        }
        let sub = self.chain.store_batch(&deltas, &self.config.node_id).await?;
        info!(
            batch_id = sub.batch_id,
            merkle_root = %sub.merkle_root_hex,
            node_count = sub.node_count,
            flops_total = sub.flops_total,
            "anchored batch on-chain"
        );
        // Record the advancement point in the trade log so the next batch
        // starts after this one. (We read by recent_trades, so here we
        // advance the idx marker for downstream consumers if needed.)
        let trades_in_batch = deltas.trade_count_total as usize;
        let mut last_idx = self.last_anchored_idx.lock().await;
        *last_idx = last_idx.saturating_add(trades_in_batch);
        Ok(Some(sub))
    }

    /// Run forever: tick at `config.interval` cadence.
    pub async fn run(self: Arc<Self>) {
        let mut ticker = tokio::time::interval(self.config.interval);
        // First tick fires immediately — skip it to match other Tirami loops.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if let Err(e) = self.tick().await {
                warn!("anchor tick failed: {e}");
            }
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockChainClient;
    use std::sync::Arc;
    use tirami_core::NodeId;
    use tirami_ledger::ComputeLedger;
    use tirami_ledger::TradeRecord;

    fn make_trade(provider: [u8; 32], consumer: [u8; 32], trm: u64, flops: u64) -> TradeRecord {
        TradeRecord {
            provider: NodeId(provider),
            consumer: NodeId(consumer),
            trm_amount: trm,
            tokens_processed: trm,
            timestamp: 0,
            model_id: "qwen2.5-0.5b".to_string(),
            flops_estimated: flops,
        }
    }

    #[tokio::test]
    async fn build_batch_returns_none_when_empty() {
        let ledger = Arc::new(Mutex::new(ComputeLedger::new()));
        let chain = Arc::new(MockChainClient::new());
        let anchor = Anchorer::new(
            AnchorerConfig::fast_test(NodeId([0u8; 32])),
            ledger,
            chain,
        );
        assert!(anchor.build_batch().await.is_none());
    }

    #[tokio::test]
    async fn build_batch_aggregates_deltas() {
        let ledger = Arc::new(Mutex::new(ComputeLedger::new()));
        {
            let mut l = ledger.lock().await;
            l.execute_trade(&make_trade([1u8; 32], [2u8; 32], 50, 50_000_000_000));
            l.execute_trade(&make_trade([1u8; 32], [3u8; 32], 30, 30_000_000_000));
        }
        let chain = Arc::new(MockChainClient::new());
        let anchor = Anchorer::new(
            AnchorerConfig::fast_test(NodeId([0u8; 32])),
            ledger.clone(),
            chain,
        );

        let batch = anchor.build_batch().await.expect("batch");
        assert_eq!(batch.trade_count_total, 2);
        assert_eq!(batch.flops_total, 80_000_000_000);
        // Provider [1u8; 32] should show 80 TRM earned.
        let prov = batch.node_deltas.iter()
            .find(|d| d.node_id == NodeId([1u8; 32]))
            .expect("provider in deltas");
        assert_eq!(prov.contributed_delta, 80);
        assert_eq!(prov.consumed_delta, 0);
    }

    #[tokio::test]
    async fn tick_submits_to_chain() {
        let ledger = Arc::new(Mutex::new(ComputeLedger::new()));
        {
            let mut l = ledger.lock().await;
            l.execute_trade(&make_trade([1u8; 32], [2u8; 32], 50, 1_000_000_000));
        }
        let chain = Arc::new(MockChainClient::new());
        let anchor = Anchorer::new(
            AnchorerConfig::fast_test(NodeId([0u8; 32])),
            ledger,
            chain.clone(),
        );
        let sub = anchor.tick().await.unwrap().expect("submission");
        assert_eq!(sub.batch_id, 0);
        assert_eq!(chain.submission_count(), 1);
    }

    #[tokio::test]
    async fn batch_ids_monotonically_increase() {
        let ledger = Arc::new(Mutex::new(ComputeLedger::new()));
        {
            let mut l = ledger.lock().await;
            l.execute_trade(&make_trade([1u8; 32], [2u8; 32], 50, 1_000_000_000));
        }
        let chain = Arc::new(MockChainClient::new());
        let anchor = Anchorer::new(
            AnchorerConfig::fast_test(NodeId([0u8; 32])),
            ledger.clone(),
            chain,
        );
        let a = anchor.tick().await.unwrap().unwrap();
        // Add another trade.
        ledger.lock().await.execute_trade(&make_trade([3u8; 32], [4u8; 32], 20, 500_000_000));
        let b = anchor.tick().await.unwrap().unwrap();
        assert!(b.batch_id > a.batch_id);
    }

    #[tokio::test]
    async fn tick_on_empty_log_returns_none() {
        let ledger = Arc::new(Mutex::new(ComputeLedger::new()));
        let chain = Arc::new(MockChainClient::new());
        let anchor = Anchorer::new(
            AnchorerConfig::fast_test(NodeId([0u8; 32])),
            ledger,
            chain.clone(),
        );
        assert!(anchor.tick().await.unwrap().is_none());
        assert_eq!(chain.submission_count(), 0);
    }
}
