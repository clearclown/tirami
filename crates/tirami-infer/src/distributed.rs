//! Distributed inference orchestrator.
//!
//! Wraps llama.cpp's `llama-cli` with `--rpc` flag for split inference
//! across multiple machines. Forge provides the P2P discovery and
//! orchestration; llama.cpp handles the tensor computation.
//!
//! ## Architecture
//!
//! ```text
//! forge-infer (this crate)
//!   │
//!   ├── LlamaCppEngine       — local inference via llama-cpp-2 library
//!   └── DistributedEngine    — distributed inference via llama-cli subprocess
//!         │
//!         ├── llama-cli --rpc peer1:port,peer2:port -m model.gguf
//!         └── Peers run rpc-server via RpcServer::spawn()
//! ```

use tirami_core::TiramiError;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Configuration for a distributed inference session.
#[derive(Debug, Clone)]
pub struct DistributedConfig {
    /// Path to the GGUF model file.
    pub model_path: PathBuf,
    /// RPC server endpoints (host:port pairs).
    pub rpc_endpoints: Vec<String>,
    /// Number of GPU layers to offload (0 = CPU only).
    pub n_gpu_layers: u32,
    /// Path to llama-cli binary.
    pub llama_cli_path: PathBuf,
}

/// Find the llama-cli binary from trusted locations.
pub fn find_llama_cli() -> Option<PathBuf> {
    // Check env var first
    if let Ok(path) = std::env::var("FORGE_LLAMA_CLI_PATH") {
        let p = PathBuf::from(&path);
        if let Ok(canonical) = p.canonicalize() {
            if canonical.is_file() {
                return Some(canonical);
            }
        }
    }

    // Trusted locations only (no arbitrary PATH search)
    for candidate in &[
        "/tmp/llama.cpp/build/bin/llama-cli",
        "/usr/local/bin/llama-cli",
        "/opt/homebrew/bin/llama-cli",
    ] {
        let p = PathBuf::from(candidate);
        if let Ok(canonical) = p.canonicalize() {
            if canonical.is_file() {
                return Some(canonical);
            }
        }
    }

    None
}

/// Validate an RPC endpoint string (must be host:port format).
fn validate_rpc_endpoint(endpoint: &str) -> Result<(), TiramiError> {
    let parts: Vec<&str> = endpoint.split(':').collect();
    if parts.len() != 2 {
        return Err(TiramiError::InferenceError(format!(
            "invalid RPC endpoint (expected host:port): {}",
            endpoint
        )));
    }
    // Validate port is numeric
    let port: u16 = parts[1].parse().map_err(|_| {
        TiramiError::InferenceError(format!("invalid port in endpoint: {}", endpoint))
    })?;
    if port < 1024 {
        return Err(TiramiError::InferenceError(format!(
            "privileged port in endpoint: {}",
            endpoint
        )));
    }
    // Reject shell metacharacters in host
    let host = parts[0];
    if host.contains(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_') {
        return Err(TiramiError::InferenceError(format!(
            "invalid characters in host: {}",
            host
        )));
    }
    Ok(())
}

/// Run distributed inference using llama-cli with --rpc.
///
/// Returns the generated text and token count.
pub fn run_distributed_inference(
    config: &DistributedConfig,
    prompt: &str,
    max_tokens: u32,
    temperature: f32,
) -> Result<(String, usize), TiramiError> {
    if config.rpc_endpoints.is_empty() {
        return Err(TiramiError::InferenceError(
            "no RPC endpoints configured".to_string(),
        ));
    }

    // Validate all endpoints
    for endpoint in &config.rpc_endpoints {
        validate_rpc_endpoint(endpoint)?;
    }

    // Validate model path
    let model_path = config
        .model_path
        .canonicalize()
        .map_err(|e| TiramiError::InferenceError(format!("invalid model path: {e}")))?;

    // Validate llama-cli path
    let cli_path = config
        .llama_cli_path
        .canonicalize()
        .map_err(|e| TiramiError::InferenceError(format!("invalid llama-cli path: {e}")))?;
    if !cli_path.is_file() {
        return Err(TiramiError::InferenceError(
            "llama-cli is not a file".to_string(),
        ));
    }

    // Sanitize prompt — reject null bytes
    if prompt.contains('\0') {
        return Err(TiramiError::InferenceError(
            "prompt contains null bytes".to_string(),
        ));
    }

    let rpc_arg = config.rpc_endpoints.join(",");

    tracing::info!(
        "Distributed inference: model={:?}, rpc={}, max_tokens={}, temp={}",
        config.model_path,
        rpc_arg,
        max_tokens,
        temperature
    );

    let mut cmd = Command::new(&cli_path);
    cmd.arg("--rpc")
        .arg(&rpc_arg)
        .arg("-m")
        .arg(&model_path)
        .arg("-p")
        .arg(prompt)
        .arg("-n")
        .arg(max_tokens.to_string())
        .arg("--temp")
        .arg(format!("{:.2}", temperature))
        .arg("-ngl")
        .arg(config.n_gpu_layers.to_string())
        .arg("--no-display-prompt")
        .arg("--log-disable")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = cmd
        .spawn()
        .map_err(|e| TiramiError::InferenceError(format!("spawn llama-cli: {e}")))?;

    let output = child
        .wait_with_output()
        .map_err(|e| TiramiError::InferenceError(format!("llama-cli wait: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TiramiError::InferenceError(format!(
            "llama-cli failed (exit {}): {}",
            output.status,
            stderr.lines().last().unwrap_or("unknown error")
        )));
    }

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let text = text.trim().to_string();

    // Estimate token count from output length (rough approximation)
    let token_count = text.split_whitespace().count().max(1);

    Ok((text, token_count))
}

/// Check if distributed inference is available (llama-cli + rpc-server binaries exist).
pub fn is_distributed_available() -> bool {
    find_llama_cli().is_some() && super::rpc_manager::is_rpc_available()
}

/// Get a status summary of distributed inference capabilities.
pub fn distributed_status() -> DistributedStatus {
    DistributedStatus {
        llama_cli_available: find_llama_cli().is_some(),
        llama_cli_path: find_llama_cli(),
        rpc_server_available: super::rpc_manager::is_rpc_available(),
        rpc_server_path: super::rpc_manager::RpcServer::find_binary(),
    }
}

#[derive(Debug, Clone)]
pub struct DistributedStatus {
    pub llama_cli_available: bool,
    pub llama_cli_path: Option<PathBuf>,
    pub rpc_server_available: bool,
    pub rpc_server_path: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_llama_cli_does_not_panic() {
        let _ = find_llama_cli();
    }

    #[test]
    fn distributed_status_reports_availability() {
        let status = distributed_status();
        println!("llama-cli: {:?}", status.llama_cli_path);
        println!("rpc-server: {:?}", status.rpc_server_path);
    }
}
