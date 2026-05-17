//! Phase 17 Wave 1.5 — per-node scoped API tokens.
//!
//! Background: pre-Phase-17 Tirami has exactly one bearer token in
//! `config.api_bearer_token`. If it leaks, every API surface is fully
//! compromised — economic endpoints, admin endpoints, governance votes.
//! For a public adversarial deployment that's a single credential worth
//! a full-network takeover, and there is no way to revoke without
//! rotating the secret on every node simultaneously.
//!
//! This module introduces a scoped, revocable token system that sits
//! alongside the legacy bearer token (preserved as the implicit Admin
//! credential for backward compatibility):
//!
//! * [`ApiScope`] grades endpoints by privilege: `ReadOnly` < `Inference`
//!   < `Economy` < `Admin`. Higher scopes subsume lower ones.
//! * [`ApiToken`] pairs a SHA-256 hash of the raw token bytes with a
//!   scope, an expiry, a human-readable label, and an owning NodeId.
//! * [`TokenStore`] stores tokens keyed by hash and exposes
//!   issue / revoke / list / verify — the operator-facing surface.
//! * Raw tokens are 32 random bytes encoded as 64-char hex. They are
//!   **shown exactly once** at issue; only the hash is persisted.
//!
//! Lock discipline: the store is held as `Arc<Mutex<TokenStore>>` in
//! `AppState`, short-lived critical sections (each at most an O(1)
//! hashmap lookup).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tirami_core::NodeId;

/// Privilege classes for API tokens. Higher scopes implicitly grant
/// lower scopes: `Admin` satisfies every check, `Economy` satisfies
/// `Inference` and `ReadOnly`, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ApiScope {
    /// `/v1/tirami/balance`, `/pricing`, `/trades`, `/network`, etc.
    ReadOnly = 0,
    /// `/v1/chat/completions` and model-loading endpoints.
    Inference = 1,
    /// `/v1/tirami/lend`, `/borrow`, `/bank/*`, `/agora/*`, etc.
    Economy = 2,
    /// Token management, state save/load, governance admin, and any
    /// endpoint not otherwise classified.
    Admin = 3,
}

impl ApiScope {
    /// True iff a token carrying `self` satisfies a requirement of `needed`.
    pub fn satisfies(&self, needed: ApiScope) -> bool {
        *self >= needed
    }

    /// Canonical lowercase string form for config / logging.
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiScope::ReadOnly => "read_only",
            ApiScope::Inference => "inference",
            ApiScope::Economy => "economy",
            ApiScope::Admin => "admin",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "read_only" | "readonly" => Some(Self::ReadOnly),
            "inference" => Some(Self::Inference),
            "economy" => Some(Self::Economy),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }
}

/// An issued API token. The raw token bytes are NEVER persisted —
/// only this record keyed by `token_hash` inside [`TokenStore`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToken {
    /// SHA-256 of the raw token string. Constant-time compared on lookup.
    pub token_hash: [u8; 32],
    /// Identity this token is bound to — surfaced in audit logs and
    /// used by rate-limit / slashing flows to attribute API calls.
    pub node_id: NodeId,
    pub scope: ApiScope,
    /// Unix ms. A zero value means "never expires" (only the legacy
    /// bearer token uses that semantics; all issued tokens have a real
    /// expiry set by the admin).
    pub expires_at_ms: u64,
    pub created_at_ms: u64,
    /// Human-readable label for dashboards. e.g. "ci-runner-us-east".
    pub label: String,
}

impl ApiToken {
    /// True once `now_ms` has crossed the expiry. Never-expiring tokens
    /// (expires_at_ms == 0) return `false`.
    pub fn is_expired(&self, now_ms: u64) -> bool {
        self.expires_at_ms != 0 && now_ms >= self.expires_at_ms
    }
}

/// Result of a token verification attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenVerdict {
    Ok(ApiScope),
    Unknown,
    Expired,
    InsufficientScope { have: ApiScope, need: ApiScope },
}

/// In-memory store of issued API tokens.
///
/// Persistence is the caller's concern — the store serializes cleanly
/// via Serde so snapshots can ride the same `save_to_path` pipeline
/// as the rest of node state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenStore {
    tokens: HashMap<[u8; 32], ApiToken>,
}

impl TokenStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mint a fresh token. Returns `(raw_token_string, ApiToken)`.
    /// The caller MUST transmit the raw string to the operator exactly
    /// once and then discard it — the store will only ever remember
    /// the hash.
    pub fn issue(
        &mut self,
        node_id: NodeId,
        scope: ApiScope,
        ttl_secs: u64,
        label: impl Into<String>,
        now_ms: u64,
    ) -> (String, ApiToken) {
        use rand::RngCore;
        let mut raw = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut raw);
        let raw_hex = hex_encode(&raw);
        let hash = hash_token_str(&raw_hex);

        let expires_at_ms = if ttl_secs == 0 {
            0 // non-expiring — reserved for operator-controlled infra tokens
        } else {
            now_ms.saturating_add(ttl_secs.saturating_mul(1_000))
        };

        let token = ApiToken {
            token_hash: hash,
            node_id,
            scope,
            expires_at_ms,
            created_at_ms: now_ms,
            label: label.into(),
        };
        self.tokens.insert(hash, token.clone());
        (raw_hex, token)
    }

    /// Revoke a token by its 32-byte hash. Returns `true` if something
    /// was actually removed.
    pub fn revoke(&mut self, token_hash: &[u8; 32]) -> bool {
        self.tokens.remove(token_hash).is_some()
    }

    /// Phase 25 C7 — rotate a token: mint a fresh raw bearer with the
    /// same scope and node_id as the old, but mark the old token to
    /// expire after `grace_secs` so callers can overlap. Returns the
    /// fresh raw token + the new `ApiToken` record, or `None` if the
    /// old hash isn't present.
    ///
    /// Audit-log responsibility belongs to the HTTP handler; the store
    /// just performs the swap.
    pub fn rotate(
        &mut self,
        old_hash: &[u8; 32],
        grace_secs: u64,
        ttl_secs: u64,
        now_ms: u64,
    ) -> Option<(String, ApiToken)> {
        let old = self.tokens.get(old_hash)?.clone();
        // Pre-shorten the old token's expiry to `now + grace_secs` so
        // callers have a bounded overlap window before it dies.
        let new_old_expiry = now_ms.saturating_add(grace_secs.saturating_mul(1_000));
        if let Some(tok) = self.tokens.get_mut(old_hash) {
            tok.expires_at_ms = new_old_expiry;
        }
        // Mint the replacement.
        let (raw, fresh) = self.issue(
            old.node_id.clone(),
            old.scope,
            ttl_secs,
            format!("rotated-from-{}", hex_encode(old_hash)),
            now_ms,
        );
        Some((raw, fresh))
    }

    /// Revoke by hex-encoded hash (admin-API convenience).
    pub fn revoke_hex(&mut self, hex_hash: &str) -> bool {
        let Some(hash) = hex_decode_32(hex_hash) else {
            return false;
        };
        self.revoke(&hash)
    }

    /// Verify a raw bearer-token string against the store.
    pub fn verify(&self, raw_token: &str, needed: ApiScope, now_ms: u64) -> TokenVerdict {
        let hash = hash_token_str(raw_token);
        let Some(tok) = self.tokens.get(&hash) else {
            return TokenVerdict::Unknown;
        };
        if tok.is_expired(now_ms) {
            return TokenVerdict::Expired;
        }
        if !tok.scope.satisfies(needed) {
            return TokenVerdict::InsufficientScope {
                have: tok.scope,
                need: needed,
            };
        }
        TokenVerdict::Ok(tok.scope)
    }

    /// Snapshot of all currently-valid tokens (expired filtered out).
    /// Intended for the admin `GET /v1/tirami/tokens` listing. Raw
    /// token strings are NOT present — only the metadata.
    pub fn list_active(&self, now_ms: u64) -> Vec<ApiToken> {
        self.tokens
            .values()
            .filter(|t| !t.is_expired(now_ms))
            .cloned()
            .collect()
    }

    /// Drop expired tokens. Call opportunistically from an admin path
    /// to keep the map bounded.
    pub fn prune_expired(&mut self, now_ms: u64) -> usize {
        let before = self.tokens.len();
        self.tokens.retain(|_, t| !t.is_expired(now_ms));
        before - self.tokens.len()
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn hash_token_str(raw: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hasher.finalize().into()
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

fn hex_decode_32(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk).ok()?;
        out[i] = u8::from_str_radix(s, 16).ok()?;
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn nid(seed: u8) -> NodeId {
        NodeId([seed; 32])
    }

    #[test]
    fn scope_ordering_is_strict_and_subsumes_lower_levels() {
        assert!(ApiScope::Admin.satisfies(ApiScope::Economy));
        assert!(ApiScope::Admin.satisfies(ApiScope::Inference));
        assert!(ApiScope::Admin.satisfies(ApiScope::ReadOnly));
        assert!(ApiScope::Economy.satisfies(ApiScope::ReadOnly));
        assert!(!ApiScope::ReadOnly.satisfies(ApiScope::Inference));
        assert!(!ApiScope::Inference.satisfies(ApiScope::Economy));
    }

    #[test]
    fn scope_parse_round_trips() {
        for s in [ApiScope::ReadOnly, ApiScope::Inference, ApiScope::Economy, ApiScope::Admin] {
            assert_eq!(ApiScope::parse(s.as_str()), Some(s));
        }
        assert_eq!(ApiScope::parse("bogus"), None);
    }

    #[test]
    fn issue_returns_raw_token_and_persists_only_hash() {
        let mut store = TokenStore::new();
        let (raw, tok) = store.issue(nid(1), ApiScope::Economy, 3_600, "ci", 1_000);
        assert_eq!(raw.len(), 64); // 32 bytes hex
        // The raw token is NOT in the persisted record; only the hash.
        assert_ne!(raw.as_bytes(), tok.token_hash);
        // Hashing the raw string reproduces the stored hash.
        assert_eq!(hash_token_str(&raw), tok.token_hash);
        // Store actually contains the token under its hash key.
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn verify_roundtrips_ok_with_sufficient_scope() {
        let mut store = TokenStore::new();
        let (raw, _) = store.issue(nid(2), ApiScope::Economy, 3_600, "ops", 1_000);
        assert!(matches!(
            store.verify(&raw, ApiScope::ReadOnly, 1_500),
            TokenVerdict::Ok(ApiScope::Economy)
        ));
        assert!(matches!(
            store.verify(&raw, ApiScope::Economy, 1_500),
            TokenVerdict::Ok(ApiScope::Economy)
        ));
    }

    #[test]
    fn verify_rejects_insufficient_scope() {
        let mut store = TokenStore::new();
        let (raw, _) = store.issue(nid(3), ApiScope::ReadOnly, 3_600, "bot", 1_000);
        let v = store.verify(&raw, ApiScope::Economy, 1_500);
        assert_eq!(
            v,
            TokenVerdict::InsufficientScope {
                have: ApiScope::ReadOnly,
                need: ApiScope::Economy,
            }
        );
    }

    #[test]
    fn verify_rejects_unknown_and_expired() {
        let mut store = TokenStore::new();
        assert_eq!(store.verify("not-a-token", ApiScope::ReadOnly, 0), TokenVerdict::Unknown);
        let (raw, _) = store.issue(nid(4), ApiScope::ReadOnly, 10, "tmp", 1_000);
        // 10 s ttl → expires at 11 000 ms; querying at 20 000 is expired.
        assert_eq!(
            store.verify(&raw, ApiScope::ReadOnly, 20_000),
            TokenVerdict::Expired
        );
    }

    #[test]
    fn zero_ttl_never_expires() {
        let mut store = TokenStore::new();
        let (raw, _) = store.issue(nid(5), ApiScope::Admin, 0, "root", 1_000);
        assert!(matches!(
            store.verify(&raw, ApiScope::Admin, u64::MAX / 2),
            TokenVerdict::Ok(ApiScope::Admin)
        ));
    }

    #[test]
    fn revoke_removes_token_immediately() {
        let mut store = TokenStore::new();
        let (raw, tok) = store.issue(nid(6), ApiScope::Economy, 3_600, "lost", 1_000);
        assert!(store.revoke(&tok.token_hash));
        assert_eq!(store.verify(&raw, ApiScope::ReadOnly, 1_500), TokenVerdict::Unknown);
        // Double-revoke is idempotent.
        assert!(!store.revoke(&tok.token_hash));
    }

    #[test]
    fn revoke_hex_roundtrips_admin_api_shape() {
        let mut store = TokenStore::new();
        let (_, tok) = store.issue(nid(7), ApiScope::ReadOnly, 3_600, "x", 1_000);
        let hex = hex_encode(&tok.token_hash);
        assert!(store.revoke_hex(&hex));
        assert!(!store.revoke_hex("not-hex"));
        assert!(!store.revoke_hex(&"0".repeat(64))); // valid hex, unknown hash
    }

    // -----------------------------------------------------------------
    // Phase 25 C7 — token rotation
    // -----------------------------------------------------------------

    #[test]
    fn rotate_mints_fresh_token_and_shortens_old_expiry() {
        let mut store = TokenStore::new();
        let (raw_old, old) =
            store.issue(nid(10), ApiScope::Economy, 3_600, "before", 1_000);
        let now = 2_000;
        let (raw_new, fresh) = store
            .rotate(&old.token_hash, 60, 3_600, now)
            .expect("rotate must succeed");
        // Two distinct raw tokens, two distinct hashes.
        assert_ne!(raw_old, raw_new);
        assert_ne!(old.token_hash, fresh.token_hash);
        // Same scope + node_id propagate to the replacement.
        assert_eq!(fresh.scope, ApiScope::Economy);
        assert_eq!(fresh.node_id, old.node_id);
        // Old token's expiry is shortened to (now + grace).
        let store_old = store.tokens.get(&old.token_hash).expect("old still present");
        assert_eq!(store_old.expires_at_ms, now + 60 * 1_000);
    }

    #[test]
    fn rotate_overlap_window_lets_old_token_still_verify_within_grace() {
        let mut store = TokenStore::new();
        let (raw_old, old) =
            store.issue(nid(11), ApiScope::ReadOnly, 3_600, "ol", 1_000);
        let now = 2_000;
        let _ = store.rotate(&old.token_hash, 30, 3_600, now).unwrap();
        // Within the 30s grace window, the OLD raw still verifies.
        assert!(matches!(
            store.verify(&raw_old, ApiScope::ReadOnly, now + 10_000),
            TokenVerdict::Ok(_)
        ));
        // Past the grace window, the OLD raw is expired.
        assert!(matches!(
            store.verify(&raw_old, ApiScope::ReadOnly, now + 60_000),
            TokenVerdict::Expired
        ));
    }

    #[test]
    fn rotate_unknown_hash_returns_none() {
        let mut store = TokenStore::new();
        let result = store.rotate(&[0xFFu8; 32], 60, 3_600, 1_000);
        assert!(result.is_none());
    }

    #[test]
    fn rotate_label_records_origin() {
        let mut store = TokenStore::new();
        let (_, old) =
            store.issue(nid(12), ApiScope::Admin, 3_600, "orig", 1_000);
        let (_, fresh) = store.rotate(&old.token_hash, 60, 3_600, 2_000).unwrap();
        assert!(fresh.label.starts_with("rotated-from-"));
    }

    #[test]
    fn list_active_filters_expired() {
        let mut store = TokenStore::new();
        store.issue(nid(8), ApiScope::ReadOnly, 10, "soon", 1_000);
        store.issue(nid(9), ApiScope::Admin, 3_600, "ok", 1_000);
        let live = store.list_active(20_000); // 19 s later — first one expired
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].scope, ApiScope::Admin);
    }

    #[test]
    fn prune_expired_drops_only_expired_tokens() {
        let mut store = TokenStore::new();
        store.issue(nid(10), ApiScope::ReadOnly, 10, "short", 1_000);
        store.issue(nid(11), ApiScope::Admin, 3_600, "long", 1_000);
        assert_eq!(store.len(), 2);
        let removed = store.prune_expired(20_000);
        assert_eq!(removed, 1);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn two_fresh_tokens_differ_and_hash_uniquely() {
        // Collision guard: OS CSPRNG is effectively certain to give
        // different 256-bit tokens on two successive issues. This test
        // defends against accidental re-keying (e.g. a seeded RNG) in
        // the issue path.
        let mut store = TokenStore::new();
        let (a, _) = store.issue(nid(12), ApiScope::ReadOnly, 10, "a", 1_000);
        let (b, _) = store.issue(nid(12), ApiScope::ReadOnly, 10, "b", 1_000);
        assert_ne!(a, b);
        assert_ne!(hash_token_str(&a), hash_token_str(&b));
    }
}
