use clap::{Parser, Subcommand};
use tirami_core::Config;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "forge")]
#[command(about = "Forge — self-expanding LLM over encrypted P2P networks")]
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

        /// Path to the persisted ledger snapshot
        #[arg(long, default_value = "forge-ledger.json")]
        ledger: String,

        /// Optional bearer token protecting administrative HTTP API routes
        #[arg(long)]
        api_token: Option<String>,
    },

    /// Start as a worker node (connects to seed for inference)
    Worker {
        /// Seed node public key (hex string from seed output)
        #[arg(short, long)]
        seed: String,

        /// Seed relay URL (optional, for NAT traversal)
        #[arg(long)]
        relay: Option<String>,
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
        #[arg(short, long, default_value = "Forge inference")]
        description: String,
    },
    /// Pay a Lightning invoice
    Pay {
        /// BOLT11 invoice string
        invoice: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
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
                    println!("Lightning balance: {} sats", wallet.lightning_balance_sats());
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
            let tokenizer_path = resolved.tokenizer_path.or_else(|| tokenizer.map(PathBuf::from))
                .ok_or_else(|| anyhow::anyhow!(
                    "tokenizer path required for this model source — use --tokenizer"
                ))?;
            let model_path = resolved.model_path;

            node.load_model(&model_path, &tokenizer_path).await?;

            if let Some(prompt) = prompt {
                let start = std::time::Instant::now();
                let response = node.chat(&prompt, max_tokens, temperature).await?;
                let elapsed = start.elapsed();
                println!("{}", response);
                eprintln!("\n---\nGenerated in {:.2}s", elapsed.as_secs_f64());
            } else {
                println!("Forge Chat (type 'quit' to exit)");
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
            ledger,
            api_token,
        } => {
            let config = Config {
                api_port: port,
                api_bind_addr: bind,
                api_bearer_token: resolve_api_token(api_token),
                ledger_path: Some(PathBuf::from(&ledger)),
                share_compute: true,
                ..Config::default()
            };
            let mut node = tirami_node::TiramiNode::new(config);

            // Resolve model via the unified dispatcher (local path, HF URL, shorthand, catalog)
            let resolved = tirami_infer::model_registry::resolve(&model)?;
            let tokenizer_path = resolved.tokenizer_path.or_else(|| tokenizer.map(PathBuf::from))
                .ok_or_else(|| anyhow::anyhow!(
                    "tokenizer path required for this model source — use --tokenizer"
                ))?;
            let model_path = resolved.model_path;

            node.load_model(&model_path, &tokenizer_path).await?;

            tracing::info!("Starting as SEED node with model: {}", model);

            // Install Ctrl-C handler for graceful shutdown
            let shutdown_ledger = node.ledger.clone();
            let shutdown_ledger_path = node.config.ledger_path.clone();
            let shutdown_bank = node.bank.clone();
            let shutdown_marketplace = node.marketplace.clone();
            let shutdown_mind = node.mind_agent.clone();
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
                    std::process::exit(0);
                }
            });

            node.run_seed().await?;
        }
        Commands::Worker { seed, relay } => {
            let config = Config::default();
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

            // Interactive worker chat
            let node_id = transport.tirami_node_id();
            let peers = transport.connected_peers().await;
            let seed_peer_id = peers
                .first()
                .ok_or_else(|| anyhow::anyhow!("no seed peer found"))?
                .clone();

            println!("Forge Worker (connected to seed)");
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
                    "llama-cli not found. Set FORGE_LLAMA_CLI_PATH or install llama.cpp"
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
            let config = Config {
                api_port: port,
                api_bind_addr: bind,
                api_bearer_token: resolve_api_token(api_token),
                ledger_path: Some(PathBuf::from(&ledger)),
                ..Config::default()
            };
            let node = tirami_node::TiramiNode::new(config);

            // Resolve model spec via the unified dispatcher — handles local path,
            // HF URL, HF shorthand, catalog name, and ~/.models scan.
            if let Some(model) = model {
                let resolved = tirami_infer::model_registry::resolve(&model)?;
                let tokenizer_path = resolved.tokenizer_path
                    .or_else(|| tokenizer.map(PathBuf::from))
                    .ok_or_else(|| anyhow::anyhow!(
                        "tokenizer path required for this model source — use --tokenizer"
                    ))?;
                node.load_model(&resolved.model_path, &tokenizer_path).await?;
            }

            tracing::info!("Starting local API server on port {}", port);

            // Install Ctrl-C handler for graceful shutdown
            let shutdown_ledger = node.ledger.clone();
            let shutdown_ledger_path = node.config.ledger_path.clone();
            let shutdown_bank = node.bank.clone();
            let shutdown_marketplace = node.marketplace.clone();
            let shutdown_mind = node.mind_agent.clone();
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

            println!("Forge status: {}", status.status);
            println!("Model loaded: {}", status.model_loaded);
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

            println!("Forge topology: {}", topology.status);
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
                    "Local: node={} cpu={} mem={:.1}GB avail={:.1}GB metal={} region={}",
                    local.node_id.to_hex(),
                    local.cpu_cores,
                    local.memory_gb,
                    local.available_memory_gb,
                    local.metal_available,
                    local.region
                );
            } else {
                println!("Local capability: unavailable");
            }

            if topology.connected_peers.is_empty() {
                println!("Connected peers: none");
            } else {
                println!("Connected peers:");
                for peer in topology.connected_peers {
                    println!(
                        "  node={} cpu={} avail={:.1}GB metal={} region={}",
                        peer.node_id.to_hex(),
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
                    if let Some(invoice_info) =
                        tirami_lightning::payment::create_settlement_invoice(
                            provider.net_cu,
                            &rate,
                            hours,
                        )
                    {
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
    }

    Ok(())
}

fn resolve_api_token(flag: Option<String>) -> Option<String> {
    flag.or_else(|| std::env::var("FORGE_API_TOKEN").ok())
        .filter(|token| !token.is_empty())
}
