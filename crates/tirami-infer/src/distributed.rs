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

    // `-st` so a build whose llama-cli defaults to conversation mode exits
    // after one turn instead of blocking on an empty stdin — measured on
    // b10360, where the run hung until killed.
    cmd.arg("-st");

    // `--log-disable` used to be passed here, and removing it was not enough:
    // on b10360 the per-device load lines only appear at verbose level, so
    // without `-v` stderr comes back **empty** and every split would be
    // reported as "not distributed". See `verify_layers_distributed`.
    cmd.arg("-v");

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

/// Confirm llama.cpp actually placed layers on every RPC device.
///
/// `ggml-rpc` does not return an error when it cannot reach a server: it
/// reports `free = 0, total = 0` (ggml-rpc.cpp:828-833). llama.cpp then assigns
/// that device nothing and runs on one machine — **exit 0, normal output**.
/// Reproduced on an Apple M4 against llama.cpp b10360 by pointing `--rpc` at a
/// dead port: 62 of 62 layers landed on the local Metal device, generation ran
/// at 190.7 tok/s, and `RPC0` never appeared in the log at all.
///
/// The signal is per-layer device assignment:
///
/// ```text
/// 0.00.079.606 D load_tensors: layer   0 assigned to device RPC0, is_swa = 0
/// ```
///
/// Two things this deliberately does **not** rely on:
///
/// - **The `model buffer size` line.** Issue #164 quoted it, and it is present,
///   but on b10360 it reads `0.00 MiB` for *every* device — including the local
///   Metal one — on a split that demonstrably worked (32 layers on RPC0, 30 on
///   MTL0, 176.9 tok/s). Treating 0 as "contributed nothing" rejects healthy
///   runs, so a positive size is accepted as corroboration and a zero proves
///   nothing.
/// - **A bare `starts_with("load_tensors:")`.** Real lines carry a
///   `TIMESTAMP LEVEL` prefix; matching the start of the line never fires.
///
/// Requires llama-cli to run with `-v`. Without it stderr is empty.
pub fn verify_layers_distributed(
    llama_stderr: &str,
    expected_devices: usize,
) -> Result<(), TiramiError> {
    use std::collections::BTreeSet;

    let mut with_layers: BTreeSet<String> = BTreeSet::new();
    let mut sized: BTreeSet<String> = BTreeSet::new();

    for line in llama_stderr.lines() {
        // `load_tensors:` is mid-line, after a timestamp and level.
        let Some(rest) = line.split_once("load_tensors:").map(|(_, r)| r) else {
            continue;
        };

        if let Some(device) = rest
            .split_once("assigned to device ")
            .map(|(_, d)| d.trim_start())
        {
            let device: String = device
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if device.starts_with("RPC") {
                with_layers.insert(device);
            }
            continue;
        }

        if rest.contains("model buffer size") {
            if let Some(device) = rest.split_whitespace().find(|t| t.starts_with("RPC")) {
                let device: String = device
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                let size: f64 = rest
                    .rsplit('=')
                    .next()
                    .and_then(|tail| tail.split_whitespace().next())
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0.0);
                if size > 0.0 {
                    sized.insert(device);
                }
            }
        }
    }

    // Either signal is enough: b10360 gives per-layer assignment, older builds
    // give a non-zero buffer size.
    let live: BTreeSet<&String> = with_layers.union(&sized).collect();

    if live.len() >= expected_devices {
        return Ok(());
    }

    Err(TiramiError::InferenceError(format!(
        "distribution did not take effect: {} of {expected_devices} RPC devices \
         received layers{}. A ggml-rpc server that cannot be reached reports 0/0 \
         capacity instead of failing, so inference would have run on one machine \
         while exiting 0 and looking healthy.",
        live.len(),
        if live.is_empty() {
            String::new()
        } else {
            format!(
                " (only: {})",
                live.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            )
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

    /// Captured from a real run: Apple M4, llama.cpp b10360, `ggml-rpc-server
    /// -d MTL0` on loopback. 32 layers landed on RPC0, 30 on MTL0, generation
    /// ran at 176.9 tok/s.
    const REAL_SPLIT_OK: &str = include_str!("../tests/fixtures/split-ok-b10360.log");

    /// Same command with `--rpc` pointed at a dead port. llama-cli **exits 0**,
    /// puts all 62 layers on MTL0, generates at 190.7 tok/s, and never mentions
    /// RPC0 — the silent degradation #164 describes.
    const REAL_SILENTLY_LOCAL: &str =
        include_str!("../tests/fixtures/split-silently-local-b10360.log");

    #[test]
    fn a_real_working_split_is_accepted() {
        verify_layers_distributed(REAL_SPLIT_OK, 1)
            .expect("32 layers on RPC0 is a working split");
    }

    /// The case the check exists for. Nothing else in the output distinguishes
    /// it from success.
    #[test]
    fn a_real_silent_fallback_to_local_is_rejected() {
        let err = verify_layers_distributed(REAL_SILENTLY_LOCAL, 1)
            .expect_err("no RPC device took layers");
        assert!(err.to_string().contains("0 of 1"), "{err}");
    }

    /// b10360 reports `0.00 MiB` for *every* device, local included, on a split
    /// that worked. Treating a zero as "contributed nothing" would reject every
    /// healthy run on this build.
    #[test]
    fn a_zero_buffer_size_does_not_by_itself_mean_failure() {
        assert!(
            REAL_SPLIT_OK.contains("RPC0[127.0.0.1:50052] model buffer size =     0.00 MiB"),
            "fixture should still contain the zero-sized line"
        );
        verify_layers_distributed(REAL_SPLIT_OK, 1).expect("layer assignment outweighs the zero");
    }

    /// Older builds (the shape #164 quoted) report a real size and may not emit
    /// per-layer assignment. Both signals have to work.
    #[test]
    fn a_non_zero_buffer_size_alone_is_accepted() {
        let legacy = "\
load_tensors:      Metal model buffer size = 35021.00 MiB
load_tensors: RPC0[127.0.0.1:50052] model buffer size = 17434.00 MiB";
        verify_layers_distributed(legacy, 1).expect("a sized RPC device counts");
    }

    /// The timestamp + level prefix is why matching the start of the line fails.
    #[test]
    fn the_log_prefix_does_not_hide_the_signal() {
        assert!(
            REAL_SPLIT_OK
                .lines()
                .filter(|l| l.contains("load_tensors:"))
                .all(|l| !l.trim_start().starts_with("load_tensors:")),
            "real lines carry a prefix; a starts_with match would never fire"
        );
    }

    #[test]
    fn fewer_loaded_devices_than_requested_is_an_error() {
        // Two peers were asked for; only one took layers.
        assert!(verify_layers_distributed(REAL_SPLIT_OK, 2).is_err());
    }

    #[test]
    fn distributed_status_reports_availability() {
        let status = distributed_status();
        println!("llama-cli: {:?}", status.llama_cli_path);
        println!("rpc-server: {:?}", status.rpc_server_path);
    }
}
