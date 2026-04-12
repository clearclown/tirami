use tirami_core::{LayerRange, ModelManifest, PeerCapability, PipelineStage, PipelineTopology};

/// Assigns model layers to nodes based on their capabilities.
pub struct ShardAssigner;

impl ShardAssigner {
    /// Given a model and a set of peers, compute a pipeline topology.
    ///
    /// The first peer in the list is assumed to be the coordinator (phone/initiator)
    /// and always receives the first layers.
    pub fn assign(
        model: &ModelManifest,
        peers: &[PeerCapability],
    ) -> Result<PipelineTopology, tirami_core::TiramiError> {
        if peers.is_empty() {
            return Err(tirami_core::TiramiError::ShardAssignmentError(
                "no peers available".to_string(),
            ));
        }

        // Single node: assign all layers
        if peers.len() == 1 {
            return Ok(PipelineTopology {
                model_id: model.id.clone(),
                stages: vec![PipelineStage {
                    node_id: peers[0].node_id.clone(),
                    layer_range: LayerRange::new(0, model.total_layers),
                    position: 0,
                }],
            });
        }

        // Multi-node: distribute layers proportional to available memory
        let total_memory: f32 = peers.iter().map(|p| p.available_memory_gb).sum();
        let mut stages = Vec::new();
        let mut current_layer = 0u32;

        for (i, peer) in peers.iter().enumerate() {
            let fraction = peer.available_memory_gb / total_memory;
            let layer_count = if i == peers.len() - 1 {
                // Last peer gets remaining layers
                model.total_layers - current_layer
            } else {
                ((model.total_layers as f32 * fraction).round() as u32).max(1)
            };

            let end = (current_layer + layer_count).min(model.total_layers);
            if current_layer >= end {
                break;
            }

            stages.push(PipelineStage {
                node_id: peer.node_id.clone(),
                layer_range: LayerRange::new(current_layer, end),
                position: i as u8,
            });

            current_layer = end;
        }

        Ok(PipelineTopology {
            model_id: model.id.clone(),
            stages,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tirami_core::{ModelId, NodeId};

    fn make_peer(name: &str, memory_gb: f32) -> PeerCapability {
        let mut node_id = [0u8; 32];
        for (i, byte) in name.as_bytes().iter().take(node_id.len()).enumerate() {
            node_id[i] = *byte;
        }

        PeerCapability {
            node_id: NodeId(node_id),
            cpu_cores: 8,
            memory_gb,
            metal_available: true,
            bandwidth_mbps: 100.0,
            battery_pct: None,
            available_memory_gb: memory_gb,
            region: "test".to_string(),
        }
    }

    fn make_model(layers: u32) -> ModelManifest {
        ModelManifest {
            id: ModelId("test-model".to_string()),
            total_layers: layers,
            hidden_dim: 4096,
            vocab_size: 32000,
            head_count: 32,
            kv_head_count: 32,
            context_length: 2048,
            file_size_bytes: 0,
            quantization: "Q4_0".to_string(),
        }
    }

    #[test]
    fn single_node_gets_all_layers() {
        let model = make_model(32);
        let peers = vec![make_peer("phone", 8.0)];
        let topo = ShardAssigner::assign(&model, &peers).unwrap();
        assert_eq!(topo.stages.len(), 1);
        assert_eq!(topo.stages[0].layer_range, LayerRange::new(0, 32));
    }

    #[test]
    fn two_nodes_split_layers() {
        let model = make_model(32);
        let peers = vec![make_peer("phone", 4.0), make_peer("mac", 12.0)];
        let topo = ShardAssigner::assign(&model, &peers).unwrap();
        assert_eq!(topo.stages.len(), 2);
        // Phone gets ~25% of layers, mac gets ~75%
        let phone_layers = topo.stages[0].layer_range.count();
        let mac_layers = topo.stages[1].layer_range.count();
        assert_eq!(phone_layers + mac_layers, 32);
        assert!(mac_layers > phone_layers);
    }
}
