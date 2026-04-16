//! Batch delta structures — what gets sent to the chain.
//!
//! Each anchor batch summarizes the net TRM movement per node since the
//! previous anchor. The chain receives the aggregate (Merkle root + net
//! deltas), NOT the individual trades, to keep on-chain cost bounded.

use serde::{Deserialize, Serialize};
use tirami_core::NodeId;

/// Per-node net TRM delta for the batch window.
///
/// `contributed_delta` and `consumed_delta` are *additive* since the
/// previous batch. Negative balances are not allowed — receivers enforce
/// this when applying.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeDelta {
    pub node_id: NodeId,
    /// TRM earned by providing inference in this batch window.
    pub contributed_delta: u64,
    /// TRM spent by consuming inference in this batch window.
    pub consumed_delta: u64,
    /// Total FLOP proven useful in this batch (for PoUW mint on-chain).
    pub flops_in_batch: u64,
    /// Trades this node participated in during the window.
    pub trade_count: u32,
}

/// Aggregate batch payload committed to the chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchDeltas {
    /// Monotonic batch identifier assigned by the anchorer.
    pub batch_id: u64,
    /// Unix ms when the batch window closed.
    pub batch_closed_at: u64,
    /// Per-node summary of trades in this window.
    pub node_deltas: Vec<NodeDelta>,
    /// Merkle root over the trade log (same format as
    /// `ComputeLedger::compute_trade_merkle_root`).
    pub trade_merkle_root: [u8; 32],
    /// Count of trades included in the Merkle tree.
    pub trade_count_total: u64,
    /// Sum of all `flops_estimated` values in this batch.
    pub flops_total: u64,
}

impl BatchDeltas {
    /// Returns true if the batch has any settled activity worth anchoring.
    pub fn is_nonempty(&self) -> bool {
        self.trade_count_total > 0 && !self.node_deltas.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_batch_is_not_nonempty() {
        let b = BatchDeltas {
            batch_id: 1,
            batch_closed_at: 0,
            node_deltas: vec![],
            trade_merkle_root: [0u8; 32],
            trade_count_total: 0,
            flops_total: 0,
        };
        assert!(!b.is_nonempty());
    }

    #[test]
    fn batch_with_trades_is_nonempty() {
        let b = BatchDeltas {
            batch_id: 1,
            batch_closed_at: 100,
            node_deltas: vec![NodeDelta {
                node_id: NodeId([1u8; 32]),
                contributed_delta: 50,
                consumed_delta: 0,
                flops_in_batch: 50_000_000_000,
                trade_count: 1,
            }],
            trade_merkle_root: [1u8; 32],
            trade_count_total: 1,
            flops_total: 50_000_000_000,
        };
        assert!(b.is_nonempty());
    }
}
