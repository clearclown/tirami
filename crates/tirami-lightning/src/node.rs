//! Embedded Lightning node for Forge.

use ldk_node::bitcoin::Network;
use ldk_node::{Builder, Node};
use std::path::PathBuf;
use std::sync::Arc;

/// Configuration for the Forge Lightning wallet.
#[derive(Debug, Clone)]
pub struct WalletConfig {
    /// Directory for Lightning node data (keys, channels, etc.)
    pub data_dir: PathBuf,
    /// Bitcoin network (Testnet, Signet, or Mainnet)
    pub network: Network,
    /// Esplora server URL for chain data (no Bitcoin Core needed)
    pub esplora_url: String,
    /// Rapid Gossip Sync URL for network graph
    pub rgs_url: Option<String>,
}

impl Default for WalletConfig {
    fn default() -> Self {
        Self {
            data_dir: dirs_home().join(".forge/lightning"),
            network: Network::Signet, // Safe default for development
            esplora_url: "https://mutinynet.com/api".to_string(),
            rgs_url: None,
        }
    }
}

fn dirs_home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// A self-sovereign Lightning wallet embedded in the Forge node.
///
/// Enables earning and spending Bitcoin for inference without
/// any custodian or third-party service.
pub struct ForgeWallet {
    node: Arc<Node>,
    config: WalletConfig,
}

impl ForgeWallet {
    /// Create and start a new Lightning node.
    pub fn start(config: WalletConfig) -> anyhow::Result<Self> {
        tracing::info!(
            "Starting Lightning node (network={:?}, data={:?})",
            config.network,
            config.data_dir
        );

        let mut builder = Builder::new();
        builder.set_network(config.network);
        builder.set_storage_dir_path(config.data_dir.to_string_lossy().to_string());
        builder.set_chain_source_esplora(config.esplora_url.clone(), None);

        if let Some(ref rgs) = config.rgs_url {
            builder.set_gossip_source_rgs(rgs.clone());
        }

        let node = builder.build()?;
        node.start()?;

        let node_id = node.node_id();
        tracing::info!("Lightning node started: {}", node_id);

        // Log funding address
        let funding_address = node.onchain_payment().new_address()?;
        tracing::info!("Fund your Lightning node: {}", funding_address);

        Ok(Self {
            node: Arc::new(node),
            config,
        })
    }

    /// Get the Lightning node ID (public key).
    pub fn node_id(&self) -> String {
        self.node.node_id().to_string()
    }

    /// Get a new on-chain Bitcoin address for funding the node.
    pub fn funding_address(&self) -> anyhow::Result<String> {
        let addr = self.node.onchain_payment().new_address()?;
        Ok(addr.to_string())
    }

    /// Get the current on-chain balance in satoshis.
    pub fn onchain_balance_sats(&self) -> u64 {
        let balances = self.node.list_balances();
        balances.total_onchain_balance_sats
    }

    /// Get the current Lightning channel balance in satoshis.
    pub fn lightning_balance_sats(&self) -> u64 {
        let balances = self.node.list_balances();
        balances.total_lightning_balance_sats
    }

    /// Create a BOLT11 invoice to receive payment for inference.
    ///
    /// `amount_msats`: amount in millisatoshis (1 sat = 1000 msats)
    /// `description`: what the payment is for
    /// `expiry_secs`: how long the invoice is valid
    pub fn create_invoice(
        &self,
        amount_msats: u64,
        description: &str,
        expiry_secs: u32,
    ) -> anyhow::Result<String> {
        let invoice = self
            .node
            .bolt11_payment()
            .receive(amount_msats, description, expiry_secs)?;
        Ok(invoice.to_string())
    }

    /// Pay a BOLT11 invoice (for consuming inference).
    pub fn pay_invoice(&self, invoice_str: &str) -> anyhow::Result<PaymentId> {
        let invoice: ldk_node::lightning_invoice::Bolt11Invoice = invoice_str
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid invoice: {e}"))?;

        let payment_id = self.node.bolt11_payment().send(&invoice, None)?;
        Ok(PaymentId(payment_id.0.to_vec()))
    }

    /// Get the network this wallet is connected to.
    pub fn network(&self) -> Network {
        self.config.network
    }

    /// Stop the Lightning node gracefully.
    pub fn stop(&self) -> anyhow::Result<()> {
        self.node.stop()?;
        tracing::info!("Lightning node stopped");
        Ok(())
    }

    /// Get a reference to the underlying LDK node.
    pub fn inner(&self) -> &Node {
        &self.node
    }
}

impl Drop for ForgeWallet {
    fn drop(&mut self) {
        if let Err(e) = self.node.stop() {
            tracing::warn!("Failed to stop Lightning node: {}", e);
        }
    }
}

/// Opaque payment identifier.
#[derive(Debug, Clone)]
pub struct PaymentId(pub Vec<u8>);

impl std::fmt::Display for PaymentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

/// Helper to create a default home directory.
mod dirs {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}
