use crate::agent_loop::{AgentLoopStats, AgentTickInput, spawn_agent_tick_loop};
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
    /// Phase 13 — staking pool for TRM lock-up.
    pub staking_pool: Arc<Mutex<tirami_ledger::StakingPool>>,
    /// Phase 13 — referral tracker for sponsor bonuses.
    pub referral_tracker: Arc<Mutex<tirami_ledger::ReferralTracker>>,
    /// Phase 13 — governance state for stake-weighted voting.
    pub governance: Arc<Mutex<tirami_ledger::GovernanceState>>,
    /// Phase 16 — on-chain anchor client. Defaults to MockChainClient; real
    /// Base L2 client swaps in via future `with_chain_client` builder method.
    pub chain_client: Arc<tirami_anchor::MockChainClient>,
    /// Phase 18.5 — the user's personal Tirami agent. `None` until the
    /// operator configures one (future `POST /v1/tirami/agent/init`). The
    /// HTTP API's `AppState.personal_agent` shares this Arc so the tick
    /// loop and the status endpoint see the same state.
    pub personal_agent: Arc<Mutex<Option<tirami_mind::PersonalAgent>>>,
    /// Phase 18.5 — observability counters for the agent tick loop.
    /// Exposed via `/v1/tirami/agent/status` as `loop.{ticks, last_action,
    /// last_tick_ms}`.
    pub agent_loop_stats: Arc<Mutex<AgentLoopStats>>,
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
            staking_pool: Arc::new(Mutex::new(tirami_ledger::StakingPool::new())),
            referral_tracker: Arc::new(Mutex::new(tirami_ledger::ReferralTracker::new())),
            governance: Arc::new(Mutex::new(tirami_ledger::GovernanceState::new(0))),
            chain_client: Arc::new(tirami_anchor::MockChainClient::new()),
            personal_agent: Arc::new(Mutex::new(None)),
            agent_loop_stats: Arc::new(Mutex::new(AgentLoopStats::new())),
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
            self.staking_pool.clone(),
            self.referral_tracker.clone(),
            self.governance.clone(),
            self.chain_client.clone(),
            self.personal_agent.clone(),
            self.agent_loop_stats.clone(),
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

    /// Spawn the HTTP API server as a background tokio task.
    ///
    /// Returns a `JoinHandle` so the caller can optionally await it, but
    /// typically it runs until the process exits.
    pub fn spawn_api(&self) -> tokio::task::JoinHandle<()> {
        let engine_api = self.engine.clone();
        let ledger_api = self.ledger.clone();
        let manifest_api = self.model_manifest.clone();
        let topology_api = self.advertised_topology.clone();
        let cluster_api = self.cluster.clone();
        let gossip_api = self.gossip.clone();
        let bank_api = self.bank.clone();
        let marketplace_api = self.marketplace.clone();
        let mind_agent_api = self.mind_agent.clone();
        let staking_pool_api = self.staking_pool.clone();
        let referral_tracker_api = self.referral_tracker.clone();
        let governance_api = self.governance.clone();
        let chain_client_api = self.chain_client.clone();
        let personal_agent_api = self.personal_agent.clone();
        let agent_loop_stats_api = self.agent_loop_stats.clone();
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
                staking_pool_api,
                referral_tracker_api,
                governance_api,
                chain_client_api,
                personal_agent_api,
                agent_loop_stats_api,
            );
            let addr = api_config.api_socket_addr();
            if let Ok(listener) = tokio::net::TcpListener::bind(&addr).await {
                tracing::info!("HTTP API at http://{}", addr);
                let _ = axum::serve(listener, app).await;
            }
        })
    }

    /// Phase 18.5-part-2 — spawn the PersonalAgent tick loop.
    ///
    /// Fires every `config.agent_tick_interval_secs` (default 30s).
    /// Samples a minimal [`AgentTickInput`] (all-zero in the scaffold
    /// because live utilization / task-queue plumbing ships later)
    /// and calls [`crate::agent_loop::run_tick_once`]. Stats are
    /// visible at `/v1/tirami/agent/status → loop.{ticks,last_action,
    /// last_tick_ms}`.
    pub fn spawn_agent_loop(&self) -> tokio::task::JoinHandle<()> {
        let interval_secs = self.config.agent_tick_interval_secs;
        spawn_agent_tick_loop(
            self.personal_agent.clone(),
            self.agent_loop_stats.clone(),
            interval_secs,
            || AgentTickInput::default(),
        )
    }

    /// Phase 18.5-part-3e — populate [`Self::personal_agent`] with a
    /// default [`tirami_mind::PersonalAgent`] tied to the local node
    /// identity, unless the operator disabled it via
    /// `Config::personal_agent_enabled = false`.
    ///
    /// Idempotent: leaves the slot alone when something has already
    /// populated it (e.g. a future state-snapshot reload). This is
    /// the plumbing that makes `tirami start` yield a working agent
    /// without a separate init call — the killer-app commitment
    /// from `docs/killer-app.md`.
    pub async fn ensure_personal_agent(&self, wallet: tirami_core::NodeId) {
        if !self.config.personal_agent_enabled {
            tracing::info!("personal agent disabled by config (personal_agent_enabled = false)");
            return;
        }
        let mut guard = self.personal_agent.lock().await;
        if guard.is_some() {
            return;
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let agent =
            tirami_mind::PersonalAgent::new(wallet.clone(), tirami_mind::TrmBudget::default(), now_ms);
        tracing::info!("personal agent configured for {}", wallet.to_hex());
        *guard = Some(agent);
    }

    /// Initialize P2P transport.
    pub async fn init_transport(&mut self) -> Result<Arc<ForgeTransport>, tirami_core::TiramiError> {
        if let Some(transport) = self.transport.as_ref() {
            return Ok(transport.clone());
        }

        // Phase 17 Wave 4.2 — pick up the operator's DDoS cap from
        // Config. `0` disables the cap entirely (dangerous on public
        // nodes; documented in docs/operator-guide.md).
        let transport = ForgeTransport::new_with_max_connections(
            self.config.max_concurrent_connections as usize,
        )
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
        tracing::info!("=== TIRAMI SEED NODE ===");
        tracing::info!("Public key: {}", id);
        tracing::info!("Node ID: {}", transport.tirami_node_id());
        tracing::info!("Full address: {:?}", addr);
        tracing::info!("Workers connect with: tirami worker --seed {}", id);

        // Accept connections
        let _accept_handle = transport.start_accepting();

        // API server in background
        self.spawn_api();

        // Phase 14.1 — self-register in the PeerRegistry and spawn the
        // periodic PriceSignal broadcast loop.
        self.self_register_price_signal().await;
        self.spawn_price_signal_loop(transport.clone());

        // Phase 16 — spawn the periodic on-chain anchor loop (MockChainClient
        // by default; real Base L2 wiring lands once tirami-contracts ships).
        self.spawn_anchor_loop();

        // Phase 17 Wave 1.3 — spawn the slashing loop so apply_slash has a
        // live production call path. Runs every `slashing_interval_secs`
        // (default 300s) and burns stake for nodes exceeding the collusion
        // trust-penalty threshold.
        self.spawn_slashing_loop();

        // Phase 17 Wave 4.3 — spawn the trade-log checkpoint loop so
        // in-memory `trade_log` memory stays bounded over long
        // operation. Seals trades older than
        // `config.checkpoint_retain_secs` into the JSON-lines archive.
        self.spawn_checkpoint_loop();

        // Phase 14.3 — challenger-side audit loop: periodically picks peers
        // per their AuditTier probability and sends AuditChallenge messages.
        self.spawn_audit_challenger_loop(transport.clone());

        // Phase 18.5-part-3e — auto-configure the user's PersonalAgent
        // using the local node identity as the wallet, unless the
        // operator opted out via `Config::personal_agent_enabled =
        // false` (CLI: `tirami start --no-agent`). Done BEFORE the
        // tick loop spawns so the very first tick sees a populated
        // slot. Idempotent — if someone already pre-populated the
        // slot (e.g. from a persisted state path in a future patch),
        // we leave it alone.
        self.ensure_personal_agent(transport.tirami_node_id()).await;

        // Phase 18.5-part-2 — PersonalAgent tick loop. Drives the
        // auto-earn / auto-spend heuristic when an agent is
        // configured; a no-op when it isn't. Spawned unconditionally
        // so enabling the agent at runtime doesn't require a restart.
        self.spawn_agent_loop();

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
                self.staking_pool.clone(),
            )
            .await
            .map_err(|e| tirami_core::TiramiError::NetworkError(format!("seed: {e}")))?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Phase 14.1 — PriceSignal broadcast
    // -----------------------------------------------------------------------

    /// Build the current PriceSignal from local state.
    async fn build_price_signal(&self, node_id: tirami_core::NodeId) -> tirami_core::PriceSignal {
        let model_capabilities = {
            let manifest = self.model_manifest.lock().await;
            manifest.as_ref().map(|m| vec![m.id.clone()]).unwrap_or_default()
        };

        let available_cu = {
            let ledger = self.ledger.lock().await;
            let bal = ledger.effective_balance(&node_id);
            bal.max(0) as u64
        };

        // Phase 19 / Tier C — advertise HTTP endpoint so consumers
        // can auto-resolve NodeId → URL. Only emit when the API is
        // bound to a non-loopback address (otherwise the URL is
        // useless to peers). `0.0.0.0` is treated as "listen on all"
        // but we need a caller-reachable address, so we surface it
        // only if the operator explicitly set the bind to a public
        // interface name. Callers can always override with a
        // `peer.url` override on the HTTP request.
        let http_endpoint = derive_public_http_endpoint(
            &self.config.api_bind_addr,
            self.config.api_port,
        );

        tirami_core::PriceSignal {
            node_id,
            // Phase 14.1: static 1.0 multiplier. Dynamic pricing policy
            // (based on load, energy cost, market EMA) lands in Phase 14.5.
            price_multiplier: 1.0,
            available_cu,
            model_capabilities,
            // Conservative default; updated as latency EMA collects data.
            latency_hint_ms: 100,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            http_endpoint,
        }
    }

    /// Immediately register our own PriceSignal into the PeerRegistry so
    /// select_provider (Phase 14.2) can find us before the first gossip tick.
    async fn self_register_price_signal(&self) {
        let Some(transport) = self.transport.as_ref() else { return };
        let node_id = transport.tirami_node_id();
        let signal = self.build_price_signal(node_id).await;
        let mut ledger = self.ledger.lock().await;
        ledger.ingest_price_signal(&signal);
    }

    /// Spawn the periodic price signal broadcast task (30s default).
    /// Phase 14.3 — challenger-side audit loop.
    ///
    /// Every 60s: roll each peer's `AuditTier.audit_probability()` and, for
    /// those selected, compute the expected output hash locally (via our own
    /// engine's `generate_audit`) then send an `AuditChallenge`. Responses
    /// flow through the pipeline handler which calls `record_audit_result`.
    ///
    /// Requires a loaded model: we can only validate hashes for models we can
    /// run ourselves. If no model is loaded the loop is a no-op.
    fn spawn_audit_challenger_loop(&self, transport: Arc<ForgeTransport>) {
        let ledger = self.ledger.clone();
        let engine = self.engine.clone();
        let model_manifest = self.model_manifest.clone();
        let my_id = transport.tirami_node_id();

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
            // Skip the immediate first tick — let peers register first.
            ticker.tick().await;

            loop {
                ticker.tick().await;

                // Only run if we have a model loaded — audit requires running
                // the same forward pass locally.
                let Some(local_model) = model_manifest.lock().await.as_ref().map(|m| m.id.clone())
                else {
                    continue;
                };

                // Select audit targets from the PeerRegistry.
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let targets: Vec<_> = {
                    let guard = ledger.lock().await;
                    guard
                        .peer_registry
                        .select_audit_targets(now_ms, || rand::random::<f64>())
                        .into_iter()
                        // Must match our own loaded model.
                        .filter(|(_, m)| *m == local_model)
                        // Never audit ourselves.
                        .filter(|(id, _)| *id != my_id)
                        .collect()
                };

                if targets.is_empty() {
                    continue;
                }

                // Build a deterministic challenge input. For simplicity the
                // current revision uses a canned sequence of tokens; future
                // versions could pick from a shared corpus.
                let input_tokens: Vec<u32> = (1..=16).collect();

                for (target, model_id) in targets {
                    use tirami_infer::InferenceEngine;

                    // Compute the expected hash with our own engine.
                    let expected_hash = {
                        let mut eng = engine.lock().await;
                        match eng.generate_audit(&input_tokens) {
                            Ok(h) => h,
                            Err(e) => {
                                tracing::warn!(%e, "generate_audit (challenger) failed");
                                continue;
                            }
                        }
                    };

                    // Register the pending challenge.
                    let challenge = {
                        let mut guard = ledger.lock().await;
                        guard.audit_tracker.issue_challenge(
                            target.clone(),
                            model_id.clone(),
                            expected_hash,
                            now_ms,
                        )
                    };

                    // Send over P2P transport.
                    let msg = tirami_proto::Envelope {
                        msg_id: rand::random(),
                        sender: my_id.clone(),
                        timestamp: now_ms,
                        payload: tirami_proto::Payload::AuditChallenge(
                            tirami_proto::AuditChallengeMsg {
                                challenge_id: challenge.challenge_id,
                                challenger: my_id.clone(),
                                target: target.clone(),
                                model_id,
                                input_tokens: input_tokens.clone(),
                                expected_output_hash: expected_hash,
                                // Phase 17 Wave 2.1 — preserve legacy
                                // final-layer semantics on this emitter;
                                // a follow-up will randomize the layer.
                                layer_index: None,
                                timestamp: now_ms,
                            },
                        ),
                    };

                    // Convert target NodeId → PeerId for transport send.
                    // tirami-net uses a stringified peer id; we match by hex.
                    let peers = transport.connected_peers().await;
                    if let Some(peer_id) = peers.iter().find(|p| {
                        p.to_string().contains(&hex::encode(&target.0[..8]))
                    }) {
                        if let Err(e) = transport.send_to(peer_id, &msg).await {
                            tracing::debug!(%e, "audit challenge send failed");
                        } else {
                            tracing::info!(
                                target = %target.to_hex(),
                                challenge_id = challenge.challenge_id,
                                "sent audit challenge"
                            );
                        }
                    }
                }

                // House-keeping: drop expired challenges.
                let mut guard = ledger.lock().await;
                let pruned = guard.audit_tracker.prune_expired(now_ms);
                if pruned > 0 {
                    tracing::debug!(count = pruned, "pruned expired audit challenges");
                }
            }
        });
    }

    /// Phase 17 Wave 1.3 — spawn the periodic slashing / trust-penalty loop.
    ///
    /// Every 5 minutes (configurable via `config.slashing_interval_secs`,
    /// clamped to ≥ 60s to avoid runaway CPU on large ledgers), run the
    /// collusion detector against the trade log and slash stakes that
    /// cross [`ComputeLedger::SLASH_PENALTY_THRESHOLD`]. The result is
    /// persisted to both the snapshot (via the normal `save_to_path`
    /// cycle on trade events) and the in-memory `slash_events` audit
    /// trail readable by operators.
    ///
    /// Wave 1.3 closes the "apply_slash is dead code" finding from the
    /// Phase 17 security audit — this is the production call path that
    /// keeps it alive.
    fn spawn_slashing_loop(&self) {
        let ledger = self.ledger.clone();
        let staking = self.staking_pool.clone();
        let ledger_path = self.config.ledger_path.clone();
        let interval_secs = self.config.slashing_interval_secs.max(60);

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            // Skip the immediate first fire; let the node bootstrap.
            ticker.tick().await;

            loop {
                ticker.tick().await;

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);

                let events = {
                    let mut l = ledger.lock().await;
                    let mut s = staking.lock().await;
                    l.update_trust_penalties(&mut s, now_ms)
                };

                if !events.is_empty() {
                    tracing::warn!(
                        "Slashing loop applied {} penalties (total burned: {} TRM)",
                        events.len(),
                        events.iter().map(|e| e.burned_trm).sum::<u64>()
                    );
                    for e in &events {
                        tracing::warn!(
                            "  slashed {} → penalty {:.2}, burned {} TRM, reason={}",
                            e.node_id.to_hex(),
                            e.trust_penalty,
                            e.burned_trm,
                            e.reason
                        );
                    }
                    // Persist so a restart doesn't lose the audit trail.
                    if let Some(path) = ledger_path.as_ref() {
                        let l = ledger.lock().await;
                        if let Err(e) = l.save_to_path(path) {
                            tracing::error!("Failed to persist slash events: {}", e);
                        }
                    }
                }
            }
        });
    }

    /// Phase 17 Wave 4.3 — spawn the trade-log checkpoint loop.
    ///
    /// Every `config.checkpoint_interval_secs` (default 1 h, clamped
    /// ≥ 60 s) this loop calls `ComputeLedger::seal_and_archive`
    /// with `cutoff = now - checkpoint_retain_secs` (default 24 h).
    /// The effect: `trade_log` in memory never grows past roughly
    /// one retain-window's worth of trades, while historical trades
    /// remain recoverable from `config.archive_path`.
    ///
    /// If `config.archive_path` is `None`, the seal still prunes
    /// in-memory state but the archive write is a no-op (the
    /// `ArchivePath::none()` branch of `seal_and_archive`).
    fn spawn_checkpoint_loop(&self) {
        let ledger = self.ledger.clone();
        let ledger_path = self.config.ledger_path.clone();
        let interval_secs = self.config.checkpoint_interval_secs.max(60);
        let retain_ms: u64 = self
            .config
            .checkpoint_retain_secs
            .saturating_mul(1_000);
        let archive = tirami_ledger::ArchivePath(self.config.archive_path.clone());

        tokio::spawn(async move {
            let mut ticker =
                tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            // Skip the immediate first fire; let the node bootstrap.
            ticker.tick().await;

            loop {
                ticker.tick().await;

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let cutoff_ms = now_ms.saturating_sub(retain_ms);

                let outcome = {
                    let mut l = ledger.lock().await;
                    l.seal_and_archive(cutoff_ms, now_ms, &archive)
                };

                match outcome {
                    Ok(checkpoint) if checkpoint.is_nonempty() => {
                        tracing::info!(
                            sealed = checkpoint.trade_count_sealed,
                            cutoff_ms,
                            merkle_root = %hex::encode(checkpoint.merkle_root),
                            "checkpoint loop sealed trades to archive"
                        );
                        // Persist the checkpoint record so it survives
                        // restart. The trade_log prune is in-memory-only
                        // until the next save.
                        if let Some(path) = ledger_path.as_ref() {
                            let l = ledger.lock().await;
                            if let Err(e) = l.save_to_path(path) {
                                tracing::error!(
                                    "failed to persist ledger after checkpoint: {}",
                                    e
                                );
                            }
                        }
                    }
                    Ok(_) => {
                        tracing::debug!("checkpoint loop: nothing to seal");
                    }
                    Err(e) => {
                        tracing::error!("checkpoint loop archive write failed: {}", e);
                    }
                }
            }
        });
    }

    /// Phase 16 — spawn the periodic on-chain anchor loop.
    ///
    /// Default uses `MockChainClient` (in-memory). The anchor interval comes
    /// from `config.anchor_interval_secs` (default 3600 — 60 min); operators
    /// can shorten for local dev. Runs forever, logs errors but never panics.
    fn spawn_anchor_loop(&self) {
        let ledger = self.ledger.clone();
        let chain = self.chain_client.clone();
        let node_id = match self.transport.as_ref() {
            Some(t) => t.tirami_node_id(),
            None => {
                // Fallback: synthesize a deterministic node id from model manifest
                // hash if there's no transport (local-only `tirami node` mode).
                tirami_core::NodeId([0u8; 32])
            }
        };
        let interval_secs = self.config.anchor_interval_secs.max(10);

        tokio::spawn(async move {
            let config = tirami_anchor::AnchorerConfig {
                interval: std::time::Duration::from_secs(interval_secs),
                max_trades_per_batch: 10_000,
                node_id,
            };
            let anchorer = std::sync::Arc::new(tirami_anchor::Anchorer::new(
                config,
                ledger,
                chain,
            ));
            anchorer.run().await;
        });
    }

    fn spawn_price_signal_loop(&self, transport: Arc<ForgeTransport>) {
        let ledger = self.ledger.clone();
        let gossip = self.gossip.clone();
        let model_manifest = self.model_manifest.clone();
        // Build a lightweight closure that re-creates the signal each tick.
        let node_id = transport.tirami_node_id();
        // Phase 19 / Tier C — advertise HTTP endpoint so peers can
        // auto-resolve NodeId → URL for forwarded chat requests.
        let http_endpoint = derive_public_http_endpoint(
            &self.config.api_bind_addr,
            self.config.api_port,
        );

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(30));
            // First tick fires immediately; skip it (we already self-registered).
            ticker.tick().await;

            loop {
                ticker.tick().await;

                // Build fresh signal.
                let model_capabilities = {
                    let manifest = model_manifest.lock().await;
                    manifest
                        .as_ref()
                        .map(|m| vec![m.id.clone()])
                        .unwrap_or_default()
                };
                let available_cu = {
                    let l = ledger.lock().await;
                    l.effective_balance(&node_id).max(0) as u64
                };
                let signal = tirami_core::PriceSignal {
                    node_id: node_id.clone(),
                    price_multiplier: 1.0,
                    available_cu,
                    model_capabilities,
                    latency_hint_ms: 100,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0),
                    http_endpoint: http_endpoint.clone(),
                };

                // Update own PeerRegistry entry.
                {
                    let mut l = ledger.lock().await;
                    l.ingest_price_signal(&signal);
                }

                // Gossip to peers.
                tirami_net::gossip::broadcast_price_signal(&transport, &gossip, &signal).await;
            }
        });
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
        tracing::info!("Shutting down Tirami node...");

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

        tracing::info!("Tirami node shut down");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tirami_core::NodeId;

    fn wallet() -> NodeId {
        NodeId([0xAAu8; 32])
    }

    #[tokio::test]
    async fn ensure_personal_agent_populates_slot_when_enabled() {
        let config = Config {
            personal_agent_enabled: true,
            ..Config::default()
        };
        let node = TiramiNode::new(config);
        assert!(node.personal_agent.lock().await.is_none());
        node.ensure_personal_agent(wallet()).await;
        let guard = node.personal_agent.lock().await;
        let agent = guard.as_ref().expect("agent configured");
        assert_eq!(agent.wallet, wallet());
    }

    #[tokio::test]
    async fn ensure_personal_agent_skips_when_disabled() {
        let config = Config {
            personal_agent_enabled: false,
            ..Config::default()
        };
        let node = TiramiNode::new(config);
        node.ensure_personal_agent(wallet()).await;
        assert!(node.personal_agent.lock().await.is_none());
    }

    #[tokio::test]
    async fn ensure_personal_agent_is_idempotent() {
        let config = Config::default();
        let node = TiramiNode::new(config);
        node.ensure_personal_agent(wallet()).await;
        // Tamper so we can confirm a second call doesn't overwrite.
        {
            let mut guard = node.personal_agent.lock().await;
            guard.as_mut().unwrap().record_earn(999);
        }
        node.ensure_personal_agent(wallet()).await;
        let guard = node.personal_agent.lock().await;
        assert_eq!(guard.as_ref().unwrap().earned_today_trm, 999);
    }

    // ---------------------------------------------------------------
    // Phase 19 / Tier C — derive_public_http_endpoint
    // ---------------------------------------------------------------

    #[test]
    fn loopback_bind_returns_none() {
        assert!(super::derive_public_http_endpoint("127.0.0.1", 3000).is_none());
        assert!(super::derive_public_http_endpoint("localhost", 3000).is_none());
        assert!(super::derive_public_http_endpoint("::1", 3000).is_none());
    }

    #[test]
    fn zero_bind_is_suppressed() {
        // 0.0.0.0 is a "listen on all" sentinel; peers can't reach
        // it as a URL. Suppress rather than advertise garbage.
        assert!(super::derive_public_http_endpoint("0.0.0.0", 3000).is_none());
        assert!(super::derive_public_http_endpoint("::", 3000).is_none());
    }

    #[test]
    fn public_bind_produces_http_url() {
        let url = super::derive_public_http_endpoint("100.64.1.1", 3000)
            .expect("non-loopback bind should advertise");
        assert_eq!(url, "http://100.64.1.1:3000");
    }

    #[test]
    fn ipv6_public_bind_is_bracketed() {
        let url = super::derive_public_http_endpoint("2001:db8::1", 3000)
            .expect("v6 public bind should advertise");
        assert_eq!(url, "http://[2001:db8::1]:3000");
    }
}

/// Phase 19 / Tier C — derive the HTTP endpoint this node should
/// advertise on its PriceSignal so peers can resolve `NodeId → URL`.
///
/// Rules:
/// - Loopback addresses (`127.0.0.1`, `::1`, `localhost`) → `None`
///   because no peer can reach them.
/// - Wildcard binds (`0.0.0.0`, `::`) → `None` because the value
///   isn't a valid URL host.
/// - Anything else → `Some("http://<host>:<port>")` with v6
///   addresses bracketed per RFC 3986.
///
/// Operators who want TLS or a reverse-proxy URL can extend this
/// helper by reading a config override (future `config.public_http_url`).
pub(crate) fn derive_public_http_endpoint(bind: &str, port: u16) -> Option<String> {
    let b = bind.trim();
    if b.is_empty() {
        return None;
    }
    let lower = b.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "127.0.0.1" | "::1" | "localhost" | "0.0.0.0" | "::"
    ) {
        return None;
    }
    if b.contains(':') && !b.starts_with('[') {
        // Raw IPv6 literal — bracket for URL form.
        Some(format!("http://[{b}]:{port}"))
    } else {
        Some(format!("http://{b}:{port}"))
    }
}
