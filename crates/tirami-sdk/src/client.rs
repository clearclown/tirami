use crate::error::SdkError;
use crate::types::*;
use reqwest::Client;

/// Async HTTP client for the Forge compute economy API.
///
/// All methods are thin, typed wrappers around the forge-node REST endpoints.
/// For endpoints where the response schema is complex or variable, the raw
/// `serde_json::Value` is returned (matching the previous Python dict-return
/// pattern).
pub struct TiramiClient {
    pub(crate) base_url: String,
    pub(crate) token: Option<String>,
    client: Client,
}

impl TiramiClient {
    /// Create a new client.
    ///
    /// - `base_url` — Node address, e.g. `"http://127.0.0.1:3000"`.
    ///   Trailing slashes are stripped automatically.
    /// - `token` — Optional bearer token for authenticated endpoints.
    pub fn new(base_url: &str, token: Option<&str>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.map(String::from),
            client: Client::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Send an authenticated GET and return the body as a JSON value.
    async fn get(&self, path: &str) -> Result<serde_json::Value, SdkError> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.get(&url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(SdkError::Api { status, message: body });
        }
        Ok(resp.json().await?)
    }

    /// Send an authenticated POST with a JSON body and return the body as a JSON value.
    async fn post(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, SdkError> {
        self.post_json_with_header(path, body, "", "").await
    }

    /// Phase 15 Step 2 alias for `post` — same behavior, clearer name.
    async fn post_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, SdkError> {
        self.post_json_with_header(path, body, "", "").await
    }

    /// Phase 14.3 fix — POST with an additional header (e.g. X-Tirami-Node-Id).
    /// Pass `""` for either header arg to skip.
    async fn post_json_with_header(
        &self,
        path: &str,
        body: &serde_json::Value,
        header_name: &str,
        header_value: &str,
    ) -> Result<serde_json::Value, SdkError> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.post(&url).json(body);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        if !header_name.is_empty() && !header_value.is_empty() {
            req = req.header(header_name, header_value);
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(SdkError::Api { status, message: body });
        }
        Ok(resp.json().await?)
    }

    // -----------------------------------------------------------------------
    // Economy
    // -----------------------------------------------------------------------

    /// `GET /v1/tirami/balance` — CU balance: contributed, consumed, reserved,
    /// effective_balance, reputation.
    pub async fn balance(&self) -> Result<Balance, SdkError> {
        let v = self.get("/v1/tirami/balance").await?;
        Ok(serde_json::from_value(v)?)
    }

    /// `GET /v1/tirami/pricing` — Market price: trm_per_token, supply/demand
    /// factors, deflation, cost estimates.
    pub async fn pricing(&self) -> Result<Pricing, SdkError> {
        let v = self.get("/v1/tirami/pricing").await?;
        Ok(serde_json::from_value(v)?)
    }

    /// `GET /v1/tirami/trades?limit=N` — Recent trade history.
    pub async fn trades(&self, limit: u32) -> Result<serde_json::Value, SdkError> {
        self.get(&format!("/v1/tirami/trades?limit={}", limit)).await
    }

    /// `GET /v1/tirami/network` — Mesh economic summary with Merkle root.
    pub async fn network(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/network").await
    }

    /// `GET /v1/tirami/providers` — Providers ranked by reputation and cost.
    pub async fn providers(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/providers").await
    }

    // -----------------------------------------------------------------------
    // Phase 14.1 / 14.2 — PeerRegistry + scheduling
    // -----------------------------------------------------------------------

    /// `GET /v1/tirami/peers` — Phase 14.1 PeerRegistry dump.
    ///
    /// Returns every peer the local node has observed via PriceSignal gossip.
    /// Each entry includes `price_multiplier`, `available_cu`, `audit_tier`,
    /// `latency_ema_ms`, and the models advertised. Returns `PeersResponse`
    /// for easy field access.
    pub async fn peers(&self) -> Result<PeersResponse, SdkError> {
        let v = self.get("/v1/tirami/peers").await?;
        Ok(serde_json::from_value(v)?)
    }

    /// `POST /v1/tirami/schedule` — Phase 14.2 Ledger-as-Brain probe.
    ///
    /// Asks the node "given this model + token budget, who would you pick
    /// as provider and what's the estimated cost?" Does NOT reserve TRM —
    /// read-only. Useful for agents to shop around before committing.
    ///
    /// - `consumer` — optional hex NodeId (64 chars). If `None`, the node's
    ///   own `local_node_id` is used.
    pub async fn schedule(
        &self,
        model_id: &str,
        max_tokens: u64,
        consumer: Option<&str>,
    ) -> Result<Schedule, SdkError> {
        let mut body = serde_json::json!({
            "model_id": model_id,
            "max_tokens": max_tokens,
        });
        if let Some(c) = consumer {
            body["consumer"] = serde_json::Value::String(c.to_string());
        }
        let v = self.post_json("/v1/tirami/schedule", &body).await?;
        Ok(serde_json::from_value(v)?)
    }

    // -----------------------------------------------------------------------
    // Phase 14.3 (fix #61) — consumer identity header
    // -----------------------------------------------------------------------

    /// Run a chat completion on behalf of a specific consumer NodeId.
    ///
    /// Equivalent to [`chat`] but sends the `X-Tirami-Node-Id` header so the
    /// resulting trade is recorded with the given consumer instead of the
    /// anonymous `0xff…` fallback. Essential for cross-node economy.
    ///
    /// - `consumer_hex` — 64-char hex NodeId of the consumer.
    pub async fn chat_as(
        &self,
        consumer_hex: &str,
        model: &str,
        prompt: &str,
        max_tokens: u32,
    ) -> Result<serde_json::Value, SdkError> {
        let body = serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": max_tokens,
        });
        self.post_json_with_header(
            "/v1/chat/completions",
            &body,
            "X-Tirami-Node-Id",
            consumer_hex,
        )
        .await
    }

    /// Check whether this node can afford a request of `estimated_tokens` output
    /// tokens.  Calls `balance()` and `pricing()` under the hood.
    pub async fn can_afford(&self, estimated_tokens: u64) -> Result<bool, SdkError> {
        let pricing = self.pricing().await?;
        let cost = (pricing.trm_per_token * estimated_tokens as f64) as i64;
        let balance = self.balance().await?;
        Ok(balance.effective_balance >= cost)
    }

    // -----------------------------------------------------------------------
    // Inference
    // -----------------------------------------------------------------------

    /// `POST /v1/chat/completions` — OpenAI-compatible chat completions.
    pub async fn chat(
        &self,
        model: &str,
        messages: &[serde_json::Value],
        max_tokens: u32,
    ) -> Result<ChatCompletion, SdkError> {
        let body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
        });
        let v = self.post("/v1/chat/completions", &body).await?;
        Ok(serde_json::from_value(v)?)
    }

    // -----------------------------------------------------------------------
    // Safety
    // -----------------------------------------------------------------------

    /// `GET /v1/tirami/safety` — Kill switch, circuit breaker, budget policy status.
    pub async fn safety(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/safety").await
    }

    /// `POST /v1/tirami/kill` with `activate: true` — EMERGENCY: freeze all CU
    /// transactions.
    pub async fn kill(&self, reason: &str) -> Result<serde_json::Value, SdkError> {
        self.post(
            "/v1/tirami/kill",
            &serde_json::json!({"activate": true, "reason": reason}),
        )
        .await
    }

    /// `POST /v1/tirami/kill` with `activate: false` — Resume normal CU
    /// transactions.
    pub async fn resume(&self) -> Result<serde_json::Value, SdkError> {
        self.post("/v1/tirami/kill", &serde_json::json!({"activate": false}))
            .await
    }

    // -----------------------------------------------------------------------
    // Settlement
    // -----------------------------------------------------------------------

    /// `POST /v1/tirami/invoice` — Create a Lightning invoice to convert CU to
    /// Bitcoin.
    pub async fn invoice(&self, trm_amount: u64) -> Result<serde_json::Value, SdkError> {
        self.post(
            "/v1/tirami/invoice",
            &serde_json::json!({"trm_amount": trm_amount}),
        )
        .await
    }

    /// `GET /settlement?hours=N` — Export a settlement statement for the given
    /// time window.
    pub async fn settlement(&self, hours: u64) -> Result<serde_json::Value, SdkError> {
        self.get(&format!("/settlement?hours={}", hours)).await
    }

    // -----------------------------------------------------------------------
    // Lending (Phase 5.5)
    // -----------------------------------------------------------------------

    /// `POST /v1/tirami/lend` — Contribute CU to the lending pool.
    pub async fn lend(
        &self,
        amount: u64,
        max_term_hours: u64,
        min_interest_rate: Option<f64>,
    ) -> Result<serde_json::Value, SdkError> {
        let mut body = serde_json::json!({
            "amount": amount,
            "max_term_hours": max_term_hours,
        });
        if let Some(rate) = min_interest_rate {
            body["min_interest_rate"] = serde_json::json!(rate);
        }
        self.post("/v1/tirami/lend", &body).await
    }

    /// `POST /v1/tirami/borrow` — Request a CU loan.
    ///
    /// - `lender` — Optional hex NodeId of a specific lender.
    pub async fn borrow(
        &self,
        amount: u64,
        term_hours: u64,
        collateral: u64,
        lender: Option<&str>,
    ) -> Result<serde_json::Value, SdkError> {
        let mut body = serde_json::json!({
            "amount": amount,
            "term_hours": term_hours,
            "collateral": collateral,
        });
        if let Some(l) = lender {
            body["lender"] = serde_json::json!(l);
        }
        self.post("/v1/tirami/borrow", &body).await
    }

    /// `POST /v1/tirami/repay` — Repay an outstanding loan by loan_id.
    pub async fn repay(&self, loan_id: &str) -> Result<serde_json::Value, SdkError> {
        self.post("/v1/tirami/repay", &serde_json::json!({"loan_id": loan_id}))
            .await
    }

    /// `GET /v1/tirami/credit` — Credit score and component breakdown for this
    /// node.
    pub async fn credit(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/credit").await
    }

    /// `GET /v1/tirami/pool` — Lending pool status and this node's max borrow
    /// capacity.
    pub async fn pool(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/pool").await
    }

    /// `GET /v1/tirami/loans` — Active loans where this node is lender or
    /// borrower.
    pub async fn loans(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/loans").await
    }

    /// `POST /v1/tirami/lend-to` — Lender-initiated loan proposal to a specific
    /// borrower.
    pub async fn lend_to(
        &self,
        borrower: &str,
        amount: u64,
        term_hours: u64,
        collateral: u64,
        interest_rate_per_hour: Option<f64>,
    ) -> Result<serde_json::Value, SdkError> {
        let mut body = serde_json::json!({
            "borrower": borrower,
            "amount": amount,
            "term_hours": term_hours,
            "collateral": collateral,
        });
        if let Some(rate) = interest_rate_per_hour {
            body["interest_rate_per_hour"] = serde_json::json!(rate);
        }
        self.post("/v1/tirami/lend-to", &body).await
    }

    // -----------------------------------------------------------------------
    // Routing (Phase 6)
    // -----------------------------------------------------------------------

    /// `GET /v1/tirami/route` — Find the optimal inference provider for a
    /// request.
    ///
    /// - `mode` — `"cost"` | `"quality"` | `"balanced"` (default).
    pub async fn route(
        &self,
        model: Option<&str>,
        max_cu: Option<u64>,
        mode: &str,
        max_tokens: Option<u32>,
    ) -> Result<serde_json::Value, SdkError> {
        let mut path = format!("/v1/tirami/route?mode={}", mode);
        if let Some(m) = model {
            path.push_str(&format!("&model={}", m));
        }
        if let Some(c) = max_cu {
            path.push_str(&format!("&max_cu={}", c));
        }
        if let Some(t) = max_tokens {
            path.push_str(&format!("&max_tokens={}", t));
        }
        self.get(&path).await
    }

    // -----------------------------------------------------------------------
    // L2 Bank (Phase 8)
    // -----------------------------------------------------------------------

    /// `GET /v1/tirami/bank/portfolio` — PortfolioManager state.
    pub async fn bank_portfolio(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/bank/portfolio").await
    }

    /// `POST /v1/tirami/bank/tick` — Run one PortfolioManager tick cycle.
    pub async fn bank_tick(&self) -> Result<serde_json::Value, SdkError> {
        self.post("/v1/tirami/bank/tick", &serde_json::json!({})).await
    }

    /// `POST /v1/tirami/bank/strategy` — Hot-swap portfolio strategy.
    ///
    /// - `strategy` — `"conservative"` | `"highyield"` | `"balanced"`.
    pub async fn bank_set_strategy(
        &self,
        strategy: &str,
        base_commit_fraction: Option<f64>,
    ) -> Result<serde_json::Value, SdkError> {
        let mut body = serde_json::json!({"strategy": strategy});
        if let Some(f) = base_commit_fraction {
            body["base_commit_fraction"] = serde_json::json!(f);
        }
        self.post("/v1/tirami/bank/strategy", &body).await
    }

    /// `POST /v1/tirami/bank/risk` — Set risk tolerance.
    ///
    /// - `tolerance` — `"conservative"` | `"balanced"` | `"aggressive"`.
    pub async fn bank_set_risk(&self, tolerance: &str) -> Result<serde_json::Value, SdkError> {
        self.post(
            "/v1/tirami/bank/risk",
            &serde_json::json!({"tolerance": tolerance}),
        )
        .await
    }

    /// `GET /v1/tirami/bank/futures` — List active futures contracts.
    pub async fn bank_list_futures(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/bank/futures").await
    }

    /// `POST /v1/tirami/bank/futures` — Create a new CU futures contract.
    pub async fn bank_create_future(
        &self,
        counterparty_hex: &str,
        notional_trm: u64,
        strike_price_msats: u64,
        expires_at_ms: u64,
        margin_fraction: Option<f64>,
    ) -> Result<serde_json::Value, SdkError> {
        let mut body = serde_json::json!({
            "counterparty_hex": counterparty_hex,
            "notional_trm": notional_trm,
            "strike_price_msats": strike_price_msats,
            "expires_at_ms": expires_at_ms,
        });
        if let Some(m) = margin_fraction {
            body["margin_fraction"] = serde_json::json!(m);
        }
        self.post("/v1/tirami/bank/futures", &body).await
    }

    /// `GET /v1/tirami/bank/risk-assessment` — Portfolio VaR, concentration,
    /// leverage.
    pub async fn bank_risk_assessment(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/bank/risk-assessment").await
    }

    /// `POST /v1/tirami/bank/optimize` — Run YieldOptimizer with a VaR budget.
    pub async fn bank_optimize(&self, max_var_99_cu: u64) -> Result<serde_json::Value, SdkError> {
        self.post(
            "/v1/tirami/bank/optimize",
            &serde_json::json!({"max_var_99_cu": max_var_99_cu}),
        )
        .await
    }

    // -----------------------------------------------------------------------
    // L4 Agora (Phase 8)
    // -----------------------------------------------------------------------

    /// `POST /v1/tirami/agora/register` — Register an agent in the Agora
    /// marketplace.
    ///
    /// - `tier` — `"small"` | `"medium"` | `"large"` | `"frontier"`.
    pub async fn agora_register(
        &self,
        agent_hex: &str,
        models_served: &[&str],
        trm_per_token: u64,
        tier: &str,
        last_seen_ms: Option<u64>,
    ) -> Result<serde_json::Value, SdkError> {
        let mut body = serde_json::json!({
            "agent_hex": agent_hex,
            "models_served": models_served,
            "trm_per_token": trm_per_token,
            "tier": tier,
        });
        if let Some(ts) = last_seen_ms {
            body["last_seen_ms"] = serde_json::json!(ts);
        }
        self.post("/v1/tirami/agora/register", &body).await
    }

    /// `GET /v1/tirami/agora/agents` — List all registered AgentProfiles.
    pub async fn agora_list_agents(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/agora/agents").await
    }

    /// `GET /v1/tirami/agora/reputation/{hex}` — Reputation score for a specific
    /// agent.
    pub async fn agora_reputation(&self, agent_hex: &str) -> Result<serde_json::Value, SdkError> {
        self.get(&format!("/v1/tirami/agora/reputation/{}", agent_hex))
            .await
    }

    /// `POST /v1/tirami/agora/find` — Find agents matching model patterns and
    /// optional filters.
    pub async fn agora_find(
        &self,
        model_patterns: &[&str],
        max_trm_per_token: Option<u64>,
        tier: Option<&str>,
        min_reputation: Option<f64>,
    ) -> Result<serde_json::Value, SdkError> {
        let mut body = serde_json::json!({"model_patterns": model_patterns});
        if let Some(m) = max_trm_per_token {
            body["max_trm_per_token"] = serde_json::json!(m);
        }
        if let Some(t) = tier {
            body["tier"] = serde_json::json!(t);
        }
        if let Some(r) = min_reputation {
            body["min_reputation"] = serde_json::json!(r);
        }
        self.post("/v1/tirami/agora/find", &body).await
    }

    /// `GET /v1/tirami/agora/stats` — Agora registry statistics.
    pub async fn agora_stats(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/agora/stats").await
    }

    /// `GET /v1/tirami/agora/snapshot` — Export a RegistrySnapshot for backup or
    /// migration.
    pub async fn agora_snapshot(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/agora/snapshot").await
    }

    /// `POST /v1/tirami/agora/restore` — Restore the Agora registry from a
    /// RegistrySnapshot.
    pub async fn agora_restore(
        &self,
        snapshot: &serde_json::Value,
    ) -> Result<serde_json::Value, SdkError> {
        self.post("/v1/tirami/agora/restore", snapshot).await
    }

    // -----------------------------------------------------------------------
    // L3 Mind (Phase 8)
    // -----------------------------------------------------------------------

    /// `POST /v1/tirami/mind/init` — Initialise the TiramiMindAgent.
    ///
    /// - `optimizer` — `"echo"` | `"prompt_rewrite"` | `"cu_paid"`.
    pub async fn mind_init(
        &self,
        system_prompt: &str,
        optimizer: &str,
        api_url: Option<&str>,
        api_key: Option<&str>,
        model: Option<&str>,
    ) -> Result<serde_json::Value, SdkError> {
        let mut body =
            serde_json::json!({"system_prompt": system_prompt, "optimizer": optimizer});
        if let Some(u) = api_url {
            body["api_url"] = serde_json::json!(u);
        }
        if let Some(k) = api_key {
            body["api_key"] = serde_json::json!(k);
        }
        if let Some(m) = model {
            body["model"] = serde_json::json!(m);
        }
        self.post("/v1/tirami/mind/init", &body).await
    }

    /// `GET /v1/tirami/mind/state` — Current TiramiMindAgent state.
    pub async fn mind_state(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/mind/state").await
    }

    /// `POST /v1/tirami/mind/improve` — Run N self-improvement cycles.
    pub async fn mind_improve(&self, n_cycles: usize) -> Result<serde_json::Value, SdkError> {
        self.post(
            "/v1/tirami/mind/improve",
            &serde_json::json!({"n_cycles": n_cycles}),
        )
        .await
    }

    /// `POST /v1/tirami/mind/budget` — Update TiramiMindAgent CU budget limits.
    /// Omit a field to leave it unchanged.
    pub async fn mind_budget(
        &self,
        max_trm_per_cycle: Option<u64>,
        max_trm_per_day: Option<u64>,
        max_cycles_per_day: Option<u64>,
    ) -> Result<serde_json::Value, SdkError> {
        let mut body = serde_json::json!({});
        if let Some(v) = max_trm_per_cycle {
            body["max_trm_per_cycle"] = serde_json::json!(v);
        }
        if let Some(v) = max_trm_per_day {
            body["max_trm_per_day"] = serde_json::json!(v);
        }
        if let Some(v) = max_cycles_per_day {
            body["max_cycles_per_day"] = serde_json::json!(v);
        }
        self.post("/v1/tirami/mind/budget", &body).await
    }

    /// `GET /v1/tirami/mind/stats` — TiramiMindAgent lifetime stats.
    pub async fn mind_stats(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/mind/stats").await
    }

    // -----------------------------------------------------------------------
    // AgentNet
    // -----------------------------------------------------------------------

    /// `GET /v1/agentnet/feed` — Social feed for AI agents.
    pub async fn agentnet_feed(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/agentnet/feed").await
    }

    /// `POST /v1/agentnet/post` — Post a message to the AgentNet feed.
    pub async fn agentnet_post(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, SdkError> {
        self.post("/v1/agentnet/post", body).await
    }

    /// `POST /v1/agentnet/profile` — Upsert an agent profile.
    pub async fn agentnet_upsert_profile(
        &self,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, SdkError> {
        self.post("/v1/agentnet/profile", body).await
    }

    /// `GET /v1/agentnet/discover` — Discover agents on the network.
    pub async fn agentnet_discover(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/agentnet/discover").await
    }

    /// `GET /v1/agentnet/leaderboard` — Agent reputation leaderboard.
    pub async fn agentnet_leaderboard(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/agentnet/leaderboard").await
    }

    // -----------------------------------------------------------------------
    // Governance
    // -----------------------------------------------------------------------

    /// `POST /v1/tirami/governance/propose` — Create a governance proposal.
    pub async fn governance_propose(
        &self,
        proposer: &str,
        kind: &str,
        name: Option<&str>,
        new_value: Option<f64>,
        description: Option<&str>,
        deadline_ms: u64,
    ) -> Result<serde_json::Value, SdkError> {
        let mut body = serde_json::json!({
            "proposer": proposer,
            "kind": kind,
            "deadline_ms": deadline_ms,
        });
        if let Some(n) = name {
            body["name"] = serde_json::json!(n);
        }
        if let Some(v) = new_value {
            body["new_value"] = serde_json::json!(v);
        }
        if let Some(d) = description {
            body["description"] = serde_json::json!(d);
        }
        self.post("/v1/tirami/governance/propose", &body).await
    }

    /// `POST /v1/tirami/governance/vote` — Cast a vote on a governance proposal.
    pub async fn governance_vote(
        &self,
        voter: &str,
        proposal_id: u64,
        approve: bool,
        stake: f64,
        reputation: f64,
        epochs_participated: u64,
    ) -> Result<serde_json::Value, SdkError> {
        self.post(
            "/v1/tirami/governance/vote",
            &serde_json::json!({
                "voter": voter,
                "proposal_id": proposal_id,
                "approve": approve,
                "stake": stake,
                "reputation": reputation,
                "epochs_participated": epochs_participated,
            }),
        )
        .await
    }

    /// `GET /v1/tirami/governance/proposals` — List active governance proposals.
    pub async fn governance_proposals(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/governance/proposals").await
    }

    /// `GET /v1/tirami/governance/tally/{id}` — Get tally result for a governance
    /// proposal.
    pub async fn governance_tally(&self, proposal_id: u64) -> Result<serde_json::Value, SdkError> {
        self.get(&format!("/v1/tirami/governance/tally/{}", proposal_id))
            .await
    }

    // -----------------------------------------------------------------------
    // Observability / Admin
    // -----------------------------------------------------------------------

    /// `GET /metrics` — Prometheus metrics export (no auth required).
    pub async fn metrics(&self) -> Result<String, SdkError> {
        let url = format!("{}/metrics", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.text().await?)
    }

    /// `GET /v1/tirami/anchor?network=X` — Bitcoin OP_RETURN anchor status.
    pub async fn anchor(&self, network: &str) -> Result<serde_json::Value, SdkError> {
        self.get(&format!("/v1/tirami/anchor?network={}", network))
            .await
    }

    /// `GET /v1/tirami/collusion/{hex}` — Collusion resistance report for a peer.
    pub async fn collusion(&self, hex: &str) -> Result<serde_json::Value, SdkError> {
        self.get(&format!("/v1/tirami/collusion/{}", hex)).await
    }

    /// `GET /v1/tirami/reputation-gossip-status` — Reputation gossip debug info.
    pub async fn reputation_gossip_status(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/tirami/reputation-gossip-status").await
    }

    /// `POST /v1/tirami/admin/save-state` — Trigger manual ledger state
    /// persistence.
    pub async fn save_state(&self) -> Result<serde_json::Value, SdkError> {
        self.post("/v1/tirami/admin/save-state", &serde_json::json!({}))
            .await
    }

    /// `GET /status` — Node health, market price, recent trades.
    pub async fn status(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/status").await
    }

    /// `GET /health` — Lightweight liveness check (no auth required).
    pub async fn health(&self) -> Result<serde_json::Value, SdkError> {
        let url = format!("{}/health", self.base_url);
        let resp = self.client.get(&url).send().await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(SdkError::Api { status, message: body });
        }
        Ok(resp.json().await?)
    }

    /// `GET /topology` — Model manifest and peer capabilities.
    pub async fn topology(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/topology").await
    }

    /// `POST /v1/tirami/policy` — Set a budget policy on the safety controller.
    pub async fn set_policy(&self, body: &serde_json::Value) -> Result<serde_json::Value, SdkError> {
        self.post("/v1/tirami/policy", body).await
    }

    /// `GET /v1/models` — List loaded models (OpenAI-compatible).
    pub async fn models(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/v1/models").await
    }
}

// ---------------------------------------------------------------------------
// Autonomous agent helper
// ---------------------------------------------------------------------------

/// An autonomous agent that manages its own compute budget.
///
/// Wraps a `TiramiClient` and exposes high-level budget-aware methods.
///
/// ```rust,no_run
/// use tirami_sdk::TiramiAgent;
///
/// #[tokio::main]
/// async fn main() {
///     let mut agent = TiramiAgent::new("http://127.0.0.1:3000", None, 500, 100);
///     while agent.has_budget().await {
///         if let Ok(Some(result)) = agent.think("What should I do next?", 256).await {
///             println!("{:?}", result);
///         } else {
///             break;
///         }
///     }
/// }
/// ```
pub struct TiramiAgent {
    pub client: TiramiClient,
    pub max_cu_per_task: u64,
    pub min_balance: i64,
    pub total_spent: u64,
}

impl TiramiAgent {
    /// Create a new autonomous agent.
    pub fn new(
        base_url: &str,
        token: Option<&str>,
        max_cu_per_task: u64,
        min_balance: i64,
    ) -> Self {
        Self {
            client: TiramiClient::new(base_url, token),
            max_cu_per_task,
            min_balance,
            total_spent: 0,
        }
    }

    /// Returns `true` if the node's effective balance exceeds `min_balance`.
    pub async fn has_budget(&self) -> bool {
        match self.client.balance().await {
            Ok(b) => b.effective_balance > self.min_balance,
            Err(_) => false,
        }
    }

    /// Run inference if within budget. Returns `None` if the node can't afford
    /// `max_tokens` output tokens.
    pub async fn think(
        &mut self,
        prompt: &str,
        max_tokens: u32,
    ) -> Result<Option<ChatCompletion>, crate::error::SdkError> {
        if !self.client.can_afford(max_tokens as u64).await? {
            return Ok(None);
        }
        let messages = vec![serde_json::json!({"role": "user", "content": prompt})];
        let result = self.client.chat("", &messages, max_tokens).await?;
        if let Some(ref forge) = result.x_tirami {
            self.total_spent += forge.trm_cost;
        }
        if self.total_spent >= self.max_cu_per_task {
            return Ok(None);
        }
        Ok(Some(result))
    }

    /// Borrow CU if the agent's balance is insufficient for an upcoming task.
    ///
    /// Returns `None` if existing balance is sufficient.  Returns an error if
    /// credit score is below 0.2.
    pub async fn borrow_for_task(
        &self,
        needed_cu: u64,
        term_hours: u64,
    ) -> Result<Option<serde_json::Value>, crate::error::SdkError> {
        let balance = self.client.balance().await?;
        if balance.effective_balance as u64 >= needed_cu {
            return Ok(None);
        }
        // Verify credit score before attempting to borrow
        let credit = self.client.credit().await?;
        let score = credit
            .get("score")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        if score < 0.2 {
            return Err(crate::error::SdkError::Api {
                status: 403,
                message: format!("credit score {score:.2} too low to borrow (minimum 0.2)"),
            });
        }
        let shortfall = needed_cu.saturating_sub(balance.effective_balance.max(0) as u64);
        let collateral = (shortfall / 3).max(1);
        let loan = self
            .client
            .borrow(shortfall, term_hours, collateral, None)
            .await?;
        Ok(Some(loan))
    }

    /// Get this agent's economic status summary.
    pub async fn agent_status(&self) -> Result<serde_json::Value, crate::error::SdkError> {
        let balance = self.client.balance().await?;
        Ok(serde_json::json!({
            "balance": balance.effective_balance,
            "total_spent_this_session": self.total_spent,
            "budget_remaining": self.max_cu_per_task.saturating_sub(self.total_spent),
            "reputation": balance.reputation,
        }))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_new_trims_trailing_slash() {
        let c = TiramiClient::new("http://localhost:3000/", None);
        assert_eq!(c.base_url, "http://localhost:3000");
    }

    #[test]
    fn test_client_new_preserves_token() {
        let c = TiramiClient::new("http://localhost:3000", Some("tok"));
        assert_eq!(c.token, Some("tok".to_string()));
    }

    #[test]
    fn test_client_no_token() {
        let c = TiramiClient::new("http://localhost:3000", None);
        assert!(c.token.is_none());
    }

    #[test]
    fn test_balance_response_deserializes() {
        let json = r#"{"node_id":"0000","contributed":21,"consumed":0,"reserved":0,"net_balance":21,"effective_balance":1021,"reputation":0.5}"#;
        let b: Balance = serde_json::from_str(json).unwrap();
        assert_eq!(b.contributed, 21);
        assert_eq!(b.effective_balance, 1021);
        assert!((b.reputation - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pricing_response_deserializes() {
        let json = r#"{"trm_per_token":1.0,"supply_factor":1.0,"demand_factor":1.0,"cu_purchasing_power":1.0,"deflation_factor":1.0,"total_trades_ever":0,"estimated_cost_100_tokens":100,"estimated_cost_1000_tokens":1000}"#;
        let p: Pricing = serde_json::from_str(json).unwrap();
        assert_eq!(p.total_trades_ever, 0);
        assert_eq!(p.estimated_cost_100_tokens, 100);
        assert_eq!(p.estimated_cost_1000_tokens, 1000);
    }

    #[test]
    fn test_pricing_response_deserializes_without_optional_field() {
        // cu_purchasing_power has #[serde(default)] so it can be absent
        let json = r#"{"trm_per_token":2.0,"supply_factor":0.9,"demand_factor":1.1,"deflation_factor":0.99,"total_trades_ever":42,"estimated_cost_100_tokens":200,"estimated_cost_1000_tokens":2000}"#;
        let p: Pricing = serde_json::from_str(json).unwrap();
        assert_eq!(p.total_trades_ever, 42);
        assert!((p.cu_purchasing_power - 0.0).abs() < f64::EPSILON); // default
    }

    #[test]
    fn test_chat_completion_deserializes() {
        let json = r#"{"id":"chatcmpl-x","object":"chat.completion","created":0,"model":"smollm2","choices":[],"usage":{},"x_tirami":{"trm_cost":5,"effective_balance":1005}}"#;
        let c: ChatCompletion = serde_json::from_str(json).unwrap();
        assert_eq!(c.id, "chatcmpl-x");
        let forge = c.x_tirami.unwrap();
        assert_eq!(forge.trm_cost, 5);
        assert_eq!(forge.effective_balance, 1005);
    }

    #[test]
    fn test_chat_completion_without_x_tirami() {
        let json = r#"{"id":"chatcmpl-y","object":"chat.completion","created":0,"model":"llama","choices":[],"usage":{}}"#;
        let c: ChatCompletion = serde_json::from_str(json).unwrap();
        assert!(c.x_tirami.is_none());
    }

    #[test]
    fn test_sdk_error_display_api() {
        let e = SdkError::Api {
            status: 404,
            message: "not found".into(),
        };
        let s = e.to_string();
        assert!(s.contains("404"));
        assert!(s.contains("not found"));
    }

    #[test]
    fn test_sdk_error_display_json() {
        let inner: serde_json::Error = serde_json::from_str::<Balance>("bad").unwrap_err();
        let e = SdkError::Json(inner);
        assert!(e.to_string().contains("JSON"));
    }

    #[tokio::test]
    async fn test_governance_propose_builds_request() {
        let c = TiramiClient::new("http://localhost:3000", Some("tok"));
        // Will fail to connect, but verifies the method compiles and accepts args
        let res = c
            .governance_propose("abc123", "parameter_change", Some("base_rate"), Some(0.05), Some("lower base rate"), 9999)
            .await;
        assert!(res.is_err()); // no server running
    }

    #[tokio::test]
    async fn test_governance_vote_builds_request() {
        let c = TiramiClient::new("http://localhost:3000", Some("tok"));
        let res = c
            .governance_vote("abc123", 1, true, 100.0, 0.8, 5)
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_governance_proposals_builds_request() {
        let c = TiramiClient::new("http://localhost:3000", Some("tok"));
        let res = c.governance_proposals().await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_governance_tally_builds_request() {
        let c = TiramiClient::new("http://localhost:3000", Some("tok"));
        let res = c.governance_tally(42).await;
        assert!(res.is_err());
    }

    #[test]
    fn test_forge_agent_new() {
        let agent = TiramiAgent::new("http://127.0.0.1:3000", Some("secret"), 1000, 50);
        assert_eq!(agent.max_cu_per_task, 1000);
        assert_eq!(agent.min_balance, 50);
        assert_eq!(agent.total_spent, 0);
        assert_eq!(agent.client.token, Some("secret".to_string()));
    }
}
