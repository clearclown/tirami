//! Node wallet — persistent Ed25519 key store for trade & loan signing.
//!
//! # Why a wallet abstraction
//!
//! A Tirami node has a single 32-byte Ed25519 seed that underlies both
//! the iroh QUIC keypair (transport identity) and the trade/loan
//! signing key. Treating that seed as a *wallet* — same model Bitcoin
//! Core / OpenSSH use — gives us:
//!
//! * **Stable identity across restarts.** Same seed → same public
//!   `NodeId` → reputation and lending history survive process bounces.
//! * **Auto-bootstrap on first run.** If the configured `node_key_path`
//!   is missing, we generate a fresh seed, write it atomically with
//!   `0600` permissions, and log the new public NodeId so the operator
//!   knows their "address". Matches `wallet.dat` / `id_ed25519` UX.
//! * **HTTP-layer signing.** The wallet `SigningKey` is plumbed onto
//!   `AppState`, so HTTP handlers like `/v1/tirami/borrow` sign loans
//!   as the *real* node identity instead of one-shot ephemeral keys.
//!
//! # File format
//!
//! Either 32 raw bytes OR 64 ASCII hex characters (optionally with a
//! trailing newline). Same format the legacy `load_node_secret_key`
//! has accepted; the wallet loader is a strict superset.
//!
//! # File permissions
//!
//! On unix, fresh wallets are written with mode `0600`. Existing files
//! with world- or group-readable bits trigger a runtime warning so the
//! operator can `chmod 600` them.

use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::RngCore;
use rand::rngs::OsRng;
use std::fs;
use std::io;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// 32-byte Ed25519 seed wrapped with derived signing/verifying keys.
///
/// Cheap to clone — the seed and `SigningKey` are both `Copy`-sized.
#[derive(Clone)]
pub struct WalletKey {
    seed: [u8; 32],
    signing: SigningKey,
}

impl WalletKey {
    /// Build a wallet from a raw 32-byte Ed25519 seed.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let signing = SigningKey::from_bytes(&seed);
        Self { seed, signing }
    }

    /// Generate a fresh wallet from OS randomness. Used by
    /// `load_or_create` when the file is missing.
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        Self::from_seed(seed)
    }

    /// Raw seed bytes. Callers that need them (e.g. to construct an
    /// `iroh::SecretKey` from the same material) hold this read-only.
    pub fn seed(&self) -> &[u8; 32] {
        &self.seed
    }

    /// Borrow the underlying Ed25519 signing key. The wallet retains
    /// ownership so the key never outlives the wallet handle.
    pub fn signing_key(&self) -> &SigningKey {
        &self.signing
    }

    /// Derive the Ed25519 verifying key (public component).
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }

    /// 32 bytes of the public verifying key. Equivalent to the wire
    /// `NodeId` for this wallet.
    pub fn verifying_key_bytes(&self) -> [u8; 32] {
        self.verifying_key().to_bytes()
    }

    /// Load a wallet from `path`. If the file does not exist, generate
    /// a fresh wallet and atomically write it with mode `0600`.
    /// Returns `(wallet, was_created)` so callers can log distinctly
    /// for first-run bootstrap vs. ordinary restart.
    pub fn load_or_create(path: &Path) -> io::Result<(Self, bool)> {
        match fs::read(path) {
            Ok(bytes) => {
                let seed = parse_seed_bytes(&bytes).map_err(|reason| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid wallet file {}: {reason}", path.display()),
                    )
                })?;
                warn_if_world_readable(path);
                Ok((Self::from_seed(seed), false))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                let wallet = Self::generate();
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        fs::create_dir_all(parent)?;
                    }
                }
                write_seed_atomic(path, &wallet.seed)?;
                Ok((wallet, true))
            }
            Err(err) => Err(err),
        }
    }
}

impl std::fmt::Debug for WalletKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WalletKey")
            .field("verifying_key_hex", &hex::encode(self.verifying_key_bytes()))
            .field("seed", &"<redacted>")
            .finish()
    }
}

/// Parse either 32 raw bytes or 64 ASCII hex chars (optionally with
/// trailing whitespace / newline).
pub(crate) fn parse_seed_bytes(bytes: &[u8]) -> Result<[u8; 32], &'static str> {
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

fn write_seed_atomic(path: &Path, seed: &[u8; 32]) -> io::Result<()> {
    // Write to a sibling tmp file first, fsync-via-rename so a crash
    // between create and write can never leave a half-written wallet.
    let tmp = match path.file_name() {
        Some(name) => {
            let mut buf = std::ffi::OsString::from(name);
            buf.push(".tmp");
            path.with_file_name(buf)
        }
        None => path.with_extension("key.tmp"),
    };
    fs::write(&tmp, seed)?;
    #[cfg(unix)]
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(unix)]
fn warn_if_world_readable(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            tracing::warn!(
                "wallet file {} has permissive mode {:o} (other/group readable) — \
                 recommend `chmod 600`",
                path.display(),
                mode
            );
        }
    }
}

#[cfg(not(unix))]
fn warn_if_world_readable(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Create a unique scratch dir under `std::env::temp_dir()` and
    /// return its path. Drops are best-effort via `RemoveOnDrop`.
    struct ScratchDir(std::path::PathBuf);
    impl ScratchDir {
        fn new(label: &str) -> Self {
            let n = TEST_DIR_COUNTER.fetch_add(1, Ordering::SeqCst);
            let pid = std::process::id();
            let path = std::env::temp_dir().join(format!("tirami-wallet-{label}-{pid}-{n}"));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn from_seed_is_deterministic() {
        let seed = [0x42u8; 32];
        let a = WalletKey::from_seed(seed);
        let b = WalletKey::from_seed(seed);
        assert_eq!(a.verifying_key_bytes(), b.verifying_key_bytes());
        assert_eq!(a.seed(), b.seed());
    }

    #[test]
    fn generate_produces_distinct_seeds() {
        let a = WalletKey::generate();
        let b = WalletKey::generate();
        assert_ne!(a.seed(), b.seed());
        assert_ne!(a.verifying_key_bytes(), b.verifying_key_bytes());
    }

    #[test]
    fn load_or_create_writes_fresh_seed_when_missing() {
        let dir = ScratchDir::new("fresh");
        let path = dir.path().join("subdir").join("node.key");
        assert!(!path.exists());

        let (wallet, created) = WalletKey::load_or_create(&path).unwrap();
        assert!(created);
        assert!(path.exists());

        let raw = fs::read(&path).unwrap();
        assert_eq!(raw.len(), 32);
        assert_eq!(&raw[..], wallet.seed());
    }

    #[test]
    fn load_or_create_reloads_existing_seed() {
        let dir = ScratchDir::new("reload");
        let path = dir.path().join("node.key");
        let (first, created_first) = WalletKey::load_or_create(&path).unwrap();
        assert!(created_first);

        let (second, created_second) = WalletKey::load_or_create(&path).unwrap();
        assert!(!created_second);
        assert_eq!(first.seed(), second.seed());
        assert_eq!(first.verifying_key_bytes(), second.verifying_key_bytes());
    }

    #[test]
    fn load_or_create_accepts_hex_format() {
        let dir = ScratchDir::new("hex");
        let path = dir.path().join("node.key");
        let seed = [0xABu8; 32];
        fs::write(&path, format!("{}\n", hex::encode(seed))).unwrap();

        let (wallet, created) = WalletKey::load_or_create(&path).unwrap();
        assert!(!created);
        assert_eq!(wallet.seed(), &seed);
    }

    #[test]
    fn load_or_create_rejects_malformed_file() {
        let dir = ScratchDir::new("bad");
        let path = dir.path().join("node.key");
        fs::write(&path, b"not a key").unwrap();

        let err = WalletKey::load_or_create(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[cfg(unix)]
    #[test]
    fn auto_created_wallet_has_mode_0600() {
        let dir = ScratchDir::new("perms");
        let path = dir.path().join("node.key");
        let (_w, _created) = WalletKey::load_or_create(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "fresh wallet must be 0600, got {:o}", mode);
    }

    #[test]
    fn sign_round_trips() {
        let w = WalletKey::from_seed([7u8; 32]);
        use ed25519_dalek::{Signer, Verifier};
        let msg = b"economic statement of the realm";
        let sig = w.signing_key().sign(msg);
        assert!(w.verifying_key().verify(msg, &sig).is_ok());
    }
}
