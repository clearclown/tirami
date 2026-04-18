use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Path to the local GGUF model file.
    pub model_path: Option<PathBuf>,

    /// Optional path to a persisted ledger snapshot.
    pub ledger_path: Option<PathBuf>,

    /// Optional path to the persisted forge-bank (L2) state.
    pub bank_state_path: Option<PathBuf>,

    /// Optional path to the persisted forge-agora (L4) marketplace state.
    pub marketplace_state_path: Option<PathBuf>,

    /// Optional path to the persisted forge-mind (L3) agent snapshot.
    pub mind_state_path: Option<PathBuf>,

    /// Whether to share compute with the network.
    pub share_compute: bool,

    /// Maximum memory (GB) to dedicate to inference.
    pub max_memory_gb: f32,

    /// Port for the local HTTP API.
    pub api_port: u16,

    /// Bind address for the local HTTP API.
    pub api_bind_addr: String,

    /// Optional bearer token protecting administrative API endpoints.
    pub api_bearer_token: Option<String>,

    /// Maximum accepted HTTP request body size for the local API.
    pub api_max_request_body_bytes: usize,

    /// Bootstrap relay addresses for WAN discovery.
    pub bootstrap_relays: Vec<String>,

    /// Region hint for peer discovery.
    pub region: String,

    /// Maximum accepted prompt length for API and remote inference requests.
    pub max_prompt_chars: usize,

    /// Maximum number of tokens a single request may ask the runtime to generate.
    pub max_generate_tokens: u32,

    /// Maximum number of concurrent remote inference requests the seed will execute.
    pub max_concurrent_remote_inference_requests: usize,

    /// Settlement window duration in hours (Issue #19). 0 = manual only.
    pub settlement_window_hours: u64,

    /// Phase 16 — interval between on-chain anchor batches (seconds).
    /// Default 3600 (60 min). §20 spec recommends 10 min for production
    /// (600), but dev/test defaults err on the longer side.
    #[serde(default = "default_anchor_interval_secs")]
    pub anchor_interval_secs: u64,

    /// Phase 17 Wave 1.3 — interval between slashing sweeps (seconds).
    /// Default 300 (5 min). Clamped to ≥60 at spawn time to bound CPU
    /// on large trade logs. Operators running dev clusters can shorten
    /// via config; production should leave at the default.
    #[serde(default = "default_slashing_interval_secs")]
    pub slashing_interval_secs: u64,

    /// Phase 17 Wave 1.6 — opt-in to post-quantum hybrid signatures
    /// (Ed25519 + ML-DSA). When `true`, the node signs outbound trades
    /// with both halves and rejects inbound trades whose PQ half fails.
    /// When `false` (current default), the PQ machinery stays dormant
    /// and every signature is pure Ed25519, preserving interop with
    /// pre-Phase-17 peers.
    ///
    /// The default stays `false` until the ML-DSA dep can be pulled in
    /// without dependency conflicts (currently blocked on
    /// `digest 0.11.0-rc.10` via iroh 0.97). The scaffold + mock
    /// verifier are in tirami-core::crypto.
    #[serde(default)]
    pub pq_signatures: bool,

    /// Phase 17 Wave 2.3 — opt into per-ASN rate limiting on inbound
    /// traffic. When `true`, the transport consults
    /// `tirami_net::asn_rate_limit::AsnRateLimiter` so a cloud-Sybil
    /// that spins many IPs inside one ASN shares a single 5 000 msg/s
    /// bucket instead of one-per-peer. Requires an IP→ASN resolver;
    /// see `tirami-net::asn_rate_limit` for options (StaticAsnResolver
    /// for tests, future MaxMind GeoLite2-ASN reader for production).
    /// Default `false` so operators without the DB are unaffected.
    #[serde(default)]
    pub asn_rate_limit_enabled: bool,

    /// Phase 17 Wave 3.4 — DDoS mitigation: maximum concurrent peer
    /// connections the transport will accept before dropping new
    /// handshakes. Default 1 000 — well above what a healthy private
    /// mesh needs, tight enough that a public node can't be coerced
    /// into fd exhaustion by a flood attacker.
    ///
    /// Set to `0` to disable the cap (unbounded). Do NOT do this on
    /// any node reachable from the public internet; see
    /// `docs/operator-guide.md#ddos-mitigation` for why.
    #[serde(default = "default_max_concurrent_connections")]
    pub max_concurrent_connections: u32,

    /// Phase 17 Wave 4.3 — interval between trade-log seal passes
    /// (seconds). Each pass calls `ComputeLedger::seal_and_archive`
    /// with `cutoff = now - checkpoint_retain_secs` so trades older
    /// than the retain window move from memory to the archive file.
    /// Default 3600 s (1 hour). Clamped to ≥ 60 s at spawn time.
    #[serde(default = "default_checkpoint_interval_secs")]
    pub checkpoint_interval_secs: u64,

    /// Phase 17 Wave 4.3 — how long trades are retained in the
    /// in-memory `trade_log` before being sealed into the archive.
    /// Default 86 400 s (24 h). Operators who need longer online
    /// windows for /v1/tirami/trades can raise this at the cost of
    /// memory.
    #[serde(default = "default_checkpoint_retain_secs")]
    pub checkpoint_retain_secs: u64,

    /// Phase 17 Wave 4.3 — filesystem path for the JSON-lines
    /// archive. `None` disables archival writes (the seal pass
    /// still prunes in-memory, but historical trades are lost —
    /// only acceptable for dev nodes).
    #[serde(default)]
    pub archive_path: Option<std::path::PathBuf>,

    /// Phase 18.3 — zkML rollout gate. See
    /// `tirami_ledger::zk::ProofPolicy`. Stored as a string here
    /// to avoid circular dependencies between tirami-core and
    /// tirami-ledger. Valid values: "disabled" (default),
    /// "optional", "recommended", "required".
    ///
    /// The network-wide value is Constitutionally ratcheted: once
    /// set to "required", it cannot be downgraded by governance.
    /// Individual operators may run ahead of the network (e.g.
    /// require proofs locally while the network is still
    /// "optional"), but not behind.
    #[serde(default = "default_proof_policy")]
    pub proof_policy: String,

    /// Phase 18.5-part-2 — interval (seconds) between PersonalAgent
    /// tick-loop fires. Default 30 s. Clamped to ≥1 at spawn time.
    /// Shorter values give snappier auto-earn/auto-spend response at
    /// the cost of more frequent ledger reads; longer values reduce
    /// load on quiet nodes.
    #[serde(default = "default_agent_tick_interval_secs")]
    pub agent_tick_interval_secs: u64,

    /// Phase 18.5-part-3e — auto-configure a [`PersonalAgent`] at
    /// `run_seed` time using the local node identity as the wallet.
    /// Default `true` so `tirami start` immediately gives the user
    /// a working agent (the killer-app commitment from
    /// `docs/killer-app.md`). Operators running a pure-server node
    /// can set this to `false` (CLI: `tirami start --no-agent`).
    #[serde(default = "default_personal_agent_enabled")]
    pub personal_agent_enabled: bool,
}

fn default_anchor_interval_secs() -> u64 {
    3600
}

fn default_slashing_interval_secs() -> u64 {
    300
}

fn default_max_concurrent_connections() -> u32 {
    1_000
}

fn default_checkpoint_interval_secs() -> u64 {
    3_600
}

fn default_checkpoint_retain_secs() -> u64 {
    24 * 3_600
}

fn default_proof_policy() -> String {
    "disabled".to_string()
}

fn default_agent_tick_interval_secs() -> u64 {
    30
}

fn default_personal_agent_enabled() -> bool {
    true
}

impl Config {
    pub fn api_socket_addr(&self) -> String {
        format!("{}:{}", self.api_bind_addr, self.api_port)
    }

    pub fn validate_inference_request(
        &self,
        prompt: &str,
        max_tokens: u32,
        temperature: f32,
        top_p: Option<f32>,
    ) -> Result<(), crate::TiramiError> {
        let prompt_chars = prompt.chars().count();
        if prompt_chars == 0 {
            return Err(crate::TiramiError::InvalidRequest(
                "prompt must not be empty".to_string(),
            ));
        }
        if prompt_chars > self.max_prompt_chars {
            return Err(crate::TiramiError::InvalidRequest(format!(
                "prompt too large: {prompt_chars} chars > limit {}",
                self.max_prompt_chars
            )));
        }
        if max_tokens == 0 {
            return Err(crate::TiramiError::InvalidRequest(
                "max_tokens must be greater than zero".to_string(),
            ));
        }
        if max_tokens > self.max_generate_tokens {
            return Err(crate::TiramiError::InvalidRequest(format!(
                "max_tokens too large: {max_tokens} > limit {}",
                self.max_generate_tokens
            )));
        }
        if !temperature.is_finite() || !(0.0..=2.0).contains(&temperature) {
            return Err(crate::TiramiError::InvalidRequest(
                "temperature must be finite and within 0.0..=2.0".to_string(),
            ));
        }
        if let Some(top_p) = top_p {
            if !top_p.is_finite() || !(0.0..=1.0).contains(&top_p) || top_p == 0.0 {
                return Err(crate::TiramiError::InvalidRequest(
                    "top_p must be finite and within (0.0, 1.0]".to_string(),
                ));
            }
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model_path: None,
            ledger_path: None,
            bank_state_path: None,
            marketplace_state_path: None,
            mind_state_path: None,
            share_compute: false,
            max_memory_gb: 4.0,
            api_port: 3000,
            api_bind_addr: "127.0.0.1".to_string(),
            api_bearer_token: None,
            api_max_request_body_bytes: 64 * 1024,
            bootstrap_relays: vec![],
            region: "unknown".to_string(),
            max_prompt_chars: 8_192,
            max_generate_tokens: 1_024,
            max_concurrent_remote_inference_requests: 4,
            settlement_window_hours: 24,
            anchor_interval_secs: 3600,
            slashing_interval_secs: 300,
            pq_signatures: false,
            asn_rate_limit_enabled: false,
            max_concurrent_connections: 1_000,
            checkpoint_interval_secs: 3_600,
            checkpoint_retain_secs: 24 * 3_600,
            archive_path: None,
            proof_policy: "disabled".to_string(),
            agent_tick_interval_secs: 30,
            personal_agent_enabled: true,
        }
    }
}
