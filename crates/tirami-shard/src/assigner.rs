use tirami_core::{LayerRange, ModelManifest, PeerCapability, PipelineStage, PipelineTopology};

/// Assigns model layers to nodes based on their capabilities.
pub struct ShardAssigner;

/// Headroom over the raw weight size, for the KV cache, activations, and the
/// runtime itself. Deliberately coarse — the point is to stop planning a
/// split that cannot load, not to predict RSS.
const MEMORY_OVERHEAD_FACTOR: f32 = 1.2;

const BYTES_PER_GB: f32 = 1024.0 * 1024.0 * 1024.0;

impl ShardAssigner {
    /// Memory the model needs before it will load, in GiB.
    fn required_memory_gb(model: &ModelManifest) -> f32 {
        (model.file_size_bytes as f32 / BYTES_PER_GB) * MEMORY_OVERHEAD_FACTOR
    }

    /// Pick the fewest peers whose free memory covers the model.
    ///
    /// `peers[0]` is the coordinator and is always kept. The rest are taken
    /// largest-first until the model fits, or all of them if it never does.
    ///
    /// This replaces "one stage per connected peer", which made the plan
    /// worse every time a node joined: #164 measured ~3.83 round-trips per
    /// token against `graph splits = 3`, so RTT sensitivity scales with the
    /// number of split points. Cutting splits from 3 to 1 cuts the RTT term
    /// to roughly a quarter — the difference between 3.9 tok/s and something
    /// usable on a 50 ms link.
    fn select_peers(model: &ModelManifest, peers: &[PeerCapability]) -> Vec<PeerCapability> {
        let required = Self::required_memory_gb(model);

        let mut selected = vec![peers[0].clone()];
        let mut covered = peers[0].available_memory_gb;
        if covered >= required {
            return selected;
        }

        let mut rest: Vec<&PeerCapability> = peers[1..].iter().collect();
        rest.sort_by(|a, b| {
            b.available_memory_gb
                .partial_cmp(&a.available_memory_gb)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for peer in rest {
            selected.push(peer.clone());
            covered += peer.available_memory_gb;
            if covered >= required {
                break;
            }
        }

        selected
    }

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

        let peers = Self::select_peers(model, peers);
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
            protocol_version: tirami_core::TIRAMI_PROTOCOL_VERSION,
            features: tirami_core::base_protocol_features(),
            cpu_cores: 8,
            memory_gb,
            metal_available: true,
            bandwidth_mbps: 100.0,
            battery_pct: None,
            available_memory_gb: memory_gb,
            region: "test".to_string(),
        }
    }

    /// `gb` is the on-disk weight size. `required_memory_gb` multiplies it by
    /// `MEMORY_OVERHEAD_FACTOR`, so a 10 GB model needs 12 GB of headroom.
    fn make_model_of(layers: u32, gb: f32) -> ModelManifest {
        ModelManifest {
            id: ModelId("test-model".to_string()),
            total_layers: layers,
            hidden_dim: 4096,
            vocab_size: 32000,
            head_count: 32,
            kv_head_count: 32,
            context_length: 2048,
            file_size_bytes: (gb * BYTES_PER_GB) as u64,
            quantization: "Q4_0".to_string(),
        }
    }

    #[test]
    fn single_node_gets_all_layers() {
        let model = make_model_of(32, 4.0);
        let peers = vec![make_peer("phone", 8.0)];
        let topo = ShardAssigner::assign(&model, &peers).unwrap();
        assert_eq!(topo.stages.len(), 1);
        assert_eq!(topo.stages[0].layer_range, LayerRange::new(0, 32));
    }

    #[test]
    fn two_nodes_split_layers_when_the_model_does_not_fit_on_one() {
        // 10 GB × 1.2 = 12 GB required; the coordinator has 4.
        let model = make_model_of(32, 10.0);
        let peers = vec![make_peer("phone", 4.0), make_peer("mac", 12.0)];
        let topo = ShardAssigner::assign(&model, &peers).unwrap();
        assert_eq!(topo.stages.len(), 2);
        // Phone gets ~25% of layers, mac gets ~75%
        let phone_layers = topo.stages[0].layer_range.count();
        let mac_layers = topo.stages[1].layer_range.count();
        assert_eq!(phone_layers + mac_layers, 32);
        assert!(mac_layers > phone_layers);
    }

    /// #164 §B — round-trips per token scale with the number of split points,
    /// so a peer that is not needed must not become one. This is the case the
    /// old "one stage per connected peer" logic got exactly backwards.
    #[test]
    fn extra_peers_do_not_add_split_points() {
        let model = make_model_of(32, 4.0); // needs 4.8 GB
        let peers = vec![
            make_peer("coordinator", 16.0),
            make_peer("idle-1", 64.0),
            make_peer("idle-2", 64.0),
            make_peer("idle-3", 64.0),
        ];

        let topo = ShardAssigner::assign(&model, &peers).unwrap();
        assert_eq!(
            topo.stages.len(),
            1,
            "a model that fits on the coordinator must not be split"
        );
        assert_eq!(topo.stages[0].layer_range, LayerRange::new(0, 32));
    }

    /// Take the fewest peers that cover the model, largest first — not every
    /// peer that happens to be connected.
    #[test]
    fn selection_takes_the_minimum_number_of_peers() {
        // 40 GB × 1.2 = 48 GB required. Coordinator 8 + biggest peer 48 = 56,
        // so exactly two stages, and the small peers stay out of the plan.
        let model = make_model_of(80, 40.0);
        let peers = vec![
            make_peer("coordinator", 8.0),
            make_peer("small", 4.0),
            make_peer("huge", 48.0),
            make_peer("medium", 16.0),
        ];

        let topo = ShardAssigner::assign(&model, &peers).unwrap();
        assert_eq!(topo.stages.len(), 2, "got {:?}", topo.stages);
        assert_eq!(topo.stages[0].node_id, make_peer("coordinator", 8.0).node_id);
        assert_eq!(topo.stages[1].node_id, make_peer("huge", 48.0).node_id);
    }

    /// If nothing covers the model, use everything rather than refusing —
    /// llama.cpp reports the real shortfall far better than a planner guess.
    #[test]
    fn an_oversized_model_falls_back_to_every_peer() {
        let model = make_model_of(80, 500.0);
        let peers = vec![
            make_peer("coordinator", 8.0),
            make_peer("a", 8.0),
            make_peer("b", 8.0),
        ];

        let topo = ShardAssigner::assign(&model, &peers).unwrap();
        assert_eq!(topo.stages.len(), 3);
    }
}
