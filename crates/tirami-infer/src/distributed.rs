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
    /// `-ts` — proportional split across local + RPC devices, e.g. `"21,43"`.
    /// #164's working two-machine run needed this; without it llama.cpp picks
    /// its own ratio and can leave a device nearly empty.
    pub tensor_split: Option<String>,
    /// `-c` — context size. `None` leaves llama.cpp's default.
    pub context_size: Option<u32>,
    /// `--no-mmap`. Set for RPC runs: mapped weights are not what gets shipped
    /// to a remote device, and mmap makes the load timings unreadable.
    pub no_mmap: bool,
}

impl DistributedConfig {
    /// Minimal config: everything llama.cpp can default, defaulted.
    pub fn new(model_path: PathBuf, rpc_endpoints: Vec<String>, llama_cli_path: PathBuf) -> Self {
        Self {
            model_path,
            rpc_endpoints,
            n_gpu_layers: 99,
            llama_cli_path,
            tensor_split: None,
            context_size: None,
            no_mmap: true,
        }
    }
}

/// Find the llama-cli binary from trusted locations.
pub fn find_llama_cli() -> Option<PathBuf> {
    // Check env vars first. `TIRAMI_LLAMA_CLI_PATH` is the current
    // name; `FORGE_LLAMA_CLI_PATH` is accepted as a legacy alias so
    // operators with older setups still work (fix #77).
    for env_name in ["TIRAMI_LLAMA_CLI_PATH", "FORGE_LLAMA_CLI_PATH"] {
        if let Ok(path) = std::env::var(env_name) {
            let p = PathBuf::from(&path);
            if let Ok(canonical) = p.canonicalize() {
                if canonical.is_file() {
                    return Some(canonical);
                }
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
        .arg("--no-display-prompt");

    if let Some(split) = config.tensor_split.as_deref() {
        cmd.arg("-ts").arg(split);
    }
    if let Some(ctx) = config.context_size {
        cmd.arg("-c").arg(ctx.to_string());
    }
    if config.no_mmap {
        cmd.arg("--no-mmap");
    }

    // NOTE: `--log-disable` used to be passed here. It suppressed exactly the
    // `load_tensors: RPC0[...] model buffer size` lines that
    // `verify_layers_distributed` needs — see that function for why silence
    // is dangerous rather than tidy.
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let child = cmd
        .spawn()
        .map_err(|e| TiramiError::InferenceError(format!("spawn llama-cli: {e}")))?;

    let output = child
        .wait_with_output()
        .map_err(|e| TiramiError::InferenceError(format!("llama-cli wait: {e}")))?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        return Err(TiramiError::InferenceError(format!(
            "llama-cli failed (exit {}): {}",
            output.status,
            stderr.lines().last().unwrap_or("unknown error")
        )));
    }

    verify_layers_distributed(&stderr, config.rpc_endpoints.len())?;

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let text = text.trim().to_string();

    // Estimate token count from output length (rough approximation)
    let token_count = text.split_whitespace().count().max(1);

    Ok((text, token_count))
}

/// Confirm llama.cpp actually placed weights on every RPC device.
///
/// `ggml-rpc` does not return an error when it cannot reach a server: it
/// reports `free = 0, total = 0` (ggml-rpc.cpp:828-833). llama.cpp reads that
/// as a device with no capacity and assigns it zero layers. Inference then
/// **succeeds**, on one machine, and the logs look normal — you are simply not
/// distributed, and nothing says so (#164).
///
/// The load line is the only place the truth appears:
///
/// ```text
/// load_tensors: RPC0[127.0.0.1:50052] model buffer size = 17434.00 MiB
/// ```
///
/// A missing line, or one reading 0, means that peer contributed nothing.
pub fn verify_layers_distributed(
    llama_stderr: &str,
    expected_devices: usize,
) -> Result<(), TiramiError> {
    let mut live = 0usize;
    let mut zeroed = Vec::new();

    for line in llama_stderr.lines() {
        let line = line.trim();
        if !line.starts_with("load_tensors:") || !line.contains("model buffer size") {
            continue;
        }
        // Only RPC devices matter; the local backend always has layers.
        let Some(device) = line
            .split_whitespace()
            .find(|tok| tok.starts_with("RPC"))
        else {
            continue;
        };

        let size: f64 = line
            .rsplit('=')
            .next()
            .and_then(|tail| tail.split_whitespace().next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(0.0);

        if size > 0.0 {
            live += 1;
        } else {
            zeroed.push(device.to_string());
        }
    }

    if live == expected_devices {
        return Ok(());
    }

    Err(TiramiError::InferenceError(format!(
        "distribution did not take effect: {live} of {expected_devices} RPC devices \
         received layers{}. A ggml-rpc server that cannot be reached reports 0/0 \
         capacity instead of failing, so inference would have run on one machine \
         while appearing healthy.",
        if zeroed.is_empty() {
            String::new()
        } else {
            format!(" (zero-sized: {})", zeroed.join(", "))
        }
    )))
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

    /// Shape taken from a real two-machine run (#164): local Metal device
    /// plus one RPC device, both loaded.
    const LOADED: &str = "\
llama_model_loader: loaded meta data with 30 key-value pairs
load_tensors: offloading 62 repeating layers to GPU
load_tensors:      Metal model buffer size = 35021.00 MiB
load_tensors: RPC0[127.0.0.1:50052] model buffer size = 17434.00 MiB
llama_context: n_ctx = 8192";

    /// The dangerous case. `ggml-rpc` returns free/total = 0/0 rather than an
    /// error when it cannot reach a server, so llama.cpp assigns the device no
    /// layers, inference succeeds on one machine, and nothing says so.
    const SILENTLY_NOT_DISTRIBUTED: &str = "\
load_tensors:      Metal model buffer size = 52455.00 MiB
load_tensors: RPC0[127.0.0.1:50052] model buffer size = 0.00 MiB
llama_context: n_ctx = 8192";

    /// Worse still: the device may not appear at all.
    const RPC_DEVICE_ABSENT: &str = "\
load_tensors:      Metal model buffer size = 52455.00 MiB
llama_context: n_ctx = 8192";

    #[test]
    fn distribution_is_confirmed_when_every_rpc_device_has_layers() {
        verify_layers_distributed(LOADED, 1).expect("one loaded RPC device");
    }

    #[test]
    fn a_zero_sized_rpc_device_is_an_error() {
        let err = verify_layers_distributed(SILENTLY_NOT_DISTRIBUTED, 1)
            .expect_err("0.00 MiB means the peer contributed nothing");
        let msg = err.to_string();
        assert!(msg.contains("0 of 1"), "{msg}");
        assert!(msg.contains("RPC0"), "{msg}");
    }

    #[test]
    fn a_missing_rpc_device_is_an_error() {
        assert!(
            verify_layers_distributed(RPC_DEVICE_ABSENT, 1).is_err(),
            "a device that never reported must not pass as distributed"
        );
    }

    #[test]
    fn fewer_loaded_devices_than_requested_is_an_error() {
        // Two peers were asked for; only one took layers.
        assert!(verify_layers_distributed(LOADED, 2).is_err());
    }

    #[test]
    fn distributed_status_reports_availability() {
        let status = distributed_status();
        println!("llama-cli: {:?}", status.llama_cli_path);
        println!("rpc-server: {:?}", status.rpc_server_path);
    }
}
