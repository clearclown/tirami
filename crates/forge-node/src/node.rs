use crate::pipeline::PipelineCoordinator;
use crate::topology::{TopologySnapshot, build_local_capability, build_topology_snapshot};
use forge_core::Config;
use forge_core::{ModelManifest, PipelineTopology};
use forge_infer::{CandleEngine, InferenceEngine, parse_gguf_metadata};
use forge_ledger::{ComputeLedger, SettlementStatement};
use forge_net::{ClusterManager, ForgeTransport};
use std::sync::Arc;
use tokio::sync::Mutex;

/// The main Forge node — protocol daemon.
pub struct ForgeNode {
    pub config: Config,
    pub engine: Arc<Mutex<CandleEngine>>,
    pub ledger: Arc<Mutex<ComputeLedger>>,
    pub model_manifest: Arc<Mutex<Option<ModelManifest>>>,
    pub advertised_topology: Arc<Mutex<Option<PipelineTopology>>>,
    transport: Option<Arc<ForgeTransport>>,
    cluster: Option<Arc<ClusterManager>>,
}

impl ForgeNode {
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

        Self {
            config,
            engine: Arc::new(Mutex::new(CandleEngine::new())),
            ledger: Arc::new(Mutex::new(ledger)),
            model_manifest: Arc::new(Mutex::new(None)),
            advertised_topology: Arc::new(Mutex::new(None)),
            transport: None,
            cluster: None,
        }
    }

    /// Load a model from disk.
    pub async fn load_model(
        &self,
        model_path: &std::path::Path,
        tokenizer_path: &std::path::Path,
    ) -> Result<(), forge_core::ForgeError> {
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
    ) -> Result<String, forge_core::ForgeError> {
        let mut engine = self.engine.lock().await;
        let tokens = engine.generate(prompt, max_tokens, temperature, None)?;
        Ok(tokens.join(""))
    }

    /// Start the HTTP API server.
    pub async fn serve_api(&self) -> Result<(), forge_core::ForgeError> {
        let app = crate::api::create_router(
            self.engine.clone(),
            self.ledger.clone(),
            self.model_manifest.clone(),
            self.advertised_topology.clone(),
            self.cluster.clone(),
        );
        let addr = format!("0.0.0.0:{}", self.config.api_port);
        tracing::info!("API server listening on {}", addr);

        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| forge_core::ForgeError::NetworkError(format!("bind: {e}")))?;
        axum::serve(listener, app)
            .await
            .map_err(|e| forge_core::ForgeError::NetworkError(format!("serve: {e}")))?;
        Ok(())
    }

    /// Initialize P2P transport.
    pub async fn init_transport(&mut self) -> Result<Arc<ForgeTransport>, forge_core::ForgeError> {
        if let Some(transport) = self.transport.as_ref() {
            return Ok(transport.clone());
        }

        let transport = ForgeTransport::new()
            .await
            .map_err(|e| forge_core::ForgeError::NetworkError(format!("transport: {e}")))?;
        let transport = Arc::new(transport);
        let local_capability =
            build_local_capability(&self.config, transport.forge_node_id());
        self.cluster = Some(Arc::new(ClusterManager::new(
            transport.clone(),
            local_capability,
        )));
        self.transport = Some(transport.clone());
        Ok(transport)
    }

    /// Run as a Seed node — holds model, serves inference, earns CU.
    pub async fn run_seed(&mut self) -> Result<(), forge_core::ForgeError> {
        let transport = self.init_transport().await?;

        let addr = transport.endpoint_addr();
        let id = transport.endpoint_id();
        tracing::info!("=== FORGE SEED NODE ===");
        tracing::info!("Public key: {}", id);
        tracing::info!("Node ID: {}", transport.forge_node_id());
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
        let api_port = self.config.api_port;
        tokio::spawn(async move {
            let app = crate::api::create_router(
                engine_api,
                ledger_api,
                manifest_api,
                topology_api,
                cluster_api,
            );
            let addr = format!("0.0.0.0:{}", api_port);
            if let Ok(listener) = tokio::net::TcpListener::bind(&addr).await {
                tracing::info!("HTTP API at http://localhost:{}", api_port);
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
                self.config.ledger_path.clone(),
            )
            .await
            .map_err(|e| forge_core::ForgeError::NetworkError(format!("seed: {e}")))?;

        Ok(())
    }

    /// Connect to a seed node as a worker.
    pub async fn connect_to_seed(
        &mut self,
        seed_addr: iroh::EndpointAddr,
    ) -> Result<Arc<ForgeTransport>, forge_core::ForgeError> {
        let transport = self.init_transport().await?;
        let peer = transport
            .connect(seed_addr)
            .await
            .map_err(|e| forge_core::ForgeError::NetworkError(format!("connect: {e}")))?;

        if let Some(cluster) = self.cluster.as_ref() {
            cluster
                .handshake(&peer)
                .await
                .map_err(|e| forge_core::ForgeError::NetworkError(format!("handshake: {e}")))?;
            cluster.start_heartbeat();

            let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
            loop {
                let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
                if timeout.is_zero() {
                    break;
                }

                match tokio::time::timeout(timeout, transport.recv()).await {
                    Ok(Some((peer_id, envelope))) => match envelope.payload.clone() {
                        forge_proto::Payload::Welcome(_) | forge_proto::Payload::Heartbeat(_) => {
                            cluster.handle_message(&peer_id, envelope).await;
                        }
                        forge_proto::Payload::PipelineTopology(topology) => {
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
    pub async fn network_stats(&self) -> forge_ledger::NetworkStats {
        self.ledger.lock().await.network_stats()
    }

    /// Persist the current ledger snapshot if a path is configured.
    pub async fn persist_ledger(&self) -> Result<(), forge_core::ForgeError> {
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
    pub async fn topology_snapshot(&self) -> Result<TopologySnapshot, forge_core::ForgeError> {
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
}
