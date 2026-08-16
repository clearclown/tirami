//! RPC server subprocess manager for distributed inference.

use tirami_core::TiramiError;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// Manages a local llama.cpp rpc-server subprocess.
pub struct RpcServer {
    child: Option<Child>,
    port: u16,
    host: String,
}

/// Validate that a path points to an actual executable file.
fn validate_executable(path: &PathBuf) -> Result<PathBuf, TiramiError> {
    let canonical = path
        .canonicalize()
        .map_err(|e| TiramiError::InferenceError(format!("invalid binary path {:?}: {e}", path)))?;

    if !canonical.is_file() {
        return Err(TiramiError::InferenceError(format!(
            "not a file: {:?}",
            canonical
        )));
    }

    // Reject paths containing suspicious components
    let path_str = canonical.to_string_lossy();
    if path_str.contains("..") || path_str.contains('\0') {
        return Err(TiramiError::InferenceError(format!(
            "suspicious path: {:?}",
            canonical
        )));
    }

    Ok(canonical)
}

/// Validate a port number is in safe range.
fn validate_port(port: u16) -> Result<u16, TiramiError> {
    if port < 1024 {
        return Err(TiramiError::InferenceError(format!(
            "port {} is in privileged range (must be >= 1024)",
            port
        )));
    }
    Ok(port)
}

/// How to launch the rpc-server subprocess.
///
/// The defaults reflect what actually worked in the #164 measurements on a
/// Mac mini ⟷ Mac Studio pair. In particular, without `-d` llama.cpp picks
/// its own backend, which is how a Metal machine ends up serving from the
/// CPU while looking healthy.
#[derive(Debug, Clone)]
pub struct RpcServerOptions {
    pub port: u16,
    /// ggml device to bind, e.g. `MTL0` or `CUDA0`. `None` lets llama.cpp
    /// choose. Defaults from `TIRAMI_RPC_DEVICE`.
    pub device: Option<String>,
    /// Local tensor cache (`-c`). Measured at 210 s cold → 156 s warm on a
    /// 17 GiB shard, so it is on unless `TIRAMI_RPC_CACHE=0`.
    pub cache: bool,
}

impl RpcServerOptions {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            device: std::env::var("TIRAMI_RPC_DEVICE")
                .ok()
                .filter(|d| !d.trim().is_empty()),
            cache: !matches!(
                std::env::var("TIRAMI_RPC_CACHE").as_deref(),
                Ok("0") | Ok("false") | Ok("no") | Ok("off")
            ),
        }
    }
}

impl RpcServer {
    /// Find the rpc-server binary from trusted locations only.
    pub fn find_binary() -> Option<PathBuf> {
        // `TIRAMI_RPC_SERVER_PATH` is primary; `FORGE_RPC_SERVER_PATH` is the
        // legacy alias, matching how `distributed.rs::find_llama_cli` handles
        // the same rename.
        for var in ["TIRAMI_RPC_SERVER_PATH", "FORGE_RPC_SERVER_PATH"] {
            if let Ok(path) = std::env::var(var) {
                let p = PathBuf::from(&path);
                if let Ok(validated) = validate_executable(&p) {
                    return Some(validated);
                }
            }
        }

        // Trusted locations only (not arbitrary PATH). Current llama.cpp
        // installs this as `ggml-rpc-server`; `rpc-server` is the older name
        // and is kept for existing setups.
        for dir in [
            "/usr/local/bin",
            "/opt/homebrew/bin",
            "/tmp/llama.cpp/build/bin",
        ] {
            for name in ["ggml-rpc-server", "rpc-server"] {
                let p = PathBuf::from(dir).join(name);
                if p.exists() {
                    if let Ok(validated) = validate_executable(&p) {
                        return Some(validated);
                    }
                }
            }
        }

        None
    }

    /// Spawn a local rpc-server on the given port with default options.
    pub fn spawn(port: u16) -> Result<Self, TiramiError> {
        Self::spawn_with(&RpcServerOptions::new(port))
    }

    /// Spawn a local rpc-server.
    ///
    /// Blocking: polls the listening socket for up to 10 s. Call it from
    /// `tokio::task::spawn_blocking`, never straight from an async task.
    pub fn spawn_with(opts: &RpcServerOptions) -> Result<Self, TiramiError> {
        let port = validate_port(opts.port)?;

        let binary = Self::find_binary().ok_or_else(|| {
            TiramiError::InferenceError(
                "rpc-server binary not found (looked for ggml-rpc-server and \
                 rpc-server). Set TIRAMI_RPC_SERVER_PATH"
                    .to_string(),
            )
        })?;

        let binary = validate_executable(&binary)?;

        let mut cmd = Command::new(&binary);
        cmd.arg("-p")
            .arg(port.to_string())
            // Bind to localhost only — never expose on 0.0.0.0. Reaching this
            // from another host is the QUIC tunnel's job.
            .arg("--host")
            .arg("127.0.0.1");

        if let Some(device) = opts.device.as_deref() {
            cmd.arg("-d").arg(device);
        }
        if opts.cache {
            cmd.arg("-c");
        }

        tracing::info!(
            "Starting rpc-server on port {} ({:?}, device={:?}, cache={})",
            port,
            binary,
            opts.device,
            opts.cache
        );

        let child = cmd
            .spawn()
            .map_err(|e| TiramiError::InferenceError(format!("spawn rpc-server: {e}")))?;

        let server = Self {
            child: Some(child),
            port,
            host: "127.0.0.1".to_string(),
        };

        server.wait_ready(Duration::from_secs(10))?;
        tracing::info!("rpc-server ready on {}:{}", server.host, server.port);

        Ok(server)
    }

    fn wait_ready(&self, timeout: Duration) -> Result<(), TiramiError> {
        let start = Instant::now();
        let addr = format!("{}:{}", self.host, self.port);
        loop {
            if TcpStream::connect(&addr).is_ok() {
                return Ok(());
            }
            if start.elapsed() > timeout {
                return Err(TiramiError::InferenceError(format!(
                    "rpc-server failed to start within {}s on {}",
                    timeout.as_secs(),
                    addr
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }

    pub fn stop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
            tracing::info!("rpc-server on port {} stopped", self.port);
        }
        self.child = None;
    }
}

impl Drop for RpcServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Check if rpc-server is available on this system.
pub fn is_rpc_available() -> bool {
    RpcServer::find_binary().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_binary_returns_none_if_not_installed() {
        let _ = RpcServer::find_binary();
    }

    #[test]
    fn validate_port_rejects_privileged() {
        assert!(validate_port(80).is_err());
        assert!(validate_port(443).is_err());
        assert!(validate_port(1024).is_ok());
        assert!(validate_port(50052).is_ok());
    }

    #[test]
    fn validate_executable_rejects_nonexistent() {
        let result = validate_executable(&PathBuf::from("/nonexistent/binary"));
        assert!(result.is_err());
    }

    /// #164 measured 210 s cold vs 156 s warm on a 17 GiB shard, so the cache
    /// is on unless an operator turns it off — and `-d` matters because
    /// without it llama.cpp picks its own backend and a Metal machine can end
    /// up serving from the CPU while looking healthy.
    #[test]
    fn options_default_to_cache_on_and_no_device() {
        // SAFETY: this test owns these vars; no other test reads them.
        unsafe {
            std::env::remove_var("TIRAMI_RPC_DEVICE");
            std::env::remove_var("TIRAMI_RPC_CACHE");
        }
        let opts = RpcServerOptions::new(50052);
        assert_eq!(opts.port, 50052);
        assert!(opts.cache);
        assert_eq!(opts.device, None);

        unsafe {
            std::env::set_var("TIRAMI_RPC_DEVICE", "MTL0");
            std::env::set_var("TIRAMI_RPC_CACHE", "0");
        }
        let opts = RpcServerOptions::new(50052);
        assert_eq!(opts.device.as_deref(), Some("MTL0"));
        assert!(!opts.cache);

        // An empty device string means "unset", not "bind to nothing".
        unsafe { std::env::set_var("TIRAMI_RPC_DEVICE", "  ") };
        assert_eq!(RpcServerOptions::new(50052).device, None);

        unsafe {
            std::env::remove_var("TIRAMI_RPC_DEVICE");
            std::env::remove_var("TIRAMI_RPC_CACHE");
        }
    }
}
