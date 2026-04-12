//! Forward-based activation routing for split inference.
//!
//! This module implements the activation tensor routing protocol
//! described in docs/protocol-spec.md. In the current implementation,
//! the seed still runs the full model, but the Forward/TokenResult
//! messages are used for the wire protocol so that future split
//! inference only needs to change the execution path, not the protocol.
//!
//! ## Current flow (seed runs full model):
//! ```text
//! Worker → InferenceRequest → Seed
//! Seed tokenizes, runs full forward, streams TokenStreamMsg
//! ```
//!
//! ## Target flow (split inference):
//! ```text
//! Coordinator → Forward(activation) → Stage1 → Forward(activation) → Stage2
//! Stage2 → TokenResult → Coordinator → TokenStreamMsg → Worker
//! ```

use tirami_core::{DType, NodeId, PipelineTopology, TensorMeta};
use tirami_net::ForgeTransport;
use tirami_proto::{Envelope, Forward, Payload, TokenResult};

/// Send an activation tensor to the next pipeline stage via Forward message.
pub async fn send_activation(
    transport: &ForgeTransport,
    peer_id: &str,
    sender: &NodeId,
    request_id: u64,
    sequence_pos: u32,
    activation: &[f32],
) -> anyhow::Result<()> {
    // Convert f32 → raw bytes (little-endian)
    let byte_len = activation.len() * 4;
    let mut tensor_data = Vec::with_capacity(byte_len);
    for &val in activation {
        tensor_data.extend_from_slice(&val.to_le_bytes());
    }

    let msg = Envelope {
        msg_id: rand::random(),
        sender: sender.clone(),
        timestamp: now_millis(),
        payload: Payload::Forward(Forward {
            request_id,
            sequence_pos,
            tensor_meta: TensorMeta {
                shape: vec![1, activation.len() as u32],
                dtype: DType::F32,
                byte_len: byte_len as u32,
            },
            tensor_data,
        }),
    };

    transport.send_to(peer_id, &msg).await?;
    Ok(())
}

/// Receive an activation tensor from a Forward message.
/// Returns (request_id, sequence_pos, activation_f32).
pub fn decode_forward(fwd: &Forward) -> anyhow::Result<(u64, u32, Vec<f32>)> {
    if fwd.tensor_meta.dtype != DType::F32 {
        anyhow::bail!(
            "unsupported activation dtype: {:?} (expected F32)",
            fwd.tensor_meta.dtype
        );
    }

    let expected_bytes = fwd.tensor_meta.byte_len as usize;
    if fwd.tensor_data.len() != expected_bytes {
        anyhow::bail!(
            "tensor size mismatch: expected {} bytes, got {}",
            expected_bytes,
            fwd.tensor_data.len()
        );
    }

    let activation: Vec<f32> = fwd
        .tensor_data
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    Ok((fwd.request_id, fwd.sequence_pos, activation))
}

/// Send a TokenResult back from the final pipeline stage.
pub async fn send_token_result(
    transport: &ForgeTransport,
    peer_id: &str,
    sender: &NodeId,
    request_id: u64,
    tokens: Vec<u32>,
) -> anyhow::Result<()> {
    let msg = Envelope {
        msg_id: rand::random(),
        sender: sender.clone(),
        timestamp: now_millis(),
        payload: Payload::TokenResult(TokenResult { request_id, tokens }),
    };

    transport.send_to(peer_id, &msg).await?;
    Ok(())
}

/// Describes the node's position in a pipeline and its neighbors.
#[derive(Debug, Clone)]
pub struct PipelinePosition {
    pub is_first: bool,
    pub is_last: bool,
    pub upstream_peer: Option<String>,
    pub downstream_peer: Option<String>,
}

/// Determine this node's position in the pipeline topology.
pub fn find_position(
    topology: &PipelineTopology,
    my_node_id: &NodeId,
    peer_lookup: &std::collections::HashMap<NodeId, String>,
) -> Option<PipelinePosition> {
    let idx = topology
        .stages
        .iter()
        .position(|s| &s.node_id == my_node_id)?;

    let is_first = idx == 0;
    let is_last = idx == topology.stages.len() - 1;

    let upstream_peer = if is_first {
        None
    } else {
        let upstream_node = &topology.stages[idx - 1].node_id;
        peer_lookup.get(upstream_node).cloned()
    };

    let downstream_peer = if is_last {
        None
    } else {
        let downstream_node = &topology.stages[idx + 1].node_id;
        peer_lookup.get(downstream_node).cloned()
    };

    Some(PipelinePosition {
        is_first,
        is_last,
        upstream_peer,
        downstream_peer,
    })
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use tirami_core::{LayerRange, ModelId, PipelineStage};

    #[test]
    fn activation_encode_decode_roundtrip() {
        let original: Vec<f32> = vec![1.0, -2.5, 3.14, 0.0, f32::MAX, f32::MIN];

        // Encode
        let byte_len = original.len() * 4;
        let mut tensor_data = Vec::with_capacity(byte_len);
        for &val in &original {
            tensor_data.extend_from_slice(&val.to_le_bytes());
        }

        let fwd = Forward {
            request_id: 42,
            sequence_pos: 0,
            tensor_meta: TensorMeta {
                shape: vec![1, original.len() as u32],
                dtype: DType::F32,
                byte_len: byte_len as u32,
            },
            tensor_data,
        };

        // Decode
        let (req_id, seq_pos, decoded) = decode_forward(&fwd).unwrap();
        assert_eq!(req_id, 42);
        assert_eq!(seq_pos, 0);
        assert_eq!(decoded, original);
    }

    #[test]
    fn find_pipeline_position_first_node() {
        let topology = PipelineTopology {
            model_id: ModelId("test".to_string()),
            stages: vec![
                PipelineStage {
                    node_id: NodeId([1u8; 32]),
                    layer_range: LayerRange::new(0, 16),
                    position: 0,
                },
                PipelineStage {
                    node_id: NodeId([2u8; 32]),
                    layer_range: LayerRange::new(16, 32),
                    position: 1,
                },
            ],
        };

        let mut peer_lookup = std::collections::HashMap::new();
        peer_lookup.insert(NodeId([1u8; 32]), "peer1".to_string());
        peer_lookup.insert(NodeId([2u8; 32]), "peer2".to_string());

        let pos = find_position(&topology, &NodeId([1u8; 32]), &peer_lookup).unwrap();
        assert!(pos.is_first);
        assert!(!pos.is_last);
        assert!(pos.upstream_peer.is_none());
        assert_eq!(pos.downstream_peer.as_deref(), Some("peer2"));
    }

    #[test]
    fn find_pipeline_position_last_node() {
        let topology = PipelineTopology {
            model_id: ModelId("test".to_string()),
            stages: vec![
                PipelineStage {
                    node_id: NodeId([1u8; 32]),
                    layer_range: LayerRange::new(0, 16),
                    position: 0,
                },
                PipelineStage {
                    node_id: NodeId([2u8; 32]),
                    layer_range: LayerRange::new(16, 32),
                    position: 1,
                },
            ],
        };

        let mut peer_lookup = std::collections::HashMap::new();
        peer_lookup.insert(NodeId([1u8; 32]), "peer1".to_string());
        peer_lookup.insert(NodeId([2u8; 32]), "peer2".to_string());

        let pos = find_position(&topology, &NodeId([2u8; 32]), &peer_lookup).unwrap();
        assert!(!pos.is_first);
        assert!(pos.is_last);
        assert_eq!(pos.upstream_peer.as_deref(), Some("peer1"));
        assert!(pos.downstream_peer.is_none());
    }
}
