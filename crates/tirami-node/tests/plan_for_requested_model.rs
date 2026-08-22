//! Planning a split must not require the model to be loadable on this node.
//!
//! The first implementation took its plan from `advertised_topology`, which is
//! built from whatever model the node **loaded locally**. Splitting a model too
//! large for one machine therefore required first loading it on one machine —
//! which is the exact thing the feature exists to avoid.
//!
//! Measured on an Apple M4 (Metal working set 26.2 GiB) with
//! Hermes-4-70B-Q4_K_M (39.6 GiB):
//!
//! ```text
//! common_fit_params: failed to fit params to free device memory ... abort
//! load_tensors: MTL0 model buffer size = 39979.48 MiB
//! ggml-metal-device.m:657: GGML_ASSERT([rsets->data count] == 0) failed
//! ```
//!
//! These tests need a real GGUF. Point `TIRAMI_TEST_LARGE_GGUF` at one; without
//! it they skip rather than pretend.

use std::path::PathBuf;
use tirami_core::{NodeId, PeerCapability};
use tirami_node::split_inference::plan_split_for_model;

fn large_gguf() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var("TIRAMI_TEST_LARGE_GGUF").ok()?);
    if path.is_file() {
        Some(path)
    } else {
        eprintln!("skipping: TIRAMI_TEST_LARGE_GGUF does not point at a file");
        None
    }
}

fn peer(id: u8, available_gb: f32) -> PeerCapability {
    PeerCapability {
        node_id: NodeId([id; 32]),
        protocol_version: tirami_core::TIRAMI_PROTOCOL_VERSION,
        features: tirami_core::base_protocol_features(),
        cpu_cores: 8,
        memory_gb: available_gb,
        metal_available: true,
        bandwidth_mbps: 100.0,
        battery_pct: None,
        available_memory_gb: available_gb,
        region: "test".to_string(),
    }
}

/// The headline case. A 39.6 GiB model against a 26.2 GiB coordinator and a
/// 29 GiB peer: neither fits it alone, together they do, so the plan must use
/// both — derived from the file on disk, with nothing loaded.
#[test]
fn a_model_too_large_for_this_node_still_gets_a_plan() {
    let Some(model) = large_gguf() else {
        return;
    };

    let local = peer(1, 26.2);
    let peers = vec![peer(2, 29.0)];

    let plan = plan_split_for_model(&model, &local, &peers)
        .expect("planning must not require loading the model");

    assert_eq!(
        plan.stages.len(),
        2,
        "39.6 GiB fits on neither node alone, so both must be used; got {:?}",
        plan.stages
    );
    assert_eq!(
        plan.stages[0].node_id, local.node_id,
        "the coordinator keeps the first stage"
    );

    let total: u32 = plan
        .stages
        .iter()
        .map(|s| s.layer_range.end - s.layer_range.start)
        .sum();
    assert!(total > 0, "the plan must cover layers");
}

/// The same file against a machine that can hold it needs no remote stage.
/// This is what keeps the endpoint from splitting for no reason.
#[test]
fn a_node_that_can_hold_the_model_plans_a_single_stage() {
    let Some(model) = large_gguf() else {
        return;
    };

    let local = peer(1, 128.0);
    let peers = vec![peer(2, 128.0)];

    let plan = plan_split_for_model(&model, &local, &peers).expect("planning succeeds");

    assert_eq!(
        plan.stages.len(),
        1,
        "a model that fits must not be split across a peer"
    );
}

/// A path that is not a GGUF has to fail loudly; guessing a manifest would
/// produce a plan for a model that does not exist.
#[test]
fn a_non_gguf_path_is_an_error() {
    let local = peer(1, 26.2);
    assert!(plan_split_for_model(std::path::Path::new("/etc/hosts"), &local, &[]).is_err());
}
