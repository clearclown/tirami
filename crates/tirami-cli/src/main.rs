use clap::{Parser, Subcommand};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use tirami_core::Config;

#[derive(Parser)]
#[command(name = "tirami")]
#[command(about = "Tirami — distributed LLM inference where compute is currency")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Chat with your local LLM (auto-downloads model if needed)
    Chat {
        /// Model name (e.g., "qwen2.5:0.5b") or path to GGUF file
        #[arg(short, long, default_value = "qwen2.5:0.5b")]
        model: String,

        /// Path to tokenizer.json (auto-resolved if using model name)
        #[arg(short, long)]
        tokenizer: Option<String>,

        /// Initial prompt (interactive mode if omitted)
        prompt: Option<String>,

        /// Maximum tokens to generate
        #[arg(short = 'n', long, default_value = "256")]
        max_tokens: u32,

        /// Sampling temperature
        #[arg(long, default_value = "0.7")]
        temperature: f32,
    },

    /// List available models
    Models,

    /// One-command bootstrap: generate key, download model, join network, start earning TRM.
    ///
    /// This is the recommended way to join Tirami. Equivalent to running
    /// `seed` but with auto-generated keys, auto-downloaded models, and
    /// automatic HTTP API binding. Designed so a new user can participate
    /// in ~30 seconds.
    Start {
        /// Model to serve (e.g., "qwen2.5:0.5b"). Auto-downloaded from HuggingFace.
        #[arg(short, long, default_value = "qwen2.5:0.5b")]
        model: String,

        /// Port for the HTTP API.
        #[arg(short, long, default_value = "3000")]
        port: u16,

        /// Bind address for the HTTP API. Default 127.0.0.1 (local only).
        /// Use 0.0.0.0 to accept remote requests.
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,

        /// Fixed P2P UDP bind socket for direct peers, e.g. 0.0.0.0:7700.
        #[arg(long)]
        p2p_bind: Option<String>,

        /// Public bootstrap peer to join on startup. Repeatable.
        /// Format: PUBLIC_KEY, PUBLIC_KEY@RELAY_URL, or PUBLIC_KEY@IP:PORT.
        #[arg(long = "bootstrap-peer")]
        bootstrap_peers: Vec<String>,

        /// Optional bearer token protecting administrative HTTP API routes.
        /// Can also be set with TIRAMI_API_TOKEN.
        #[arg(long)]
        api_token: Option<String>,

        /// Run in pure-server mode without auto-configuring a
        /// PersonalAgent. Default is OFF (agent auto-configured);
        /// pass --no-agent to opt out. Useful for hosting nodes
        /// that serve the mesh but don't need a user-facing agent
        /// state on that machine.
        #[arg(long, default_value_t = false)]
        no_agent: bool,
    },

    /// Print this node's stable P2P identity without starting a model.
    Identity,

    /// Start as a seed node (holds model, serves inference)
    Seed {
        /// Model name (e.g., "qwen2.5:0.5b") or path to GGUF file
        #[arg(short, long, default_value = "qwen2.5:0.5b")]
        model: String,

        /// Path to tokenizer.json (auto-resolved if using model name)
        #[arg(short, long)]
        tokenizer: Option<String>,

        /// Port for the local HTTP API
        #[arg(short, long, default_value = "3000")]
        port: u16,

        /// Bind address for the local HTTP API
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,

        /// Fixed P2P UDP bind socket for direct peers, e.g. 0.0.0.0:7700.
        #[arg(long)]
        p2p_bind: Option<String>,

        /// Path to the persisted ledger snapshot
        #[arg(long, default_value = "forge-ledger.json")]
        ledger: String,

        /// Optional bearer token protecting administrative HTTP API routes
        #[arg(long)]
        api_token: Option<String>,

        /// Public bootstrap peer to join on startup. Repeatable.
        /// Format: PUBLIC_KEY, PUBLIC_KEY@RELAY_URL, or PUBLIC_KEY@IP:PORT.
        #[arg(long = "bootstrap-peer")]
        bootstrap_peers: Vec<String>,
    },

    /// Start as a worker node (connects to seed for inference)
    Worker {
        /// Seed node public key (hex string from seed output)
        #[arg(short, long)]
        seed: String,

        /// Seed relay URL (optional, for NAT traversal)
        #[arg(long)]
        relay: Option<String>,

        /// Port for the local HTTP API
        #[arg(short, long, default_value = "3000")]
        port: u16,

        /// Bind address for the local HTTP API
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,

        /// Fixed P2P UDP bind socket for direct peers, e.g. 0.0.0.0:7700.
        #[arg(long)]
        p2p_bind: Option<String>,

        /// Optional bearer token protecting administrative HTTP API routes
        #[arg(long)]
        api_token: Option<String>,

        /// Path to the persisted ledger snapshot
        #[arg(long, default_value = "forge-ledger.json")]
        ledger: String,

        /// Run as daemon (no interactive prompt)
        #[arg(long)]
        daemon: bool,
    },

    /// Start a local API server (no P2P)
    Node {
        /// Path to a GGUF model file
        #[arg(short, long)]
        model: Option<String>,

        /// Path to tokenizer.json file
        #[arg(short, long)]
        tokenizer: Option<String>,

        /// Port for the local API
        #[arg(short, long, default_value = "3000")]
        port: u16,

        /// Bind address for the local HTTP API
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,

        /// Path to the persisted ledger snapshot
        #[arg(long, default_value = "forge-ledger.json")]
        ledger: String,

        /// Optional bearer token protecting administrative HTTP API routes
        #[arg(long)]
        api_token: Option<String>,
    },

    /// Show cluster status
    Status {
        /// Base URL of a running forge node API
        #[arg(long, default_value = "http://127.0.0.1:3000")]
        url: String,

        /// Optional bearer token for a protected forge node API
        #[arg(long)]
        api_token: Option<String>,
    },

    /// Show the current model/capability-based topology plan
    Topology {
        /// Base URL of a running forge node API
        #[arg(long, default_value = "http://127.0.0.1:3000")]
        url: String,

        /// Optional bearer token for a protected forge node API
        #[arg(long)]
        api_token: Option<String>,
    },

    /// Bitcoin Lightning wallet management
    Wallet {
        #[command(subcommand)]
        action: WalletAction,
    },

    /// Run distributed inference across RPC peers
    Distribute {
        /// Path to GGUF model file
        #[arg(short, long)]
        model: String,

        /// RPC server endpoints (host:port), comma-separated
        #[arg(long)]
        rpc: String,

        /// Prompt text
        prompt: String,

        /// Maximum tokens to generate
        #[arg(short = 'n', long, default_value = "256")]
        max_tokens: u32,

        /// Sampling temperature
        #[arg(long, default_value = "0.7")]
        temperature: f32,

        /// GPU layers to offload (0 = CPU only)
        #[arg(long, default_value = "99")]
        ngl: u32,
    },

    /// Export a settlement statement from a running forge node API
    Settle {
        /// Base URL of a running forge node API
        #[arg(long, default_value = "http://127.0.0.1:3000")]
        url: String,

        /// Optional bearer token for a protected forge node API
        #[arg(long)]
        api_token: Option<String>,

        /// Settlement window size in hours
        #[arg(long, default_value = "24")]
        hours: u64,

        /// Optional reference price per CU for external payout estimation
        #[arg(long)]
        price: Option<f64>,

        /// Optional output path for the exported JSON statement
        #[arg(long)]
        out: Option<String>,

        /// Generate a Lightning invoice for net CU earned
        #[arg(long)]
        pay: bool,
    },

    /// Tirami Su (TRM tokenomics) — supply, staking, referrals
    Su {
        #[command(subcommand)]
        action: SuCommands,

        /// Base URL of the Tirami node
        #[arg(short, long, default_value = "http://127.0.0.1:3000")]
        url: String,
    },

    /// Phase 18.5 — personal AI agent management.
    ///
    /// Your agent lives on this node, manages its own TRM wallet,
    /// and autonomously earns / spends on the Tirami mesh while
    /// you use it for day-to-day tasks. `tirami agent status` is
    /// the one-glance summary you'll want in your terminal.
    Agent {
        #[command(subcommand)]
        action: AgentCommands,

        /// Base URL of the Tirami node running locally.
        #[arg(short, long, default_value = "http://127.0.0.1:3000")]
        url: String,

        /// Bearer token for the local Tirami HTTP API.
        /// Can also be set with TIRAMI_API_TOKEN.
        #[arg(long)]
        api_token: Option<String>,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    /// Print the personal agent's current state (balance,
    /// today's earn/spend, preferences). Calls
    /// `GET /v1/tirami/agent/status` on the local node.
    Status,
    /// Human-readable one-liner (the `summary` field only).
    Summary,
    /// Ask your personal agent to handle a task and print the
    /// result. Calls `POST /v1/tirami/agent/task` on the local
    /// node; the agent decides whether to run it locally or on
    /// the mesh (see --remote for the mesh hints).
    Chat {
        /// Prompt for the agent.
        prompt: String,
        /// Max tokens the response may grow to.
        #[arg(short = 'n', long, default_value = "256")]
        max_tokens: u32,
        /// Force size classification: local | remote | hybrid.
        /// Default: derived from max_tokens (≤256 → local).
        #[arg(long)]
        size: Option<String>,
        /// Estimated TRM cost (caller's hint for the budget
        /// checker). Defaults to 1 TRM per 100 tokens.
        #[arg(long)]
        estimated_trm: Option<u64>,
        /// NodeId (hex) of the peer to dispatch remote tasks to.
        /// Must be combined with --peer-url.
        #[arg(long)]
        peer_node_id: Option<String>,
        /// Base URL of the peer's HTTP API (e.g.
        /// http://192.0.2.7:3000). Must be combined with
        /// --peer-node-id.
        #[arg(long)]
        peer_url: Option<String>,
        /// Bearer token for the peer's protected HTTP API.
        /// Must be combined with --peer-node-id and --peer-url.
        #[arg(long)]
        peer_api_token: Option<String>,
    },
}

#[derive(Subcommand)]
enum SuCommands {
    /// Show TRM supply stats (total supply, mint rate, epoch info)
    Supply,

    /// Stake TRM for yield
    Stake {
        /// Amount of TRM to stake
        amount: f64,

        /// Lock duration: 7d, 30d, 90d, or 365d
        duration: String,
    },

    /// Unstake a previously staked position
    Unstake {
        /// Index of the stake to unstake
        stake_index: usize,
    },

    /// Record a referral between two nodes
    Refer {
        /// Referrer node ID (hex)
        referrer: String,

        /// Referred node ID (hex)
        referred: String,
    },

    /// Show referral stats
    Referrals,
}

#[derive(Subcommand)]
enum WalletAction {
    /// Show wallet info (node ID, balances, funding address)
    Info,
    /// Create a Lightning invoice to receive sats
    Invoice {
        /// Amount in satoshis
        amount_sats: u64,
        /// Description
        #[arg(short, long, default_value = "Tirami inference")]
        description: String,
    },
    /// Pay a Lightning invoice
    Pay {
        /// BOLT11 invoice string
        invoice: String,
    },
}

/// Default tracing filter when `RUST_LOG` is unset.
///
/// `"info"` for Tirami internals, `error` for iroh's multicast / IPv6
/// relay probes and noisy address-set warnings that spam logs on
/// Tailscale, Docker-heavy, or IPv4-only hosts (fix #75). Operators who
/// want the raw firehose: `RUST_LOG=info`.
const DEFAULT_TRACING_FILTER: &str = "info,swarm_discovery=error,iroh::socket::transports::relay=error,iroh::socket::remote_map::remote_state=error,iroh_relay=error,noq_udp=error";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(DEFAULT_TRACING_FILTER)),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start {
            model,
            port,
            bind,
            p2p_bind,
            bootstrap_peers,
            api_token,
            no_agent,
        } => {
            run_start_command(
                model,
                port,
                bind,
                no_agent,
                bootstrap_peers,
                api_token,
                p2p_bind,
            )
            .await?;
        }
        Commands::Identity => {
            run_identity_command()?;
        }
        Commands::Models => {
            tirami_infer::model_registry::list_models();
        }
        Commands::Wallet { action } => {
            let config = tirami_lightning::node::WalletConfig::default();
            let wallet = tirami_lightning::ForgeWallet::start(config)?;

            match action {
                WalletAction::Info => {
                    println!("Lightning Node ID: {}", wallet.node_id());
                    println!("Network: {:?}", wallet.network());
                    println!("On-chain balance: {} sats", wallet.onchain_balance_sats());
                    println!(
                        "Lightning balance: {} sats",
                        wallet.lightning_balance_sats()
                    );
                    println!("Funding address: {}", wallet.funding_address()?);

                    let rate = tirami_lightning::payment::ExchangeRate::default();
                    println!("Exchange rate: {} msats/CU", rate.msats_per_cu);
                }
                WalletAction::Invoice {
                    amount_sats,
                    description,
                } => {
                    let amount_msats = amount_sats * 1000;
                    let invoice = wallet.create_invoice(amount_msats, &description, 3600)?;
                    println!("{}", invoice);
                }
                WalletAction::Pay { invoice } => {
                    let payment_id = wallet.pay_invoice(&invoice)?;
                    println!("Payment sent: {}", payment_id);
                }
            }
        }
        Commands::Chat {
            model,
            tokenizer,
            prompt,
            max_tokens,
            temperature,
        } => {
            let config = Config::default();
            let node = tirami_node::TiramiNode::new(config);

            // Resolve model via the unified dispatcher (local path, HF URL, shorthand, catalog)
            let resolved = tirami_infer::model_registry::resolve(&model)?;
            let tokenizer_path = resolved
                .tokenizer_path
                .or_else(|| tokenizer.map(PathBuf::from))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "tokenizer path required for this model source — use --tokenizer"
                    )
                })?;
            let model_path = resolved.model_path;

            node.load_model(&model_path, &tokenizer_path).await?;

            if let Some(prompt) = prompt {
                let start = std::time::Instant::now();
                let response = node.chat(&prompt, max_tokens, temperature).await?;
                let elapsed = start.elapsed();
                println!("{}", response);
                eprintln!("\n---\nGenerated in {:.2}s", elapsed.as_secs_f64());
            } else {
                println!("Tirami Chat (type 'quit' to exit)");
                println!("Model: {}", model);
                println!("---");

                let stdin = io::stdin();
                loop {
                    print!("> ");
                    io::stdout().flush()?;

                    let mut input = String::new();
                    stdin.lock().read_line(&mut input)?;
                    let input = input.trim();

                    if input.is_empty() {
                        continue;
                    }
                    if input == "quit" || input == "exit" {
                        break;
                    }

                    let start = std::time::Instant::now();
                    match node.chat(input, max_tokens, temperature).await {
                        Ok(response) => {
                            println!("{}", response);
                            eprintln!("[{:.2}s]", start.elapsed().as_secs_f64());
                        }
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
            }
        }
        Commands::Seed {
            model,
            tokenizer,
            port,
            bind,
            p2p_bind,
            ledger,
            api_token,
            bootstrap_peers,
        } => {
            let node_key_path = ensure_default_node_key()?;
            let api_bearer_token = resolve_api_token_for_bind(&bind, api_token)?;
            let mut config = Config::for_data_dir(default_data_dir()?);
            config.api_port = port;
            config.api_bind_addr = bind;
            config.api_bearer_token = api_bearer_token;
            config.p2p_bind_addr = p2p_bind;
            config.node_key_path = Some(node_key_path);
            config.ledger_path = Some(PathBuf::from(&ledger));
            config.bootstrap_peers = resolve_bootstrap_peers(bootstrap_peers);
            config.share_compute = true;
            let mut node = tirami_node::TiramiNode::new(config);

            // Resolve model via the unified dispatcher (local path, HF URL, shorthand, catalog)
            let resolved = tirami_infer::model_registry::resolve(&model)?;
            let tokenizer_path = resolved
                .tokenizer_path
                .or_else(|| tokenizer.map(PathBuf::from))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "tokenizer path required for this model source — use --tokenizer"
                    )
                })?;
            let model_path = resolved.model_path;

            node.load_model(&model_path, &tokenizer_path).await?;

            tracing::info!("Starting as SEED node with model: {}", model);

            // Install Ctrl-C handler for graceful shutdown
            let shutdown_ledger = node.ledger.clone();
            let shutdown_ledger_path = node.config.ledger_path.clone();
            let shutdown_bank = node.bank.clone();
            let shutdown_marketplace = node.marketplace.clone();
            let shutdown_mind = node.mind_agent.clone();
            let shutdown_personal_agent = node.personal_agent.clone();
            let shutdown_config = node.config.clone();
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    tracing::info!("Received Ctrl-C, persisting state...");
                    // Persist ledger
                    if let Some(path) = shutdown_ledger_path {
                        if let Err(e) = shutdown_ledger.lock().await.save_to_path(&path) {
                            tracing::warn!("Failed to persist ledger on shutdown: {}", e);
                        } else {
                            tracing::info!("Ledger persisted to {}", path.display());
                        }
                    }
                    // Persist bank state
                    if let Some(ref path) = shutdown_config.bank_state_path {
                        let bank = shutdown_bank.lock().await;
                        if let Err(e) = tirami_node::state_persist::save_bank(&*bank, path) {
                            tracing::warn!("Failed to persist bank state: {}", e);
                        }
                    }
                    // Persist marketplace state
                    if let Some(ref path) = shutdown_config.marketplace_state_path {
                        let mp = shutdown_marketplace.lock().await;
                        if let Err(e) = tirami_node::state_persist::save_marketplace(&*mp, path) {
                            tracing::warn!("Failed to persist marketplace state: {}", e);
                        }
                    }
                    // Persist mind agent state
                    if let Some(ref path) = shutdown_config.mind_state_path {
                        let mind = shutdown_mind.lock().await;
                        if let Some(agent) = mind.as_ref() {
                            if let Err(e) = tirami_node::state_persist::save_mind(agent, path) {
                                tracing::warn!("Failed to persist mind state: {}", e);
                            }
                        }
                    }
                    if let Some(ref path) = shutdown_config.personal_agent_state_path {
                        let personal = shutdown_personal_agent.lock().await;
                        if let Some(agent) = personal.as_ref() {
                            if let Err(e) =
                                tirami_node::state_persist::save_personal_agent(agent, path)
                            {
                                tracing::warn!("Failed to persist personal agent state: {}", e);
                            }
                        }
                    }
                    std::process::exit(0);
                }
            });

            node.run_seed().await?;
        }
        Commands::Worker {
            seed,
            relay,
            port,
            bind,
            p2p_bind,
            api_token,
            ledger,
            daemon,
        } => {
            let node_key_path = ensure_default_node_key()?;
            let api_bearer_token = resolve_api_token_for_bind(&bind, api_token)?;
            let mut config = Config::for_data_dir(default_data_dir()?);
            config.api_port = port;
            config.api_bind_addr = bind.clone();
            config.api_bearer_token = api_bearer_token;
            config.p2p_bind_addr = p2p_bind;
            config.node_key_path = Some(node_key_path);
            config.ledger_path = Some(PathBuf::from(&ledger));
            let mut node = tirami_node::TiramiNode::new(config);

            let public_key: iroh::PublicKey = seed
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid seed public key: {}", e))?;

            let mut seed_addr = iroh::EndpointAddr::new(public_key);
            if let Some(relay_url) = relay {
                let url: iroh::RelayUrl = relay_url
                    .parse()
                    .map_err(|e| anyhow::anyhow!("invalid relay URL: {}", e))?;
                seed_addr.addrs.insert(iroh::TransportAddr::Relay(url));
            }

            let transport = node.connect_to_seed(seed_addr).await?;
            tracing::info!("Connected to seed. Ready for inference.");

            // Phase 19 / Tier C — auto-configure the PersonalAgent
            // on the worker too. Without this `/v1/tirami/agent/*`
            // returns 412 on worker nodes, breaking the
            // killer-app story for users who run a worker
            // (HTTP → P2P forward) instead of a full seed. Also
            // spawn the background tick loop so the agent observes
            // activity.
            node.ensure_personal_agent(transport.tirami_node_id()).await;
            node.spawn_agent_loop();

            // Spawn HTTP API server in background (same pattern as Seed)
            node.spawn_api();

            // Install Ctrl-C handler for graceful shutdown
            let shutdown_ledger = node.ledger.clone();
            let shutdown_ledger_path = node.config.ledger_path.clone();
            let shutdown_bank = node.bank.clone();
            let shutdown_marketplace = node.marketplace.clone();
            let shutdown_mind = node.mind_agent.clone();
            let shutdown_personal_agent = node.personal_agent.clone();
            let shutdown_config = node.config.clone();
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    tracing::info!("Received Ctrl-C, persisting state...");
                    if let Some(path) = shutdown_ledger_path {
                        if let Err(e) = shutdown_ledger.lock().await.save_to_path(&path) {
                            tracing::warn!("Failed to persist ledger on shutdown: {}", e);
                        } else {
                            tracing::info!("Ledger persisted to {}", path.display());
                        }
                    }
                    if let Some(ref path) = shutdown_config.bank_state_path {
                        let bank = shutdown_bank.lock().await;
                        if let Err(e) = tirami_node::state_persist::save_bank(&*bank, path) {
                            tracing::warn!("Failed to persist bank state: {}", e);
                        }
                    }
                    if let Some(ref path) = shutdown_config.marketplace_state_path {
                        let mp = shutdown_marketplace.lock().await;
                        if let Err(e) = tirami_node::state_persist::save_marketplace(&*mp, path) {
                            tracing::warn!("Failed to persist marketplace state: {}", e);
                        }
                    }
                    if let Some(ref path) = shutdown_config.mind_state_path {
                        let mind = shutdown_mind.lock().await;
                        if let Some(agent) = mind.as_ref() {
                            if let Err(e) = tirami_node::state_persist::save_mind(agent, path) {
                                tracing::warn!("Failed to persist mind state: {}", e);
                            }
                        }
                    }
                    if let Some(ref path) = shutdown_config.personal_agent_state_path {
                        let personal = shutdown_personal_agent.lock().await;
                        if let Some(agent) = personal.as_ref() {
                            if let Err(e) =
                                tirami_node::state_persist::save_personal_agent(agent, path)
                            {
                                tracing::warn!("Failed to persist personal agent state: {}", e);
                            }
                        }
                    }
                    std::process::exit(0);
                }
            });

            if daemon {
                tracing::info!("Worker running in daemon mode (Ctrl-C to stop)");
                // Block forever — the HTTP API runs in the background
                std::future::pending::<()>().await;
            } else {
                // Interactive worker chat
                let node_id = transport.tirami_node_id();
                let peers = transport.connected_peers().await;
                let seed_peer_id = peers
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("no seed peer found"))?
                    .clone();

                println!("Tirami Worker (connected to seed)");
                println!("HTTP API at http://{}:{}", bind, port);
                println!("Type a prompt to send to the seed for inference.");
                println!("---");

                let stdin = io::stdin();
                loop {
                    print!("> ");
                    io::stdout().flush()?;

                    let mut input = String::new();
                    stdin.lock().read_line(&mut input)?;
                    let input = input.trim();

                    if input.is_empty() {
                        continue;
                    }
                    if input == "quit" || input == "exit" {
                        break;
                    }

                    match tirami_node::pipeline::PipelineCoordinator::request_inference(
                        &transport,
                        &seed_peer_id,
                        &node_id,
                        input,
                        256,
                        0.7,
                    )
                    .await
                    {
                        Ok(response) => println!("{}", response),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
            }
        }
        Commands::Distribute {
            model,
            rpc,
            prompt,
            max_tokens,
            temperature,
            ngl,
        } => {
            let llama_cli = tirami_infer::distributed::find_llama_cli().ok_or_else(|| {
                anyhow::anyhow!(
                    "llama-cli not found. Set TIRAMI_LLAMA_CLI_PATH or install llama.cpp"
                )
            })?;

            let rpc_endpoints: Vec<String> = rpc.split(',').map(|s| s.trim().to_string()).collect();

            let config = tirami_infer::distributed::DistributedConfig {
                model_path: PathBuf::from(&model),
                rpc_endpoints,
                n_gpu_layers: ngl,
                llama_cli_path: llama_cli,
            };

            let start = std::time::Instant::now();
            let (text, token_count) = tirami_infer::distributed::run_distributed_inference(
                &config,
                &prompt,
                max_tokens,
                temperature,
            )?;

            println!("{}", text);
            eprintln!(
                "\n---\nDistributed inference: {} tokens in {:.2}s",
                token_count,
                start.elapsed().as_secs_f64()
            );
        }
        Commands::Node {
            model,
            tokenizer,
            port,
            bind,
            ledger,
            api_token,
        } => {
            let api_bearer_token = resolve_api_token_for_bind(&bind, api_token)?;
            let mut config = Config::for_data_dir(ensure_default_data_dir()?);
            config.api_port = port;
            config.api_bind_addr = bind;
            config.api_bearer_token = api_bearer_token;
            config.ledger_path = Some(PathBuf::from(&ledger));
            let node = tirami_node::TiramiNode::new(config);

            // Resolve model spec via the unified dispatcher — handles local path,
            // HF URL, HF shorthand, catalog name, and ~/.models scan.
            if let Some(model) = model {
                let resolved = tirami_infer::model_registry::resolve(&model)?;
                let tokenizer_path = resolved
                    .tokenizer_path
                    .or_else(|| tokenizer.map(PathBuf::from))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "tokenizer path required for this model source — use --tokenizer"
                        )
                    })?;
                node.load_model(&resolved.model_path, &tokenizer_path)
                    .await?;
            }

            tracing::info!("Starting local API server on port {}", port);

            // Install Ctrl-C handler for graceful shutdown
            let shutdown_ledger = node.ledger.clone();
            let shutdown_ledger_path = node.config.ledger_path.clone();
            let shutdown_bank = node.bank.clone();
            let shutdown_marketplace = node.marketplace.clone();
            let shutdown_mind = node.mind_agent.clone();
            let shutdown_personal_agent = node.personal_agent.clone();
            let shutdown_config = node.config.clone();
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    tracing::info!("Received Ctrl-C, persisting state...");
                    // Persist ledger
                    if let Some(path) = shutdown_ledger_path {
                        if let Err(e) = shutdown_ledger.lock().await.save_to_path(&path) {
                            tracing::warn!("Failed to persist ledger on shutdown: {}", e);
                        } else {
                            tracing::info!("Ledger persisted to {}", path.display());
                        }
                    }
                    // Persist bank state
                    if let Some(ref path) = shutdown_config.bank_state_path {
                        let bank = shutdown_bank.lock().await;
                        if let Err(e) = tirami_node::state_persist::save_bank(&*bank, path) {
                            tracing::warn!("Failed to persist bank state: {}", e);
                        }
                    }
                    // Persist marketplace state
                    if let Some(ref path) = shutdown_config.marketplace_state_path {
                        let mp = shutdown_marketplace.lock().await;
                        if let Err(e) = tirami_node::state_persist::save_marketplace(&*mp, path) {
                            tracing::warn!("Failed to persist marketplace state: {}", e);
                        }
                    }
                    // Persist mind agent state
                    if let Some(ref path) = shutdown_config.mind_state_path {
                        let mind = shutdown_mind.lock().await;
                        if let Some(agent) = mind.as_ref() {
                            if let Err(e) = tirami_node::state_persist::save_mind(agent, path) {
                                tracing::warn!("Failed to persist mind state: {}", e);
                            }
                        }
                    }
                    if let Some(ref path) = shutdown_config.personal_agent_state_path {
                        let personal = shutdown_personal_agent.lock().await;
                        if let Some(agent) = personal.as_ref() {
                            if let Err(e) =
                                tirami_node::state_persist::save_personal_agent(agent, path)
                            {
                                tracing::warn!("Failed to persist personal agent state: {}", e);
                            }
                        }
                    }
                    std::process::exit(0);
                }
            });

            node.serve_api().await?;
        }
        Commands::Status { url, api_token } => {
            let base = url.trim_end_matches('/');
            let client = reqwest::Client::new();
            let mut request = client.get(format!("{base}/status"));
            if let Some(token) = resolve_api_token(api_token) {
                request = request.bearer_auth(token);
            }
            let status: tirami_node::api::StatusResponse =
                request.send().await?.error_for_status()?.json().await?;

            println!("Tirami status: {}", status.status);
            println!("Model loaded: {}", status.model_loaded);
            println!("Protocol: v{}", status.protocol_version);
            if !status.protocol_features.is_empty() {
                println!("Features: {}", status.protocol_features.join(", "));
            }
            println!(
                "Market price: {:.2} CU/token (demand {:.2} / supply {:.2})",
                status.market_price.effective_trm_per_token(),
                status.market_price.demand_factor,
                status.market_price.supply_factor
            );
            println!(
                "Network: {} nodes, {} contributed CU, {} consumed CU, {} trades",
                status.network.total_nodes,
                status.network.total_contributed_cu,
                status.network.total_consumed_cu,
                status.network.total_trades
            );

            if status.recent_trades.is_empty() {
                println!("Recent trades: none");
            } else {
                println!("Recent trades:");
                for trade in status.recent_trades.iter().take(5) {
                    println!(
                        "  t={} provider={} consumer={} cu={} tokens={} model={}",
                        trade.timestamp,
                        trade.provider,
                        trade.consumer,
                        trade.trm_amount,
                        trade.tokens_processed,
                        trade.model_id
                    );
                }
            }

            // Show distributed inference capability
            let dist = tirami_infer::distributed::distributed_status();
            println!(
                "Distributed: llama-cli={} rpc-server={}",
                if dist.llama_cli_available {
                    "available"
                } else {
                    "not found"
                },
                if dist.rpc_server_available {
                    "available"
                } else {
                    "not found"
                },
            );
        }
        Commands::Topology { url, api_token } => {
            let base = url.trim_end_matches('/');
            let client = reqwest::Client::new();
            let mut request = client.get(format!("{base}/topology"));
            if let Some(token) = resolve_api_token(api_token) {
                request = request.bearer_auth(token);
            }
            let topology: tirami_node::api::TopologyResponse =
                request.send().await?.error_for_status()?.json().await?;

            println!("Tirami topology: {}", topology.status);
            if let Some(model) = topology.model {
                println!(
                    "Model: {} (layers={}, hidden={}, quant={})",
                    model.id.0, model.total_layers, model.hidden_dim, model.quantization
                );
            } else {
                println!("Model: not loaded");
            }

            if let Some(local) = topology.local_capability {
                println!(
                    "Local: node={} protocol=v{} cpu={} mem={:.1}GB avail={:.1}GB metal={} region={}",
                    local.node_id.to_hex(),
                    local.protocol_version,
                    local.cpu_cores,
                    local.memory_gb,
                    local.available_memory_gb,
                    local.metal_available,
                    local.region
                );
                if !local.features.is_empty() {
                    println!("Local features: {}", local.features.join(", "));
                }
            } else {
                println!("Local capability: unavailable");
            }

            if topology.connected_peers.is_empty() {
                println!("Connected peers: none");
            } else {
                println!("Connected peers:");
                for peer in topology.connected_peers {
                    println!(
                        "  node={} protocol=v{} cpu={} avail={:.1}GB metal={} region={}",
                        peer.node_id.to_hex(),
                        peer.protocol_version,
                        peer.cpu_cores,
                        peer.available_memory_gb,
                        peer.metal_available,
                        peer.region
                    );
                }
            }

            match topology.planned_topology {
                Some(plan) => {
                    println!("Planned stages:");
                    for stage in plan.stages {
                        println!(
                            "  pos={} node={} layers={}..{}",
                            stage.position,
                            stage.node_id.to_hex(),
                            stage.layer_range.start,
                            stage.layer_range.end
                        );
                    }
                }
                None => println!("Planned topology: unavailable"),
            }

            match topology.advertised_topology {
                Some(plan) => {
                    println!("Advertised topology:");
                    for stage in plan.stages {
                        println!(
                            "  pos={} node={} layers={}..{}",
                            stage.position,
                            stage.node_id.to_hex(),
                            stage.layer_range.start,
                            stage.layer_range.end
                        );
                    }
                }
                None => println!("Advertised topology: none"),
            }
        }
        Commands::Settle {
            url,
            api_token,
            hours,
            price,
            out,
            pay,
        } => {
            let base = url.trim_end_matches('/');
            let mut endpoint = format!("{base}/settlement?hours={hours}");
            if let Some(price) = price {
                endpoint.push_str(&format!("&reference_price_per_cu={price}"));
            }

            let client = reqwest::Client::new();
            let mut request = client.get(endpoint);
            if let Some(token) = resolve_api_token(api_token) {
                request = request.bearer_auth(token);
            }
            let statement: tirami_ledger::SettlementStatement =
                request.send().await?.error_for_status()?.json().await?;

            let json = serde_json::to_string_pretty(&statement)?;
            if let Some(path) = out {
                std::fs::write(&path, &json)?;
                println!("Settlement statement written to {}", path);
            } else {
                println!("{}", json);
            }

            if pay {
                // Find the local node's net CU from the statement
                // Use the node with the highest positive net_cu as the provider
                let best_provider = statement
                    .nodes
                    .iter()
                    .filter(|n| n.net_cu > 0)
                    .max_by_key(|n| n.net_cu);

                if let Some(provider) = best_provider {
                    let rate = tirami_lightning::payment::ExchangeRate::default();
                    if let Some(invoice_info) = tirami_lightning::payment::create_settlement_invoice(
                        provider.net_cu,
                        &rate,
                        hours,
                    ) {
                        println!("\n--- Lightning Settlement ---");
                        println!("Provider: {}", provider.node_id);
                        println!("Net CU earned: {}", invoice_info.net_cu);
                        println!(
                            "Amount: {} msats ({} sats)",
                            invoice_info.amount_msats, invoice_info.amount_sats
                        );

                        // Create the actual LN invoice
                        let wallet_config = tirami_lightning::node::WalletConfig::default();
                        let wallet = tirami_lightning::ForgeWallet::start(wallet_config)?;
                        let bolt11 = wallet.create_invoice(
                            invoice_info.amount_msats,
                            &invoice_info.description,
                            3600,
                        )?;
                        println!("Lightning invoice: {}", bolt11);
                        println!("Share this invoice with the consumer to receive payment.");
                    } else {
                        println!("\nNo positive net CU to settle.");
                    }
                } else {
                    println!("\nNo provider with positive net CU in this window.");
                }
            }
        }
        Commands::Agent {
            action,
            url,
            api_token,
        } => {
            let base = url.trim_end_matches('/');
            let client = reqwest::Client::new();
            let resolved_api_token = resolve_api_token(api_token);

            let mut status_request = client.get(format!("{base}/v1/tirami/agent/status"));
            if let Some(token) = resolved_api_token.as_deref() {
                status_request = status_request.bearer_auth(token);
            }
            let resp = status_request.send().await?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                eprintln!("Error: HTTP {} — {}", status, body);
                std::process::exit(1);
            }
            let json: serde_json::Value = resp.json().await?;

            match action {
                AgentCommands::Status => {
                    let configured = json["configured"].as_bool().unwrap_or(false);
                    println!("Personal Agent");
                    println!("──────────────────────────────────");
                    if !configured {
                        println!("  Configured:       no");
                        if let Some(s) = json["summary"].as_str() {
                            println!("  Summary:          {}", s);
                        }
                        return Ok(());
                    }
                    println!("  Configured:       yes");
                    if let Some(w) = json["wallet"].as_str() {
                        println!("  Wallet:           {}", w);
                    }
                    println!("  Earned today:     {} TRM", json["earned_today_trm"]);
                    println!("  Spent today:      {} TRM", json["spent_today_trm"]);
                    println!("  Net today:        {} TRM", json["net_today_trm"]);
                    if let Some(prefs) = json.get("preferences") {
                        println!("  Preferences:");
                        println!("    Daily cap:      {} TRM", prefs["daily_spend_limit_trm"]);
                        println!("    Per-task cap:   {} TRM", prefs["per_task_budget_trm"]);
                        println!("    Auto-earn:      {}", prefs["auto_earn_enabled"]);
                        println!("    Auto-spend:     {}", prefs["auto_spend_enabled"]);
                        println!("    Auto-stake:     {}", prefs["auto_stake_fraction"]);
                        println!(
                            "    Idle threshold: {}",
                            prefs["idle_utilization_threshold"]
                        );
                        println!("    Idle grace (s): {}", prefs["idle_grace_seconds"]);
                        println!("    Min peer rep:   {}", prefs["min_peer_reputation"]);
                        println!("    Content filter: {}", prefs["content_filter"]);
                    }
                    if let Some(s) = json["summary"].as_str() {
                        println!("  Summary:          {}", s);
                    }
                }
                AgentCommands::Summary => {
                    if let Some(s) = json["summary"].as_str() {
                        println!("{}", s);
                    } else {
                        println!("{}", serde_json::to_string(&json)?);
                    }
                }
                AgentCommands::Chat {
                    prompt,
                    max_tokens,
                    size,
                    estimated_trm,
                    peer_node_id,
                    peer_url,
                    peer_api_token,
                } => {
                    // Previous `resp` / `json` were bound for status;
                    // we just shadow them below.
                    let mut body = serde_json::json!({
                        "prompt": prompt,
                        "max_tokens": max_tokens,
                    });
                    if let Some(s) = size {
                        body["size"] = serde_json::json!(s);
                    }
                    if let Some(t) = estimated_trm {
                        body["estimated_trm"] = serde_json::json!(t);
                    }
                    match (peer_node_id, peer_url, peer_api_token) {
                        (Some(node_id), Some(url), maybe_peer_token) => {
                            let mut peer = serde_json::json!({
                                "node_id": node_id,
                                "url": url,
                            });
                            if let Some(token) = maybe_peer_token {
                                peer["api_token"] = serde_json::json!(token);
                            }
                            body["peer"] = peer;
                        }
                        (Some(_), None, _) | (None, Some(_), _) | (None, None, Some(_)) => {
                            eprintln!(
                                "Error: --peer-node-id and --peer-url must be supplied together; --peer-api-token requires both"
                            );
                            std::process::exit(2);
                        }
                        (None, None, None) => {}
                    }
                    let mut request = client
                        .post(format!("{base}/v1/tirami/agent/task"))
                        .json(&body);
                    if let Some(token) = resolved_api_token.as_deref() {
                        request = request.bearer_auth(token);
                    }
                    let resp = request.send().await?;
                    let status = resp.status();
                    let json: serde_json::Value = resp.json().await.unwrap_or_default();
                    if !status.is_success() {
                        let detail = json
                            .as_object()
                            .and_then(|o| o.get("error").or_else(|| o.get("reason")))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| json.to_string());
                        eprintln!("Error: HTTP {} — {}", status, detail);
                        std::process::exit(1);
                    }
                    let task_status = json["status"].as_str().unwrap_or("unknown");
                    match task_status {
                        "run_local" | "run_remote" => {
                            if let Some(out) = json["output"].as_str() {
                                println!("{}", out);
                            }
                            let trm = json["cost_trm"].as_u64().unwrap_or(0);
                            let where_ = if task_status == "run_local" {
                                "local".to_string()
                            } else {
                                let p = json["provider"].as_str().unwrap_or("?");
                                format!("remote (via {p})")
                            };
                            eprintln!("\n— {} · {} TRM", where_, trm);
                        }
                        "ask_user" => {
                            let reason = json["reason"]
                                .as_str()
                                .unwrap_or("agent wants confirmation");
                            let cost = json["estimated_cost_trm"].as_u64().unwrap_or(0);
                            println!("(agent paused — {})", reason);
                            println!("  estimated cost: {} TRM", cost);
                            println!(
                                "  re-run with higher --estimated-trm or add --peer-node-id/--peer-url to proceed."
                            );
                        }
                        "pending" => {
                            let reason = json["reason"]
                                .as_str()
                                .unwrap_or("remote dispatch scaffold");
                            println!("(pending — {})", reason);
                        }
                        "refused" => {
                            let reason = json["reason"].as_str().unwrap_or("refused");
                            println!("(agent refused — {})", reason);
                        }
                        other => {
                            println!("(unknown status '{other}')");
                            println!("{}", serde_json::to_string_pretty(&json)?);
                        }
                    }
                }
            }
        }
        Commands::Su { action, url } => {
            let base = url.trim_end_matches('/');
            let client = reqwest::Client::new();
            let placeholder_node_id =
                "0000000000000000000000000000000000000000000000000000000000000000";

            match action {
                SuCommands::Supply => {
                    let resp = client
                        .get(format!("{base}/v1/tirami/su/supply"))
                        .send()
                        .await?;
                    if !resp.status().is_success() {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        eprintln!("Error: HTTP {} — {}", status, body);
                        std::process::exit(1);
                    }
                    let json: serde_json::Value = resp.json().await?;
                    println!("TRM Supply");
                    println!("──────────────────────────────────");
                    println!("  Total supply:       {}", json["total_supply"]);
                    println!("  Total minted:       {}", json["total_minted"]);
                    println!("  Supply factor:      {}", json["supply_factor"]);
                    println!("  Current epoch:      {}", json["current_epoch"]);
                    println!("  Epoch yield rate:   {}", json["epoch_yield_rate"]);
                    println!("  Effective mint rate: {}", json["effective_mint_rate"]);
                }
                SuCommands::Stake { amount, duration } => {
                    let body = serde_json::json!({
                        "node_id": placeholder_node_id,
                        "amount": amount,
                        "duration": duration,
                    });
                    let resp = client
                        .post(format!("{base}/v1/tirami/su/stake"))
                        .json(&body)
                        .send()
                        .await?;
                    if !resp.status().is_success() {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        eprintln!("Error: HTTP {} — {}", status, body);
                        std::process::exit(1);
                    }
                    let json: serde_json::Value = resp.json().await?;
                    println!("Stake created");
                    println!("──────────────────────────────────");
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
                SuCommands::Unstake { stake_index } => {
                    let body = serde_json::json!({
                        "node_id": placeholder_node_id,
                        "stake_index": stake_index,
                    });
                    let resp = client
                        .post(format!("{base}/v1/tirami/su/unstake"))
                        .json(&body)
                        .send()
                        .await?;
                    if !resp.status().is_success() {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        eprintln!("Error: HTTP {} — {}", status, body);
                        std::process::exit(1);
                    }
                    let json: serde_json::Value = resp.json().await?;
                    println!("Unstake complete");
                    println!("──────────────────────────────────");
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
                SuCommands::Refer { referrer, referred } => {
                    let body = serde_json::json!({
                        "referrer": referrer,
                        "referred": referred,
                    });
                    let resp = client
                        .post(format!("{base}/v1/tirami/su/refer"))
                        .json(&body)
                        .send()
                        .await?;
                    if !resp.status().is_success() {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        eprintln!("Error: HTTP {} — {}", status, body);
                        std::process::exit(1);
                    }
                    let json: serde_json::Value = resp.json().await?;
                    println!("Referral recorded");
                    println!("──────────────────────────────────");
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
                SuCommands::Referrals => {
                    let resp = client
                        .get(format!("{base}/v1/tirami/su/referrals"))
                        .send()
                        .await?;
                    if !resp.status().is_success() {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        eprintln!("Error: HTTP {} — {}", status, body);
                        std::process::exit(1);
                    }
                    let json: serde_json::Value = resp.json().await?;
                    println!("Referral Stats");
                    println!("──────────────────────────────────");
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
            }
        }
    }

    Ok(())
}

fn resolve_api_token(flag: Option<String>) -> Option<String> {
    // `TIRAMI_API_TOKEN` is the primary env var; `FORGE_API_TOKEN` is
    // accepted as a legacy alias so older operator scripts still work
    // (fix #77).
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
    use std::fs;

    let tirami_dir = ensure_default_data_dir()?;
    let key_path = tirami_dir.join("node.key");
    if !key_path.exists() {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut OsRng);
        fs::write(&key_path, signing_key.to_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&key_path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&key_path, perms)?;
        }
        println!("🔑 Generated new node key at {}", key_path.display());
    }

    Ok(key_path)
}

fn load_node_secret_key(path: &Path) -> anyhow::Result<iroh::SecretKey> {
    let bytes = std::fs::read(path)?;
    let raw = parse_node_secret_key_bytes(&bytes)
        .map_err(|reason| anyhow::anyhow!("invalid node key file {}: {reason}", path.display()))?;
    Ok(iroh::SecretKey::from_bytes(&raw))
}

fn parse_node_secret_key_bytes(bytes: &[u8]) -> Result<[u8; 32], &'static str> {
    if bytes.len() == 32 {
        let mut raw = [0u8; 32];
        raw.copy_from_slice(bytes);
        return Ok(raw);
    }

    let Ok(text) = std::str::from_utf8(bytes) else {
        return Err("expected 32 raw bytes or 64 lowercase/uppercase hex characters");
    };
    let text = text.trim();
    if text.len() != 64 || !text.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return Err("expected 32 raw bytes or 64 lowercase/uppercase hex characters");
    }

    let decoded =
        hex::decode(text).map_err(|_| "expected valid hex-encoded Ed25519 secret bytes")?;
    let mut raw = [0u8; 32];
    raw.copy_from_slice(&decoded);
    Ok(raw)
}

fn run_identity_command() -> anyhow::Result<()> {
    let key_path = ensure_default_node_key()?;
    let secret_key = load_node_secret_key(&key_path)?;
    let public_key = secret_key.public();
    let node_id = tirami_core::NodeId(*public_key.as_bytes());

    println!("Node key: {}", key_path.display());
    println!("Public key: {}", public_key);
    println!("Node ID: {}", node_id.to_hex());
    println!("Direct bootstrap peer: {}@IP:PORT", public_key);
    Ok(())
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

    #[test]
    fn node_secret_key_parser_accepts_raw_and_hex() {
        let raw = [7u8; 32];
        assert_eq!(parse_node_secret_key_bytes(&raw).unwrap(), raw);
        assert_eq!(
            parse_node_secret_key_bytes(hex::encode(raw).as_bytes()).unwrap(),
            raw
        );
    }

    #[test]
    fn node_secret_key_parser_rejects_wrong_length() {
        assert!(parse_node_secret_key_bytes(b"too-short").is_err());
    }
}

/// One-command bootstrap: `tirami start`.
///
/// Bitcoin-style zero-config participation:
/// 1. Generate ~/.tirami/node.key if missing (Ed25519)
/// 2. Wire durable ledger/bank/agora/mind/archive state under ~/.tirami
/// 3. Resolve & download model from HuggingFace if missing
/// 4. Start seed node (P2P + HTTP API + inference)
/// 5. Print welcome banner with earning estimates
async fn run_start_command(
    model: String,
    port: u16,
    bind: String,
    no_agent: bool,
    bootstrap_peers: Vec<String>,
    api_token: Option<String>,
    p2p_bind: Option<String>,
) -> anyhow::Result<()> {
    use std::fs;

    // ------------------------------------------------------------------
    // Phase 1: Resolve ~/.tirami/ directory
    // ------------------------------------------------------------------
    let home = std::env::var("HOME").map_err(|_| anyhow::anyhow!("HOME not set"))?;
    let tirami_dir = PathBuf::from(&home).join(".tirami");
    if !tirami_dir.exists() {
        fs::create_dir_all(&tirami_dir)?;
        println!("📁 Created {}", tirami_dir.display());
    }

    // ------------------------------------------------------------------
    // Phase 2: Key generation (only if missing)
    // ------------------------------------------------------------------
    let key_path = tirami_dir.join("node.key");
    let key_was_generated = if !key_path.exists() {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut OsRng);
        fs::write(&key_path, signing_key.to_bytes())?;
        // Secure file permissions (user read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&key_path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&key_path, perms)?;
        }
        println!("🔑 Generated new node key at {}", key_path.display());
        true
    } else {
        false
    };

    // ------------------------------------------------------------------
    // Phase 3: Ledger path
    // ------------------------------------------------------------------
    let ledger_path = tirami_dir.join("ledger.json");
    let bootstrap_peers = resolve_bootstrap_peers(bootstrap_peers);
    let api_bearer_token = resolve_api_token_for_bind(&bind, api_token)?;

    // ------------------------------------------------------------------
    // Phase 4: Print startup banner before model download (can be slow)
    // ------------------------------------------------------------------
    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║         🌱 Tirami — GPU Airbnb × AI Agent Economy            ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("   Data dir:  {}", tirami_dir.display());
    println!("   Model:     {}", model);
    println!("   Ledger:    {}", ledger_path.display());
    println!("   API:       http://{}:{}", bind, port);
    if let Some(bind) = p2p_bind.as_ref() {
        println!("   P2P bind:  {}", bind);
    }
    if !bootstrap_peers.is_empty() {
        println!("   Bootstrap: {} peer(s)", bootstrap_peers.len());
    }
    println!();
    println!("📦 Resolving model (will auto-download from HuggingFace if needed)...");

    // ------------------------------------------------------------------
    // Phase 5: Model resolution (downloads if missing)
    // ------------------------------------------------------------------
    let resolved = tirami_infer::model_registry::resolve(&model)?;
    let tokenizer_path = resolved.tokenizer_path.ok_or_else(|| {
        anyhow::anyhow!(
            "Tokenizer not found for model '{}'. Try a catalog model like 'qwen2.5:0.5b'.",
            model
        )
    })?;
    println!("✅ Model ready: {}", resolved.model_path.display());

    // ------------------------------------------------------------------
    // Phase 6: Build config + seed node
    // ------------------------------------------------------------------
    let mut config = Config::for_data_dir(&tirami_dir);
    config.api_port = port;
    config.api_bind_addr = bind.clone();
    config.api_bearer_token = api_bearer_token;
    config.p2p_bind_addr = p2p_bind;
    config.bootstrap_peers = bootstrap_peers;
    config.share_compute = true;
    // Phase 18.5-part-3e — killer-app ergonomics: `tirami start`
    // yields a configured PersonalAgent by default. --no-agent
    // flips this off for operators running pure-server nodes.
    config.personal_agent_enabled = !no_agent;
    let mut node = tirami_node::TiramiNode::new(config);

    println!("🧠 Loading model into memory (this may take 10-60 seconds)...");
    node.load_model(&resolved.model_path, &tokenizer_path)
        .await?;
    println!("✅ Model loaded");

    // ------------------------------------------------------------------
    // Phase 7: Ctrl-C handler (same as seed command)
    // ------------------------------------------------------------------
    let shutdown_ledger = node.ledger.clone();
    let shutdown_ledger_path = node.config.ledger_path.clone();
    let shutdown_bank = node.bank.clone();
    let shutdown_marketplace = node.marketplace.clone();
    let shutdown_mind = node.mind_agent.clone();
    let shutdown_personal_agent = node.personal_agent.clone();
    let shutdown_config = node.config.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            println!("\n💾 Persisting state and shutting down...");
            if let Some(path) = shutdown_ledger_path {
                let _ = shutdown_ledger.lock().await.save_to_path(&path);
            }
            if let Some(ref path) = shutdown_config.bank_state_path {
                let bank = shutdown_bank.lock().await;
                let _ = tirami_node::state_persist::save_bank(&*bank, path);
            }
            if let Some(ref path) = shutdown_config.marketplace_state_path {
                let mp = shutdown_marketplace.lock().await;
                let _ = tirami_node::state_persist::save_marketplace(&*mp, path);
            }
            if let Some(ref path) = shutdown_config.mind_state_path {
                let mind = shutdown_mind.lock().await;
                if let Some(agent) = mind.as_ref() {
                    let _ = tirami_node::state_persist::save_mind(agent, path);
                }
            }
            if let Some(ref path) = shutdown_config.personal_agent_state_path {
                let personal = shutdown_personal_agent.lock().await;
                if let Some(agent) = personal.as_ref() {
                    let _ = tirami_node::state_persist::save_personal_agent(agent, path);
                }
            }
            std::process::exit(0);
        }
    });

    // ------------------------------------------------------------------
    // Phase 8: Print ready banner
    // ------------------------------------------------------------------
    println!();
    println!("🟢 Tirami node is running. Press Ctrl-C to stop.");
    if key_was_generated {
        println!();
        println!("   💡 First-time setup complete.");
        println!("      Your node earns TRM by serving inference to AI agents.");
        println!(
            "      Run `tirami status --url http://{}:{}` in another terminal.",
            bind, port
        );
    }
    println!();

    // ------------------------------------------------------------------
    // Phase 9: Run seed (blocks until Ctrl-C)
    // ------------------------------------------------------------------
    node.run_seed().await?;

    Ok(())
}
