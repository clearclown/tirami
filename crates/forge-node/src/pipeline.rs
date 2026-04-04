use forge_core::{Config, ModelManifest, NodeId, PipelineTopology};
use forge_ledger::{ComputeLedger, SignedTradeRecord, TradeRecord};
use forge_net::{ClusterManager, ForgeTransport, GossipState};
use forge_proto::{
    Envelope, ErrorCode, ErrorMsg, InferenceRequest, Payload, PipelineTopologyMsg, RpcServerFailed,
    RpcServerReady, TokenStreamMsg, TradeAccept, TradeProposal, Welcome,
};
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

/// The role of a node in the inference pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineRole {
    /// Seed node: holds model, serves inference requests.
    Seed,
    /// Worker node: connects to seed, requests inference.
    Worker,
}

/// Handles distributed inference requests over the P2P network.
pub struct PipelineCoordinator {
    transport: Arc<ForgeTransport>,
}

impl PipelineCoordinator {
    pub fn new(transport: Arc<ForgeTransport>) -> Self {
        Self { transport }
    }

    /// Run the seed loop — accept and process inference requests.
    pub async fn run_seed(
        &self,
        engine: Arc<Mutex<forge_infer::CandleEngine>>,
        ledger: Arc<Mutex<ComputeLedger>>,
        model_manifest: Arc<Mutex<Option<ModelManifest>>>,
        advertised_topology: Arc<Mutex<Option<PipelineTopology>>>,
        cluster: Option<Arc<ClusterManager>>,
        config: Config,
        ledger_path: Option<std::path::PathBuf>,
    ) -> anyhow::Result<()> {
        let node_id = self.transport.forge_node_id();
        tracing::info!("Pipeline seed running, waiting for requests...");
        let _heartbeat = cluster.as_ref().map(|c| c.start_heartbeat());
        let _failure_detector = cluster.as_ref().map(|c| c.start_failure_detector(15));
        let request_slots = Arc::new(Semaphore::new(
            config.max_concurrent_remote_inference_requests,
        ));
        let gossip = Arc::new(Mutex::new(GossipState::new()));

        loop {
            match self.transport.recv().await {
                Some((peer_id, envelope)) => match envelope.payload.clone() {
                    Payload::Hello(_) => {
                        if let Some(cluster) = cluster.as_ref() {
                            cluster.handle_message(&peer_id, envelope).await;

                            let welcome = Envelope {
                                msg_id: rand::random(),
                                sender: node_id.clone(),
                                timestamp: now_millis(),
                                payload: Payload::Welcome(Welcome {
                                    version: forge_net::PROTOCOL_VERSION,
                                    capability: cluster.local_capability().clone(),
                                    known_peers: Vec::new(),
                                }),
                            };

                            if let Err(err) = self.transport.send_to(&peer_id, &welcome).await {
                                tracing::warn!("Failed to send Welcome to {}: {}", peer_id, err);
                            }

                            if let Some(plan) = current_pipeline_topology(
                                &model_manifest,
                                &advertised_topology,
                                cluster,
                            )
                            .await
                            {
                                let msg = Envelope {
                                    msg_id: rand::random(),
                                    sender: node_id.clone(),
                                    timestamp: now_millis(),
                                    payload: Payload::PipelineTopology(PipelineTopologyMsg {
                                        model_id: plan.model_id,
                                        stages: plan.stages,
                                    }),
                                };

                                if let Err(err) = self.transport.send_to(&peer_id, &msg).await {
                                    tracing::warn!(
                                        "Failed to send PipelineTopology to {}: {}",
                                        peer_id,
                                        err
                                    );
                                }
                            }
                        }
                    }
                    Payload::Welcome(_) => {
                        if let Some(cluster) = cluster.as_ref() {
                            cluster.handle_message(&peer_id, envelope).await;
                        }
                    }
                    Payload::InferenceRequest(req) => {
                        // Never log prompt content — privacy protection
                        tracing::info!(
                            "Inference request from {}: {} chars, max {} tokens",
                            peer_id,
                            req.prompt_text.len(),
                            req.max_tokens
                        );
                        let permit = match request_slots.clone().try_acquire_owned() {
                            Ok(permit) => permit,
                            Err(_) => {
                                tracing::warn!(
                                    "Rejecting inference request {} from {}: seed at concurrency limit {}",
                                    req.request_id,
                                    peer_id,
                                    config.max_concurrent_remote_inference_requests
                                );
                                if let Err(err) = send_protocol_error(
                                    &self.transport,
                                    &peer_id,
                                    &node_id,
                                    req.request_id,
                                    ErrorCode::Busy,
                                    "seed is at max concurrent inference capacity".to_string(),
                                    true,
                                )
                                .await
                                {
                                    tracing::warn!(
                                        "Failed to send busy error to {}: {}",
                                        peer_id,
                                        err
                                    );
                                }
                                continue;
                            }
                        };

                        let engine = engine.clone();
                        let ledger = ledger.clone();
                        let transport = self.transport.clone();
                        let node_id = node_id.clone();
                        let sender_id = envelope.sender.clone();
                        let peer_id = peer_id.clone();
                        let config = config.clone();
                        let ledger_path = ledger_path.clone();
                        let gossip = gossip.clone();

                        tokio::spawn(async move {
                            let _permit = permit;
                            if let Err(e) = handle_inference(
                                &config,
                                engine,
                                ledger,
                                ledger_path,
                                transport,
                                node_id,
                                sender_id,
                                &peer_id,
                                req,
                                gossip,
                            )
                            .await
                            {
                                tracing::error!("Inference failed: {}", e);
                            }
                        });
                    }
                    Payload::Heartbeat(hb) => {
                        if let Some(cluster) = cluster.as_ref() {
                            cluster.handle_message(&peer_id, envelope).await;
                        }
                        tracing::debug!("Heartbeat from {}: load={:.0}%", peer_id, hb.load * 100.0);
                    }
                    Payload::StartRpcServer(req) => {
                        tracing::info!(
                            "Peer {} requests RPC server start (layers {}..{}, port {})",
                            peer_id,
                            req.layer_range.start,
                            req.layer_range.end,
                            req.port
                        );

                        // Attempt to start rpc-server subprocess
                        let transport = self.transport.clone();
                        let node_id = node_id.clone();
                        let peer_id = peer_id.clone();
                        tokio::spawn(async move {
                            match forge_infer::rpc_manager::RpcServer::spawn(req.port) {
                                Ok(_server) => {
                                    let msg = Envelope {
                                        msg_id: rand::random(),
                                        sender: node_id,
                                        timestamp: now_millis(),
                                        payload: Payload::RpcServerReady(RpcServerReady {
                                            port: req.port,
                                        }),
                                    };
                                    let _ = transport.send_to(&peer_id, &msg).await;
                                    // Keep server alive by holding _server
                                    // In production, store in a map
                                    tracing::info!("RPC server running on port {}", req.port);
                                    // Keep the task alive to hold the server process
                                    tokio::signal::ctrl_c().await.ok();
                                }
                                Err(e) => {
                                    let msg = Envelope {
                                        msg_id: rand::random(),
                                        sender: node_id,
                                        timestamp: now_millis(),
                                        payload: Payload::RpcServerFailed(RpcServerFailed {
                                            reason: e.to_string(),
                                        }),
                                    };
                                    let _ = transport.send_to(&peer_id, &msg).await;
                                }
                            }
                        });
                    }
                    Payload::RpcServerReady(ready) => {
                        tracing::info!(
                            "Peer {} has RPC server ready on port {}",
                            peer_id,
                            ready.port
                        );
                        // QUIC tunnel + engine RPC configuration will be wired
                        // when split-inference runtime lands (Phase 4).
                    }
                    Payload::RpcServerFailed(failed) => {
                        tracing::warn!(
                            "Peer {} failed to start RPC server: {}",
                            peer_id,
                            failed.reason
                        );
                    }
                    Payload::TradeAccept(_) | Payload::TradeProposal(_) => {
                        // Handled within handle_inference tasks via wait_for_trade_accept
                        tracing::debug!("Trade message in main loop from {} (handled by task)", peer_id);
                    }
                    Payload::TradeGossip(trade_gossip) => {
                        let gossip = gossip.clone();
                        let ledger = ledger.clone();
                        let ledger_path = ledger_path.clone();
                        tokio::spawn(async move {
                            if let Some(signed) =
                                forge_net::gossip::handle_trade_gossip(&gossip, &trade_gossip).await
                            {
                                let mut ledger = ledger.lock().await;
                                ledger.execute_trade(&signed.trade);
                                if let Some(path) = ledger_path.as_ref() {
                                    let _ = ledger.save_to_path(path);
                                }
                                tracing::info!(
                                    "Gossip trade recorded: {} CU ({} → {})",
                                    signed.trade.cu_amount,
                                    signed.trade.provider.to_hex(),
                                    signed.trade.consumer.to_hex()
                                );
                            }
                        });
                    }
                    _ => {}
                },
                None => {
                    tracing::info!("Transport closed");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Worker: send an inference request to a seed and collect streamed text.
    /// After receiving the response, handles dual-sign trade protocol.
    pub async fn request_inference(
        transport: &ForgeTransport,
        seed_peer_id: &str,
        node_id: &NodeId,
        prompt: &str,
        max_tokens: u32,
        temperature: f32,
    ) -> anyhow::Result<String> {
        let request_id: u64 = rand::random();

        let envelope = Envelope {
            msg_id: request_id,
            sender: node_id.clone(),
            timestamp: now_millis(),
            payload: Payload::InferenceRequest(InferenceRequest {
                request_id,
                prompt_text: prompt.to_string(),
                max_tokens,
                temperature,
                top_p: 0.9,
            }),
        };
        envelope.validate_for_peer(node_id)?;

        transport.send_to(seed_peer_id, &envelope).await?;
        tracing::debug!("Sent inference request {} to {}", request_id, seed_peer_id);

        // Collect streamed tokens
        let mut result = String::new();
        loop {
            match transport.recv().await {
                Some((peer_id, response)) => match response.payload {
                    Payload::TokenStream(ts) => {
                        if ts.request_id == request_id {
                            result.push_str(&ts.text);
                            if ts.is_final {
                                break;
                            }
                        }
                    }
                    Payload::Error(err) => {
                        if err.request_id == request_id {
                            anyhow::bail!("{:?}: {}", err.code, err.message);
                        }
                    }
                    Payload::TradeProposal(proposal) => {
                        if proposal.request_id == request_id {
                            // Counter-sign the trade
                            let trade = TradeRecord {
                                provider: proposal.provider,
                                consumer: proposal.consumer,
                                cu_amount: proposal.cu_amount,
                                tokens_processed: proposal.tokens_processed,
                                timestamp: proposal.timestamp,
                                model_id: proposal.model_id,
                            };
                            let canonical = trade.canonical_bytes();
                            let consumer_sig = transport.sign(&canonical).to_vec();

                            let accept = Envelope {
                                msg_id: request_id * 10000 + 10000,
                                sender: node_id.clone(),
                                timestamp: now_millis(),
                                payload: Payload::TradeAccept(TradeAccept {
                                    request_id,
                                    consumer_sig,
                                }),
                            };
                            if let Err(e) = transport.send_to(&peer_id, &accept).await {
                                tracing::warn!("Failed to send TradeAccept: {}", e);
                            } else {
                                tracing::debug!(
                                    "Trade accepted: {} CU for request {}",
                                    trade.cu_amount,
                                    request_id
                                );
                            }
                        }
                    }
                    _ => {}
                },
                None => break,
            }
        }

        Ok(result)
    }
}

async fn current_pipeline_topology(
    model_manifest: &Arc<Mutex<Option<ModelManifest>>>,
    advertised_topology: &Arc<Mutex<Option<PipelineTopology>>>,
    cluster: &Arc<ClusterManager>,
) -> Option<PipelineTopology> {
    let model = model_manifest.lock().await.clone();
    let connected_peers = cluster
        .discovery()
        .peers_by_capability()
        .await
        .into_iter()
        .filter_map(|peer| peer.capability)
        .collect();

    let snapshot = crate::topology::build_topology_snapshot(
        model,
        Some(cluster.local_capability().clone()),
        connected_peers,
    )
    .ok()?;

    let topology = snapshot.planned_topology;
    *advertised_topology.lock().await = topology.clone();
    topology
}

/// Handle a single inference request from a worker.
async fn handle_inference(
    config: &Config,
    engine: Arc<Mutex<forge_infer::CandleEngine>>,
    ledger: Arc<Mutex<ComputeLedger>>,
    ledger_path: Option<std::path::PathBuf>,
    transport: Arc<ForgeTransport>,
    node_id: NodeId,
    consumer_id: NodeId,
    peer_id: &str,
    req: InferenceRequest,
    gossip: Arc<Mutex<GossipState>>,
) -> anyhow::Result<()> {
    use forge_infer::InferenceEngine;

    if let Err(err) = config.validate_inference_request(
        &req.prompt_text,
        req.max_tokens,
        req.temperature,
        Some(req.top_p),
    ) {
        send_protocol_error(
            &transport,
            peer_id,
            &node_id,
            req.request_id,
            ErrorCode::InvalidRequest,
            err.to_string(),
            false,
        )
        .await?;
        return Ok(());
    }

    // Reserve CU for this inference (prevents double-spending)
    let estimated_cost = {
        let mut ledger = ledger.lock().await;
        let cost = ledger.estimate_cost(req.max_tokens as u64, 32, 32);
        if !ledger.reserve_cu(&consumer_id, cost) {
            tracing::warn!("Consumer {} cannot afford {} CU", peer_id, cost);
            send_protocol_error(
                &transport,
                peer_id,
                &node_id,
                req.request_id,
                ErrorCode::InsufficientBalance,
                format!("insufficient CU balance for estimated cost {cost}"),
                true,
            )
            .await?;
            return Ok(());
        }
        cost
    };

    // Run inference
    let tokens = match {
        let mut engine = engine.lock().await;
        engine.generate(
            &req.prompt_text,
            req.max_tokens,
            req.temperature,
            Some(req.top_p as f64),
        )
    } {
        Ok(tokens) => tokens,
        Err(err) => {
            // Release reservation on failure
            ledger.lock().await.release_reserve(&consumer_id, estimated_cost);
            send_protocol_error(
                &transport,
                peer_id,
                &node_id,
                req.request_id,
                ErrorCode::Internal,
                err.to_string(),
                true,
            )
            .await?;
            return Ok(());
        }
    };

    let total_tokens = tokens.len() as u64;

    if tokens.is_empty() {
        let msg = Envelope {
            msg_id: req.request_id,
            sender: node_id,
            timestamp: now_millis(),
            payload: Payload::TokenStream(TokenStreamMsg {
                request_id: req.request_id,
                text: String::new(),
                is_final: true,
            }),
        };
        transport.send_to(peer_id, &msg).await?;
        return Ok(());
    }

    // Stream tokens back to worker
    for (i, text) in tokens.iter().enumerate() {
        let is_final = i == tokens.len() - 1;
        let msg = Envelope {
            msg_id: req.request_id * 10000 + i as u64,
            sender: node_id.clone(),
            timestamp: now_millis(),
            payload: Payload::TokenStream(TokenStreamMsg {
                request_id: req.request_id,
                text: text.clone(),
                is_final,
            }),
        };
        if let Err(e) = transport.send_to(peer_id, &msg).await {
            tracing::warn!("Failed to send token to {}: {}", peer_id, e);
            break;
        }
    }

    // Dual-sign trade: provider proposes, consumer counter-signs
    // Release excess reservation (reserved max_tokens, actual may be less)
    let cu_amount = {
        let mut ledger = ledger.lock().await;
        let actual_cost = ledger.estimate_cost(total_tokens, 32, 32);
        if estimated_cost > actual_cost {
            ledger.release_reserve(&consumer_id, estimated_cost - actual_cost);
        }
        actual_cost
    };
    let trade = TradeRecord {
        provider: node_id.clone(),
        consumer: consumer_id.clone(),
        cu_amount,
        tokens_processed: total_tokens,
        timestamp: now_millis(),
        model_id: "active".to_string(),
    };

    let canonical = trade.canonical_bytes();
    let provider_sig = transport.sign(&canonical).to_vec();

    // Send TradeProposal to consumer
    let proposal_msg = Envelope {
        msg_id: req.request_id * 10000 + 9999,
        sender: node_id.clone(),
        timestamp: now_millis(),
        payload: Payload::TradeProposal(TradeProposal {
            request_id: req.request_id,
            provider: node_id.clone(),
            consumer: consumer_id.clone(),
            cu_amount,
            tokens_processed: total_tokens,
            timestamp: trade.timestamp,
            model_id: trade.model_id.clone(),
            provider_sig: provider_sig.clone(),
        }),
    };
    transport.send_to(peer_id, &proposal_msg).await?;

    // Wait for TradeAccept with timeout (5 seconds)
    let accept_result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        wait_for_trade_accept(&transport, req.request_id),
    )
    .await;

    match accept_result {
        Ok(Some(consumer_sig)) => {
            // Record dual-signed trade
            let signed = SignedTradeRecord {
                trade: trade.clone(),
                provider_sig,
                consumer_sig,
            };
            match signed.verify() {
                Ok(()) => {
                    let mut ledger = ledger.lock().await;
                    ledger.execute_trade(&signed.trade);
                    if let Some(path) = ledger_path.as_ref() {
                        ledger.save_to_path(path)?;
                    }
                    tracing::info!(
                        "Signed trade recorded: {} CU for {} tokens to {}",
                        trade.cu_amount,
                        total_tokens,
                        peer_id
                    );
                    // Reputation boost for successful signed trade
                    ledger.update_reputation(&trade.provider, 0.01);
                    drop(ledger);
                    // Broadcast to mesh via gossip
                    forge_net::gossip::broadcast_trade(&transport, &gossip, &signed).await;
                }
                Err(e) => {
                    tracing::warn!("Trade signature verification failed: {}", e);
                    // 50% penalty on unsigned trades (Issue #3)
                    let mut penalized = trade.clone();
                    penalized.cu_amount /= 2;
                    let mut ledger = ledger.lock().await;
                    ledger.execute_trade(&penalized);
                    if let Some(path) = ledger_path.as_ref() {
                        ledger.save_to_path(path)?;
                    }
                }
            }
        }
        _ => {
            // Timeout: 50% penalty on unsigned trades (Issue #3)
            tracing::debug!("TradeAccept timeout from {}, recording penalized trade", peer_id);
            let mut penalized = trade.clone();
            penalized.cu_amount /= 2;
            let mut ledger = ledger.lock().await;
            ledger.execute_trade(&penalized);
            if let Some(path) = ledger_path.as_ref() {
                ledger.save_to_path(path)?;
            }
        }
    }

    Ok(())
}

/// Wait for a TradeAccept message matching the given request_id.
async fn wait_for_trade_accept(
    transport: &ForgeTransport,
    request_id: u64,
) -> Option<Vec<u8>> {
    loop {
        match transport.recv().await {
            Some((_peer_id, envelope)) => {
                if let Payload::TradeAccept(accept) = envelope.payload {
                    if accept.request_id == request_id {
                        return Some(accept.consumer_sig);
                    }
                }
                // Ignore other messages while waiting
            }
            None => return None,
        }
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

async fn send_protocol_error(
    transport: &ForgeTransport,
    peer_id: &str,
    sender: &NodeId,
    request_id: u64,
    code: ErrorCode,
    message: String,
    retryable: bool,
) -> anyhow::Result<()> {
    let msg = Envelope {
        msg_id: request_id,
        sender: sender.clone(),
        timestamp: now_millis(),
        payload: Payload::Error(ErrorMsg {
            request_id,
            code,
            message,
            retryable,
        }),
    };
    transport.send_to(peer_id, &msg).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_infer::CandleEngine;
    use forge_ledger::ComputeLedger;
    use forge_net::ForgeTransport;
    use tokio::time::{Duration, timeout};

    #[tokio::test]
    async fn worker_request_inference_surfaces_typed_error() {
        let transport_worker = ForgeTransport::new().await.expect("worker");
        let transport_seed = ForgeTransport::new().await.expect("seed");
        let _accept_seed = transport_seed.start_accepting();

        let addr_seed = transport_seed.endpoint_addr();
        let peer_seed = transport_worker
            .connect(addr_seed)
            .await
            .expect("connect to seed");
        tokio::time::sleep(Duration::from_millis(200)).await;

        let worker_peer_id = transport_worker.forge_node_id().to_hex();
        let seed_peer_id = peer_seed.peer_id().to_string();
        let worker_node_id = transport_worker.forge_node_id();
        let request_transport = transport_worker;

        let worker_task = tokio::spawn(async move {
            PipelineCoordinator::request_inference(
                &request_transport,
                &seed_peer_id,
                &worker_node_id,
                "hello",
                32,
                0.7,
            )
            .await
        });

        let (_peer_id, received) = timeout(Duration::from_secs(5), transport_seed.recv())
            .await
            .expect("timeout")
            .expect("receive request");

        let request_id = match received.payload {
            Payload::InferenceRequest(req) => req.request_id,
            other => panic!(
                "Expected InferenceRequest, got {:?}",
                std::mem::discriminant(&other)
            ),
        };

        let msg = Envelope {
            msg_id: request_id,
            sender: transport_seed.forge_node_id(),
            timestamp: 0,
            payload: Payload::Error(ErrorMsg {
                request_id,
                code: ErrorCode::InsufficientBalance,
                message: "insufficient CU balance".to_string(),
                retryable: true,
            }),
        };

        transport_seed
            .send_to(&worker_peer_id, &msg)
            .await
            .expect("send error");

        let result = worker_task.await.expect("worker task");
        let err = result.expect_err("typed error should surface as Err");
        assert!(
            err.to_string().contains("InsufficientBalance"),
            "unexpected error: {err}"
        );

        transport_seed.close().await;
    }

    #[tokio::test]
    async fn seed_rejects_requests_when_at_concurrency_limit() {
        let transport_worker = ForgeTransport::new().await.expect("worker");
        let transport_seed = Arc::new(ForgeTransport::new().await.expect("seed"));
        let _accept_seed = transport_seed.start_accepting();

        let coordinator = PipelineCoordinator::new(transport_seed.clone());
        let seed_task = tokio::spawn({
            let transport_seed = transport_seed.clone();
            async move {
                coordinator
                    .run_seed(
                        Arc::new(Mutex::new(CandleEngine::new())),
                        Arc::new(Mutex::new(ComputeLedger::new())),
                        Arc::new(Mutex::new(None)),
                        Arc::new(Mutex::new(None)),
                        None,
                        Config {
                            max_concurrent_remote_inference_requests: 0,
                            ..Config::default()
                        },
                        None,
                    )
                    .await
                    .expect("seed loop");
                transport_seed.close().await;
            }
        });

        let addr_seed = transport_seed.endpoint_addr();
        let peer_seed = transport_worker
            .connect(addr_seed)
            .await
            .expect("connect to seed");
        tokio::time::sleep(Duration::from_millis(200)).await;

        let result = PipelineCoordinator::request_inference(
            &transport_worker,
            peer_seed.peer_id(),
            &transport_worker.forge_node_id(),
            "hello",
            32,
            0.7,
        )
        .await;

        let err = result.expect_err("busy seed should reject request");
        assert!(err.to_string().contains("Busy"), "unexpected error: {err}");

        transport_worker.close().await;
        transport_seed.close().await;
        seed_task.await.expect("seed task");
    }
}
