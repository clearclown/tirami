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

    /// Issue #156 — optional path to the persisted staking-pool
    /// snapshot. When `Some(_)`, `TiramiNode::new` loads the pool at
    /// startup, and every `/v1/tirami/su/stake`, `/su/unstake`, plus
    /// the slashing loop persist after mutation. `None` keeps the
    /// pre-#156 behaviour (in-memory only — operator loses locked
    /// TRM on restart).
    #[serde(default)]
    pub staking_state_path: Option<PathBuf>,

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

    /// Phase 25 C4 — per-node gossip dedup capacity. Operators
    /// running global-scale nodes should raise this to keep the
    /// dedup horizon long enough for sustained TPS; resource-
    /// constrained edge nodes can lower it. Default 100,000
    /// matches the historical hardcoded const.
    #[serde(default = "default_gossip_max_seen")]
    pub gossip_max_seen: usize,

    /// Phase 25 C2 — global cap on concurrent in-flight
    /// `/v1/chat/completions` requests. Excess requests get
    /// HTTP 429 + `Retry-After`. 0 disables the cap (legacy
    /// behaviour). Default 64 protects against client-side
    /// runaway loops without bottlenecking honest agents.
    #[serde(default = "default_chat_concurrency_cap")]
    pub chat_concurrency_cap: u32,

    /// #163 — whether this node will fork a llama.cpp `rpc-server` when a
    /// peer asks it to (`Payload::StartRpcServer`).
    ///
    /// Default **`false`**. Before this flag the receive handler honoured
    /// the request from any connected peer, on any port ≥ 1024, with no
    /// authorization check at all — `Payload::TradeProposal` verifies the
    /// sender, this did not. Spawning a process on request is not something
    /// a node should do because someone asked nicely, so it is opt-in.
    ///
    /// Turn it on for machines you intend to contribute to a model split.
    #[serde(default)]
    pub rpc_server_enabled: bool,
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

fn default_gossip_max_seen() -> usize {
    100_000
}

fn default_chat_concurrency_cap() -> u32 {
    64
}

fn default_personal_agent_enabled() -> bool {
    true
}

/// Conventional file name for operator overrides inside a data directory.
///
/// `docs/operator-guide.md`, `docs/phase-14-design.md`, and
/// `docs/deployments/global-scale-kubernetes.md` have documented this file
/// since Phase 14; until #162 nothing actually read it.
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// Every `Config` field name, as it appears in a TOML document.
///
/// Used only to tell an operator that a key they wrote was not recognised.
/// `Config` carries a container-level `#[serde(default)]`, so an unknown or
/// misspelled key would otherwise deserialize silently to its default — the
/// operator sees a setting in their file and gets the opposite behaviour,
/// which is the exact failure mode #162 was reported for.
///
/// Kept honest by `known_fields_matches_struct`, which fails if a field is
/// added to `Config` without being listed here.
const KNOWN_FIELDS: &[&str] = &[
    "model_path",
    "node_key_path",
    "ledger_path",
    "bank_state_path",
    "marketplace_state_path",
    "staking_state_path",
    "mind_state_path",
    "personal_agent_state_path",
    "agent_identity_path",
    "agent_identity_passphrase_env",
    "share_compute",
    "max_memory_gb",
    "api_port",
    "api_bind_addr",
    "api_bearer_token",
    "api_max_request_body_bytes",
    "stake_gate_enabled",
    "bootstrap_relays",
    "p2p_bind_addr",
    "bootstrap_peers",
    "region",
    "max_prompt_chars",
    "max_generate_tokens",
    "max_concurrent_remote_inference_requests",
    "settlement_window_hours",
    "anchor_interval_secs",
    "slashing_interval_secs",
    "welcome_loan_settle_interval_secs",
    "pq_signatures",
    "asn_rate_limit_enabled",
    "max_concurrent_connections",
    "checkpoint_interval_secs",
    "checkpoint_retain_secs",
    "archive_path",
    "proof_policy",
    "agent_tick_interval_secs",
    "personal_agent_enabled",
    "zkml_backend",
    "metrics_require_bearer",
    "max_slashes_per_tick",
    "gossip_max_seen",
    "chat_concurrency_cap",
    "rpc_server_enabled",
];

impl Config {
    /// Path of the operator override file for a given data directory.
    pub fn config_file_path(data_dir: impl Into<PathBuf>) -> PathBuf {
        data_dir.into().join(CONFIG_FILE_NAME)
    }

    /// Build a config from a TOML document, rooted at `data_dir`.
    ///
    /// Keys absent from the document keep their default, and the durable
    /// state paths are re-derived from `data_dir` afterwards so an operator
    /// override file never has to restate them.
    ///
    /// Returns the config together with any keys that were not recognised,
    /// so the caller can warn instead of silently discarding them.
    pub fn from_toml_str(
        doc: &str,
        data_dir: impl Into<PathBuf>,
    ) -> Result<(Self, Vec<String>), crate::TiramiError> {
        let table: toml::Table = doc
            .parse()
            .map_err(|e| crate::TiramiError::Config(format!("invalid TOML: {e}")))?;

        let unknown: Vec<String> = table
            .keys()
            .filter(|key| !KNOWN_FIELDS.contains(&key.as_str()))
            .cloned()
            .collect();

        let mut config: Config = table
            .try_into()
            .map_err(|e| crate::TiramiError::Config(format!("invalid config value: {e}")))?;
        config.set_data_dir(data_dir);

        Ok((config, unknown))
    }

    /// Load `<data_dir>/config.toml` if it exists.
    ///
    /// A missing file is not an error — it is the normal case for a node
    /// that has never been configured by hand. A file that exists but
    /// cannot be read or parsed *is* an error: silently falling back to
    /// defaults would hide the operator's intent.
    pub fn load_from_data_dir(
        data_dir: impl Into<PathBuf>,
    ) -> Result<(Self, Vec<String>), crate::TiramiError> {
        let data_dir = data_dir.into();
        let path = Self::config_file_path(&data_dir);
        match std::fs::read_to_string(&path) {
            Ok(doc) => Self::from_toml_str(&doc, data_dir),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok((Self::for_data_dir(data_dir), Vec::new()))
            }
            Err(e) => Err(crate::TiramiError::Config(format!(
                "cannot read {}: {e}",
                path.display()
            ))),
        }
    }

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
        self.staking_state_path = Some(data_dir.join("staking.json"));
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
            staking_state_path: None,
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
            gossip_max_seen: 100_000,
            chat_concurrency_cap: 64,
            rpc_server_enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, KNOWN_FIELDS};
    use std::path::PathBuf;

    /// `KNOWN_FIELDS` drives the unknown-key warning. If it drifts from the
    /// struct, an operator's real setting gets reported as a typo (or a real
    /// typo goes unreported), so pin the two together.
    #[test]
    fn known_fields_matches_struct() {
        // Serialize a Config with every `Option` populated — TOML has no
        // null, so a `None` field would simply not appear as a key.
        let mut config = Config::for_data_dir("/tmp/known-fields");
        config.model_path = Some(PathBuf::from("/tmp/m.gguf"));
        config.agent_identity_path = Some(PathBuf::from("/tmp/id.json"));
        config.api_bearer_token = Some("t".to_string());
        config.p2p_bind_addr = Some("0.0.0.0:7700".to_string());

        let table = toml::Table::try_from(&config).expect("Config serializes to TOML");

        let mut actual: Vec<&str> = table.keys().map(String::as_str).collect();
        let mut listed: Vec<&str> = KNOWN_FIELDS.to_vec();
        actual.sort_unstable();
        listed.sort_unstable();

        assert_eq!(
            actual, listed,
            "KNOWN_FIELDS is out of sync with `struct Config`"
        );
    }

    #[test]
    fn toml_override_applies_and_keeps_data_dir_paths() {
        let (config, unknown) =
            Config::from_toml_str("stake_gate_enabled = false\napi_port = 8080\n", "/tmp/cfg")
                .expect("valid TOML");

        assert!(unknown.is_empty());
        assert!(!config.stake_gate_enabled, "override must be applied");
        assert_eq!(config.api_port, 8080);
        // Unmentioned keys keep their default...
        assert_eq!(config.api_bind_addr, "127.0.0.1");
        // ...and durable paths are still derived from the data dir.
        assert_eq!(
            config.ledger_path,
            Some(PathBuf::from("/tmp/cfg/ledger.json"))
        );
    }

    #[test]
    fn toml_reports_unrecognised_keys() {
        // A plausible typo: the real field is `stake_gate_enabled`.
        let (config, unknown) =
            Config::from_toml_str("stake_gate_enable = false\n", "/tmp/cfg").expect("valid TOML");

        assert_eq!(unknown, vec!["stake_gate_enable".to_string()]);
        assert!(
            config.stake_gate_enabled,
            "a typo must not silently disable the gate"
        );
    }

    #[test]
    fn toml_parse_failure_is_an_error() {
        assert!(Config::from_toml_str("this is not toml", "/tmp/cfg").is_err());
        assert!(
            Config::from_toml_str("api_port = \"not a number\"", "/tmp/cfg").is_err(),
            "a wrongly-typed value must not fall back to the default"
        );
    }

    #[test]
    fn missing_config_file_is_not_an_error() {
        let (config, unknown) = Config::load_from_data_dir("/tmp/tirami-no-such-dir-162")
            .expect("absent config.toml is the normal case");

        assert!(unknown.is_empty());
        assert!(config.stake_gate_enabled, "default is on");
    }

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
