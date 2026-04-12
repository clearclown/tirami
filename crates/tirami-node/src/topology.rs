use tirami_core::{Config, TiramiError, ModelManifest, NodeId, PeerCapability, PipelineTopology};
use tirami_shard::ShardAssigner;
use serde::{Deserialize, Serialize};

/// A runtime snapshot of the current split-inference plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologySnapshot {
    pub model: Option<ModelManifest>,
    pub local_capability: Option<PeerCapability>,
    pub connected_peers: Vec<PeerCapability>,
    pub planned_topology: Option<PipelineTopology>,
}

/// Build the local node capability advertisement used during cluster handshakes.
pub fn build_local_capability(config: &Config, node_id: NodeId) -> PeerCapability {
    let cpu_cores = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .min(u16::MAX as usize) as u16;

    PeerCapability {
        node_id,
        cpu_cores,
        memory_gb: config.max_memory_gb,
        metal_available: cfg!(target_os = "macos"),
        bandwidth_mbps: 100.0,
        battery_pct: None,
        available_memory_gb: config.max_memory_gb,
        region: config.region.clone(),
    }
}

/// Compute the current topology plan from the local model and connected peers.
pub fn build_topology_snapshot(
    model: Option<ModelManifest>,
    local_capability: Option<PeerCapability>,
    mut connected_peers: Vec<PeerCapability>,
) -> Result<TopologySnapshot, TiramiError> {
    connected_peers.sort_by(|a, b| {
        b.available_memory_gb
            .partial_cmp(&a.available_memory_gb)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let planned_topology = match (model.as_ref(), local_capability.clone()) {
        (Some(model), Some(local)) => {
            let mut peers = Vec::with_capacity(1 + connected_peers.len());
            peers.push(local);
            peers.extend(connected_peers.iter().cloned());
            Some(ShardAssigner::assign(model, &peers)?)
        }
        _ => None,
    };

    Ok(TopologySnapshot {
        model,
        local_capability,
        connected_peers,
        planned_topology,
    })
}

#[cfg(test)]
mod tests {
    use super::{build_local_capability, build_topology_snapshot};
    use tirami_core::{Config, ModelId, ModelManifest, NodeId};

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
    fn local_only_snapshot_plans_single_stage() {
        let config = Config {
            max_memory_gb: 8.0,
            region: "test".to_string(),
            ..Config::default()
        };
        let local = build_local_capability(&config, NodeId([1u8; 32]));

        let snapshot = build_topology_snapshot(Some(make_model(32)), Some(local), vec![]).unwrap();
        let topology = snapshot.planned_topology.unwrap();

        assert_eq!(topology.stages.len(), 1);
        assert_eq!(topology.stages[0].node_id, NodeId([1u8; 32]));
        assert_eq!(topology.stages[0].layer_range.start, 0);
        assert_eq!(topology.stages[0].layer_range.end, 32);
    }

    #[test]
    fn connected_peers_keep_local_stage_first() {
        let config = Config {
            max_memory_gb: 4.0,
            region: "test".to_string(),
            ..Config::default()
        };
        let local = build_local_capability(&config, NodeId([1u8; 32]));
        let remote = build_local_capability(
            &Config {
                max_memory_gb: 12.0,
                region: "test".to_string(),
                ..Config::default()
            },
            NodeId([2u8; 32]),
        );

        let snapshot =
            build_topology_snapshot(Some(make_model(32)), Some(local), vec![remote]).unwrap();
        let topology = snapshot.planned_topology.unwrap();

        assert_eq!(topology.stages.len(), 2);
        assert_eq!(topology.stages[0].node_id, NodeId([1u8; 32]));
        assert_eq!(topology.stages[1].node_id, NodeId([2u8; 32]));
    }
}
