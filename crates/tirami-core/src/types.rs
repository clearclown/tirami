use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};

/// Unique identifier for a node, derived from its Ed25519 public key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub [u8; 32]);

impl NodeId {
    pub fn from_public_key(key: &VerifyingKey) -> Self {
        Self(key.to_bytes())
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(value: &str) -> Result<Self, String> {
        let value = value.strip_prefix("forge_").unwrap_or(value);
        let bytes = hex::decode(value).map_err(|e| format!("decode node id: {e}"))?;
        if bytes.len() != 32 {
            return Err(format!("expected 32 bytes, got {}", bytes.len()));
        }

        let mut node = [0u8; 32];
        node.copy_from_slice(&bytes);
        Ok(Self(node))
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "forge_{}", hex::encode(&self.0[..8]))
    }
}

impl std::str::FromStr for NodeId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

/// Identifier for a model (e.g., "llama-3.2-1b-q4").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelId(pub String);

/// A contiguous range of transformer layers assigned to a single node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerRange {
    pub start: u32,
    pub end: u32, // exclusive
}

impl LayerRange {
    pub fn new(start: u32, end: u32) -> Self {
        assert!(start < end, "LayerRange: start must be < end");
        Self { start, end }
    }

    pub fn count(&self) -> u32 {
        self.end - self.start
    }

    pub fn contains(&self, layer: u32) -> bool {
        layer >= self.start && layer < self.end
    }
}

/// Metadata about a model, parsed from GGUF file headers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    pub id: ModelId,
    pub total_layers: u32,
    pub hidden_dim: u32,
    pub vocab_size: u32,
    pub head_count: u32,
    pub kv_head_count: u32,
    pub context_length: u32,
    pub file_size_bytes: u64,
    pub quantization: String, // e.g., "Q4_0", "Q4_K_M"
}

/// A node's hardware and network capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCapability {
    pub node_id: NodeId,
    pub cpu_cores: u16,
    pub memory_gb: f32,
    pub metal_available: bool,
    pub bandwidth_mbps: f32,
    pub battery_pct: Option<u8>,
    pub available_memory_gb: f32,
    pub region: String,
}

/// A single stage in the inference pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStage {
    pub node_id: NodeId,
    pub layer_range: LayerRange,
    pub position: u8,
}

/// Full pipeline topology for distributed inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTopology {
    pub model_id: ModelId,
    pub stages: Vec<PipelineStage>,
}

/// Data type for tensors in transit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DType {
    F16,
    F32,
    I8,
}

/// Metadata for an activation tensor being transmitted between pipeline stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorMeta {
    pub shape: Vec<u32>,
    pub dtype: DType,
    pub byte_len: u32,
}

/// A unit of compute work performed by a node (for the ledger).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkUnit {
    pub node_id: NodeId,
    pub timestamp: u64,
    pub layers_computed: LayerRange,
    pub model_id: ModelId,
    pub tokens_processed: u64,
    pub estimated_flops: u64,
}

/// A node's compute balance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeBalance {
    pub node_id: NodeId,
    pub contributed: u64,
    pub consumed: u64,
    #[serde(default)]
    pub reserved: u64,
    pub reputation: f64,
}

impl NodeBalance {
    pub fn balance(&self) -> i64 {
        self.contributed as i64 - self.consumed as i64
    }

    pub fn available_balance(&self) -> i64 {
        self.balance() - self.reserved as i64
    }
}

#[cfg(test)]
mod tests {
    use super::NodeId;

    #[test]
    fn node_id_hex_roundtrip() {
        let original = NodeId([7u8; 32]);
        let parsed = original.to_hex().parse::<NodeId>().unwrap();
        assert_eq!(parsed, original);
    }
}
