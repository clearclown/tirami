use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to the local GGUF model file.
    pub model_path: Option<PathBuf>,

    /// Optional path to a persisted ledger snapshot.
    pub ledger_path: Option<PathBuf>,

    /// Whether to share compute with the network.
    pub share_compute: bool,

    /// Maximum memory (GB) to dedicate to inference.
    pub max_memory_gb: f32,

    /// Port for the local HTTP API.
    pub api_port: u16,

    /// Bootstrap relay addresses for WAN discovery.
    pub bootstrap_relays: Vec<String>,

    /// Region hint for peer discovery.
    pub region: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model_path: None,
            ledger_path: None,
            share_compute: false,
            max_memory_gb: 4.0,
            api_port: 3000,
            bootstrap_relays: vec![],
            region: "unknown".to_string(),
        }
    }
}
