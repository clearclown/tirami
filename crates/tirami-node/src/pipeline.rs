use tirami_core::{Config, ModelManifest, NodeId, PipelineTopology};
use tirami_ledger::{
    ComputeLedger, InferenceIneligible, LoanRecord, LoanStatus, SignedTradeRecord, StakingPool,
    TradeRecord,
};
use tirami_net::{ClusterManager, ForgeTransport, GossipState};
use tirami_proto::{
    Envelope, ErrorCode, ErrorMsg, InferenceRequest, LoanAccept, Payload, PipelineTopologyMsg,
    RpcServerFailed, RpcServerReady, TokenStreamMsg, TradeAccept, TradeProposal, Welcome,
};
use tirami_net::gossip::handle_reputation_gossip;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex, Semaphore};

/// Per-request TradeAccept dispatcher.
///
/// The seed's main recv loop is the single consumer of
/// `transport.recv()`. When it pulls a `Payload::TradeAccept` off
/// the wire, it must hand the consumer's signature back to the
/// matching `handle_inference` task — otherwise that task times
/// out at 5s and records a half-TRM penalized trade. This map
/// keys per-request oneshot senders created by `handle_inference`
/// and looked up by the main loop.
///
/// Same problem applies to the borrow flow (`LoanAccept`) — a
/// follow-up can adopt the same pattern.
pub(crate) type TradeAcceptDispatcher =
    Arc<Mutex<HashMap<u64, oneshot::Sender<Vec<u8>>>>>;

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
        engine: Arc<Mutex<tirami_infer::CandleEngine>>,
        ledger: Arc<Mutex<ComputeLedger>>,
        model_manifest: Arc<Mutex<Option<ModelManifest>>>,
        advertised_topology: Arc<Mutex<Option<PipelineTopology>>>,
        cluster: Option<Arc<ClusterManager>>,
        config: Config,
        ledger_path: Option<std::path::PathBuf>,
        gossip: Arc<Mutex<GossipState>>,
        // Phase 17 Wave 1.4 — staking pool is now plumbed into the audit
        // handler so an AuditVerdict::Failed burns the target's stake
        // and records a SlashEvent, not just a peer-registry demotion.
        staking_pool: Arc<Mutex<tirami_ledger::StakingPool>>,
        // Phase 23 Wave 2 — portable agent identity. When `Some(_)`,
        // outbound P2P trades are signed with the agent's key and the
        // `provider` attribution flips to the agent's pubkey
        // (`did:tirami:<hex>`-equivalent) instead of the machine
        // node id. Pass `Arc::new(Mutex::new(None))` to keep the
        // pre-Wave-2 behaviour.
        agent_identity: Arc<Mutex<Option<tirami_mind::AgentIdentity>>>,
    ) -> anyhow::Result<()> {
        let node_id = self.transport.tirami_node_id();
        tracing::info!("Pipeline seed running, waiting for requests...");
        let _heartbeat = cluster.as_ref().map(|c| c.start_heartbeat());
        let _failure_detector = cluster.as_ref().map(|c| c.start_failure_detector(15));
        let request_slots = Arc::new(Semaphore::new(
            config.max_concurrent_remote_inference_requests,
        ));
        // Fix #80 — per-request TradeAccept dispatcher. Main recv
        // loop routes incoming TradeAccept messages to the matching
        // `handle_inference` task so the 5s timeout + penalty path
        // only fires when the consumer is truly unresponsive.
        let trade_accept_dispatcher: TradeAcceptDispatcher =
            Arc::new(Mutex::new(HashMap::new()));

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
                                    version: tirami_net::PROTOCOL_VERSION,
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
                        let dispatcher = trade_accept_dispatcher.clone();
                        // Phase 23 Wave 2 — snapshot the current
                        // AgentIdentity (if any) so the trade is
                        // attributed to the agent rather than the
                        // machine. Cloning is cheap (Ed25519
                        // SigningKey is a 32-byte seed copy).
                        let agent_snapshot: Option<tirami_mind::AgentIdentity> = {
                            let guard = agent_identity.lock().await;
                            guard.as_ref().cloned()
                        };

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
                                dispatcher,
                                agent_snapshot,
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
                            match tirami_infer::rpc_manager::RpcServer::spawn(req.port) {
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
                    Payload::TradeAccept(accept) => {
                        // Fix #80 — route the consumer signature to
                        // the matching handle_inference task.
                        let mut dispatch = trade_accept_dispatcher.lock().await;
                        if let Some(sender) = dispatch.remove(&accept.request_id) {
                            let _ = sender.send(accept.consumer_sig);
                        } else {
                            tracing::debug!(
                                "Orphan TradeAccept for request_id={} from {}",
                                accept.request_id,
                                peer_id
                            );
                        }
                    }
                    Payload::TradeProposal(_) => {
                        tracing::debug!(
                            "Unexpected TradeProposal in seed main loop from {}",
                            peer_id
                        );
                    }
                    Payload::LoanAccept(_) => {
                        // Handled by wait_for_loan_accept on the lender side.
                        tracing::debug!(
                            "LoanAccept in main loop from {} (handled by task)",
                            peer_id
                        );
                    }
                    Payload::LoanProposal(proposal) => {
                        // Borrower side: verify lender's signature and terms, then counter-sign.
                        let ledger = ledger.clone();
                        let transport = self.transport.clone();
                        let node_id = node_id.clone();
                        let peer_id = peer_id.clone();
                        let ledger_path = ledger_path.clone();
                        tokio::spawn(async move {
                            // Reconstruct the LoanRecord (status=Active, repaid_at=None)
                            let mut loan = LoanRecord {
                                loan_id: [0u8; 32],
                                lender: proposal.lender.clone(),
                                borrower: proposal.borrower.clone(),
                                principal_trm: proposal.principal_trm,
                                interest_rate_per_hour: proposal.interest_rate_per_hour,
                                term_hours: proposal.term_hours,
                                collateral_trm: proposal.collateral_trm,
                                status: LoanStatus::Active,
                                created_at: proposal.created_at,
                                due_at: proposal.due_at,
                                repaid_at: None,
                            };
                            loan.loan_id = loan.compute_loan_id();
                            let canonical = loan.canonical_bytes();

                            // Verify lender's signature
                            use ed25519_dalek::{Signature, VerifyingKey};
                            let lender_key = match VerifyingKey::from_bytes(&loan.lender.0) {
                                Ok(k) => k,
                                Err(_) => {
                                    tracing::warn!(
                                        "LoanProposal from {} has invalid lender key",
                                        peer_id
                                    );
                                    return;
                                }
                            };
                            let lender_sig_arr: [u8; 64] =
                                match proposal.lender_sig.as_slice().try_into() {
                                    Ok(a) => a,
                                    Err(_) => {
                                        tracing::warn!(
                                            "LoanProposal from {} has malformed lender_sig",
                                            peer_id
                                        );
                                        return;
                                    }
                                };
                            if lender_key
                                .verify_strict(&canonical, &Signature::from_bytes(&lender_sig_arr))
                                .is_err()
                            {
                                tracing::warn!(
                                    "LoanProposal lender_sig verification failed from {}",
                                    peer_id
                                );
                                return;
                            }

                            // Safety checks (pool reserves / LTV / credit) are enforced on
                            // the lender side via SafetyController. The borrower-side check
                            // runs at ledger.create_loan time after we've counter-signed,
                            // mirroring how TradeAccept trusts the proposal contents.
                            let _ = ledger; // retained for future borrower-side checks

                            // Counter-sign with this node's key
                            let borrower_sig = transport.sign(&canonical).to_vec();
                            let _ = ledger_path; // not used until create_loan runs here

                            let accept = Envelope {
                                msg_id: proposal.request_id * 10000 + 10000,
                                sender: node_id.clone(),
                                timestamp: now_millis(),
                                payload: Payload::LoanAccept(LoanAccept {
                                    request_id: proposal.request_id,
                                    borrower_sig,
                                }),
                            };
                            if let Err(e) = transport.send_to(&peer_id, &accept).await {
                                tracing::warn!(
                                    "failed to send LoanAccept to {}: {}",
                                    peer_id,
                                    e
                                );
                            } else {
                                tracing::info!(
                                    "LoanAccept sent to {} for {} CU principal",
                                    peer_id,
                                    proposal.principal_trm
                                );
                            }
                        });
                    }
                    Payload::TradeGossip(trade_gossip) => {
                        let gossip = gossip.clone();
                        let ledger = ledger.clone();
                        let ledger_path = ledger_path.clone();
                        // Phase 21 Wave 3 — capture the staking pool +
                        // gate flag so the spawned task can consult
                        // `ComputeLedger::inference_eligibility` before
                        // it records a remote-originated trade.
                        let staking = staking_pool.clone();
                        let stake_gate_enabled = config.stake_gate_enabled;
                        tokio::spawn(async move {
                            if let Some(signed) =
                                tirami_net::gossip::handle_trade_gossip(&gossip, &trade_gossip).await
                            {
                                let mut ledger = ledger.lock().await;
                                // Phase 21 Wave 3 — gate the gossip-
                                // receive path on the same eligibility
                                // verdict the HTTP path enforces. Defense
                                // in depth: the dual-sig already
                                // validates the bilateral agreement, but
                                // this keeps the local ledger free of
                                // contribution-inflation from remote
                                // providers this node's policy
                                // considers ineligible (slashed, or
                                // past the stakeless cap without stake
                                // and without a welcome loan).
                                //
                                // Failure here only refuses the LOCAL
                                // recording — the remote bilateral
                                // agreement is unaffected.
                                {
                                    let staking_guard = staking.lock().await;
                                    if let Err(ineligible) = check_gossip_trade_eligibility(
                                        &ledger,
                                        &staking_guard,
                                        &signed.trade.provider,
                                        now_millis(),
                                        stake_gate_enabled,
                                    ) {
                                        tracing::warn!(
                                            "Rejected gossip trade: provider {} is ineligible per local policy: {}",
                                            signed.trade.provider.to_hex(),
                                            ineligible
                                        );
                                        return;
                                    }
                                }
                                // Phase 17 Wave 1.2 — route inbound gossip
                                // through the signed path so nonce dedup
                                // rejects replays of already-observed v2
                                // trades. The gossip helper already ran a
                                // first-pass verify, but dedup is stateful
                                // and must happen at the ledger layer.
                                match ledger.execute_signed_trade(&signed) {
                                    Ok(()) => {
                                        if let Some(path) = ledger_path.as_ref() {
                                            let _ = ledger.save_to_path(path);
                                        }
                                        tracing::info!(
                                            "Gossip trade recorded: {} CU ({} → {})",
                                            signed.trade.trm_amount,
                                            signed.trade.provider.to_hex(),
                                            signed.trade.consumer.to_hex()
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            "Rejected gossip trade from {}: {}",
                                            signed.trade.provider.to_hex(),
                                            e
                                        );
                                    }
                                }
                            }
                        });
                    }
                    Payload::LoanGossip(loan_gossip) => {
                        let gossip = gossip.clone();
                        let ledger = ledger.clone();
                        let ledger_path = ledger_path.clone();
                        tokio::spawn(async move {
                            if let Some(signed) =
                                tirami_net::gossip::handle_loan_gossip(&gossip, &loan_gossip).await
                            {
                                let mut ledger_guard = ledger.lock().await;
                                // Idempotent: if the loan already exists locally,
                                // create_loan returns an error, which we ignore.
                                match ledger_guard.create_loan(signed.clone()) {
                                    Ok(()) => {
                                        tracing::info!(
                                            "Gossip loan recorded: {} CU ({} → {})",
                                            signed.loan.principal_trm,
                                            signed.loan.lender.to_hex(),
                                            signed.loan.borrower.to_hex()
                                        );
                                        if let Some(path) = ledger_path.as_ref() {
                                            if let Err(e) = ledger_guard.save_to_path(path) {
                                                tracing::warn!(
                                                    "failed to persist ledger after loan gossip: {e}"
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::debug!(
                                            "loan gossip create_loan skipped: {e:?}"
                                        );
                                    }
                                }
                            }
                        });
                    }
                    Payload::ReputationGossip(obs) => {
                        let ledger = ledger.clone();
                        let gossip = gossip.clone();
                        let transport = self.transport.clone();
                        tokio::spawn(async move {
                            handle_reputation_gossip(
                                obs,
                                &ledger,
                                &gossip,
                                Some(&transport),
                            ).await;
                        });
                    }
                    // Phase 14.1 — price signal gossip.
                    Payload::PriceSignalGossip(signal) => {
                        let ledger = ledger.clone();
                        let gossip = gossip.clone();
                        let transport = self.transport.clone();
                        tokio::spawn(async move {
                            tirami_net::gossip::handle_price_signal_gossip(
                                signal,
                                &ledger,
                                &gossip,
                                Some(&transport),
                            ).await;
                        });
                    }
                    // Phase 14.3 — audit challenge: run deterministic
                    // inference on the provided tokens and reply with the
                    // output hash. Wire-format validation already happened
                    // in Envelope::validate_with_sender.
                    Payload::AuditChallenge(challenge) => {
                        let engine = engine.clone();
                        let transport = self.transport.clone();
                        let my_id = node_id.clone();
                        let sender = peer_id.clone();
                        tokio::spawn(async move {
                            use tirami_infer::InferenceEngine;
                            let start = std::time::Instant::now();
                            // Phase 17 Wave 2.1 — if the challenger asked
                            // for a specific layer, run `generate_audit`
                            // with that layer index; otherwise keep the
                            // legacy final-output-layer behavior.
                            let requested_layer = challenge.layer_index;
                            let audit_result = {
                                let mut eng = engine.lock().await;
                                match requested_layer {
                                    Some(layer) if layer != tirami_proto::AuditChallengeMsg::FINAL_OUTPUT_LAYER => {
                                        eng.generate_audit_at_layer(&challenge.input_tokens, layer)
                                    }
                                    _ => eng.generate_audit(&challenge.input_tokens),
                                }
                            };
                            match audit_result {
                                Ok(hash) => {
                                    let msg = Envelope {
                                        msg_id: rand::random(),
                                        sender: my_id,
                                        timestamp: now_millis(),
                                        payload: Payload::AuditResponse(
                                            tirami_proto::AuditResponseMsg {
                                                challenge_id: challenge.challenge_id,
                                                target: challenge.target.clone(),
                                                output_hash: hash,
                                                computation_time_ms: start.elapsed().as_millis() as u64,
                                                // Echo the layer back so the challenger
                                                // can verify we computed the right one.
                                                layer_index: requested_layer,
                                                timestamp: now_millis(),
                                            },
                                        ),
                                    };
                                    if let Err(e) = transport.send_to(&sender, &msg).await {
                                        tracing::debug!(
                                            peer = %sender,
                                            error = %e,
                                            "audit response send failed"
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        challenge_id = challenge.challenge_id,
                                        error = %e,
                                        "generate_audit failed — skipping response"
                                    );
                                }
                            }
                        });
                    }
                    // Phase 14.3 — audit response: compare against our
                    // expected hash stored in ledger.audit_tracker, then
                    // update the responder's AuditTier.
                    //
                    // Phase 17 Wave 1.4 — a Failed verdict now also burns
                    // 30% of the target's stake and records a SlashEvent
                    // with reason "audit-fail". This closes the
                    // "detection without consequence" finding from the
                    // security audit: previously, a repeatedly failing
                    // auditee only saw their AuditTier drop, which is
                    // recoverable with time; slashing is not.
                    Payload::AuditResponse(resp) => {
                        let ledger = ledger.clone();
                        let staking = staking_pool.clone();
                        tokio::spawn(async move {
                            let mut guard = ledger.lock().await;
                            // Phase 17 Wave 2.1 — include the response's
                            // layer_index so `resolve_at_layer` can reject
                            // cross-layer mismatches.
                            let verdict = guard.audit_tracker.resolve_at_layer(
                                resp.challenge_id,
                                &resp.target,
                                &resp.output_hash,
                                resp.layer_index,
                                now_millis(),
                            );
                            match verdict {
                                tirami_ledger::AuditVerdict::Passed => {
                                    guard.peer_registry.record_audit_result(&resp.target, true);
                                    tracing::info!(
                                        target = %resp.target.to_hex(),
                                        "audit passed — tier promoted"
                                    );
                                }
                                tirami_ledger::AuditVerdict::Failed => {
                                    guard.peer_registry.record_audit_result(&resp.target, false);
                                    // Hold the ledger lock, grab the
                                    // staking lock under it, call the
                                    // combined helper. Lock order
                                    // (ledger → staking) matches the
                                    // periodic slashing loop, preventing
                                    // any deadlock from lock inversion.
                                    let burned = {
                                        let mut staking_guard = staking.lock().await;
                                        guard.record_audit_failure_slash(
                                            &mut staking_guard,
                                            &resp.target,
                                            now_millis(),
                                        )
                                    };
                                    tracing::warn!(
                                        target = %resp.target.to_hex(),
                                        burned_trm = burned,
                                        "audit failed — tier demoted and stake slashed"
                                    );
                                }
                                tirami_ledger::AuditVerdict::Unknown => {
                                    tracing::debug!(
                                        challenge_id = resp.challenge_id,
                                        "audit response for unknown/expired challenge"
                                    );
                                }
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

        // Collect streamed tokens. Two completion signals:
        //   (a) is_final=true TokenStream chunk arrives.
        //   (b) TradeProposal counter-signed + TradeAccept sent.
        //
        // Fix #80-follow-up — previously this loop broke on (a)
        // which abandoned the TradeProposal flight still in the
        // wire buffer. The seed then timed out at 5s and fell
        // back to the half-TRM penalty path. Now we wait for
        // BOTH; if the TradeProposal doesn't arrive within
        // `TRADE_PROPOSAL_WAIT`, we return the text anyway (the
        // seed's timeout path will still record a trade).
        const TRADE_PROPOSAL_WAIT: std::time::Duration =
            std::time::Duration::from_secs(3);
        let mut result = String::new();
        let mut seen_final = false;
        let mut counter_signed = false;
        let overall_deadline = tokio::time::Instant::now()
            + std::time::Duration::from_secs(((max_tokens as u64) / 4).max(15));
        loop {
            if seen_final && counter_signed {
                break;
            }
            let remaining = if seen_final {
                TRADE_PROPOSAL_WAIT
            } else {
                overall_deadline.saturating_duration_since(tokio::time::Instant::now())
            };
            if remaining.is_zero() {
                break;
            }
            let next = tokio::time::timeout(remaining, transport.recv()).await;
            let envelope = match next {
                Ok(Some((_peer_id, envelope))) => envelope,
                Ok(None) => break,
                Err(_) => {
                    // Timeout: either waiting for tokens (unlikely to
                    // have completed the HTTP response) or for the
                    // trailing TradeProposal. In either case, exit.
                    break;
                }
            };
            let peer_id = &seed_peer_id;
            let response = envelope;
            match response.payload {
                    Payload::TokenStream(ts) => {
                        if ts.request_id == request_id {
                            result.push_str(&ts.text);
                            if ts.is_final {
                                seen_final = true;
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
                            // Counter-sign the trade. The nonce from the
                            // proposal is part of the canonical bytes in v2;
                            // a mismatch here breaks sig verification.
                            let trade = TradeRecord {
                                provider: proposal.provider,
                                consumer: proposal.consumer,
                                trm_amount: proposal.trm_amount,
                                tokens_processed: proposal.tokens_processed,
                                timestamp: proposal.timestamp,
                                model_id: proposal.model_id,
                                flops_estimated: 0,
                                nonce: proposal.nonce,
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
                            if let Err(e) = transport.send_to(peer_id, &accept).await {
                                tracing::warn!("Failed to send TradeAccept: {}", e);
                            } else {
                                tracing::debug!(
                                    "Trade accepted: {} CU for request {}",
                                    trade.trm_amount,
                                    request_id
                                );
                            }
                            counter_signed = true;
                        }
                    }
                    _ => {}
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
    engine: Arc<Mutex<tirami_infer::CandleEngine>>,
    ledger: Arc<Mutex<ComputeLedger>>,
    ledger_path: Option<std::path::PathBuf>,
    transport: Arc<ForgeTransport>,
    node_id: NodeId,
    consumer_id: NodeId,
    peer_id: &str,
    req: InferenceRequest,
    gossip: Arc<Mutex<GossipState>>,
    trade_accept_dispatcher: TradeAcceptDispatcher,
    // Phase 23 Wave 2 — when `Some`, outbound trade `provider`
    // attribution AND signing key follow this identity rather than
    // the machine-level transport.
    agent_identity: Option<tirami_mind::AgentIdentity>,
) -> anyhow::Result<()> {
    use tirami_infer::InferenceEngine;

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
            None,
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
    let trm_amount = {
        let mut ledger = ledger.lock().await;
        let base_cost = ledger.estimate_cost(total_tokens, 32, 32);
        // Apply reputation-adjusted pricing (Issue #24)
        let actual_cost = ledger.reputation_adjusted_cost(&consumer_id, base_cost);
        if estimated_cost > actual_cost {
            ledger.release_reserve(&consumer_id, estimated_cost - actual_cost);
        }
        actual_cost
    };
    // Phase 23 Wave 2 — resolve the effective provider id (the
    // 32-byte pubkey that will appear in the TradeRecord) BEFORE
    // serialising the canonical bytes, since the provider field is
    // part of the canonical pre-image.
    let effective_provider = agent_identity
        .as_ref()
        .map(|a| NodeId(a.public_key_bytes()))
        .unwrap_or_else(|| node_id.clone());

    let trade = TradeRecord {
        provider: effective_provider.clone(),
        consumer: consumer_id.clone(),
        trm_amount,
        tokens_processed: total_tokens,
        timestamp: now_millis(),
        model_id: "active".to_string(),
        flops_estimated: 0,
        // Phase 17 Wave 1.2 — provider-chosen replay-protection nonce.
        nonce: TradeRecord::fresh_nonce(),
    };

    let canonical = trade.canonical_bytes();
    // Phase 23 Wave 2 — sign with the agent's key when one is loaded,
    // otherwise fall back to the machine SigningKey via the transport.
    let (_, provider_sig) = resolve_outbound_trade_signing(
        &canonical,
        &node_id,
        |c| transport.sign(c).to_vec(),
        agent_identity.as_ref(),
    );

    // Fix #80 — register the dispatcher slot BEFORE sending the
    // TradeProposal so we can't race a very-fast counter-sign.
    let (accept_tx, accept_rx) = oneshot::channel::<Vec<u8>>();
    {
        let mut dispatch = trade_accept_dispatcher.lock().await;
        dispatch.insert(req.request_id, accept_tx);
    }

    // Send TradeProposal to consumer
    let proposal_msg = Envelope {
        msg_id: req.request_id * 10000 + 9999,
        sender: node_id.clone(),
        timestamp: now_millis(),
        payload: Payload::TradeProposal(TradeProposal {
            request_id: req.request_id,
            provider: node_id.clone(),
            consumer: consumer_id.clone(),
            trm_amount,
            tokens_processed: total_tokens,
            timestamp: trade.timestamp,
            model_id: trade.model_id.clone(),
            provider_sig: provider_sig.clone(),
            nonce: trade.nonce,
        }),
    };
    if let Err(e) = transport.send_to(peer_id, &proposal_msg).await {
        // Cancel the dispatcher slot so stale entries don't
        // accumulate when we can't even reach the peer.
        let mut dispatch = trade_accept_dispatcher.lock().await;
        dispatch.remove(&req.request_id);
        drop(dispatch);
        return Err(e.into());
    }

    // Wait for TradeAccept with timeout (5 seconds). The
    // dispatcher delivers the consumer signature through the
    // oneshot channel when the main recv loop receives it.
    let accept_result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        accept_rx,
    )
    .await;
    // Remove any remaining slot on timeout so the map doesn't grow.
    if accept_result.is_err() {
        let mut dispatch = trade_accept_dispatcher.lock().await;
        dispatch.remove(&req.request_id);
    }

    match accept_result {
        Ok(Ok(consumer_sig)) => {
            // Record dual-signed trade
            let signed = SignedTradeRecord {
                trade: trade.clone(),
                provider_sig,
                consumer_sig,
            attestation: None,
            };
            // Phase 17 Wave 1.2 — route through execute_signed_trade so the
            // ledger re-verifies signatures AND enforces nonce dedup. The
            // explicit signed.verify() above is retained because we want
            // to branch on signature-specific failures (50% penalty flow)
            // rather than treat them identically to replay rejections.
            match signed.verify() {
                Ok(()) => {
                    let mut ledger = ledger.lock().await;
                    match ledger.execute_signed_trade(&signed) {
                        Ok(()) => {
                            if let Some(path) = ledger_path.as_ref() {
                                ledger.save_to_path(path)?;
                            }
                            tracing::info!(
                                "Signed trade recorded: {} CU for {} tokens to {}",
                                trade.trm_amount,
                                total_tokens,
                                peer_id
                            );
                            // Reputation boost for successful signed trade
                            ledger.update_reputation(&trade.provider, 0.01);
                            drop(ledger);
                            // Broadcast to mesh via gossip
                            tirami_net::gossip::broadcast_trade(&transport, &gossip, &signed).await;
                        }
                        Err(e) => {
                            // Replay or (very unlikely) a second-pass sig
                            // failure. Do NOT broadcast; do NOT credit.
                            tracing::warn!(
                                "Trade rejected by ledger after accept from {}: {}",
                                peer_id,
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Trade signature verification failed: {}", e);
                    // 50% penalty on unsigned trades (Issue #3)
                    let mut penalized = trade.clone();
                    penalized.trm_amount /= 2;
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
            penalized.trm_amount /= 2;
            let mut ledger = ledger.lock().await;
            ledger.execute_trade(&penalized);
            if let Some(path) = ledger_path.as_ref() {
                ledger.save_to_path(path)?;
            }
        }
    }

    Ok(())
}

/// Wait for a LoanAccept message matching the given request_id.
///
/// Used by the lender-side `/v1/tirami/lend-to` flow after a `LoanProposal`
/// has been sent. Mirrors `wait_for_trade_accept`.
pub(crate) async fn wait_for_loan_accept(
    transport: &ForgeTransport,
    request_id: u64,
) -> Option<Vec<u8>> {
    loop {
        match transport.recv().await {
            Some((_peer_id, envelope)) => {
                if let Payload::LoanAccept(accept) = envelope.payload {
                    if accept.request_id == request_id {
                        return Some(accept.borrower_sig);
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

/// Phase 23 Wave 2 — resolve the `(provider_node_id, signature_bytes)`
/// pair for an outbound P2P trade.
///
/// When `agent` is `Some`, the trade is attributed to the agent's
/// 32-byte Ed25519 pubkey (the same value behind `did:tirami:<hex>`)
/// AND the signature is produced by the agent's `SigningKey`. When
/// `agent` is `None`, the function falls back to the machine
/// identity: the trade carries `machine_node_id` as provider and is
/// signed by `machine_sign`.
///
/// Extracted as a free function so the per-trade logic is unit-
/// testable without spinning up the whole pipeline.
pub(crate) fn resolve_outbound_trade_signing<F>(
    canonical: &[u8],
    machine_node_id: &NodeId,
    machine_sign: F,
    agent: Option<&tirami_mind::AgentIdentity>,
) -> (NodeId, Vec<u8>)
where
    F: FnOnce(&[u8]) -> Vec<u8>,
{
    match agent {
        Some(a) => {
            let pk = a.public_key_bytes();
            let sig = a.sign(canonical).to_bytes().to_vec();
            (NodeId(pk), sig)
        }
        None => {
            let sig = machine_sign(canonical);
            (machine_node_id.clone(), sig)
        }
    }
}

/// Phase 21 Wave 3 — does the local-policy stake gate allow the
/// node to **record** an incoming gossiped trade?
///
/// This is a deliberately small wrapper around
/// [`ComputeLedger::inference_eligibility`] so the gossip handler's
/// behaviour is unit-testable in isolation from `tokio::spawn` /
/// network setup. The semantics are intentionally local-only:
///
/// - `gate_enabled = false` → always `Ok(())`. Restores pre-Phase-21
///   behaviour for operators that explicitly opt out.
/// - `gate_enabled = true` → consult the verdict. `Ok` for Staked /
///   WelcomeLoan / BootstrapWindow; `Err(InferenceIneligible)` for
///   PreviouslySlashed / StakeRequired.
///
/// A denial here only refuses **local** recording; the dual-signed
/// trade still stands as a bilateral agreement between the remote
/// provider and consumer, and other nodes with different policies
/// may still record it.
pub(crate) fn check_gossip_trade_eligibility(
    ledger: &ComputeLedger,
    staking: &StakingPool,
    provider: &NodeId,
    now_ms: u64,
    gate_enabled: bool,
) -> Result<(), InferenceIneligible> {
    if !gate_enabled {
        return Ok(());
    }
    ledger
        .inference_eligibility(provider, staking, now_ms)
        .map(|_| ())
}

#[cfg(test)]
mod gossip_gate_tests {
    use super::*;
    use tirami_ledger::{ComputeLedger, StakeDuration, StakingPool};

    #[test]
    fn gate_disabled_always_passes_even_for_slashed_provider() {
        let mut ledger = ComputeLedger::new();
        let staking = StakingPool::new();
        let provider = NodeId([0xA1u8; 32]);
        ledger.record_slash_event(provider.clone(), 0.3, 100, "collusion", 0);
        assert!(
            check_gossip_trade_eligibility(&ledger, &staking, &provider, 1_000, false).is_ok()
        );
    }

    #[test]
    fn gate_enabled_passes_fresh_provider_via_bootstrap_window() {
        let ledger = ComputeLedger::new();
        let staking = StakingPool::new();
        let provider = NodeId([0xA2u8; 32]);
        assert!(
            check_gossip_trade_eligibility(&ledger, &staking, &provider, 1_000, true).is_ok()
        );
    }

    #[test]
    fn gate_enabled_rejects_previously_slashed_provider() {
        let mut ledger = ComputeLedger::new();
        let staking = StakingPool::new();
        let provider = NodeId([0xA3u8; 32]);
        ledger.record_slash_event(provider.clone(), 0.3, 100, "collusion", 0);
        let verdict =
            check_gossip_trade_eligibility(&ledger, &staking, &provider, 1_000, true);
        assert_eq!(verdict, Err(InferenceIneligible::PreviouslySlashed));
    }

    #[test]
    fn gate_enabled_passes_provider_with_active_welcome_loan_past_cap() {
        // Provider with an active welcome loan AND past the
        // stakeless cap. The HTTP-path test in api.rs covers the
        // same case from the server-serving side; this exercise
        // it from the gossip-receive side.
        let mut ledger = ComputeLedger::new();
        let staking = StakingPool::new();
        let provider = NodeId([0xA4u8; 32]);
        // Wall-clock now so the 72 h expiry is in the future.
        let now = now_millis();
        ledger.grant_welcome_loan(provider.clone(), "", now).expect("grant");
        let cap_buster = TradeRecord {
            provider: provider.clone(),
            consumer: NodeId([0x77u8; 32]),
            trm_amount: tirami_ledger::lending::STAKELESS_EARN_CAP_TRM + 1,
            tokens_processed: 1,
            timestamp: now,
            model_id: "pump".into(),
            flops_estimated: 0,
            nonce: [0u8; 16],
        };
        ledger.execute_trade(&cap_buster);
        assert!(
            check_gossip_trade_eligibility(&ledger, &staking, &provider, now, true).is_ok(),
            "active welcome loan should let the gate accept a past-cap provider"
        );
    }

    #[test]
    fn gate_enabled_rejects_past_cap_provider_without_stake_or_loan() {
        // Seed a balance via the public API: grant a welcome loan
        // (creates the balance entry), pump contributions past cap,
        // then mark the loan repaid so the eligibility check falls
        // through to StakeRequired.
        let mut ledger = ComputeLedger::new();
        let staking = StakingPool::new();
        let provider = NodeId([0xA5u8; 32]);
        let now = now_millis();
        ledger.grant_welcome_loan(provider.clone(), "", now).expect("grant");
        let cap_buster = TradeRecord {
            provider: provider.clone(),
            consumer: NodeId([0x77u8; 32]),
            trm_amount: tirami_ledger::lending::STAKELESS_EARN_CAP_TRM + 1,
            tokens_processed: 1,
            timestamp: now,
            model_id: "pump".into(),
            flops_estimated: 0,
            nonce: [0u8; 16],
        };
        ledger.execute_trade(&cap_buster);
        // Flip the welcome loan to repaid so it no longer satisfies
        // the gate. The remaining path is then StakeRequired.
        if let Some(grant) = ledger.welcome_loans.get_mut(&provider) {
            grant.repaid = true;
        }
        let verdict =
            check_gossip_trade_eligibility(&ledger, &staking, &provider, now, true);
        assert!(
            matches!(verdict, Err(InferenceIneligible::StakeRequired { .. })),
            "got {verdict:?}"
        );
    }

    #[test]
    fn gate_enabled_accepts_staked_provider() {
        use tirami_ledger::lending::MIN_PROVIDER_STAKE_TRM;
        let ledger = ComputeLedger::new();
        let mut staking = StakingPool::new();
        let provider = NodeId([0xA6u8; 32]);
        let now = now_millis();
        staking
            .stake(provider.clone(), MIN_PROVIDER_STAKE_TRM, StakeDuration::Days7, now)
            .expect("stake ok");
        assert!(
            check_gossip_trade_eligibility(&ledger, &staking, &provider, now, true).is_ok()
        );
    }
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
mod resolve_outbound_trade_signing_tests {
    use super::*;
    use tirami_ledger::SignedTradeRecord;
    use tirami_ledger::TradeRecord;
    use tirami_mind::AgentIdentity;

    fn sample_canonical() -> (TradeRecord, Vec<u8>) {
        let trade = TradeRecord {
            provider: NodeId([0xFFu8; 32]),
            consumer: NodeId([0xEEu8; 32]),
            trm_amount: 5,
            tokens_processed: 5,
            timestamp: super::now_millis(),
            model_id: "phase23-w2".into(),
            flops_estimated: 0,
            nonce: [0u8; 16],
        };
        let canonical = trade.canonical_bytes();
        (trade, canonical)
    }

    #[test]
    fn machine_path_uses_machine_node_id_and_callback_signer() {
        let (_, canonical) = sample_canonical();
        let machine_id = NodeId([0xAAu8; 32]);
        let signer_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let signer_flag = signer_called.clone();
        let canonical_for_assert = canonical.clone();
        let (provider, sig) = resolve_outbound_trade_signing(
            &canonical,
            &machine_id,
            move |c| {
                signer_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                // Synthetic signature: just echo a fixed 64 bytes.
                assert_eq!(c, canonical_for_assert.as_slice());
                vec![0u8; 64]
            },
            None,
        );
        assert!(signer_called.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(provider, machine_id);
        assert_eq!(sig.len(), 64);
    }

    #[test]
    fn agent_path_uses_agent_pubkey_as_provider() {
        let (_, canonical) = sample_canonical();
        let machine_id = NodeId([0xAAu8; 32]);
        let agent = AgentIdentity::generate(0, None);
        let agent_pk = agent.public_key_bytes();
        let (provider, _sig) = resolve_outbound_trade_signing(
            &canonical,
            &machine_id,
            |_| unreachable!("agent path must not invoke machine signer"),
            Some(&agent),
        );
        assert_eq!(provider, NodeId(agent_pk));
        assert_ne!(provider, machine_id, "provider must be agent, not machine");
    }

    #[test]
    fn agent_path_produces_valid_ed25519_signature() {
        let (trade, canonical) = sample_canonical();
        let machine_id = NodeId([0xAAu8; 32]);
        let agent = AgentIdentity::generate(0, None);
        let (provider, sig) = resolve_outbound_trade_signing(
            &canonical,
            &machine_id,
            |_| panic!("not used"),
            Some(&agent),
        );
        // The returned signature must be a valid Ed25519 signature
        // when verified against the returned provider (= the agent
        // pubkey). Construct a SignedTradeRecord with the agent as
        // provider AND attach a placeholder consumer sig — we only
        // care that the provider half verifies.
        let mut signed_trade = trade.clone();
        signed_trade.provider = provider.clone();
        let canonical_agent = signed_trade.canonical_bytes();
        let resigned = agent.sign(&canonical_agent).to_bytes().to_vec();
        // The helper signs the CALLER-supplied canonical (which is
        // the original trade's). Verify the equivalence: signing the
        // same input with the same key yields the same sig.
        let same_sig = agent.sign(&canonical).to_bytes().to_vec();
        assert_eq!(sig, same_sig);
        // Also confirm the agent's key can sign its own version too.
        assert_eq!(resigned.len(), 64);
    }

    #[test]
    fn two_agents_sign_the_same_canonical_with_different_signatures() {
        let (_, canonical) = sample_canonical();
        let machine_id = NodeId([0xAAu8; 32]);
        let a = AgentIdentity::generate(0, None);
        let b = AgentIdentity::generate(0, None);
        let (provider_a, sig_a) = resolve_outbound_trade_signing(
            &canonical,
            &machine_id,
            |_| panic!("not used"),
            Some(&a),
        );
        let (provider_b, sig_b) = resolve_outbound_trade_signing(
            &canonical,
            &machine_id,
            |_| panic!("not used"),
            Some(&b),
        );
        assert_ne!(provider_a, provider_b);
        assert_ne!(sig_a, sig_b);
    }

    /// End-to-end shape: build a SignedTradeRecord where BOTH halves
    /// are produced via this helper (using the agent for the provider
    /// side; using a fresh ephemeral key for the consumer side), then
    /// run `signed.verify()` and confirm it accepts. This pins the
    /// Wave-2 guarantee that "agent-signed trades remain valid under
    /// the existing ledger verifier".
    #[test]
    fn signed_trade_record_verifies_when_provider_signed_by_agent_identity() {
        use ed25519_dalek::SigningKey;
        let (mut trade, _) = sample_canonical();
        let agent = AgentIdentity::generate(0, None);
        let consumer_sk = SigningKey::from_bytes(&[0xCEu8; 32]);
        let consumer_pk = consumer_sk.verifying_key();
        // Set provider = agent pubkey, consumer = consumer_pk.
        trade.provider = NodeId(agent.public_key_bytes());
        trade.consumer = NodeId(consumer_pk.to_bytes());
        let canonical = trade.canonical_bytes();
        let (_provider, provider_sig) = resolve_outbound_trade_signing(
            &canonical,
            &NodeId([0xAAu8; 32]),
            |_| panic!("not used"),
            Some(&agent),
        );
        use ed25519_dalek::Signer;
        let consumer_sig = consumer_sk.sign(&canonical).to_bytes().to_vec();
        let signed = SignedTradeRecord {
            trade: trade.clone(),
            provider_sig: provider_sig.clone(),
            consumer_sig,
            attestation: None,
        };
        signed.verify().expect("dual-signed verify");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tirami_infer::CandleEngine;
    use tirami_ledger::ComputeLedger;
    use tirami_net::ForgeTransport;
    use tokio::time::{Duration, timeout};

    // ---------------------------------------------------------------------
    // Fix #80 — TradeAcceptDispatcher
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn trade_accept_dispatcher_delivers_signature() {
        let dispatcher: TradeAcceptDispatcher = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel::<Vec<u8>>();
        dispatcher.lock().await.insert(42, tx);

        // Simulate the seed main loop receiving a TradeAccept for
        // request_id=42 and routing the consumer_sig through the
        // oneshot channel.
        let sender = dispatcher
            .lock()
            .await
            .remove(&42)
            .expect("slot for 42 must exist");
        sender
            .send(vec![0xAAu8; 64])
            .expect("oneshot::send must succeed");

        let sig = rx.await.expect("handle_inference side must get the sig");
        assert_eq!(sig.len(), 64);
        assert!(dispatcher.lock().await.is_empty());
    }

    #[tokio::test]
    async fn trade_accept_dispatcher_orphan_request_is_noop() {
        // When the main loop receives a TradeAccept for a request_id
        // that has no registered waiter (e.g. the handle_inference
        // task already timed out and cleaned up), remove returns
        // None and the message is dropped without panic.
        let dispatcher: TradeAcceptDispatcher = Arc::new(Mutex::new(HashMap::new()));
        let slot = dispatcher.lock().await.remove(&99);
        assert!(slot.is_none());
    }

    #[tokio::test]
    async fn trade_accept_dispatcher_timeout_cleans_up_slot() {
        // Simulate: handle_inference registers a slot, the consumer
        // never sends TradeAccept, and the timeout branch removes
        // the entry. The map must not grow unboundedly.
        let dispatcher: TradeAcceptDispatcher = Arc::new(Mutex::new(HashMap::new()));
        let (tx, _rx) = oneshot::channel::<Vec<u8>>();
        dispatcher.lock().await.insert(7, tx);
        assert_eq!(dispatcher.lock().await.len(), 1);
        // Timeout branch in handle_inference removes the slot.
        dispatcher.lock().await.remove(&7);
        assert!(dispatcher.lock().await.is_empty());
    }

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

        let worker_peer_id = transport_worker.tirami_node_id().to_hex();
        let seed_peer_id = peer_seed.peer_id().to_string();
        let worker_node_id = transport_worker.tirami_node_id();
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
            sender: transport_seed.tirami_node_id(),
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
                        Arc::new(Mutex::new(GossipState::new())),
                        Arc::new(Mutex::new(tirami_ledger::StakingPool::new())),
                        Arc::new(Mutex::new(None)),
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
            &transport_worker.tirami_node_id(),
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
