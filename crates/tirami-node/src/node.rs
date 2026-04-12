use crate::pipeline::PipelineCoordinator;
use crate::state_persist;
use crate::topology::{TopologySnapshot, build_local_capability, build_topology_snapshot};
use tirami_agora::Marketplace;
use tirami_core::Config;
use tirami_core::{ModelManifest, PipelineTopology};
use tirami_infer::{CandleEngine, InferenceEngine, parse_gguf_metadata};
use tirami_ledger::{ComputeLedger, SettlementStatement};
use tirami_net::{ClusterManager, ForgeTransport, GossipState};
use std::sync::Arc;
use tokio::sync::Mutex;

/// The main Forge node — protocol daemon.
pub struct TiramiNode {
    pub config: Config,
    pub engine: Arc<Mutex<CandleEngine>>,
    pub ledger: Arc<Mutex<ComputeLedger>>,
    pub model_manifest: Arc<Mutex<Option<ModelManifest>>>,
    pub advertised_topology: Arc<Mutex<Option<PipelineTopology>>>,
    transport: Option<Arc<ForgeTransport>>,
    cluster: Option<Arc<ClusterManager>>,
    /// Shared gossip state — used by both the HTTP API (to broadcast loans
    /// and trades from endpoint handlers) and the pipeline coordinator (to
    /// broadcast trades completed during inference). Must be a single
    /// instance so dedup across both paths is coherent.
    gossip: Arc<Mutex<GossipState>>,
    /// forge-bank L2 services (persisted via bank_state_path).
    pub bank: Arc<Mutex<crate::bank_adapter::BankServices>>,
    /// forge-agora L4 marketplace (persisted via marketplace_state_path).
    pub marketplace: Arc<Mutex<Marketplace>>,
    /// forge-mind L3 agent (persisted via mind_state_path; None until init).
    pub mind_agent: Arc<Mutex<Option<tirami_mind::TiramiMindAgent>>>,
}

impl TiramiNode {
    pub fn new(config: Config) -> Self {
        let ledger = match config.ledger_path.as_ref() {
            Some(path) if path.exists() => match ComputeLedger::load_from_path(path) {
                Ok(ledger) => {
                    tracing::info!("Loaded ledger snapshot from {}", path.display());
                    ledger
                }
                Err(err) => {
                    tracing::warn!(
                        "Failed to load ledger snapshot from {}: {}",
                        path.display(),
                        err
                    );
                    ComputeLedger::new()
                }
            },
            _ => ComputeLedger::new(),
        };

        // Load forge-bank L2 state if a path is configured.
        let bank = match config.bank_state_path.as_ref() {
            Some(path) => match state_persist::load_bank(path) {
                Ok(Some(services)) => {
                    tracing::info!("Loaded bank state from {}", path.display());
                    services
                }
                Ok(None) => {
                    tracing::debug!("No bank state file at {} — starting fresh", path.display());
                    crate::bank_adapter::BankServices::new_default()
                }
                Err(err) => {
                    tracing::warn!("Failed to load bank state from {}: {}", path.display(), err);
                    crate::bank_adapter::BankServices::new_default()
                }
            },
            None => crate::bank_adapter::BankServices::new_default(),
        };

        // Load forge-agora L4 marketplace state if a path is configured.
        let marketplace = match config.marketplace_state_path.as_ref() {
            Some(path) => match state_persist::load_marketplace(path) {
                Ok(Some(mp)) => {
                    tracing::info!("Loaded marketplace state from {}", path.display());
                    mp
                }
                Ok(None) => {
                    tracing::debug!(
                        "No marketplace state file at {} — starting fresh",
                        path.display()
                    );
                    Marketplace::new()
                }
                Err(err) => {
                    tracing::warn!(
                        "Failed to load marketplace state from {}: {}",
                        path.display(),
                        err
                    );
                    Marketplace::new()
                }
            },
            None => Marketplace::new(),
        };

        // forge-mind L3 agent is always None at startup.
        // If mind_state_path is set, the saved snapshot will be merged in
        // when the client calls POST /v1/tirami/mind/init (the handler checks
        // for a snapshot file and calls agent.restore_from_snapshot()).
        let mind_agent: Option<tirami_mind::TiramiMindAgent> = None;

        Self {
            config,
            engine: Arc::new(Mutex::new(CandleEngine::new())),
            ledger: Arc::new(Mutex::new(ledger)),
            model_manifest: Arc::new(Mutex::new(None)),
            advertised_topology: Arc::new(Mutex::new(None)),
            transport: None,
            cluster: None,
            gossip: Arc::new(Mutex::new(GossipState::new())),
            bank: Arc::new(Mutex::new(bank)),
            marketplace: Arc::new(Mutex::new(marketplace)),
            mind_agent: Arc::new(Mutex::new(mind_agent)),
        }
    }

    /// Load a model from disk.
    pub async fn load_model(
        &self,
        model_path: &std::path::Path,
        tokenizer_path: &std::path::Path,
    ) -> Result<(), tirami_core::TiramiError> {
        tracing::info!("Loading model: {:?}", model_path);
        let manifest = parse_gguf_metadata(model_path)?;
        let mut engine = self.engine.lock().await;
        engine.load(model_path, tokenizer_path, None)?;
        drop(engine);
        *self.model_manifest.lock().await = Some(manifest);
        tracing::info!("Model loaded successfully");
        Ok(())
    }

    /// Generate a response locally.
    pub async fn chat(
        &self,
        prompt: &str,
        max_tokens: u32,
        temperature: f32,
    ) -> Result<String, tirami_core::TiramiError> {
        self.config
            .validate_inference_request(prompt, max_tokens, temperature, None)?;
        let mut engine = self.engine.lock().await;
        let tokens = engine.generate(prompt, max_tokens, temperature, None, None)?;
        Ok(tokens.join(""))
    }

    /// Start the HTTP API server.
    pub async fn serve_api(&self) -> Result<(), tirami_core::TiramiError> {
        let app = crate::api::create_router_with_services(
            self.config.clone(),
            self.engine.clone(),
            self.ledger.clone(),
            self.model_manifest.clone(),
            self.advertised_topology.clone(),
            self.cluster.clone(),
            self.gossip.clone(),
            self.bank.clone(),
            self.marketplace.clone(),
            Arc::new(Mutex::new(0usize)),
            self.mind_agent.clone(),
        );
        let addr = self.config.api_socket_addr();
        tracing::info!("API server listening on {}", addr);

        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| tirami_core::TiramiError::NetworkError(format!("bind: {e}")))?;
        axum::serve(listener, app)
            .await
            .map_err(|e| tirami_core::TiramiError::NetworkError(format!("serve: {e}")))?;
        Ok(())
    }

    /// Initialize P2P transport.
    pub async fn init_transport(&mut self) -> Result<Arc<ForgeTransport>, tirami_core::TiramiError> {
        if let Some(transport) = self.transport.as_ref() {
            return Ok(transport.clone());
        }

        let transport = ForgeTransport::new()
            .await
            .map_err(|e| tirami_core::TiramiError::NetworkError(format!("transport: {e}")))?;
        let transport = Arc::new(transport);
        let local_capability = build_local_capability(&self.config, transport.tirami_node_id());
        self.cluster = Some(Arc::new(ClusterManager::new(
            transport.clone(),
            local_capability,
        )));
        self.transport = Some(transport.clone());
        Ok(transport)
    }

    /// Run as a Seed node — holds model, serves inference, earns CU.
    pub async fn run_seed(&mut self) -> Result<(), tirami_core::TiramiError> {
        let transport = self.init_transport().await?;

        let addr = transport.endpoint_addr();
        let id = transport.endpoint_id();
        tracing::info!("=== FORGE SEED NODE ===");
        tracing::info!("Public key: {}", id);
        tracing::info!("Node ID: {}", transport.tirami_node_id());
        tracing::info!("Full address: {:?}", addr);
        tracing::info!("Workers connect with: forge worker --seed {}", id);

        // Accept connections
        let _accept_handle = transport.start_accepting();

        // API server in background
        let engine_api = self.engine.clone();
        let ledger_api = self.ledger.clone();
        let manifest_api = self.model_manifest.clone();
        let topology_api = self.advertised_topology.clone();
        let cluster_api = self.cluster.clone();
        let gossip_api = self.gossip.clone();
        let bank_api = self.bank.clone();
        let marketplace_api = self.marketplace.clone();
        let mind_agent_api = self.mind_agent.clone();
        let api_config = self.config.clone();
        tokio::spawn(async move {
            let app = crate::api::create_router_with_services(
                api_config.clone(),
                engine_api,
                ledger_api,
                manifest_api,
                topology_api,
                cluster_api,
                gossip_api,
                bank_api,
                marketplace_api,
                Arc::new(Mutex::new(0usize)),
                mind_agent_api,
            );
            let addr = api_config.api_socket_addr();
            if let Ok(listener) = tokio::net::TcpListener::bind(&addr).await {
                tracing::info!("HTTP API at http://{}", addr);
                let _ = axum::serve(listener, app).await;
            }
        });

        // Run pipeline coordinator with ledger
        let coordinator = PipelineCoordinator::new(transport);
        coordinator
            .run_seed(
                self.engine.clone(),
                self.ledger.clone(),
                self.model_manifest.clone(),
                self.advertised_topology.clone(),
                self.cluster.clone(),
                self.config.clone(),
                self.config.ledger_path.clone(),
                self.gossip.clone(),
            )
            .await
            .map_err(|e| tirami_core::TiramiError::NetworkError(format!("seed: {e}")))?;

        Ok(())
    }

    /// Connect to a seed node as a worker.
    pub async fn connect_to_seed(
        &mut self,
        seed_addr: iroh::EndpointAddr,
    ) -> Result<Arc<ForgeTransport>, tirami_core::TiramiError> {
        let transport = self.init_transport().await?;
        let peer = transport
            .connect(seed_addr)
            .await
            .map_err(|e| tirami_core::TiramiError::NetworkError(format!("connect: {e}")))?;

        if let Some(cluster) = self.cluster.as_ref() {
            cluster
                .handshake(&peer)
                .await
                .map_err(|e| tirami_core::TiramiError::NetworkError(format!("handshake: {e}")))?;
            cluster.start_heartbeat();

            let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
            loop {
                let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
                if timeout.is_zero() {
                    break;
                }

                match tokio::time::timeout(timeout, transport.recv()).await {
                    Ok(Some((peer_id, envelope))) => match envelope.payload.clone() {
                        tirami_proto::Payload::Welcome(_) | tirami_proto::Payload::Heartbeat(_) => {
                            cluster.handle_message(&peer_id, envelope).await;
                        }
                        tirami_proto::Payload::PipelineTopology(topology) => {
                            tracing::info!(
                                "Received topology from {} with {} stages",
                                peer_id,
                                topology.stages.len()
                            );
                            *self.advertised_topology.lock().await = Some(PipelineTopology {
                                model_id: topology.model_id,
                                stages: topology.stages,
                            });
                        }
                        _ => {}
                    },
                    Ok(None) | Err(_) => break,
                }
            }
        }

        tracing::info!("Connected to seed: {}", peer.peer_id());
        Ok(transport)
    }

    /// Get network statistics from the ledger.
    pub async fn network_stats(&self) -> tirami_ledger::NetworkStats {
        self.ledger.lock().await.network_stats()
    }

    /// Persist the current ledger snapshot if a path is configured.
    pub async fn persist_ledger(&self) -> Result<(), tirami_core::TiramiError> {
        if let Some(path) = self.config.ledger_path.as_ref() {
            self.ledger.lock().await.save_to_path(path)?;
        }
        Ok(())
    }

    /// Export a settlement statement for the given time window.
    pub async fn settlement_statement(
        &self,
        window_start: u64,
        window_end: u64,
        reference_price_per_cu: Option<f64>,
    ) -> SettlementStatement {
        self.ledger.lock().await.export_settlement_statement(
            window_start,
            window_end,
            reference_price_per_cu,
        )
    }

    /// Build a topology snapshot from the current model manifest and connected peers.
    pub async fn topology_snapshot(&self) -> Result<TopologySnapshot, tirami_core::TiramiError> {
        let model = self.model_manifest.lock().await.clone();
        let local_capability = self
            .cluster
            .as_ref()
            .map(|cluster| cluster.local_capability().clone());

        let connected_peers = match self.cluster.as_ref() {
            Some(cluster) => cluster
                .discovery()
                .peers_by_capability()
                .await
                .into_iter()
                .filter_map(|peer| peer.capability)
                .collect(),
            None => Vec::new(),
        };

        build_topology_snapshot(model, local_capability, connected_peers)
    }

    /// Persist L2/L3/L4 state to disk if paths are configured in the config.
    ///
    /// Errors are logged as warnings but do not propagate — callers should
    /// treat partial save failures as non-fatal.
    pub async fn save_state(&self) {
        if let Some(path) = self.config.bank_state_path.as_ref() {
            let bank = self.bank.lock().await;
            if let Err(e) = state_persist::save_bank(&*bank, path) {
                tracing::warn!("Failed to persist bank state to {}: {}", path.display(), e);
            } else {
                tracing::info!("Bank state persisted to {}", path.display());
            }
        }

        if let Some(path) = self.config.marketplace_state_path.as_ref() {
            let mp = self.marketplace.lock().await;
            if let Err(e) = state_persist::save_marketplace(&*mp, path) {
                tracing::warn!(
                    "Failed to persist marketplace state to {}: {}",
                    path.display(),
                    e
                );
            } else {
                tracing::info!("Marketplace state persisted to {}", path.display());
            }
        }

        if let Some(path) = self.config.mind_state_path.as_ref() {
            let mind = self.mind_agent.lock().await;
            if let Some(agent) = mind.as_ref() {
                if let Err(e) = state_persist::save_mind(agent, path) {
                    tracing::warn!(
                        "Failed to persist mind state to {}: {}",
                        path.display(),
                        e
                    );
                } else {
                    tracing::info!("Mind agent state persisted to {}", path.display());
                }
            }
        }
    }

    /// Graceful shutdown: announce leaving, persist ledger, close transport.
    pub async fn shutdown(&self) {
        tracing::info!("Shutting down Forge node...");

        // Announce leaving to all peers
        if let Some(cluster) = self.cluster.as_ref() {
            cluster
                .announce_leaving(tirami_proto::LeaveReason::Shutdown)
                .await;
        }

        // Persist ledger
        if let Err(e) = self.persist_ledger().await {
            tracing::warn!("Failed to persist ledger on shutdown: {}", e);
        } else {
            tracing::info!("Ledger persisted");
        }

        // Persist L2/L3/L4 state
        self.save_state().await;

        // Close transport
        if let Some(transport) = self.transport.as_ref() {
            transport.close().await;
        }

        tracing::info!("Forge node shut down");
    }
}
