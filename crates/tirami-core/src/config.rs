use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Path to the local GGUF model file.
    pub model_path: Option<PathBuf>,

    /// Optional path to the persisted P2P node identity secret key.
    ///
    /// The NodeId is derived from this Ed25519 key. If this is unset,
    /// the networking layer may create an ephemeral identity, which is
    /// only appropriate for tests and disposable dev sessions.
    pub node_key_path: Option<PathBuf>,

    /// Optional path to a persisted ledger snapshot.
    pub ledger_path: Option<PathBuf>,

    /// Optional path to the persisted forge-bank (L2) state.
    pub bank_state_path: Option<PathBuf>,

    /// Optional path to the persisted forge-agora (L4) marketplace state.
    pub marketplace_state_path: Option<PathBuf>,

    /// Optional path to the persisted forge-mind (L3) agent snapshot.
    pub mind_state_path: Option<PathBuf>,

    /// Optional path to the persisted user-facing PersonalAgent state.
    pub personal_agent_state_path: Option<PathBuf>,

    /// Phase 23 Wave 3 — optional path for the encrypted
    /// `AgentIdentity` bundle on disk. When `Some(_)` AND the env
    /// var named by [`Self::agent_identity_passphrase_env`] is set,
    /// `TiramiNode::new` auto-loads the identity at startup and
    /// `agent/identity/init` / `/import` write back to the same
    /// path after each mutation. When either side is missing the
    /// identity stays ephemeral (in-memory only).
    #[serde(default)]
    pub agent_identity_path: Option<PathBuf>,

    /// Phase 23 Wave 3 — name of the environment variable that
    /// carries the Argon2id passphrase for the persisted identity.
    /// Default: `"TIRAMI_AGENT_IDENTITY_PASSPHRASE"`. Operators
    /// who run multiple nodes on the same host can rebind this to
    /// per-node names.
    #[serde(default = "default_agent_identity_passphrase_env")]
    pub agent_identity_passphrase_env: String,

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

    /// Phase 21 Wave 1+2 — when `true`, refuse `/v1/chat/completions`
    /// requests unless the local node passes
    /// [`tirami_ledger::ComputeLedger::inference_eligibility`].
    /// Default **`true`** as of Wave 2: a fresh node can pass via
    /// the bootstrap window (≤ 10 TRM cumulative) or by claiming a
    /// welcome loan (`POST /v1/tirami/agent/claim-welcome`,
    /// 1 000 TRM × 72 h). Operators with custom flows that pre-
    /// inflate contribution past the cap without staking can set
    /// this to `false` explicitly.
    #[serde(default = "default_stake_gate_enabled")]
    pub stake_gate_enabled: bool,

    /// Bootstrap relay addresses for WAN discovery.
    pub bootstrap_relays: Vec<String>,

    /// Optional fixed P2P bind socket address for the iroh QUIC transport.
    ///
    /// Leave unset for an ephemeral port. Set this to something like
    /// `0.0.0.0:7700` when publishing direct bootstrap peers such as
    /// `PUBLIC_KEY@100.83.54.6:7700`.
    pub p2p_bind_addr: Option<String>,

    /// Public bootstrap peers to connect to on startup.
    ///
    /// Each entry is `PUBLIC_KEY`, `PUBLIC_KEY@RELAY_URL`, or
    /// `PUBLIC_KEY@IP:PORT`. Relays are Iroh relay URLs and are still
    /// encrypted end-to-end; direct IPs are useful for private WANs such
    /// as Tailscale.
    pub bootstrap_peers: Vec<String>,

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

    /// Phase 22 Wave 2 — interval between welcome-loan settlement
    /// sweeps (seconds). The sweep flips expired grants to either
    /// `repaid` (borrower had non-zero contributions during the
    /// 72-hour window) or `defaulted` (zero contributions; treated
    /// as a Sybil-like signal and appended to `slash_events`).
    /// Default 300 (5 min). Clamped to ≥60 at spawn time.
    #[serde(default = "default_welcome_settle_interval_secs")]
    pub welcome_loan_settle_interval_secs: u64,

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
    /// tirami-ledger. Valid values: `"disabled"`, `"optional"`
    /// (Phase 19 default), `"recommended"`, `"required"`.
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

    /// Phase 24 Wave 2 — which zkML backend to use when producing
    /// proofs for trade attestation. Stored as a kebab-case string
    /// (`"mock"`, `"ed-attest"`, `"ezkl"`, `"risc0"`, `"halo2"`) to
    /// keep tirami-core free of a tirami-zkml-bench dependency.
    /// Default `"mock"` matches `BenchBackendKind::default()`.
    #[serde(default = "default_zkml_backend")]
    pub zkml_backend: String,

    /// Phase 25 A3 — when `true`, `GET /metrics` requires the same
    /// bearer that `api_bearer_token` configures. Default `false`
    /// preserves the Prometheus-friendly default for private
    /// networks. Public-facing deployments should set this to
    /// `true` so node-internal economic state doesn't leak to
    /// scrapers without credentials.
    #[serde(default)]
    pub metrics_require_bearer: bool,

    /// Phase 25 C9 — maximum number of `SlashEvent`s the
    /// slashing engine is permitted to emit in a single tick.
    /// Defends against a logic bug or false-positive cluster that
    /// would otherwise drain the staking pool in one pass.
    /// Default 100 trades plenty of room for honest collusion
    /// detection while bounding worst-case damage per tick.
    #[serde(default = "default_max_slashes_per_tick")]
    pub max_slashes_per_tick: u32,
}

/// Phase 21 Wave 2 — stake gate is **on by default** so that fresh
/// deploys start enforcing Sybil resistance immediately. Operators
/// who want the pre-Wave-1 permissive behaviour can opt out by
/// setting `stake_gate_enabled = false`. See
/// `docs/phase-21-stake-enforcement.md` for the rationale.
fn default_stake_gate_enabled() -> bool {
    true
}

fn default_anchor_interval_secs() -> u64 {
    3600
}

/// Phase 22 Wave 2 — 5 min by default. Welcome-loan grants expire on
/// a 72-hour boundary; a 5-minute sweep means defaulted grants get
/// flagged within roughly 5 minutes of crossing the deadline.
fn default_welcome_settle_interval_secs() -> u64 {
    300
}

/// Phase 23 Wave 3 — environment-variable name carrying the
/// Argon2id passphrase for the persisted `AgentIdentity` bundle.
fn default_agent_identity_passphrase_env() -> String {
    "TIRAMI_AGENT_IDENTITY_PASSPHRASE".to_string()
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
    // Phase 19 / Tier C — promoted from "disabled" to "optional".
    // Nodes trade without proofs by default but proof-verified
    // trades get a reputation boost once an ezkl/risc0 backend is
    // wired in. Constitutional ratchet in
    // `tirami_ledger::zk::try_ratchet_proof_policy` prevents
    // downgrade, so future governance can only move UP to
    // `recommended` / `required` (the latter irreversibly).
    "optional".to_string()
}

fn default_agent_tick_interval_secs() -> u64 {
    30
}

fn default_zkml_backend() -> String {
    "mock".to_string()
}

fn default_max_slashes_per_tick() -> u32 {
    100
}

fn default_personal_agent_enabled() -> bool {
    true
}

impl Config {
    /// Build a production-oriented config rooted at `data_dir`.
    ///
    /// This wires all durable identity/economy state to predictable files:
    /// node key, ledger, L2 bank state, L4 marketplace, L3 mind snapshot,
    /// PersonalAgent state, and the append-only trade archive.
    pub fn for_data_dir(data_dir: impl Into<PathBuf>) -> Self {
        let mut config = Self::default();
        config.set_data_dir(data_dir);
        config
    }

    /// Set all durable state paths under `data_dir`.
    pub fn set_data_dir(&mut self, data_dir: impl Into<PathBuf>) {
        let data_dir = data_dir.into();
        self.node_key_path = Some(data_dir.join("node.key"));
        self.ledger_path = Some(data_dir.join("ledger.json"));
        self.bank_state_path = Some(data_dir.join("bank_state.json"));
        self.marketplace_state_path = Some(data_dir.join("marketplace_state.json"));
        self.mind_state_path = Some(data_dir.join("mind_state.json"));
        self.personal_agent_state_path = Some(data_dir.join("personal_agent.json"));
        self.archive_path = Some(data_dir.join("trades.jsonl"));
    }

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
            node_key_path: None,
            ledger_path: None,
            bank_state_path: None,
            marketplace_state_path: None,
            mind_state_path: None,
            personal_agent_state_path: None,
            agent_identity_path: None,
            agent_identity_passphrase_env: default_agent_identity_passphrase_env(),
            share_compute: false,
            max_memory_gb: 4.0,
            api_port: 3000,
            api_bind_addr: "127.0.0.1".to_string(),
            api_bearer_token: None,
            api_max_request_body_bytes: 64 * 1024,
            stake_gate_enabled: default_stake_gate_enabled(),
            bootstrap_relays: vec![],
            p2p_bind_addr: None,
            bootstrap_peers: vec![],
            region: "unknown".to_string(),
            max_prompt_chars: 8_192,
            max_generate_tokens: 1_024,
            max_concurrent_remote_inference_requests: 4,
            settlement_window_hours: 24,
            anchor_interval_secs: 3600,
            slashing_interval_secs: 300,
            welcome_loan_settle_interval_secs: 300,
            pq_signatures: false,
            asn_rate_limit_enabled: false,
            max_concurrent_connections: 1_000,
            checkpoint_interval_secs: 3_600,
            checkpoint_retain_secs: 24 * 3_600,
            archive_path: None,
            proof_policy: "optional".to_string(),
            agent_tick_interval_secs: 30,
            personal_agent_enabled: true,
            zkml_backend: "mock".to_string(),
            metrics_require_bearer: false,
            max_slashes_per_tick: 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;
    use std::path::PathBuf;

    #[test]
    fn for_data_dir_wires_all_durable_paths() {
        let config = Config::for_data_dir("/tmp/tirami-state");

        assert_eq!(
            config.node_key_path,
            Some(PathBuf::from("/tmp/tirami-state/node.key"))
        );
        assert_eq!(
            config.ledger_path,
            Some(PathBuf::from("/tmp/tirami-state/ledger.json"))
        );
        assert_eq!(
            config.bank_state_path,
            Some(PathBuf::from("/tmp/tirami-state/bank_state.json"))
        );
        assert_eq!(
            config.marketplace_state_path,
            Some(PathBuf::from("/tmp/tirami-state/marketplace_state.json"))
        );
        assert_eq!(
            config.mind_state_path,
            Some(PathBuf::from("/tmp/tirami-state/mind_state.json"))
        );
        assert_eq!(
            config.personal_agent_state_path,
            Some(PathBuf::from("/tmp/tirami-state/personal_agent.json"))
        );
        assert_eq!(
            config.archive_path,
            Some(PathBuf::from("/tmp/tirami-state/trades.jsonl"))
        );
    }
}
