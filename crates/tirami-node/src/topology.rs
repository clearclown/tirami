use serde::{Deserialize, Serialize};
use tirami_core::{Config, ModelManifest, NodeId, PeerCapability, PipelineTopology, TiramiError};
use tirami_shard::ShardAssigner;

/// A runtime snapshot of the current split-inference plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologySnapshot {
    pub model: Option<ModelManifest>,
    pub local_capability: Option<PeerCapability>,
    pub connected_peers: Vec<PeerCapability>,
    pub planned_topology: Option<PipelineTopology>,
}

/// Total and currently-free RAM in GiB, measured from the host.
///
/// Before #163 both numbers came from `config.max_memory_gb`, which defaults
/// to `4.0`. A 64 GB Mac mini and a 15 GB laptop therefore advertised exactly
/// the same capacity, and `available_memory_gb` — the *only* input
/// `ShardAssigner` consults — was a constant. Any split plan built on it was
/// fiction.
fn measure_memory_gb() -> (f32, f32) {
    use sysinfo::System;

    let mut sys = System::new();
    sys.refresh_memory();

    let total_bytes = sys.total_memory();

    // On Linux `available_memory()` is `MemAvailable`, which is exactly the
    // number we want. On macOS sysinfo 0.32 reports 0 for it, and
    // `free_memory()` there counts only genuinely free pages — 1.4 GiB on an
    // idle 32 GiB machine, because macOS keeps the rest as reclaimable cache.
    // Believing that would make every Mac look too small to serve anything.
    // `total - used` tracks free + inactive, which is what actually becomes
    // available under memory pressure (verified against `vm_stat`).
    let available_bytes = match sys.available_memory() {
        0 => total_bytes.saturating_sub(sys.used_memory()),
        n => n,
    };

    const BYTES_PER_GB: f64 = 1024.0 * 1024.0 * 1024.0;
    (
        (total_bytes as f64 / BYTES_PER_GB) as f32,
        (available_bytes as f64 / BYTES_PER_GB) as f32,
    )
}

/// GPU capabilities, expressed as `features` entries.
///
/// `features` is the existing, `Vec<String>`-shaped extension point, so this
/// adds no struct fields. That matters: `PeerCapability` travels inside
/// `Hello` / `Welcome` as bincode, which is not self-describing, so a new
/// field would break the wire for every already-running node — unlike the
/// unreachable RPC messages, these are live.
fn gpu_features() -> Vec<String> {
    let mut features = Vec::new();
    // Metal is compiled in by `llama-cpp-sys-2`'s build script on macOS
    // regardless of cargo features, so on macOS this is accurate.
    if cfg!(target_os = "macos") {
        features.push("gpu:metal".to_string());
    }
    // CUDA is a build-time choice; the feature is a pure passthrough to
    // llama.cpp with no Rust `cfg` of its own, so key off the same one.
    if cfg!(feature = "cuda") {
        features.push("gpu:cuda".to_string());
    }
    features
}

/// Build the local node capability advertisement used during cluster handshakes.
pub fn build_local_capability(config: &Config, node_id: NodeId) -> PeerCapability {
    let cpu_cores = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .min(u16::MAX as usize) as u16;

    let (total_gb, free_gb) = measure_memory_gb();

    // `max_memory_gb` stops being the *source* of the number and becomes what
    // it reads like: a ceiling the operator sets on how much of the machine
    // this node offers. 0 or negative means "no ceiling".
    let ceiling = config.max_memory_gb;
    let available_memory_gb = if ceiling > 0.0 {
        free_gb.min(ceiling)
    } else {
        free_gb
    };

    let mut features = tirami_core::advertised_protocol_features_with_backend(
        false,
        &config.proof_policy,
        &config.zkml_backend,
    );
    features.extend(gpu_features());

    PeerCapability {
        node_id,
        protocol_version: tirami_core::TIRAMI_PROTOCOL_VERSION,
        features,
        cpu_cores,
        memory_gb: total_gb,
        metal_available: cfg!(target_os = "macos"),
        // Left at the historical placeholder on purpose. It is written here
        // and read nowhere — `ShardAssigner`, `topology.rs`, and
        // `discovery.rs` all sort on `available_memory_gb` only. Advertising
        // a measured number would imply peers act on it; they do not. #164
        // measured a 60× spread (Thunderbolt 1.53 GB/s vs Wi-Fi 25 MB/s), so
        // when selection does start using link quality it should use
        // observed RTT from the live connection, not a self-reported rate.
        bandwidth_mbps: 100.0,
        battery_pct: None,
        available_memory_gb,
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
    use tirami_core::{Config, ModelId, ModelManifest, NodeId, PeerCapability};

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

    /// A capability with an explicit free-memory figure. `build_local_capability`
    /// now measures the host, which is right in production and useless in a
    /// test — the numbers would change with whatever else is running.
    fn peer_with_memory(id: u8, available_gb: f32) -> PeerCapability {
        PeerCapability {
            node_id: NodeId([id; 32]),
            protocol_version: tirami_core::TIRAMI_PROTOCOL_VERSION,
            features: tirami_core::base_protocol_features(),
            cpu_cores: 8,
            memory_gb: available_gb,
            metal_available: false,
            bandwidth_mbps: 100.0,
            battery_pct: None,
            available_memory_gb: available_gb,
            region: "test".to_string(),
        }
    }

    fn model_of_size(layers: u32, gb: f32) -> ModelManifest {
        ModelManifest {
            file_size_bytes: (gb * 1024.0 * 1024.0 * 1024.0) as u64,
            ..make_model(layers)
        }
    }

    /// #164 §B — adding a peer must not add a split point. Splitting costs
    /// round-trips per token, so a model that already fits stays on one node
    /// no matter how many peers are connected.
    #[test]
    fn a_model_that_fits_locally_is_not_split_across_peers() {
        let local = peer_with_memory(1, 32.0);
        let remote = peer_with_memory(2, 64.0);

        let snapshot =
            build_topology_snapshot(Some(model_of_size(32, 8.0)), Some(local), vec![remote])
                .unwrap();
        let topology = snapshot.planned_topology.unwrap();

        assert_eq!(
            topology.stages.len(),
            1,
            "an 8 GB model fits in 32 GB; a connected peer must not force a split"
        );
        assert_eq!(topology.stages[0].node_id, NodeId([1u8; 32]));
    }

    #[test]
    fn a_model_too_large_for_one_node_keeps_the_local_stage_first() {
        let local = peer_with_memory(1, 8.0);
        let remote = peer_with_memory(2, 48.0);

        // 40 GB × 1.2 overhead = 48 GB required, more than the local 8 GB.
        let snapshot =
            build_topology_snapshot(Some(model_of_size(32, 40.0)), Some(local), vec![remote])
                .unwrap();
        let topology = snapshot.planned_topology.unwrap();

        assert_eq!(topology.stages.len(), 2);
        assert_eq!(topology.stages[0].node_id, NodeId([1u8; 32]));
        assert_eq!(topology.stages[1].node_id, NodeId([2u8; 32]));
    }

    /// `available_memory_gb` used to be `config.max_memory_gb` verbatim, so a
    /// 64 GB machine and a 15 GB laptop advertised the same 4.0 default.
    #[test]
    fn advertised_memory_is_measured_not_configured() {
        let unconfigured = build_local_capability(&Config::default(), NodeId([1u8; 32]));

        // The default ceiling is 4.0, so `available` is capped there...
        assert!(unconfigured.available_memory_gb <= 4.0);
        // ...but `memory_gb` is now the real machine, which is certainly
        // larger than the old hardcoded default on any host that can build
        // this workspace.
        assert!(
            unconfigured.memory_gb > 4.0,
            "memory_gb must reflect the host, got {}",
            unconfigured.memory_gb
        );

        // With the ceiling lifted, free memory is reported as measured.
        let uncapped = build_local_capability(
            &Config {
                max_memory_gb: 0.0,
                ..Config::default()
            },
            NodeId([1u8; 32]),
        );
        assert!(
            uncapped.available_memory_gb > 0.0,
            "free memory must be measured, got {}",
            uncapped.available_memory_gb
        );
        assert!(uncapped.available_memory_gb <= uncapped.memory_gb);
    }
}
