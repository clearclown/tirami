//! Phase 17 Wave 2.7 — Base Sepolia / Base mainnet chain-client
//! scaffold.
//!
//! # Status
//!
//! The full `BaseClient` (backed by ethers-rs or alloy) is blocked
//! on the same dependency pin that gates `ml-dsa` in Wave 1.6:
//! `digest 0.11.0-rc.10` (iroh 0.97) vs `digest 0.11.0` (stable RC
//! pulled in by current ethers-rs). Rather than fork iroh or pin
//! the entire workspace to an alpha, this wave delivers:
//!
//! * The **configuration types** operators will need
//!   ([`BaseSepoliaConfig`], [`BaseChainMode`]).
//! * A **deployment runbook** at
//!   `docs/phase-17-wave-2.7-base-deployment.md`.
//! * A **scaffolded `BaseClient`** that implements [`ChainClient`]
//!   and returns [`ChainError::NotImplemented`] for writes so the
//!   switch-over is a one-file change once the dep resolves.
//!
//! Existing production paths continue to use [`crate::MockChainClient`].
//!
//! # What `BaseClient` will do when complete
//!
//! 1. Hold a handle to a Base RPC endpoint (configured JSON-RPC URL).
//! 2. Hold a hot wallet address + signing key for anchor transactions.
//! 3. On `store_batch`, ABI-encode the batch deltas into a call to
//!    `TiramiBridge.anchor(merkle_root, node_deltas, flops_total)`,
//!    sign, submit, and wait for `N_CONFIRMATIONS` blocks (default 3).
//! 4. On `list_submissions`, replay `BatchSubmitted` events from the
//!    bridge contract starting at `last_seen_block` (cached locally).
//!
//! See the runbook doc for the on-chain side of the same contract.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tirami_core::NodeId;

use crate::client::{BatchSubmission, ChainClient, ChainError};
use crate::proof::BatchDeltas;

// ---------------------------------------------------------------------------
// Network selection
// ---------------------------------------------------------------------------

/// Which Base network the client writes to.
///
/// **BaseMainnet** is intentionally NOT a default. Mainnet deployment
/// is gated on completion of an external security audit AND 30 days
/// of stable Sepolia operation — see
/// `docs/phase-17-wave-2.7-base-deployment.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BaseChainMode {
    /// Base Sepolia testnet (chain id 84532). Default for Phase 17.
    Sepolia,
    /// Base mainnet (chain id 8453). DO NOT use until Wave 3.3
    /// (external audit) completes.
    Mainnet,
}

impl BaseChainMode {
    /// Numeric chain ID per EIP-155.
    pub fn chain_id(&self) -> u64 {
        match self {
            BaseChainMode::Sepolia => 84_532,
            BaseChainMode::Mainnet => 8_453,
        }
    }

    /// Default public RPC URL. Operators running production should
    /// override with a private Alchemy / Infura endpoint.
    pub fn default_rpc_url(&self) -> &'static str {
        match self {
            BaseChainMode::Sepolia => "https://sepolia.base.org",
            BaseChainMode::Mainnet => "https://mainnet.base.org",
        }
    }

    /// Human-readable label for logs.
    pub fn as_str(&self) -> &'static str {
        match self {
            BaseChainMode::Sepolia => "base-sepolia",
            BaseChainMode::Mainnet => "base-mainnet",
        }
    }
}

// ---------------------------------------------------------------------------
// BaseSepoliaConfig — what operators fill in after deploy
// ---------------------------------------------------------------------------

/// Operator-supplied configuration for the Base chain client.
///
/// Populated from deployed contract addresses (see the Wave 2.7
/// runbook). Serde-friendly so operators can keep it in their
/// `tirami.toml` alongside the existing config block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BaseSepoliaConfig {
    pub mode: BaseChainMode,
    /// RPC URL. `None` means "use `mode.default_rpc_url()`".
    #[serde(default)]
    pub rpc_url: Option<String>,
    /// 20-byte EVM address (hex with `0x` prefix) of the deployed
    /// `TiramiBridge` contract.
    pub bridge_address: String,
    /// 20-byte EVM address of the TRM ERC-20. Included for
    /// convenience of operator tooling; the bridge holds the same
    /// pointer internally via `TRM.bridge()`.
    pub trm_address: String,
    /// Block confirmations to wait before considering an anchor tx
    /// final. Default 3 for Sepolia, 5 for mainnet.
    #[serde(default = "default_confirmations")]
    pub confirmations: u64,
}

fn default_confirmations() -> u64 {
    3
}

impl BaseSepoliaConfig {
    /// Construct a config for Base Sepolia with the passed addresses.
    /// Deployers paste the output of `forge script Deploy.s.sol` here.
    pub fn sepolia(bridge_address: impl Into<String>, trm_address: impl Into<String>) -> Self {
        Self {
            mode: BaseChainMode::Sepolia,
            rpc_url: None,
            bridge_address: bridge_address.into(),
            trm_address: trm_address.into(),
            confirmations: 3,
        }
    }

    /// Mainnet constructor — rejects until the external audit has
    /// unlocked the mode. The "unlock" is enforced structurally here:
    /// the function is `#[deprecated]` with a rejection note so any
    /// code calling it triggers a loud lint.
    #[deprecated(
        note = "Base mainnet deployment is gated on Wave 3.3 external audit. \
                See docs/phase-17-wave-2.7-base-deployment.md before unlocking."
    )]
    pub fn mainnet_reserved(
        bridge_address: impl Into<String>,
        trm_address: impl Into<String>,
    ) -> Self {
        Self {
            mode: BaseChainMode::Mainnet,
            rpc_url: None,
            bridge_address: bridge_address.into(),
            trm_address: trm_address.into(),
            confirmations: 5,
        }
    }

    /// Effective RPC URL: explicit override, else the network default.
    pub fn effective_rpc_url(&self) -> String {
        self.rpc_url
            .clone()
            .unwrap_or_else(|| self.mode.default_rpc_url().to_string())
    }

    /// Basic shape validation on the addresses. Does NOT verify that
    /// the contracts exist on-chain — that's the runbook's "verify on
    /// Basescan" step.
    pub fn validate(&self) -> Result<(), String> {
        is_valid_eth_address(&self.bridge_address)
            .map_err(|e| format!("bridge_address: {e}"))?;
        is_valid_eth_address(&self.trm_address).map_err(|e| format!("trm_address: {e}"))?;
        Ok(())
    }
}

fn is_valid_eth_address(addr: &str) -> Result<(), String> {
    if !addr.starts_with("0x") {
        return Err("missing 0x prefix".into());
    }
    if addr.len() != 42 {
        return Err(format!(
            "wrong length: {} (expected 42 incl 0x)",
            addr.len()
        ));
    }
    if !addr
        .bytes()
        .skip(2)
        .all(|b| b.is_ascii_hexdigit())
    {
        return Err("contains non-hex characters".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// BaseClient — scaffold
// ---------------------------------------------------------------------------

/// Production Base-chain anchor client. Scaffold today; when the
/// ethers-rs / alloy version conflict resolves, the body of
/// `store_batch` becomes a real JSON-RPC signed tx.
///
/// Currently delegates reads to a wrapped [`MockChainClient`] so
/// integration tests exercising the anchor loop don't need a live
/// RPC endpoint.
#[derive(Debug, Clone)]
pub struct BaseClient {
    cfg: BaseSepoliaConfig,
    inner_reads: crate::client::MockChainClient,
}

impl BaseClient {
    /// Construct with the operator-supplied config. Validates the
    /// shape of both addresses up-front so typos fail fast.
    pub fn new(cfg: BaseSepoliaConfig) -> Result<Self, String> {
        cfg.validate()?;
        Ok(Self {
            cfg,
            inner_reads: crate::client::MockChainClient::new(),
        })
    }

    pub fn config(&self) -> &BaseSepoliaConfig {
        &self.cfg
    }
}

#[async_trait]
impl ChainClient for BaseClient {
    async fn store_batch(
        &self,
        _deltas: &BatchDeltas,
        _submitter: &NodeId,
    ) -> Result<BatchSubmission, ChainError> {
        // Scaffold: real tx submission lands once the ethers-rs /
        // alloy dep tree can be pulled without conflicting with
        // iroh 0.97's digest pin. Operators stay on `MockChainClient`
        // until that unblocks; see the runbook for the
        // single-line switchover.
        Err(ChainError::NotImplemented(format!(
            "BaseClient::store_batch not yet wired for {}; see \
             docs/phase-17-wave-2.7-base-deployment.md",
            self.cfg.mode.as_str()
        )))
    }

    async fn list_submissions(&self) -> Vec<BatchSubmission> {
        // The scaffold returns the wrapped mock's history — this way
        // tests that mix BaseClient + Anchorer still see `store_batch`
        // results from the mock path if they pre-populate it.
        self.inner_reads.list_submissions().await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_chain_mode_ids_match_specs() {
        assert_eq!(BaseChainMode::Sepolia.chain_id(), 84_532);
        assert_eq!(BaseChainMode::Mainnet.chain_id(), 8_453);
    }

    #[test]
    fn rpc_url_defaults_per_network() {
        assert_eq!(
            BaseChainMode::Sepolia.default_rpc_url(),
            "https://sepolia.base.org"
        );
        assert_eq!(
            BaseChainMode::Mainnet.default_rpc_url(),
            "https://mainnet.base.org"
        );
    }

    #[test]
    fn sepolia_constructor_has_sensible_defaults() {
        let c = BaseSepoliaConfig::sepolia(
            "0x1234567890123456789012345678901234567890",
            "0xabcDEF1234567890123456789012345678901234",
        );
        assert_eq!(c.mode, BaseChainMode::Sepolia);
        assert_eq!(c.confirmations, 3);
        assert!(c.rpc_url.is_none());
        assert_eq!(c.effective_rpc_url(), "https://sepolia.base.org");
    }

    #[test]
    fn custom_rpc_url_overrides_default() {
        let mut c = BaseSepoliaConfig::sepolia(
            "0x1234567890123456789012345678901234567890",
            "0xabcDEF1234567890123456789012345678901234",
        );
        c.rpc_url = Some("https://my-alchemy.example".into());
        assert_eq!(c.effective_rpc_url(), "https://my-alchemy.example");
    }

    #[test]
    fn validate_rejects_missing_0x_prefix() {
        let c = BaseSepoliaConfig::sepolia(
            "1234567890123456789012345678901234567890",
            "0xabcDEF1234567890123456789012345678901234",
        );
        let e = c.validate().unwrap_err();
        assert!(e.contains("0x"), "expected 0x error, got: {e}");
    }

    #[test]
    fn validate_rejects_wrong_length() {
        let c = BaseSepoliaConfig::sepolia(
            "0xDEADBEEF",
            "0xabcDEF1234567890123456789012345678901234",
        );
        let e = c.validate().unwrap_err();
        assert!(e.contains("length"), "expected length error, got: {e}");
    }

    #[test]
    fn validate_rejects_non_hex_characters() {
        let c = BaseSepoliaConfig::sepolia(
            "0xZZZZ567890123456789012345678901234567890",
            "0xabcDEF1234567890123456789012345678901234",
        );
        let e = c.validate().unwrap_err();
        assert!(e.contains("non-hex"), "expected non-hex error, got: {e}");
    }

    #[tokio::test]
    async fn store_batch_returns_not_implemented_on_scaffold() {
        let cfg = BaseSepoliaConfig::sepolia(
            "0x1234567890123456789012345678901234567890",
            "0xabcDEF1234567890123456789012345678901234",
        );
        let client = BaseClient::new(cfg).unwrap();
        let deltas = BatchDeltas {
            batch_id: 1,
            batch_closed_at: 0,
            trade_merkle_root: [0u8; 32],
            node_deltas: Vec::new(),
            trade_count_total: 0,
            flops_total: 0,
        };
        let err = client
            .store_batch(&deltas, &NodeId([0u8; 32]))
            .await
            .unwrap_err();
        match err {
            ChainError::NotImplemented(msg) => {
                assert!(msg.contains("base-sepolia"));
                assert!(msg.contains("runbook") || msg.contains("deployment"));
            }
            other => panic!("expected NotImplemented, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_submissions_returns_empty_scaffold_history() {
        let cfg = BaseSepoliaConfig::sepolia(
            "0x1234567890123456789012345678901234567890",
            "0xabcDEF1234567890123456789012345678901234",
        );
        let client = BaseClient::new(cfg).unwrap();
        let list = client.list_submissions().await;
        assert!(list.is_empty());
    }

    #[test]
    fn serde_roundtrips_config() {
        let c = BaseSepoliaConfig::sepolia(
            "0x1234567890123456789012345678901234567890",
            "0xabcDEF1234567890123456789012345678901234",
        );
        let s = serde_json::to_string(&c).unwrap();
        let back: BaseSepoliaConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(back, c);
    }
}
