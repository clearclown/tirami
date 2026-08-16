use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tirami_core::Config;

#[derive(Parser)]
#[command(name = "forged")]
#[command(about = "Forge daemon for serving and joining the encrypted compute network")]
struct DaemonCli {
    #[command(subcommand)]
    command: DaemonCommand,
}

#[derive(Subcommand)]
enum DaemonCommand {
    /// Start as a seed node (holds model, serves inference)
    Seed {
        /// Path to a GGUF model file
        #[arg(short, long)]
        model: String,

        /// Path to tokenizer.json file
        #[arg(short, long)]
        tokenizer: String,

        /// Port for the local HTTP API. Default 3000, or `api_port` from config.toml.
        #[arg(short, long)]
        port: Option<u16>,

        /// Bind address for the local HTTP API. Default 127.0.0.1, or `api_bind_addr` from config.toml.
        #[arg(long)]
        bind: Option<String>,

        /// Fixed P2P UDP bind socket for direct peers, e.g. 0.0.0.0:7700.
        #[arg(long)]
        p2p_bind: Option<String>,

        /// Path to the persisted ledger snapshot
        #[arg(long, default_value = "forge-ledger.json")]
        ledger: String,

        /// Optional bearer token protecting administrative HTTP API routes
        #[arg(long)]
        api_token: Option<String>,

        /// Disable the provider stake gate on this node.
        ///
        /// Also settable with `TIRAMI_STAKE_GATE=0` or
        /// `stake_gate_enabled = false` in config.toml. Intended for
        /// private fleets where every node has the same owner, so Sybil
        /// resistance buys nothing and the gate only stops the fleet
        /// once the welcome loan expires (#162).
        #[arg(long, default_value_t = false)]
        no_stake_gate: bool,

        /// Public bootstrap peer to join on startup. Repeatable.
        /// Format: PUBLIC_KEY, PUBLIC_KEY@RELAY_URL, or PUBLIC_KEY@IP:PORT.
        #[arg(long = "bootstrap-peer")]
        bootstrap_peers: Vec<String>,
    },

    /// Start a local API server (no P2P)
    Node {
        /// Path to a GGUF model file
        #[arg(short, long)]
        model: Option<String>,

        /// Path to tokenizer.json file
        #[arg(short, long)]
        tokenizer: Option<String>,

        /// Port for the local API. Default 3000, or `api_port` from config.toml.
        #[arg(short, long)]
        port: Option<u16>,

        /// Bind address for the local HTTP API. Default 127.0.0.1, or `api_bind_addr` from config.toml.
        #[arg(long)]
        bind: Option<String>,

        /// Path to the persisted ledger snapshot
        #[arg(long, default_value = "forge-ledger.json")]
        ledger: String,

        /// Optional bearer token protecting administrative HTTP API routes
        #[arg(long)]
        api_token: Option<String>,

        /// Disable the provider stake gate on this node.
        ///
        /// Also settable with `TIRAMI_STAKE_GATE=0` or
        /// `stake_gate_enabled = false` in config.toml. Intended for
        /// private fleets where every node has the same owner, so Sybil
        /// resistance buys nothing and the gate only stops the fleet
        /// once the welcome loan expires (#162).
        #[arg(long, default_value_t = false)]
        no_stake_gate: bool,
    },
}

/// Default tracing filter when `RUST_LOG` is unset (keep in sync with
/// `main.rs::DEFAULT_TRACING_FILTER`). Silences iroh's multicast and
/// IPv6-relay / address-set warnings that flood non-multicast,
/// Docker-heavy, or IPv4-only hosts (fix #75). `RUST_LOG=info` restores
/// the raw output.
const DEFAULT_TRACING_FILTER: &str = "info,swarm_discovery=error,iroh::socket::transports::relay=error,iroh::socket::remote_map::remote_state=error,iroh_relay=error,noq_udp=error";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(DEFAULT_TRACING_FILTER)),
        )
        .init();

    let cli = DaemonCli::parse();

    match cli.command {
        DaemonCommand::Seed {
            model,
            tokenizer,
            port,
            bind,
            p2p_bind,
            ledger,
            api_token,
            no_stake_gate,
            bootstrap_peers,
        } => {
            let node_key_path = ensure_default_node_key()?;
            let mut config = load_operator_config(default_data_dir()?)?;
            config.api_port = port.unwrap_or(config.api_port);
            if let Some(bind) = bind {
                config.api_bind_addr = bind;
            }
            config.api_bearer_token =
                resolve_api_token_for_bind(&config.api_bind_addr, api_token)?;
            config.stake_gate_enabled =
                resolve_stake_gate(no_stake_gate, config.stake_gate_enabled);
            config.p2p_bind_addr = p2p_bind;
            config.node_key_path = Some(node_key_path);
            config.ledger_path = Some(PathBuf::from(&ledger));
            config.bootstrap_peers = resolve_bootstrap_peers(bootstrap_peers);
            config.share_compute = true;
            let mut node = tirami_node::TiramiNode::new(config);

            node.load_model(&PathBuf::from(&model), &PathBuf::from(&tokenizer))
                .await?;

            tracing::info!("Starting forged seed with model: {}", model);

            // Run seed with graceful shutdown on Ctrl+C
            tokio::select! {
                result = node.run_seed() => { result?; }
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Ctrl+C received");
                    node.shutdown().await;
                }
            }
        }
        DaemonCommand::Node {
            model,
            tokenizer,
            port,
            bind,
            ledger,
            api_token,
            no_stake_gate,
        } => {
            let mut config = load_operator_config(ensure_default_data_dir()?)?;
            config.api_port = port.unwrap_or(config.api_port);
            if let Some(bind) = bind {
                config.api_bind_addr = bind;
            }
            config.api_bearer_token =
                resolve_api_token_for_bind(&config.api_bind_addr, api_token)?;
            config.stake_gate_enabled =
                resolve_stake_gate(no_stake_gate, config.stake_gate_enabled);
            config.ledger_path = Some(PathBuf::from(&ledger));
            let api_port = config.api_port;
            let node = tirami_node::TiramiNode::new(config);

            if let (Some(model), Some(tokenizer)) = (model, tokenizer) {
                node.load_model(&PathBuf::from(&model), &PathBuf::from(&tokenizer))
                    .await?;
            }

            tracing::info!("Starting forged local API server on port {}", api_port);

            tokio::select! {
                result = node.serve_api() => { result?; }
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Ctrl+C received");
                    node.shutdown().await;
                }
            }
        }
    }

    Ok(())
}

/// Mirror of `main.rs::load_operator_config`. `tiramisu` is a separate
/// binary with no shared module, so the resolution helpers are duplicated
/// here — the same way `resolve_api_token` already is.
fn load_operator_config(data_dir: impl Into<PathBuf>) -> anyhow::Result<Config> {
    let data_dir = data_dir.into();
    let (config, unknown) = Config::load_from_data_dir(&data_dir)?;
    for key in &unknown {
        tracing::warn!(
            "ignoring unknown key `{}` in {}",
            key,
            Config::config_file_path(&data_dir).display()
        );
    }
    Ok(config)
}

/// Mirror of `main.rs::resolve_stake_gate` (#162).
fn resolve_stake_gate(no_stake_gate: bool, from_config: bool) -> bool {
    if no_stake_gate {
        return false;
    }
    match std::env::var("TIRAMI_STAKE_GATE") {
        Ok(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        ),
        Err(_) => from_config,
    }
}

fn resolve_api_token(flag: Option<String>) -> Option<String> {
    flag.or_else(|| std::env::var("TIRAMI_API_TOKEN").ok())
        .or_else(|| std::env::var("FORGE_API_TOKEN").ok())
        .filter(|token| !token.is_empty())
}

fn resolve_api_token_for_bind(bind: &str, flag: Option<String>) -> anyhow::Result<Option<String>> {
    let token = resolve_api_token(flag);
    validate_public_bind_auth(bind, token.as_deref())?;
    Ok(token)
}

fn validate_public_bind_auth(bind: &str, token: Option<&str>) -> anyhow::Result<()> {
    if is_public_bind_addr(bind) && token.map(str::trim).filter(|t| !t.is_empty()).is_none() {
        anyhow::bail!(
            "refusing to bind unauthenticated API to {}; set --api-token or TIRAMI_API_TOKEN",
            bind
        );
    }
    Ok(())
}

fn is_public_bind_addr(bind: &str) -> bool {
    let bind = bind.trim().trim_matches(['[', ']']).to_ascii_lowercase();
    !matches!(bind.as_str(), "127.0.0.1" | "::1" | "localhost")
}

fn resolve_bootstrap_peers(cli_peers: Vec<String>) -> Vec<String> {
    let mut peers = std::env::var("TIRAMI_BOOTSTRAP_PEERS")
        .ok()
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|peer| !peer.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    peers.extend(
        cli_peers
            .into_iter()
            .map(|peer| peer.trim().to_string())
            .filter(|peer| !peer.is_empty()),
    );
    peers
}

fn default_data_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME").map_err(|_| anyhow::anyhow!("HOME not set"))?;
    Ok(PathBuf::from(home).join(".tirami"))
}

fn ensure_default_data_dir() -> anyhow::Result<PathBuf> {
    let tirami_dir = default_data_dir()?;
    if !tirami_dir.exists() {
        std::fs::create_dir_all(&tirami_dir)?;
    }
    Ok(tirami_dir)
}

fn ensure_default_node_key() -> anyhow::Result<PathBuf> {
    let tirami_dir = ensure_default_data_dir()?;
    let key_path = tirami_dir.join("node.key");
    if !key_path.exists() {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut OsRng);
        std::fs::write(&key_path, signing_key.to_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&key_path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&key_path, perms)?;
        }
        tracing::info!("Generated new node key at {}", key_path.display());
    }
    Ok(key_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_bind_may_omit_api_token() {
        validate_public_bind_auth("127.0.0.1", None).unwrap();
        validate_public_bind_auth("localhost", None).unwrap();
        validate_public_bind_auth("::1", None).unwrap();
    }

    #[test]
    fn public_or_wildcard_bind_requires_api_token() {
        assert!(validate_public_bind_auth("0.0.0.0", None).is_err());
        assert!(validate_public_bind_auth("::", None).is_err());
        assert!(validate_public_bind_auth("203.0.113.10", None).is_err());
        validate_public_bind_auth("0.0.0.0", Some("token")).unwrap();
    }
}
