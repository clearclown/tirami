//! Tool definitions for the Forge MCP server.
//!
//! Every tool corresponds exactly to a Python MCP tool from `forge-mcp-server.py`.
//! Descriptions are agent-oriented — they explain *why* an agent would call the tool
//! rather than just labelling the endpoint.

use rmcp::model::Tool;
use serde_json::json;
use std::sync::Arc;

fn schema(value: serde_json::Value) -> Arc<rmcp::model::JsonObject> {
    Arc::new(
        value
            .as_object()
            .cloned()
            .unwrap_or_default(),
    )
}

/// Build the complete list of 40 Forge MCP tools.
pub fn build_tool_list() -> Vec<Tool> {
    vec![
        // ====================================================================
        // Economy (6 tools)
        // ====================================================================
        Tool::new(
            "forge_balance",
            "Check your CU (Compute Unit) balance. Returns contributed, consumed, reserved, \
             effective balance, and reputation score. Call this before any large inference \
             request to verify you have sufficient CU.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "forge_pricing",
            "Get current market price for inference. Returns CU per token, supply/demand \
             factors, and cost estimates for 100 and 1000 tokens. Call this before inference \
             to budget accurately.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "forge_trades",
            "View recent trade history. Each trade shows provider, consumer, CU amount, tokens \
             processed, and model used. Useful for auditing spend or verifying that a provider \
             delivered the work.",
            schema(json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Max trades to return (default 20)"
                    }
                }
            })),
        ),
        Tool::new(
            "tirami_network",
            "Get mesh network economic summary: total nodes, CU flow, trade count, average \
             reputation, and Merkle root (Bitcoin-anchorable). Use this to assess overall \
             network health before committing to large workloads.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "forge_providers",
            "List available compute providers ranked by reputation and cost. Use this to choose \
             the best provider for your task or to verify provider availability before routing.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "tirami_inference",
            "Run LLM inference and pay with CU. Returns the model's response plus CU cost. \
             Use forge_pricing first to estimate cost and forge_balance to confirm you have \
             enough CU.",
            schema(json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "The question or prompt to send"
                    },
                    "max_tokens": {
                        "type": "integer",
                        "description": "Maximum tokens to generate (default 256)"
                    }
                },
                "required": ["prompt"]
            })),
        ),

        // ====================================================================
        // Phase 14.1 / 14.2 — PeerRegistry + unified scheduling
        // ====================================================================
        Tool::new(
            "tirami_peers",
            "List peers known to the local node's PeerRegistry (Phase 14.1). Each entry \
             contains price_multiplier, available_cu, audit_tier, latency_ema_ms, and \
             advertised models. Use this to inspect the current market state across the \
             mesh before scheduling work.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "tirami_schedule",
            "Phase 14.2 Ledger-as-Brain probe: given model_id + max_tokens, return which \
             provider would be chosen and the estimated TRM cost. Read-only — does NOT \
             reserve TRM. Agents use this to comparison-shop before committing to inference.",
            schema(json!({
                "type": "object",
                "properties": {
                    "model_id": {
                        "type": "string",
                        "description": "Model identifier (e.g. \"qwen2.5-0.5b-instruct-q4_k_m\")"
                    },
                    "max_tokens": {
                        "type": "integer",
                        "description": "Token budget for the hypothetical request"
                    },
                    "consumer": {
                        "type": "string",
                        "description": "Optional 64-char hex NodeId of the consumer. If omitted, the local node acts as consumer."
                    }
                },
                "required": ["model_id", "max_tokens"]
            })),
        ),
        Tool::new(
            "tirami_chat_as",
            "Phase 14.3 — Run inference billed to a specific consumer NodeId instead of \
             the anonymous default. Sets the X-Tirami-Node-Id header so the resulting \
             bilateral trade is properly attributed. Required for cross-node AI agent \
             economies.",
            schema(json!({
                "type": "object",
                "properties": {
                    "consumer_hex": {
                        "type": "string",
                        "description": "64-char hex NodeId to bill the inference to"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "Prompt text"
                    },
                    "max_tokens": {
                        "type": "integer",
                        "description": "Maximum output tokens"
                    }
                },
                "required": ["consumer_hex", "prompt", "max_tokens"]
            })),
        ),

        // ====================================================================
        // Safety (2 tools)
        // ====================================================================
        Tool::new(
            "forge_safety",
            "Check safety status: kill switch state, circuit breaker, budget policy, spend \
             velocity. Call this to confirm the node is operating normally before starting \
             an autonomous spending loop.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "forge_kill_switch",
            "EMERGENCY: Activate or deactivate the kill switch. When active, ALL CU \
             transactions are frozen. Use only in emergencies such as runaway spend or \
             compromised credentials.",
            schema(json!({
                "type": "object",
                "properties": {
                    "activate": {
                        "type": "boolean",
                        "description": "true to freeze all transactions, false to resume"
                    },
                    "reason": {
                        "type": "string",
                        "description": "Human-readable reason for the action"
                    }
                },
                "required": ["activate"]
            })),
        ),

        // ====================================================================
        // Settlement (2 tools)
        // ====================================================================
        Tool::new(
            "forge_invoice",
            "Create a Lightning invoice to convert CU earnings to Bitcoin. Specify the CU \
             amount to cash out. The invoice is valid for 24 hours.",
            schema(json!({
                "type": "object",
                "properties": {
                    "trm_amount": {
                        "type": "integer",
                        "description": "CU amount to convert to sats"
                    }
                },
                "required": ["trm_amount"]
            })),
        ),
        Tool::new(
            "forge_route",
            "Find the optimal inference provider for an upcoming request. Use mode='cost' for \
             cheapest, 'quality' for highest reputation, or 'balanced' (default) for the best \
             price-quality tradeoff.",
            schema(json!({
                "type": "object",
                "properties": {
                    "model": {
                        "type": "string",
                        "description": "Optional model identifier"
                    },
                    "max_cu": {
                        "type": "integer",
                        "description": "Maximum CU budget"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["cost", "quality", "balanced"],
                        "description": "Optimization mode (default: balanced)"
                    },
                    "max_tokens": {
                        "type": "integer",
                        "description": "Expected output length (default 1000)"
                    }
                }
            })),
        ),

        // ====================================================================
        // Lending (7 tools)
        // ====================================================================
        Tool::new(
            "forge_lend",
            "Contribute idle CU to the lending pool to earn interest from borrowers. The CU \
             is reserved (cannot be spent) until withdrawn or borrowed.",
            schema(json!({
                "type": "object",
                "properties": {
                    "amount": {
                        "type": "integer",
                        "description": "CU to contribute to the pool"
                    },
                    "max_term_hours": {
                        "type": "integer",
                        "description": "Maximum loan term you will accept (default 168)"
                    },
                    "min_interest_rate": {
                        "type": "number",
                        "description": "Minimum interest rate per hour (default 0.0)"
                    }
                },
                "required": ["amount"]
            })),
        ),
        Tool::new(
            "forge_borrow",
            "Request a CU loan from the Forge lending pool. Use this when the agent's CU \
             balance is insufficient for an upcoming task. The loan will accrue interest based \
             on the borrower's credit score (0.1%-0.6% per hour). Default 3:1 collateral \
             required.",
            schema(json!({
                "type": "object",
                "properties": {
                    "amount": {
                        "type": "integer",
                        "description": "Principal CU to borrow"
                    },
                    "term_hours": {
                        "type": "integer",
                        "description": "Loan duration in hours (max 168 = 7 days)"
                    },
                    "collateral": {
                        "type": "integer",
                        "description": "CU to lock as collateral (must be >= amount/3)"
                    }
                },
                "required": ["amount", "term_hours", "collateral"]
            })),
        ),
        Tool::new(
            "forge_repay",
            "Repay an outstanding CU loan. Provide the loan_id returned from forge_borrow. \
             The collateral is released and the lender receives principal + interest.",
            schema(json!({
                "type": "object",
                "properties": {
                    "loan_id": {
                        "type": "string",
                        "description": "Hex-encoded loan_id (64 chars)"
                    }
                },
                "required": ["loan_id"]
            })),
        ),
        Tool::new(
            "forge_credit",
            "Get this node's credit score (0.0-1.0). New nodes start at 0.3. Score is \
             computed from trade history (30%), repayment history (40%), uptime (20%), and \
             account age (10%). Higher scores unlock lower interest rates.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "forge_pool",
            "View the lending pool status: total CU, lent CU, available CU, reserve ratio, \
             your maximum borrow capacity, and your offered interest rate based on your credit \
             score.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "forge_loans",
            "List all active loans where this node is either lender or borrower, with their \
             status, principal, interest rate, and due date.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        // ====================================================================
        // Bank L2 (8 tools)
        // ====================================================================
        Tool::new(
            "tirami_bank_portfolio",
            "Get the L2 bank PortfolioManager state (cash_trm, lent_cu, borrowed_cu, \
             net_exposure_cu, position_count). Use this before deciding whether to lend or \
             borrow more CU.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "tirami_bank_tick",
            "Run one PortfolioManager.tick() cycle using the current pool snapshot from the \
             ledger. Returns the Decisions produced (Lend/Borrow/Hold). Call this to let the \
             strategy auto-manage the portfolio.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "tirami_bank_set_strategy",
            "Hot-swap the portfolio strategy without losing current positions. Strategies: \
             'conservative' (lends small fraction, avoids risk), 'highyield' (maximises \
             lending income), 'balanced' (default middle ground).",
            schema(json!({
                "type": "object",
                "properties": {
                    "strategy": {
                        "type": "string",
                        "enum": ["conservative", "highyield", "balanced"],
                        "description": "Portfolio strategy name"
                    },
                    "base_commit_fraction": {
                        "type": "number",
                        "description": "Fraction of cash to commit per tick (0, 1]. Default 0.5."
                    }
                },
                "required": ["strategy"]
            })),
        ),
        Tool::new(
            "tirami_bank_set_risk",
            "Set the risk tolerance that gates portfolio decisions. Conservative: only lend \
             to high-credit peers. Balanced: moderate defaults. Aggressive: maximise yield \
             even with riskier loans.",
            schema(json!({
                "type": "object",
                "properties": {
                    "tolerance": {
                        "type": "string",
                        "enum": ["conservative", "balanced", "aggressive"],
                        "description": "Risk tolerance level"
                    }
                },
                "required": ["tolerance"]
            })),
        ),
        Tool::new(
            "tirami_bank_list_futures",
            "List all active CU futures contracts in the bank. A futures contract locks in a \
             CU price between two parties for a future date, enabling hedging against CU price \
             volatility.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "tirami_bank_create_future",
            "Create a new CU futures contract with a counterparty. Requires the counterparty \
             NodeId (hex), notional CU amount, strike price in msats, and expiry timestamp. \
             The default margin is 10% of notional.",
            schema(json!({
                "type": "object",
                "properties": {
                    "counterparty_hex": {
                        "type": "string",
                        "description": "64-char hex NodeId of the counterparty"
                    },
                    "notional_trm": {
                        "type": "integer",
                        "description": "Notional CU amount of the contract"
                    },
                    "strike_price_msats": {
                        "type": "integer",
                        "description": "Agreed strike price in millisatoshis per CU"
                    },
                    "expires_at_ms": {
                        "type": "integer",
                        "description": "Contract expiry as Unix milliseconds"
                    },
                    "margin_fraction": {
                        "type": "number",
                        "description": "Margin as fraction of notional (0, 1]. Default 0.10."
                    }
                },
                "required": [
                    "counterparty_hex",
                    "notional_trm",
                    "strike_price_msats",
                    "expires_at_ms"
                ]
            })),
        ),
        Tool::new(
            "tirami_bank_risk_assessment",
            "Get a full risk assessment of the current portfolio: portfolio_value_cu, \
             var_99_cu (99% Value-at-Risk), concentration_score, and leverage_ratio. Use \
             before large lending decisions.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "tirami_bank_optimize",
            "Run the YieldOptimizer against the current pool snapshot with a VaR budget. If \
             the optimizer finds a better allocation within the risk limit, it applies it. \
             Returns applied (bool), decisions, and a human-readable rationale.",
            schema(json!({
                "type": "object",
                "properties": {
                    "max_var_99_cu": {
                        "type": "integer",
                        "description": "Maximum allowed 99% Value-at-Risk in CU"
                    }
                },
                "required": ["max_var_99_cu"]
            })),
        ),

        // ====================================================================
        // Agora L4 (7 tools)
        // ====================================================================
        Tool::new(
            "tirami_agora_register",
            "Register this node (or any agent) in the Agora marketplace so other agents can \
             discover it for inference routing. Provide the agent's models, CU price, and \
             capability tier.",
            schema(json!({
                "type": "object",
                "properties": {
                    "agent_hex": {
                        "type": "string",
                        "description": "64-char hex NodeId of the agent"
                    },
                    "models_served": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "List of model identifiers served"
                    },
                    "trm_per_token": {
                        "type": "integer",
                        "description": "Price in CU per output token"
                    },
                    "tier": {
                        "type": "string",
                        "enum": ["small", "medium", "large", "frontier"],
                        "description": "Capability tier"
                    },
                    "last_seen_ms": {
                        "type": "integer",
                        "description": "Optional last-seen timestamp in Unix milliseconds"
                    }
                },
                "required": ["agent_hex", "models_served", "trm_per_token", "tier"]
            })),
        ),
        Tool::new(
            "tirami_agora_list_agents",
            "List all registered AgentProfiles in the Agora marketplace. Each profile \
             includes the NodeId, models served, CU price, tier, and last-seen timestamp.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "tirami_agora_reputation",
            "Get the ReputationScore for a specific agent by hex NodeId. Score components: \
             volume (trade count), recency (time since last trade), diversity (model variety), \
             consistency (fulfillment rate).",
            schema(json!({
                "type": "object",
                "properties": {
                    "agent_hex": {
                        "type": "string",
                        "description": "64-char hex NodeId of the agent"
                    }
                },
                "required": ["agent_hex"]
            })),
        ),
        Tool::new(
            "tirami_agora_find",
            "Find agents that match a set of model patterns and optional filters. Returns \
             ranked CapabilityMatch results. Use this for intelligent provider selection \
             beyond the basic /v1/tirami/providers list.",
            schema(json!({
                "type": "object",
                "properties": {
                    "model_patterns": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Glob-style model name patterns, e.g. ['qwen3-*', '*8b*']"
                    },
                    "max_trm_per_token": {
                        "type": "integer",
                        "description": "Maximum acceptable CU price per token"
                    },
                    "tier": {
                        "type": "string",
                        "enum": ["small", "medium", "large", "frontier"],
                        "description": "Required capability tier"
                    },
                    "min_reputation": {
                        "type": "number",
                        "description": "Minimum reputation score [0.0, 1.0]"
                    }
                },
                "required": ["model_patterns"]
            })),
        ),
        Tool::new(
            "tirami_agora_stats",
            "Get Agora registry statistics as a key→count map. Includes agent_count, \
             trade_count, and tier breakdowns. Useful for monitoring marketplace health.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "tirami_agora_snapshot",
            "Export a full RegistrySnapshot (all profiles and observed trades) for backup, \
             migration, or cross-node synchronisation.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "tirami_agora_restore",
            "Restore the Agora agent registry from a previously exported RegistrySnapshot. \
             Replaces all current registry state.",
            schema(json!({
                "type": "object",
                "properties": {
                    "snapshot": {
                        "type": "object",
                        "description": "RegistrySnapshot with 'profiles' and 'trades' arrays"
                    }
                },
                "required": ["snapshot"]
            })),
        ),

        // ====================================================================
        // Mind L3 (5 tools)
        // ====================================================================
        Tool::new(
            "tirami_mind_init",
            "Initialise the TiramiMindAgent with a system prompt and optimizer. The agent will \
             use this harness as the baseline for self-improvement cycles. Optimizers: 'echo' \
             (no-op, for testing), 'prompt_rewrite' (rule-based), 'cu_paid' (calls a frontier \
             LLM, costs CU).",
            schema(json!({
                "type": "object",
                "properties": {
                    "system_prompt": {
                        "type": "string",
                        "description": "Initial system prompt / harness to optimise"
                    },
                    "optimizer": {
                        "type": "string",
                        "enum": ["echo", "prompt_rewrite", "cu_paid"],
                        "description": "MetaOptimizer to use (default: echo)"
                    },
                    "api_url": {
                        "type": "string",
                        "description": "API base URL for cu_paid optimizer"
                    },
                    "api_key": {
                        "type": "string",
                        "description": "API key for cu_paid optimizer"
                    },
                    "model": {
                        "type": "string",
                        "description": "Model name for cu_paid optimizer (default: claude-sonnet-4-6)"
                    }
                },
                "required": ["system_prompt"]
            })),
        ),
        Tool::new(
            "tirami_mind_state",
            "Get the current TiramiMindAgent state: harness version number, first 80 chars of \
             the system prompt, cycle history length, and today's CU spend and cycle count \
             against the daily budget.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "tirami_mind_improve",
            "Run N self-improvement cycles. Each cycle has the MetaOptimizer propose a new \
             harness; if the benchmark score improves by more than min_score_delta and ROI >= \
             1.0, the new harness is kept. CU is deducted per proposal if a TrmPaidOptimizer \
             is configured.",
            schema(json!({
                "type": "object",
                "properties": {
                    "n_cycles": {
                        "type": "integer",
                        "description": "Number of improvement cycles to run (1-100, default 1)"
                    }
                }
            })),
        ),
        Tool::new(
            "tirami_mind_budget",
            "Update the TiramiMindAgent's CU budget limits. Omit any field to leave it \
             unchanged. Use this to tighten limits before a risky improvement run or loosen \
             them when more CU is available.",
            schema(json!({
                "type": "object",
                "properties": {
                    "max_trm_per_cycle": {
                        "type": "integer",
                        "description": "Maximum CU spent per improvement cycle"
                    },
                    "max_trm_per_day": {
                        "type": "integer",
                        "description": "Maximum total CU spent per day"
                    },
                    "max_cycles_per_day": {
                        "type": "integer",
                        "description": "Maximum improvement cycles per day"
                    }
                }
            })),
        ),
        Tool::new(
            "tirami_mind_stats",
            "Get TiramiMindAgent lifetime statistics: total cycles run, how many were kept vs \
             reverted vs deferred, overall score delta (improvement), and total CU invested \
             in self-improvement.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        // ====================================================================
        // Governance (4 tools)
        // ====================================================================
        Tool::new(
            "tirami_governance_propose",
            "Create a governance proposal to change a protocol parameter or policy. Specify \
             the proposer NodeId, proposal kind (e.g. 'parameter_change'), optional parameter \
             name and new value, a human-readable description, and a deadline in Unix \
             milliseconds.",
            schema(json!({
                "type": "object",
                "properties": {
                    "proposer": {
                        "type": "string",
                        "description": "64-char hex NodeId of the proposer"
                    },
                    "kind": {
                        "type": "string",
                        "description": "Proposal kind, e.g. 'parameter_change'"
                    },
                    "name": {
                        "type": "string",
                        "description": "Parameter name to change (optional)"
                    },
                    "new_value": {
                        "type": "number",
                        "description": "Proposed new value for the parameter (optional)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Human-readable description of the proposal (optional)"
                    },
                    "deadline_ms": {
                        "type": "integer",
                        "description": "Voting deadline as Unix milliseconds"
                    }
                },
                "required": ["proposer", "kind", "deadline_ms"]
            })),
        ),
        Tool::new(
            "tirami_governance_vote",
            "Cast a vote on an active governance proposal. The vote weight is derived from \
             stake, reputation, and participation history. Approve or reject the proposal.",
            schema(json!({
                "type": "object",
                "properties": {
                    "voter": {
                        "type": "string",
                        "description": "64-char hex NodeId of the voter"
                    },
                    "proposal_id": {
                        "type": "integer",
                        "description": "ID of the proposal to vote on"
                    },
                    "approve": {
                        "type": "boolean",
                        "description": "true to approve, false to reject"
                    },
                    "stake": {
                        "type": "number",
                        "description": "Voter's current CU stake"
                    },
                    "reputation": {
                        "type": "number",
                        "description": "Voter's reputation score [0.0, 1.0]"
                    },
                    "epochs_participated": {
                        "type": "integer",
                        "description": "Number of governance epochs the voter has participated in"
                    }
                },
                "required": ["voter", "proposal_id", "approve", "stake", "reputation", "epochs_participated"]
            })),
        ),
        Tool::new(
            "tirami_governance_proposals",
            "List all active governance proposals. Each proposal includes its ID, kind, \
             proposer, description, deadline, and current vote counts. Use this to discover \
             proposals before voting.",
            schema(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "tirami_governance_tally",
            "Get the tally result for a specific governance proposal by ID. Returns the \
             weighted approve/reject totals, quorum status, and whether the proposal has \
             passed or failed.",
            schema(json!({
                "type": "object",
                "properties": {
                    "proposal_id": {
                        "type": "integer",
                        "description": "ID of the proposal to tally"
                    }
                },
                "required": ["proposal_id"]
            })),
        ),
    ]
}
