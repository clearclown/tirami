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

impl RpcServer {
    /// Find the rpc-server binary from trusted locations only.
    pub fn find_binary() -> Option<PathBuf> {
        // Check env var first
        if let Ok(path) = std::env::var("FORGE_RPC_SERVER_PATH") {
            let p = PathBuf::from(&path);
            if validate_executable(&p).is_ok() {
                return Some(p);
            }
        }

        // Check trusted locations only (not arbitrary PATH)
        for candidate in &[
            "/usr/local/bin/rpc-server",
            "/opt/homebrew/bin/rpc-server",
            "/tmp/llama.cpp/build/bin/rpc-server",
        ] {
            let p = PathBuf::from(candidate);
            if p.exists() {
                if let Ok(validated) = validate_executable(&p) {
                    return Some(validated);
                }
            }
        }

        None
    }

    /// Spawn a local rpc-server on the given port.
    pub fn spawn(port: u16) -> Result<Self, TiramiError> {
        let port = validate_port(port)?;

        let binary = Self::find_binary().ok_or_else(|| {
            TiramiError::InferenceError(
                "rpc-server binary not found. Set FORGE_RPC_SERVER_PATH".to_string(),
            )
        })?;

        let binary = validate_executable(&binary)?;

        tracing::info!("Starting rpc-server on port {} ({:?})", port, binary);

        // Only pass safe, controlled arguments
        let child = Command::new(&binary)
            .arg("-p")
            .arg(port.to_string())
            // Bind to localhost only — never expose on 0.0.0.0
            .arg("--host")
            .arg("127.0.0.1")
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
}
