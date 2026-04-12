use clap::{Parser, Subcommand};
use tirami_core::Config;
use std::path::PathBuf;

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = DaemonCli::parse();

    match cli.command {
        DaemonCommand::Seed {
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
        } => {
            let config = Config {
                api_port: port,
                api_bind_addr: bind,
                api_bearer_token: resolve_api_token(api_token),
                ledger_path: Some(PathBuf::from(&ledger)),
                ..Config::default()
            };
            let node = tirami_node::TiramiNode::new(config);

            if let (Some(model), Some(tokenizer)) = (model, tokenizer) {
                node.load_model(&PathBuf::from(&model), &PathBuf::from(&tokenizer))
                    .await?;
            }

            tracing::info!("Starting forged local API server on port {}", port);

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

fn resolve_api_token(flag: Option<String>) -> Option<String> {
    flag.or_else(|| std::env::var("FORGE_API_TOKEN").ok())
        .filter(|token| !token.is_empty())
}
