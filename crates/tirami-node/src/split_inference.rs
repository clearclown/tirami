//! Sender side of multi-machine model splitting (#162).
//!
//! The protocol messages (`StartRpcServer` / `RpcServerReady` /
//! `RpcServerFailed`), the receive handler, the rpc-server manager, and the
//! `llama-cli --rpc` wrapper all existed. Nothing constructed a
//! `StartRpcServer`, so none of it was reachable — that is what this module
//! adds.
//!
//! Sequence for one session:
//!
//! ```text
//!   plan (ShardAssigner)
//!     → StartRpcServer to each remote stage, one session id each
//!     → collect RpcServerReady / RpcServerFailed by session id
//!     → dial the tunnel ALPN, bind a loopback port per peer
//!     → hand "127.0.0.1:<port>,..." to run_distributed_inference
//!     → StopRpcServer on the way out
//! ```
//!
//! ## Scope
//!
//! LAN and Thunderbolt. #164 measured weight transfer as latency-bound —
//! 17 GiB took 139 s at 0.755 ms RTT and 867 s at 6.97 ms, extrapolating to
//! ~40 min at 20 ms — so splitting a model across a WAN is not viable with
//! llama.cpp RPC alone. Content-addressed weight distribution (iroh-blobs) is
//! the prerequisite for that, and is not in this module.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tirami_core::{LayerRange, ModelId, NodeId, PipelineTopology, TiramiError};
use tirami_net::ForgeTransport;
use tirami_proto::{Envelope, Payload, StartRpcServer, StopRpcServer};
use tokio::sync::{Mutex, oneshot};

/// Session id → the waiting starter, resolved by the seed recv loop when the
/// peer answers. Mirrors `TradeAcceptDispatcher`: the recv loop is the single
/// consumer of `transport.recv()`, so it has to hand replies back by hand.
pub type RpcReadyDispatcher = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<u16, String>>>>>;

/// Port the first remote stage is asked to bind. Later stages take the next
/// ports up. The peer reports the port it actually bound, so a collision on
/// its side surfaces as `RpcServerFailed` rather than silent misrouting.
const FIRST_RPC_PORT: u16 = 50052;

/// How long to wait for a peer to have its rpc-server listening.
/// `RpcServer::spawn` itself polls for 10 s, so this must exceed that.
const READY_TIMEOUT: Duration = Duration::from_secs(20);

/// One peer participating in a split.
#[derive(Debug, Clone)]
pub struct RemoteStage {
    pub peer_id: String,
    pub session_id: u64,
    pub layer_range: LayerRange,
    /// Loopback port on *this* machine that tunnels to the peer's rpc-server.
    /// This is what `llama-cli --rpc` connects to; it never learns the peer
    /// exists.
    pub local_port: u16,
}

/// A live split: rpc-servers running on peers, tunnels bridging them here.
pub struct SplitSession {
    pub stages: Vec<RemoteStage>,
    tunnels: Vec<tokio::task::JoinHandle<()>>,
}

impl SplitSession {
    /// Endpoints for `DistributedConfig::rpc_endpoints`, in stage order.
    pub fn rpc_endpoints(&self) -> Vec<String> {
        self.stages
            .iter()
            .map(|s| format!("127.0.0.1:{}", s.local_port))
            .collect()
    }

    /// Release every peer's rpc-server and stop the tunnels.
    ///
    /// Worth calling even on the error path: before #163 the subprocess had no
    /// stop message at all and lived until the peer's process exited, so its
    /// port could never be reused.
    pub async fn shutdown(self, transport: &ForgeTransport, local_node: &NodeId) {
        for stage in &self.stages {
            let msg = Envelope {
                msg_id: rand::random(),
                sender: local_node.clone(),
                timestamp: now_millis(),
                payload: Payload::StopRpcServer(StopRpcServer {
                    session_id: stage.session_id,
                }),
            };
            if let Err(e) = transport.send_to(&stage.peer_id, &msg).await {
                tracing::warn!("StopRpcServer to {} failed: {}", stage.peer_id, e);
            }
        }
        for handle in self.tunnels {
            handle.abort();
        }
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Ask the OS for a free loopback port.
///
/// Binding and immediately dropping leaves a window where something else could
/// take it; the tunnel binds it again a few lines later. The alternative —
/// handing the listener through — would mean `start_seed_tunnel` could not own
/// its socket. A collision surfaces as a bind error at tunnel start, which is
/// loud, so the race is acceptable.
async fn free_local_port() -> Result<u16, TiramiError> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| TiramiError::NetworkError(format!("no free local port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| TiramiError::NetworkError(format!("local_addr: {e}")))?
        .port();
    drop(listener);
    Ok(port)
}

/// Start rpc-servers on every remote stage of `plan` and tunnel them here.
///
/// Stages whose node is `local_node` are skipped — this machine runs those
/// layers itself. Returns `Ok(None)` when the plan has no remote stage, which
/// is the normal case for a model that fits locally.
pub async fn start_split_session(
    transport: &ForgeTransport,
    dispatcher: &RpcReadyDispatcher,
    plan: &PipelineTopology,
    local_node: &NodeId,
    model_id: &ModelId,
) -> Result<Option<SplitSession>, TiramiError> {
    let remote: Vec<_> = plan
        .stages
        .iter()
        .filter(|stage| stage.node_id != *local_node)
        .collect();

    if remote.is_empty() {
        return Ok(None);
    }

    tracing::info!(
        "Starting split session for {} across {} remote stage(s)",
        model_id.0,
        remote.len()
    );

    // Phase 1 — ask every peer to start, and register where the answer goes
    // before sending so a fast reply cannot arrive first.
    let mut pending = Vec::with_capacity(remote.len());
    for (i, stage) in remote.iter().enumerate() {
        let session_id = loop {
            // 0 is reserved: the protocol rejects it, which is how a peer that
            // predates session ids gets caught rather than silently matched.
            let candidate: u64 = rand::random();
            if candidate != 0 {
                break candidate;
            }
        };
        let peer_id = stage.node_id.to_hex();
        let port = FIRST_RPC_PORT + i as u16;

        let (tx, rx) = oneshot::channel();
        dispatcher.lock().await.insert(session_id, tx);

        let msg = Envelope {
            msg_id: rand::random(),
            sender: local_node.clone(),
            timestamp: now_millis(),
            payload: Payload::StartRpcServer(StartRpcServer {
                model_id: model_id.clone(),
                layer_range: stage.layer_range,
                port,
                session_id,
            }),
        };

        if let Err(e) = transport.send_to(&peer_id, &msg).await {
            dispatcher.lock().await.remove(&session_id);
            return Err(TiramiError::NetworkError(format!(
                "StartRpcServer to {peer_id} failed: {e}"
            )));
        }

        pending.push((peer_id, session_id, stage.layer_range, rx));
    }

    // Phase 2 — collect the answers. A peer with `rpc_server_enabled = false`
    // replies `RpcServerFailed` promptly rather than timing out.
    let mut stages = Vec::with_capacity(pending.len());
    for (peer_id, session_id, layer_range, rx) in pending {
        let outcome = tokio::time::timeout(READY_TIMEOUT, rx).await;
        dispatcher.lock().await.remove(&session_id);

        let remote_port = match outcome {
            Ok(Ok(Ok(port))) => port,
            Ok(Ok(Err(reason))) => {
                return Err(TiramiError::InferenceError(format!(
                    "peer {peer_id} refused to start an rpc-server: {reason}"
                )));
            }
            Ok(Err(_)) => {
                return Err(TiramiError::NetworkError(format!(
                    "peer {peer_id} dropped the rpc-server request"
                )));
            }
            Err(_) => {
                return Err(TiramiError::NetworkError(format!(
                    "peer {peer_id} did not report an rpc-server within {}s",
                    READY_TIMEOUT.as_secs()
                )));
            }
        };

        stages.push(RemoteStage {
            peer_id,
            session_id,
            layer_range,
            // Filled in by phase 3.
            local_port: remote_port,
        });
    }

    // Phase 3 — bridge each peer's rpc-server to a loopback port here. The
    // peer's server binds 127.0.0.1 on its own machine and is unreachable
    // from the network by design, so the tunnel is what makes it addressable.
    let mut tunnels = Vec::with_capacity(stages.len());
    for stage in stages.iter_mut() {
        let conn = transport
            .connect_rpc_tunnel_to(&stage.peer_id)
            .await
            .map_err(|e| {
                TiramiError::NetworkError(format!("rpc tunnel to {} failed: {e}", stage.peer_id))
            })?;

        let local_port = free_local_port().await?;
        let handle = tirami_net::tcp_tunnel::start_seed_tunnel(local_port, stage.session_id, conn)
            .await
            .map_err(|e| TiramiError::NetworkError(format!("tunnel bind failed: {e}")))?;

        tracing::info!(
            "Split stage: peer {} layers {}..{} via 127.0.0.1:{}",
            &stage.peer_id[..8.min(stage.peer_id.len())],
            stage.layer_range.start,
            stage.layer_range.end,
            local_port
        );

        stage.local_port = local_port;
        tunnels.push(handle);
    }

    Ok(Some(SplitSession { stages, tunnels }))
}

/// Plan a split for a model **file**, without loading it.
///
/// The first version of the endpoint planned from `advertised_topology`, which
/// is built from the model this node already loaded. That made the feature
/// self-defeating: splitting a model too large for one machine first required
/// loading it on one machine. Measured on an Apple M4 (Metal working set
/// 26.2 GiB) with a 39.6 GiB GGUF, that attempt ends in
/// `GGML_ASSERT([rsets->data count] == 0) failed`.
///
/// The manifest comes from the GGUF header instead — a few KB of reads — so a
/// node can plan for a model it could never hold.
pub fn plan_split_for_model(
    model_path: &std::path::Path,
    local: &tirami_core::PeerCapability,
    peers: &[tirami_core::PeerCapability],
) -> Result<PipelineTopology, TiramiError> {
    let manifest = tirami_infer::gguf::parse_gguf_metadata(model_path)?;

    let mut candidates = Vec::with_capacity(1 + peers.len());
    candidates.push(local.clone());
    candidates.extend(peers.iter().cloned());

    tirami_shard::ShardAssigner::assign(&manifest, &candidates)
}

/// Proportional `-ts` string for a plan, in stage order.
///
/// llama.cpp splits by these weights across local + RPC devices. Passing the
/// layer counts we already planned keeps llama.cpp from re-deriving its own
/// ratio, which #164 saw leave a device nearly empty.
pub fn tensor_split_for(plan: &PipelineTopology) -> String {
    plan.stages
        .iter()
        .map(|s| (s.layer_range.end - s.layer_range.start).to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tirami_core::PipelineStage;

    fn plan(stages: &[(u8, u32, u32)]) -> PipelineTopology {
        PipelineTopology {
            model_id: ModelId("m".to_string()),
            stages: stages
                .iter()
                .enumerate()
                .map(|(i, (id, start, end))| PipelineStage {
                    node_id: NodeId([*id; 32]),
                    layer_range: LayerRange::new(*start, *end),
                    position: i as u8,
                })
                .collect(),
        }
    }

    #[test]
    fn tensor_split_follows_the_planned_layer_counts() {
        let plan = plan(&[(1, 0, 21), (2, 21, 64)]);
        assert_eq!(tensor_split_for(&plan), "21,43");
    }

    #[test]
    fn endpoints_are_loopback_in_stage_order() {
        let session = SplitSession {
            stages: vec![
                RemoteStage {
                    peer_id: "aa".to_string(),
                    session_id: 1,
                    layer_range: LayerRange::new(0, 10),
                    local_port: 41001,
                },
                RemoteStage {
                    peer_id: "bb".to_string(),
                    session_id: 2,
                    layer_range: LayerRange::new(10, 20),
                    local_port: 41002,
                },
            ],
            tunnels: Vec::new(),
        };

        assert_eq!(
            session.rpc_endpoints(),
            vec!["127.0.0.1:41001".to_string(), "127.0.0.1:41002".to_string()]
        );
    }
}
