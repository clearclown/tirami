# Security Policy

## Supported versions

| Version | Supported |
|---|---|
| 0.3.x | ✅ current |
| 0.2.x | 🟡 critical fixes only |
| < 0.2 | ❌ unsupported |

## Reporting a vulnerability

Please do **not** open a public GitHub issue for security vulnerabilities.

### Preferred: GitHub Security Advisory

Use the [private vulnerability reporting](https://github.com/clearclown/forge/security/advisories/new)
feature. This keeps the report confidential until a fix is ready.

### Alternative: Email

If GitHub Security Advisories are unavailable, email the maintainers
privately. Include:

- Affected component (e.g. `tirami-ledger::collusion`, `/v1/tirami/anchor`)
- Affected version (`git rev-parse HEAD` if you build from source)
- Reproduction steps or proof-of-concept code
- Your disclosure timeline expectations

## Disclosure timeline

- **Day 0**: Report received, private acknowledgment within 48 hours
- **Day 1-14**: Investigation, severity classification, fix development
- **Day 14-30**: Patch released, CVE requested if applicable
- **Day 30+**: Public disclosure, credit to reporter (unless declined)

We target 30-day turnaround for fixable issues. Critical issues (remote
code execution, ledger corruption, signature bypass) may fast-track.

## Scope

### In scope

- Cryptographic primitives: Ed25519 signatures, HMAC-SHA256 ledger
  integrity, SHA-256 Merkle trees, reputation gossip signatures
- Economic safety: lending pool circuit breakers, collusion detection,
  welcome-loan Sybil resistance, TRM accounting invariants
- HTTP API authentication: bearer token handling, rate limiting
- Persistence integrity: ledger.json HMAC verification, state snapshots
- Model loading: GGUF file parsing, tokenizer deserialization
- P2P transport: iroh QUIC + Noise handshake, gossip dedup
- Supply chain: `Cargo.lock` reproducibility, published wheel/crate
  content matches source tree

### Out of scope

- Denial of service via resource exhaustion on public nodes (run your
  own rate limits)
- Social engineering of maintainers
- Attacks requiring physical access to the machine running forge
- llama.cpp or `llama-cpp-2` upstream vulnerabilities (report to them)
- iroh upstream vulnerabilities (report to them)
- Third-party model files (report to the model author)
- forge-mesh (separate repo: <https://github.com/nm-arealnormalman/mesh-llm>)

## Hardening guidance

Run a Forge node in a dedicated user account with minimal privileges.
Do not expose the P2P QUIC port to the public internet without a
reverse proxy or WAF. Keep the `api_bearer_token` secret and rotate
periodically. Back up `tirami-ledger.json` and the L2/L3/L4 state
snapshots off-host.

For production deployments see `docs/operator-guide.md` (security
checklist section).

## Known limitations

These are documented design trade-offs, not undisclosed vulnerabilities:

1. **No on-chain settlement finality** — the Merkle root anchor to
   Bitcoin (`tirami_ledger::anchor`) is published but disputes require
   the forthcoming BitVM fraud-proof path (Phase 13, currently only
   a scaffold in `tirami-ledger::bitvm`).
2. **Reputation gossip is trust-on-first-use** — signed observations
   prevent forgery but do not prevent a malicious node from flooding
   their own low-quality observations. Collusion detection
   (`tirami-ledger::collusion`) mitigates but does not eliminate this.
3. **CU lending uses ephemeral signing keys** in single-node mode
   (see `/v1/tirami/lend-to` notes in `crates/tirami-node/src/api.rs`).
   Real P2P dual-signing is wired but not yet end-to-end verified in
   multi-node deployments. Do not use for real value without manual
   verification.
4. **zkML verification is mock-only** in v0.3. See
   `tirami-ledger::zk::MockVerifier`. Real backends (ezkl, risc0) are
   Phase 13+ work. Do not rely on proof-of-inference for trust
   decisions until a real backend lands.

## CVE and credit

Verified reports get a CVE filing (if severity warrants), a fix commit
with credit in the commit message, a CHANGELOG entry under
`[Unreleased] → Security`, and a GitHub Security Advisory linking the
report to the fix.
