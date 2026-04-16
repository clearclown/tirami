//! Chain client abstraction — the "what does on-chain look like" interface.
//!
//! Real implementations (Base L2 via ethers-rs, Solana, etc.) live in
//! separate crates so this core does not pull heavy chain dependencies.
//! The `MockChainClient` here is deterministic and used for tests and the
//! default tirami-node startup until a real chain is configured.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tirami_core::NodeId;

use crate::proof::BatchDeltas;

/// Transaction hash returned by a chain write.
/// Format depends on the chain; always stored as hex for portability.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TxHash(pub String);

impl TxHash {
    pub fn mock(batch_id: u64) -> Self {
        TxHash(format!("mock_{batch_id:064x}"))
    }
}

#[derive(Debug, Error)]
pub enum ChainError {
    #[error("chain write failed: {0}")]
    WriteFailed(String),
    #[error("batch already submitted: {0}")]
    DuplicateBatch(u64),
    #[error("unsupported operation: {0}")]
    Unsupported(&'static str),
}

/// Tagged record of a batch that was accepted by the chain.
/// Useful for node daemons to list past anchors via `/v1/tirami/anchors`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSubmission {
    pub batch_id: u64,
    pub tx_hash: TxHash,
    pub merkle_root_hex: String,
    pub submitted_at_ms: u64,
    pub node_count: usize,
    pub flops_total: u64,
}

/// Abstract chain write interface. Real impls live in downstream crates.
#[async_trait]
pub trait ChainClient: Send + Sync + 'static {
    /// Submit a batch of deltas for storage on the chain.
    async fn store_batch(
        &self,
        deltas: &BatchDeltas,
        submitter: &NodeId,
    ) -> Result<BatchSubmission, ChainError>;

    /// List submissions the client has observed so far.
    /// In tests this returns every mock submission; real impls may return
    /// a paginated slice of on-chain history.
    async fn list_submissions(&self) -> Vec<BatchSubmission>;
}

// ---------------------------------------------------------------------------
// MockChainClient
// ---------------------------------------------------------------------------

/// Test / default-time chain client. Stores batches in memory.
///
/// Not persistent — on node restart the mock "chain" is empty. Useful for:
/// - Unit tests that exercise the anchor loop
/// - Dev deployments where real on-chain writes aren't desired yet
/// - CI without a testnet RPC dependency
#[derive(Debug, Default, Clone)]
pub struct MockChainClient {
    inner: Arc<Mutex<MockState>>,
}

#[derive(Debug, Default)]
struct MockState {
    submissions: Vec<BatchSubmission>,
    by_batch_id: HashMap<u64, BatchSubmission>,
}

impl MockChainClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submission_count(&self) -> usize {
        self.inner.lock().expect("mock lock").submissions.len()
    }
}

#[async_trait]
impl ChainClient for MockChainClient {
    async fn store_batch(
        &self,
        deltas: &BatchDeltas,
        _submitter: &NodeId,
    ) -> Result<BatchSubmission, ChainError> {
        let mut state = self.inner.lock().expect("mock lock");
        if state.by_batch_id.contains_key(&deltas.batch_id) {
            return Err(ChainError::DuplicateBatch(deltas.batch_id));
        }
        let submission = BatchSubmission {
            batch_id: deltas.batch_id,
            tx_hash: TxHash::mock(deltas.batch_id),
            merkle_root_hex: hex::encode(deltas.trade_merkle_root),
            submitted_at_ms: now_ms(),
            node_count: deltas.node_deltas.len(),
            flops_total: deltas.flops_total,
        };
        state.by_batch_id.insert(deltas.batch_id, submission.clone());
        state.submissions.push(submission.clone());
        Ok(submission)
    }

    async fn list_submissions(&self) -> Vec<BatchSubmission> {
        self.inner.lock().expect("mock lock").submissions.clone()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proof::BatchDeltas;

    fn sample(batch_id: u64) -> BatchDeltas {
        BatchDeltas {
            batch_id,
            batch_closed_at: now_ms(),
            node_deltas: vec![],
            trade_merkle_root: [7u8; 32],
            trade_count_total: 3,
            flops_total: 1_500_000_000,
        }
    }

    #[tokio::test]
    async fn mock_accepts_new_batches() {
        let client = MockChainClient::new();
        let r = client.store_batch(&sample(1), &NodeId([0u8; 32])).await.unwrap();
        assert_eq!(r.batch_id, 1);
        assert_eq!(client.submission_count(), 1);
    }

    #[tokio::test]
    async fn mock_rejects_duplicate_batch_id() {
        let client = MockChainClient::new();
        let node = NodeId([0u8; 32]);
        client.store_batch(&sample(1), &node).await.unwrap();
        let err = client.store_batch(&sample(1), &node).await.unwrap_err();
        assert!(matches!(err, ChainError::DuplicateBatch(1)));
    }

    #[tokio::test]
    async fn list_returns_all() {
        let client = MockChainClient::new();
        let node = NodeId([0u8; 32]);
        for i in 1..=3 {
            client.store_batch(&sample(i), &node).await.unwrap();
        }
        let list = client.list_submissions().await;
        assert_eq!(list.len(), 3);
    }
}
