# Security Policy

**Last reviewed:** 2026-04-19 (Phase 19 Tier C/D enablers).

Tirami is a protocol for buying and selling compute as a currency on
an adversarial public network. Security is not optional. If you find
a vulnerability, please report it responsibly using the process
below. If you're here to exploit one, please also consider reporting:
the bounty program framework in the "Bug bounty" section pays out
competitively with market prices and reporting keeps you in
good-faith territory (see "Rules of engagement").

## Secondary markets & third-party tokenization

Tirami is **MIT-licensed open-source software**. TRM is the unit
the ledger uses to account for compute. TRM is not a financial
product, an investment contract, a security, or a commodity
offered for sale by the protocol maintainers.

Because the software is open source, anyone in the world — with or
without the maintainers' knowledge, consent, or endorsement — may:

- bridge the on-chain ERC-20 to other networks,
- list TRM on a secondary exchange,
- speculate on its future value,
- build derivatives (futures, options, structured products),
- fork the code and run a competing network.

The protocol maintainers:

- **do not** solicit investment in TRM.
- **do not** promise any return, appreciation, or yield beyond the
  mechanical "TRM ↔ compute" accounting relationship.
- **do not** sell TRM on a pre-sale, ICO, airdrop, or private round.
- **do not** receive equity, tokens, or revenue share from any
  third-party market that lists TRM.
- **cannot prevent** third parties from creating such markets
  after the open-source release.
- **explicitly disclaim** any warranty of merchantability or
  fitness for a particular purpose (see `LICENSE`, the MIT clause
  at the end).

If you are considering holding or trading TRM as a store of value,
you are making that judgement yourself and accepting all associated
risk — legal, regulatory, counterparty, and technical.

The protocol works without any external market. `1 TRM = 10⁹ FLOP`
is the definitional anchor (docs/whitepaper.md §3). The compute is
the value. Everything else is emergent.

## Mainnet deployment gate

`make deploy-base-mainnet` in `repos/tirami-contracts/Makefile`
refuses to run unless the operator has:

1. An external security audit report signed off (env var
   `AUDIT_CLEARANCE=yes`).
2. A multi-sig address configured to receive `Ownable::owner`
   (env var `MULTISIG_OWNER`).
3. Typed `i-accept-responsibility` at an interactive prompt.

An operator who bypasses these gates — for instance by patching
the Makefile, calling `forge script` directly, or deploying from
another tool — is solely responsible for the deployment. The MIT
license explicitly permits such forks and redeploys but does
**not** transfer any liability from the patcher to the original
maintainers.

See `docs/release-readiness.md` for the full Tier A–D rollout
criteria, and `docs/deployments/README.md` for the live deploy
record.

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

Run a Tirami node in a dedicated user account with minimal privileges.
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

## Encryption / PGP

For especially sensitive reports (active exploit in the wild,
pre-disclosure coordination, etc.), encrypt to the placeholder key
below. The operator will replace this block with a live key before
the bug bounty program opens.

```
-----BEGIN PGP PUBLIC KEY BLOCK-----
[PLACEHOLDER — not yet active. Until this block is replaced with a
live key, use the GitHub private-advisory flow or a direct email
to the maintainers. If you absolutely need encryption right now,
request a fresh ephemeral key via the advisory channel.]
-----END PGP PUBLIC KEY BLOCK-----
```

Fingerprint placeholder: `XXXX XXXX XXXX XXXX XXXX  XXXX XXXX XXXX XXXX XXXX`.

## Bug bounty

**Status:** Framework drafted; **active payouts not yet live**.
Activation is gated on:

1. External security audit complete (see
   `docs/security/audit-scope.md`, Phase 17 Wave 3.3).
2. Multi-sig custody configured for the bounty treasury.
3. Sepolia testnet usable by hunters for safe exploit reproduction.

When active, payouts are in TRM (post-mainnet) or USDC (pre-mainnet)
at the submitter's preference.

### Severity + indicative reward scale

Severity follows CVSS 3.1 base scores plus our own judgment about
Tirami-specific impact. Rewards below are a floor; exceptional
findings earn multipliers.

| Severity | CVSS | Reward (USD-equivalent) |
|----------|------|-------------------------|
| **Critical** | 9.0 – 10.0 | 25 000 – 50 000 |
| **High**     | 7.0 – 8.9  | 5 000 – 20 000  |
| **Medium**   | 4.0 – 6.9  | 1 000 – 4 000   |
| **Low**      | 0.1 – 3.9  | 200 – 800       |
| **Informational** | N/A | Hall of Fame + TRM airdrop |

Example findings with rough indicative payouts:

- Sign any TradeRecord for any provider without their key →
  **Critical**, $ 40 k.
- Bypass the welcome-loan per-ASN limiter end-to-end → **High**,
  $ 12 k.
- Crash a seed node with a specific malformed envelope → **High**,
  $ 8 k.
- Read unauthenticated `/v1/tirami/balance` by omitting auth →
  **Medium**, $ 2 k.
- Trivially extract a bearer token from client-side logs → **Low**,
  $ 500 (and please do report).

### Rules of engagement

Good-faith compliance with the following: we commit to not pursue
legal action against you.

- **DO** test against `localhost` or a Sepolia testnet deployment.
- **DO** use the GitHub private advisory flow or the email addresses
  listed above.
- **DO NOT** exfiltrate or disclose user data you gain via an
  exploit.
- **DO NOT** attempt sustained denial-of-service against any
  production seed node (Sepolia staging is fine to stress-test).
- **DO NOT** publicly disclose before the SLA windows close (see
  "Disclosure timeline"). We target 30 days post-fix for public
  disclosure; you may invoke public disclosure yourself if we miss
  the SLA without prior coordination.

## Hall of Fame

Researchers whose reports led to fixed vulnerabilities are credited
here. Template:

- **YYYY-MM-DD** · `<Handle or Name>` · <one-line finding> · <severity> · CVE-YYYY-NNNN

*Currently empty — bounty program not yet live. Check back after
Phase 17 external audit completes.*
