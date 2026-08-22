//! Integration tests for the split-inference orchestrator (#162).
//!
//! These call `start_split_session` against real `ForgeTransport` endpoints.
//! An earlier unit test in `split_inference.rs` re-implemented the
//! local-vs-remote filter inside the test and asserted on its own copy, so it
//! would have passed with the production branch inverted. These do not.

use std::collections::HashMap;
use std::sync::Arc;
use tirami_core::{ModelId, NodeId, PipelineStage, PipelineTopology};
use tirami_core::LayerRange;
use tirami_net::ForgeTransport;
use tirami_node::split_inference::{RpcReadyDispatcher, start_split_session};
use tokio::sync::Mutex;

fn is_socket_bind_denied(err: &anyhow::Error) -> bool {
    let message = format!("{err:#}");
    message.contains("Operation not permitted") || message.contains("Permission denied")
}

async fn transport_or_skip(label: &str) -> Option<ForgeTransport> {
    match ForgeTransport::new().await {
        Ok(t) => Some(t),
        Err(err) if is_socket_bind_denied(&err) => {
            eprintln!("skipping split-session test: {label}: {err:#}");
            None
        }
        Err(err) => panic!("{label}: {err:#}"),
    }
}

fn dispatcher() -> RpcReadyDispatcher {
    Arc::new(Mutex::new(HashMap::new()))
}

fn plan_of(stages: &[(NodeId, u32, u32)]) -> PipelineTopology {
    PipelineTopology {
        model_id: ModelId("test-model".to_string()),
        stages: stages
            .iter()
            .enumerate()
            .map(|(i, (id, start, end))| PipelineStage {
                node_id: id.clone(),
                layer_range: LayerRange::new(*start, *end),
                position: i as u8,
            })
            .collect(),
    }
}

/// A plan whose only stage is this node must not open a session — and must say
/// so by returning `None`, not by returning an empty session.
#[tokio::test]
async fn a_local_only_plan_opens_no_session() {
    let Some(transport) = transport_or_skip("local-only").await else {
        return;
    };
    let local = transport.tirami_node_id();
    let plan = plan_of(&[(local.clone(), 0, 32)]);

    let result = start_split_session(
        &transport,
        &dispatcher(),
        &plan,
        &local,
        &ModelId("test-model".to_string()),
    )
    .await;

    match result {
        Ok(None) => {}
        Ok(Some(session)) => panic!(
            "opened {} stage(s) for a plan with no remote node",
            session.stages.len()
        ),
        Err(e) => panic!("a local-only plan should be a no-op, got: {e}"),
    }

    transport.close().await;
}

/// A stage naming a peer we have never connected to cannot be started. The
/// orchestrator must surface that rather than hanging or reporting success.
#[tokio::test]
async fn an_unreachable_peer_fails_instead_of_hanging() {
    let Some(transport) = transport_or_skip("unreachable-peer").await else {
        return;
    };
    let local = transport.tirami_node_id();
    let stranger = NodeId([0xAB; 32]);
    let plan = plan_of(&[(local.clone(), 0, 4), (stranger, 4, 32)]);

    let started = std::time::Instant::now();
    let result = start_split_session(
        &transport,
        &dispatcher(),
        &plan,
        &local,
        &ModelId("test-model".to_string()),
    )
    .await;
    let elapsed = started.elapsed();

    assert!(result.is_err(), "an unconnected peer must not report success");
    // The 20 s ready-timeout is for a peer that accepted the request. A peer we
    // cannot even send to has to fail immediately.
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "failed only after {elapsed:?}; an unsendable peer should not wait out the ready timeout"
    );

    transport.close().await;
}

/// Two real endpoints: the coordinator plans a split naming the peer, and the
/// peer captures the `StartRpcServer` that arrives. This asserts on the wire
/// message, not on internal bookkeeping — an earlier version inspected the
/// dispatcher after the fact and passed even with `session_id` hard-coded to 0.
#[tokio::test]
async fn the_request_that_reaches_the_peer_is_well_formed() {
    let Some(coordinator) = transport_or_skip("coordinator").await else {
        return;
    };
    let Some(peer) = transport_or_skip("peer").await else {
        coordinator.close().await;
        return;
    };
    let peer = Arc::new(peer);

    let _accept = peer.start_accepting();
    let peer_node = peer.tirami_node_id();
    if coordinator.connect(peer.endpoint_addr()).await.is_err() {
        eprintln!("skipping: could not connect the two endpoints");
        return;
    }

    // Capture whatever the peer receives.
    let captured: Arc<Mutex<Option<tirami_proto::StartRpcServer>>> = Arc::new(Mutex::new(None));
    {
        let peer = Arc::clone(&peer);
        let captured = Arc::clone(&captured);
        tokio::spawn(async move {
            while let Some((_from, envelope)) = peer.recv().await {
                if let tirami_proto::Payload::StartRpcServer(req) = envelope.payload {
                    *captured.lock().await = Some(req);
                    break;
                }
            }
        });
    }

    let local = coordinator.tirami_node_id();
    let plan = plan_of(&[(local.clone(), 0, 4), (peer_node, 4, 32)]);

    // The peer never replies, so this ends in a ready-timeout. The request it
    // sent first is what we are checking.
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        start_split_session(
            &coordinator,
            &dispatcher(),
            &plan,
            &local,
            &ModelId("test-model".to_string()),
        ),
    )
    .await;

    let req = captured
        .lock()
        .await
        .clone()
        .expect("the peer should have received a StartRpcServer");

    assert_ne!(
        req.session_id, 0,
        "session_id 0 is rejected by protocol validation, so it can never be minted"
    );
    assert_eq!(
        req.layer_range,
        LayerRange::new(4, 32),
        "the peer must be told the layers it was planned for"
    );
    assert_eq!(req.model_id.0, "test-model");
    assert!(
        req.port >= 1024,
        "a privileged port is rejected by protocol validation, got {}",
        req.port
    );

    coordinator.close().await;
    peer.close().await;
}
