//! Forge MCP Server — Rust implementation
//!
//! Exposes all 40 Forge compute-economy endpoints as MCP tools for Claude Code,
//! Cursor, and other MCP-compatible AI clients.
//!
//! Usage:
//!   FORGE_URL=http://127.0.0.1:3000 FORGE_API_TOKEN=<token> forge-mcp
//!
//! Claude Code / Cursor configuration:
//! ```json
//! {
//!   "mcpServers": {
//!     "forge": {
//!       "command": "forge-mcp",
//!       "env": {
//!         "FORGE_URL": "http://127.0.0.1:3000",
//!         "FORGE_API_TOKEN": "my-token"
//!       }
//!     }
//!   }
//! }
//! ```

mod tools;

use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    transport::io::stdio,
};
use serde_json::Value;
use std::sync::Arc;
use tracing::info;

/// Shared HTTP client state.
#[derive(Clone)]
struct TiramiClient {
    base_url: String,
    token: Option<String>,
    http: reqwest::Client,
}

impl TiramiClient {
    fn new(base_url: String, token: Option<String>) -> Self {
        Self {
            base_url,
            token,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client build failed"),
        }
    }

    fn auth_header(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        if let Some(tok) = &self.token {
            if !tok.is_empty() {
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    format!("Bearer {tok}").parse().unwrap(),
                );
            }
        }
        headers
    }

    async fn get(&self, path: &str) -> Result<Value, String> {
        let url = format!("{}{}", self.base_url, path);
        self.http
            .get(&url)
            .headers(self.auth_header())
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<Value>()
            .await
            .map_err(|e| e.to_string())
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value, String> {
        self.post_with_header(path, body, "", "").await
    }

    /// Phase 14.3 — POST with an additional header (e.g. X-Tirami-Node-Id).
    /// Pass `""` for name or value to skip.
    async fn post_with_header(
        &self,
        path: &str,
        body: Value,
        header_name: &str,
        header_value: &str,
    ) -> Result<Value, String> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.post(&url).headers(self.auth_header()).json(&body);
        if !header_name.is_empty() && !header_value.is_empty() {
            req = req.header(header_name, header_value);
        }
        req.send()
            .await
            .map_err(|e| e.to_string())?
            .json::<Value>()
            .await
            .map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// MCP Server handler
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ForgeMcpServer {
    client: Arc<TiramiClient>,
}

impl ForgeMcpServer {
    pub fn new(base_url: String, token: Option<String>) -> Self {
        Self {
            client: Arc::new(TiramiClient::new(base_url, token)),
        }
    }

    fn text_result(text: String) -> CallToolResult {
        CallToolResult::success(vec![Content::text(text)])
    }

    fn error_result(msg: String) -> CallToolResult {
        CallToolResult::error(vec![Content::text(msg)])
    }

    async fn dispatch(&self, name: &str, args: Value) -> CallToolResult {
        let obj = args.as_object().cloned().unwrap_or_default();

        let result: Result<Value, String> = match name {
            // ----------------------------------------------------------------
            // Economy (6 tools)
            // ----------------------------------------------------------------
            "forge_balance" => self.client.get("/v1/tirami/balance").await,
            "forge_pricing" => self.client.get("/v1/tirami/pricing").await,
            "forge_trades" => {
                let limit = obj.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);
                self.client
                    .get(&format!("/v1/tirami/trades?limit={limit}"))
                    .await
            }
            "tirami_network" => self.client.get("/v1/tirami/network").await,
            "forge_providers" => self.client.get("/v1/tirami/providers").await,
            "tirami_inference" => {
                let prompt = obj
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let max_tokens = obj.get("max_tokens").and_then(|v| v.as_i64()).unwrap_or(256);
                self.client
                    .post(
                        "/v1/chat/completions",
                        serde_json::json!({
                            "messages": [{"role": "user", "content": prompt}],
                            "max_tokens": max_tokens
                        }),
                    )
                    .await
            }

            // ----------------------------------------------------------------
            // Phase 14.1 / 14.2 — PeerRegistry + unified scheduling
            // ----------------------------------------------------------------
            "tirami_peers" => self.client.get("/v1/tirami/peers").await,
            "tirami_schedule" => {
                let model_id = obj
                    .get("model_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let max_tokens = obj.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(256);
                let mut body = serde_json::json!({
                    "model_id": model_id,
                    "max_tokens": max_tokens,
                });
                if let Some(consumer) = obj.get("consumer").and_then(|v| v.as_str()) {
                    body["consumer"] = serde_json::Value::String(consumer.to_string());
                }
                self.client.post("/v1/tirami/schedule", body).await
            }
            "tirami_anchors" => self.client.get("/v1/tirami/anchors").await,
            "tirami_chat_as" => {
                let consumer_hex = obj
                    .get("consumer_hex")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let prompt = obj
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let max_tokens = obj.get("max_tokens").and_then(|v| v.as_i64()).unwrap_or(256);
                self.client
                    .post_with_header(
                        "/v1/chat/completions",
                        serde_json::json!({
                            "messages": [{"role": "user", "content": prompt}],
                            "max_tokens": max_tokens,
                        }),
                        "X-Tirami-Node-Id",
                        &consumer_hex,
                    )
                    .await
            }

            // ----------------------------------------------------------------
            // Safety (2 tools)
            // ----------------------------------------------------------------
            "forge_safety" => self.client.get("/v1/tirami/safety").await,
            "forge_kill_switch" => {
                let activate = obj
                    .get("activate")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let reason = obj
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.client
                    .post(
                        "/v1/tirami/kill",
                        serde_json::json!({
                            "activate": activate,
                            "reason": reason,
                            "operator": "mcp-agent"
                        }),
                    )
                    .await
            }

            // ----------------------------------------------------------------
            // Settlement (2 tools)
            // ----------------------------------------------------------------
            "forge_invoice" => {
                let trm_amount = obj
                    .get("trm_amount")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                self.client
                    .post("/v1/tirami/invoice", serde_json::json!({"trm_amount": trm_amount}))
                    .await
            }
            "forge_route" => {
                let mut params: Vec<String> = Vec::new();
                if let Some(m) = obj.get("model").and_then(|v| v.as_str()) {
                    params.push(format!("model={m}"));
                }
                if let Some(mc) = obj.get("max_cu").and_then(|v| v.as_i64()) {
                    params.push(format!("max_cu={mc}"));
                }
                if let Some(mode) = obj.get("mode").and_then(|v| v.as_str()) {
                    params.push(format!("mode={mode}"));
                }
                if let Some(mt) = obj.get("max_tokens").and_then(|v| v.as_i64()) {
                    params.push(format!("max_tokens={mt}"));
                }
                let path = if params.is_empty() {
                    "/v1/tirami/route".to_string()
                } else {
                    format!("/v1/tirami/route?{}", params.join("&"))
                };
                self.client.get(&path).await
            }

            // ----------------------------------------------------------------
            // Lending (7 tools)
            // ----------------------------------------------------------------
            "forge_lend" => self.client.post("/v1/tirami/lend", args).await,
            "forge_borrow" => self.client.post("/v1/tirami/borrow", args).await,
            "forge_repay" => self.client.post("/v1/tirami/repay", args).await,
            "forge_credit" => self.client.get("/v1/tirami/credit").await,
            "forge_pool" => self.client.get("/v1/tirami/pool").await,
            "forge_loans" => self.client.get("/v1/tirami/loans").await,
            "forge_lend_to" => self.client.post("/v1/tirami/lend-to", args).await,

            // ----------------------------------------------------------------
            // Bank L2 (8 tools)
            // ----------------------------------------------------------------
            "tirami_bank_portfolio" => self.client.get("/v1/tirami/bank/portfolio").await,
            "tirami_bank_tick" => {
                self.client
                    .post("/v1/tirami/bank/tick", serde_json::json!({}))
                    .await
            }
            "tirami_bank_set_strategy" => {
                let strategy = obj
                    .get("strategy")
                    .and_then(|v| v.as_str())
                    .unwrap_or("balanced")
                    .to_string();
                let mut body = serde_json::json!({"strategy": strategy});
                if let Some(f) = obj.get("base_commit_fraction") {
                    body["base_commit_fraction"] = f.clone();
                }
                self.client.post("/v1/tirami/bank/strategy", body).await
            }
            "tirami_bank_set_risk" => {
                let tolerance = obj
                    .get("tolerance")
                    .and_then(|v| v.as_str())
                    .unwrap_or("balanced")
                    .to_string();
                self.client
                    .post("/v1/tirami/bank/risk", serde_json::json!({"tolerance": tolerance}))
                    .await
            }
            "tirami_bank_list_futures" => self.client.get("/v1/tirami/bank/futures").await,
            "tirami_bank_create_future" => {
                self.client.post("/v1/tirami/bank/futures", args).await
            }
            "tirami_bank_risk_assessment" => {
                self.client.get("/v1/tirami/bank/risk-assessment").await
            }
            "tirami_bank_optimize" => {
                self.client.post("/v1/tirami/bank/optimize", args).await
            }

            // ----------------------------------------------------------------
            // Agora L4 (7 tools)
            // ----------------------------------------------------------------
            "tirami_agora_register" => {
                self.client.post("/v1/tirami/agora/register", args).await
            }
            "tirami_agora_list_agents" => self.client.get("/v1/tirami/agora/agents").await,
            "tirami_agora_reputation" => {
                let hex = obj
                    .get("agent_hex")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.client
                    .get(&format!("/v1/tirami/agora/reputation/{hex}"))
                    .await
            }
            "tirami_agora_find" => self.client.post("/v1/tirami/agora/find", args).await,
            "tirami_agora_stats" => self.client.get("/v1/tirami/agora/stats").await,
            "tirami_agora_snapshot" => self.client.get("/v1/tirami/agora/snapshot").await,
            "tirami_agora_restore" => {
                let snapshot = obj
                    .get("snapshot")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                self.client
                    .post("/v1/tirami/agora/restore", snapshot)
                    .await
            }

            // ----------------------------------------------------------------
            // Mind L3 (5 tools)
            // ----------------------------------------------------------------
            "tirami_mind_init" => self.client.post("/v1/tirami/mind/init", args).await,
            "tirami_mind_state" => self.client.get("/v1/tirami/mind/state").await,
            "tirami_mind_improve" => {
                let n_cycles = obj.get("n_cycles").and_then(|v| v.as_i64()).unwrap_or(1);
                self.client
                    .post(
                        "/v1/tirami/mind/improve",
                        serde_json::json!({"n_cycles": n_cycles}),
                    )
                    .await
            }
            "tirami_mind_budget" => self.client.post("/v1/tirami/mind/budget", args).await,
            "tirami_mind_stats" => self.client.get("/v1/tirami/mind/stats").await,

            // ----------------------------------------------------------------
            // Governance (4 tools)
            // ----------------------------------------------------------------
            "tirami_governance_propose" => {
                let mut body = serde_json::json!({
                    "proposer": obj.get("proposer").and_then(|v| v.as_str()).unwrap_or(""),
                    "kind": obj.get("kind").and_then(|v| v.as_str()).unwrap_or(""),
                    "deadline_ms": obj.get("deadline_ms").and_then(|v| v.as_u64()).unwrap_or(0),
                });
                if let Some(n) = obj.get("name") {
                    body["name"] = n.clone();
                }
                if let Some(v) = obj.get("new_value") {
                    body["new_value"] = v.clone();
                }
                if let Some(d) = obj.get("description") {
                    body["description"] = d.clone();
                }
                self.client
                    .post("/v1/tirami/governance/propose", body)
                    .await
            }
            "tirami_governance_vote" => {
                self.client.post("/v1/tirami/governance/vote", args).await
            }
            "tirami_governance_proposals" => {
                self.client.get("/v1/tirami/governance/proposals").await
            }
            "tirami_governance_tally" => {
                let id = obj
                    .get("proposal_id")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                self.client
                    .get(&format!("/v1/tirami/governance/tally/{id}"))
                    .await
            }

            other => {
                return Self::error_result(format!("Unknown tool: {other}"));
            }
        };

        match result {
            Ok(v) => Self::text_result(
                serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string()),
            ),
            Err(e) => Self::error_result(format!("Error: {e}")),
        }
    }
}

impl ServerHandler for ForgeMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("forge-mcp", env!("CARGO_PKG_VERSION"))
            )
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async move {
            Ok(ListToolsResult::with_all_items(tools::build_tool_list()))
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move {
            let args = request
                .arguments
                .map(|a| Value::Object(a))
                .unwrap_or(serde_json::json!({}));
            Ok(self.dispatch(&request.name, args).await)
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    let base_url =
        std::env::var("FORGE_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
    let token = std::env::var("FORGE_API_TOKEN").ok();

    info!(
        "forge-mcp starting, FORGE_URL={base_url}, auth={}",
        token.as_deref().map(|_| "yes").unwrap_or("no")
    );

    let server = ForgeMcpServer::new(base_url, token);
    let transport = stdio();
    server
        .serve(transport)
        .await
        .inspect_err(|e| eprintln!("forge-mcp error: {e}"))
        .ok();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::tools;

    #[test]
    fn test_tool_list_has_44_tools() {
        // Phase 15 added 3 (peers/schedule/chat_as); Phase 16 added tirami_anchors.
        let tools = tools::build_tool_list();
        assert_eq!(tools.len(), 44, "expected 44 tools, got {}", tools.len());
    }

    #[test]
    fn test_phase_15_16_tools_present() {
        let tools = tools::build_tool_list();
        let names: std::collections::HashSet<String> = tools
            .iter()
            .map(|t| t.name.as_ref().to_string())
            .collect();
        assert!(names.contains("tirami_peers"), "tirami_peers missing");
        assert!(names.contains("tirami_schedule"), "tirami_schedule missing");
        assert!(names.contains("tirami_chat_as"), "tirami_chat_as missing");
        assert!(names.contains("tirami_anchors"), "tirami_anchors missing");
    }

    #[test]
    fn test_tool_names_are_unique() {
        let tools = tools::build_tool_list();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_ref().to_string()).collect();
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(
            names.len(),
            unique.len(),
            "duplicate tool names detected"
        );
    }

    #[test]
    fn test_forge_balance_has_empty_input_schema() {
        let tools = tools::build_tool_list();
        let balance = tools
            .iter()
            .find(|t| t.name == "forge_balance")
            .expect("forge_balance tool not found");
        // input_schema must be an object with empty or no "properties"
        let schema = serde_json::Value::Object(balance.input_schema.as_ref().clone());
        let props = schema
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|o| o.len())
            .unwrap_or(0);
        assert_eq!(props, 0, "forge_balance should have empty properties");
    }

    #[test]
    fn test_all_tools_have_descriptions() {
        let tools = tools::build_tool_list();
        for tool in &tools {
            assert!(
                tool.description.is_some(),
                "tool {} has no description",
                tool.name
            );
        }
    }

    #[test]
    fn test_tool_groups_coverage() {
        let tools = tools::build_tool_list();
        let names: std::collections::HashSet<_> =
            tools.iter().map(|t| t.name.as_ref()).collect();

        // Economy
        for n in &[
            "forge_balance",
            "forge_pricing",
            "forge_trades",
            "tirami_network",
            "forge_providers",
            "tirami_inference",
        ] {
            assert!(names.contains(*n), "missing economy tool: {n}");
        }
        // Safety
        for n in &["forge_safety", "forge_kill_switch"] {
            assert!(names.contains(*n), "missing safety tool: {n}");
        }
        // Settlement
        for n in &["forge_invoice", "forge_route"] {
            assert!(names.contains(*n), "missing settlement tool: {n}");
        }
        // Lending
        for n in &[
            "forge_lend",
            "forge_borrow",
            "forge_repay",
            "forge_credit",
            "forge_pool",
            "forge_loans",
        ] {
            assert!(names.contains(*n), "missing lending tool: {n}");
        }
        // Bank L2
        for n in &[
            "tirami_bank_portfolio",
            "tirami_bank_tick",
            "tirami_bank_set_strategy",
            "tirami_bank_set_risk",
            "tirami_bank_list_futures",
            "tirami_bank_create_future",
            "tirami_bank_risk_assessment",
            "tirami_bank_optimize",
        ] {
            assert!(names.contains(*n), "missing bank tool: {n}");
        }
        // Agora L4
        for n in &[
            "tirami_agora_register",
            "tirami_agora_list_agents",
            "tirami_agora_reputation",
            "tirami_agora_find",
            "tirami_agora_stats",
            "tirami_agora_snapshot",
            "tirami_agora_restore",
        ] {
            assert!(names.contains(*n), "missing agora tool: {n}");
        }
        // Mind L3
        for n in &[
            "tirami_mind_init",
            "tirami_mind_state",
            "tirami_mind_improve",
            "tirami_mind_budget",
            "tirami_mind_stats",
        ] {
            assert!(names.contains(*n), "missing mind tool: {n}");
        }
        // Governance
        for n in &[
            "tirami_governance_propose",
            "tirami_governance_vote",
            "tirami_governance_proposals",
            "tirami_governance_tally",
        ] {
            assert!(names.contains(*n), "missing governance tool: {n}");
        }
    }
}
