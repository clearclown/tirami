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

    let raw = String::from_utf8_lossy(&output.stdout);
    let text = extract_generated_text(&raw, prompt);

    // Estimate token count from output length (rough approximation)
    let token_count = text.split_whitespace().count().max(1);

    Ok((text, token_count))
}

/// Pull the model's completion out of llama-cli's stdout.
///
/// b10360's llama-cli prints a terminal UI regardless of `-no-cnv`,
/// `--no-display-prompt`, or `--simple-io`: a loading spinner, an ASCII
/// banner, build/model/ftype lines, a `available commands:` block, then the
/// echoed prompt, the completion, a timing line, and `Exiting...`.
///
/// Returning that whole blob as the answer is what the first version did — a
/// real split-inference call came back with 111 "tokens" of banner. So the
/// completion is bracketed out instead:
///
/// ```text
/// > The capital of France is        <- echoed prompt, last `> ` line
/// The capital of France is Paris.   <- what we want
///
/// [ Prompt: 585.8 t/s | Generation: 213.6 t/s ]   <- end marker
/// ```
///
/// Builds that print only the completion have neither marker, so an unmarked
/// output is returned trimmed and unchanged.
pub fn extract_generated_text(stdout: &str, prompt: &str) -> String {
    // The spinner writes backspaces; strip control characters other than
    // newline and tab so they cannot end up in an API response.
    let cleaned: String = stdout
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect();

    let lines: Vec<&str> = cleaned.lines().collect();

    // Start after the last echoed-prompt line, if there is one.
    let start = lines
        .iter()
        .rposition(|l| l.starts_with("> "))
        .map(|i| i + 1);

    // Stop at the timing line or the exit banner.
    let end = lines.iter().position(|l| {
        let l = l.trim_start();
        l.starts_with("[ Prompt:") || l == "Exiting..."
    });

    let body = match (start, end) {
        (Some(s), Some(e)) if s < e => lines[s..e].join("\n"),
        (Some(s), None) => lines[s..].join("\n"),
        (None, Some(e)) => lines[..e].join("\n"),
        // No markers: an older build that printed only the completion.
        (None, None) => cleaned.clone(),
        // Markers out of order — do not guess.
        _ => cleaned.clone(),
    };

    let body = body.trim();

    // `--no-display-prompt` is passed, but this build echoes the prompt into
    // the completion line anyway. Drop it when it leads.
    let prompt = prompt.trim();
    if !prompt.is_empty() {
        if let Some(rest) = body.strip_prefix(prompt) {
            return rest.trim().to_string();
        }
    }

    body.to_string()
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
/// Two signals, either of which is sufficient:
///
/// - **Per-layer assignment** (above). Present on b10360.
/// - **A non-zero `model buffer size`** for an RPC device — the shape issue
///   #164 quoted, and what older builds report.
///
/// The buffer-size signal needs care. b10360's llama-cli loads the model
/// **twice**, and the first pass reports `0.00 MiB` for *every* device,
/// local Metal included. Reading only the first occurrence and concluding the
/// number is unusable is wrong; the second pass carries the real figures
/// (`RPC0 … = 37.82 MiB` on the captured run). Any positive size counts, and a
/// zero is simply ignored rather than treated as proof of failure.
///
/// A bare `starts_with("load_tensors:")` never fires: real lines carry a
/// `TIMESTAMP LEVEL` prefix.
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

    /// Real stdout from b10360: spinner, ASCII banner, build info, a command
    /// list, the echoed prompt, the completion, a timing line, `Exiting...`.
    const REAL_STDOUT: &str = include_str!("../tests/fixtures/llama-cli-stdout-b10360.txt");

    /// The first version returned this whole blob as the answer — a live
    /// split-inference call came back with 111 "tokens" of banner.
    #[test]
    fn the_completion_is_pulled_out_of_the_terminal_ui() {
        let text = extract_generated_text(REAL_STDOUT, "The capital of France is");

        assert_eq!(text, "Paris.", "got: {text:?}");
        assert!(!text.contains("build"), "banner leaked: {text:?}");
        assert!(!text.contains("Prompt:"), "timing line leaked: {text:?}");
        assert!(!text.contains("Exiting"), "exit banner leaked: {text:?}");
    }

    /// The spinner writes backspaces; they must not reach an API response.
    #[test]
    fn control_characters_are_stripped() {
        let text = extract_generated_text(REAL_STDOUT, "The capital of France is");
        assert!(
            !text.chars().any(|c| c.is_control() && c != '\n' && c != '\t'),
            "control characters survived: {text:?}"
        );
    }

    /// A build that prints only the completion has neither marker.
    #[test]
    fn output_without_markers_passes_through() {
        assert_eq!(extract_generated_text("  Paris.\n", "irrelevant"), "Paris.");
    }

    /// This build echoes the prompt into the completion line despite
    /// `--no-display-prompt`.
    #[test]
    fn a_leading_prompt_echo_is_removed() {
        let out = "> Hello\nHello world\n\n[ Prompt: 1.0 t/s | Generation: 2.0 t/s ]";
        assert_eq!(extract_generated_text(out, "Hello"), "world");
    }

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

    /// b10360 loads the model twice. The first pass reports `0.00 MiB` for
    /// every device, the second the real figures. Stopping at the first
    /// occurrence is how one concludes the number is unusable; it is not.
    #[test]
    fn the_fixture_carries_both_a_zero_and_a_real_buffer_size() {
        assert!(
            REAL_SPLIT_OK.contains("RPC0[127.0.0.1:50052] model buffer size =     0.00 MiB"),
            "first pass reports zero"
        );
        assert!(
            REAL_SPLIT_OK.contains("RPC0[127.0.0.1:50052] model buffer size =    37.82 MiB"),
            "second pass reports the real size"
        );
        verify_layers_distributed(REAL_SPLIT_OK, 1).expect("a leading zero must not decide it");
    }

    /// Pins the per-layer signal on its own: a build that reports assignment
    /// but no buffer sizes must still be recognised as distributed.
    #[test]
    fn layer_assignment_alone_is_sufficient() {
        let only_layers = "\
0.00.079.606 D load_tensors: layer   0 assigned to device RPC0, is_swa = 0
0.00.079.609 D load_tensors: layer   1 assigned to device RPC0, is_swa = 0
0.00.079.612 D load_tensors: layer   2 assigned to device MTL0, is_swa = 0";
        verify_layers_distributed(only_layers, 1).expect("RPC0 took layers");
    }

    /// Pins the other direction: a device that only ever reported zero, with no
    /// layers assigned to it, contributed nothing.
    #[test]
    fn a_zero_size_with_no_layers_is_not_distributed() {
        let zero_only = "\
0.00.082.446 I load_tensors:         MTL0 model buffer size =    98.87 MiB
0.00.082.447 I load_tensors: RPC0[127.0.0.1:50052] model buffer size =     0.00 MiB";
        assert!(
            verify_layers_distributed(zero_only, 1).is_err(),
            "0.00 MiB and no assigned layers means the peer contributed nothing"
        );
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
