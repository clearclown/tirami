# Phase 19 — Public-launch post drafts

> Draft copy for the Phase 19 public announcement once the external audit
> and ≥ 30-day Sepolia stable window clear. Not yet published. Supersedes
> the Phase 10 teaser at `docs/hn-teaser-draft.md`.

Status at time of writing (2026-04-19): 1,192 Rust tests + 15 Solidity
tests pass, 16 Rust crates, 11-language README, whitepaper + arXiv tarball
built, mainnet deploy still audit-gated. **Read `docs/release-readiness.md`
before you post.**

---

## Option A — Hacker News "Show HN"

**Title:** `Show HN: Tirami – Distributed LLM inference where compute is the currency (Rust, MIT)`

**Text:**

I've been building **Tirami**, a Rust protocol where compute itself is the unit
of account. **No token sale, no ICO, no pre-mine, no team treasury.**
1 TRM (Tirami Resource Merit) = 10⁹ FLOP of verified inference. You earn
TRM by running a local model for someone else; you spend TRM by asking a peer
to run inference for you.

Everything runs locally. The default demo pulls Qwen2.5-0.5B-Instruct GGUF,
starts a node, and within about 20 seconds you have an OpenAI-compatible
HTTP endpoint that charges and pays in TRM.

**What's actually running today** (1,192 Rust unit tests + 15 Solidity tests
pass, verified):

- OpenAI-compatible chat with dual-signed P2P trade (Ed25519, 128-bit nonce
  replay protection) over iroh-QUIC + Noise.
- HTTP → P2P auto-forward: workers without a local model forward chat to
  a connected peer.
- Peer auto-discovery via `PriceSignal.http_endpoint` on the gossip stream.
- Governance with 18 immutable constitutional parameters (e.g.
  `TOTAL_TRM_SUPPLY = 21B`, `FLOPS_PER_CU = 10⁹`, `SLASH_RATE_*`) and a
  21-entry mutable whitelist. `create_proposal` refuses unknown names.
- Slashing loop running every `slashing_interval_secs` against collusion
  detector + audit-tier failures.
- Stake pool, welcome loan (sunset at epoch 2, constitutional), referral
  bonus, credit scoring, lending circuit breakers.
- Base Sepolia contracts compile + test (TRM ERC-20 + TiramiBridge).
- Prometheus `/metrics` with `tirami_*` prefix.

**What's scaffolded but not production-wired**: zkML proof-of-inference
(`MockBackend` only — real `ezkl` / `risc0` in Phase 20+), ML-DSA
post-quantum hybrid signatures (blocked by iroh 0.97), TEE attestation
(`tirami-attestation` is a scaffold), daemon worker gossip-recv loop
(issue #88).

**What's explicitly not done** (gates before public mainnet):

- External security audit (candidates: Trail of Bits, Zellic, Open Zeppelin,
  Least Authority).
- Base L2 mainnet deploy. The `make deploy-base-mainnet` target *refuses*
  to execute without `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER` + an
  interactive prompt.
- Live bug-bounty (PGP key is placeholder).
- ≥ 30-day Sepolia stable + ≥ 7-day 10-node stress test.

Full status breakdown in `README.md` — I wrote a "Status Honesty" section
specifically so HN readers know what is real vs design intent.

**Why not just another AI-compute token?** TRM isn't sold, listed, or promoted
by the maintainers. If someone else tokenises it on a secondary market,
that's their decision and their risk — the maintainers explicitly have no
way to prevent this (it's MIT OSS) and no way to profit from it. See
`SECURITY.md § Secondary Markets`.

**Theory:** the economics are written up as a 13-section preprint at
[tirami-economics](https://github.com/clearclown/tirami-economics)
(`papers/build/compute-standard.pdf` for local review, arXiv tarball in the
same directory). 18 chapters of economic commentary from monetary theory
(Soddy/technocracy) through Bitcoin PoW → Tirami PoUW.

**Code:** https://github.com/clearclown/tirami (Rust, MIT)

Would love feedback on: (1) the zkML rollout schedule — we're betting on
`ProofPolicy = Optional` with a ratchet that cannot regress to
`Required` → `Optional`, (2) the `welcome_loan` sunset mechanic, (3)
anything that feels like it deserves a louder warning in Status Honesty.

---

## Option B — r/LocalLLaMA

**Title:** `Tirami — distributed LLM inference mesh where compute itself is the unit of account (1,192 tests pass, Rust, MIT)`

**Body:**

I just pushed Phase 19 of a project I've been working on for a year. Sharing
here because r/LocalLLaMA is the community that would most benefit: **you
already have the hardware; this lets your spare capacity earn and trade compute
with other nodes without touching a crypto exchange.**

[Short demo gif/asciinema link placeholder]

**Core idea:** 1 TRM = 10⁹ FLOP of verified inference. Your Mac Mini runs
Qwen2.5 → someone else's prompt gets served → you earn TRM → you spend TRM
to route your own prompts to a node with a bigger model. Zero human-mediated
crypto exchange.

**Stack:** Rust, 16 workspace crates, OpenAI-compatible HTTP API (drop-in
`OPENAI_BASE_URL`), iroh QUIC + Noise P2P transport, llama.cpp via
`llama-cpp-2` for Metal/CUDA/CPU, Prometheus metrics. 1,192 unit tests,
15 Solidity tests for the on-chain bridge.

**What works now on your machine:**

```
$ git clone https://github.com/clearclown/tirami && cd tirami
$ bash scripts/demo-e2e.sh
# 1. downloads Qwen2.5-0.5B
# 2. spins up the node
# 3. runs 3 chat completions on Metal
# 4. shows real TRM in /v1/tirami/balance
# 5. demonstrates the dual-signed P2P trade path
```

**What doesn't work yet:**

- zkML proof-of-inference — only `MockBackend`. Real proofs (`ezkl`,
  `risc0`) are Phase 20+.
- TEE attestation — scaffold only.
- Mainnet — deploy target refuses without external audit clearance.

**Why this matters for r/LocalLLaMA specifically:**

- **Your spare VRAM becomes an income stream** without any custodial wallet
  or centralized rewards pool. Dual-signed trades mean you settle
  peer-to-peer with cryptographic receipts.
- **PersonalAgent auto-chooses local vs remote**: `tirami agent chat "..."`
  runs on your local model if it fits and forwards to the cheapest remote
  peer if not.
- **No token speculation can distort your earnings**: TRM isn't listed, sold,
  or bridged by the maintainers. MIT OSS means third parties technically
  *can* try to list a derivative, but secondary-market risk is on them —
  you earn TRM by actually running inference.

Code: https://github.com/clearclown/tirami (MIT).
Docs + README in 11 languages: `docs/translations/{ja,zh-CN,zh-TW,es,fr,ru,uk,hi,ar,fa,he}/`.
Theory: https://github.com/clearclown/tirami-economics.

AMA / feedback / PR welcome.

---

## Option C — r/MachineLearning

**Title:** `[P] Tirami — 16-crate Rust protocol making distributed LLM inference a unit of account (pre-audit, MIT)`

**Body:**

Posting here because the design touches a few ML-adjacent research threads:
(i) proof-of-inference via zkML, (ii) AI-agent budget accounting, (iii)
economic game theory against Sybil/collusion in a mesh.

**TL;DR**: every accepted inference trade is a dual-signed `SignedTradeRecord`
pinning `(provider, consumer, model_id, tokens, trm_cost, flops_estimated,
nonce)`. Trades are gossiped, Merkle-rooted, and anchored on Base L2
every 10 minutes. 1 TRM = 10⁹ FLOP. The supply cap is constitutionally
fixed at 21 B in a Rust `const` that governance proposals cannot rewrite.

**Research-relevant design choices:**

- **Proof policy ratchet.** `ProofPolicy` ∈ {`Optional`, `Recommended`,
  `Required`} is monotone-increasing: governance can promote but cannot
  demote. Default is `Optional` today (Phase 19); trades with a zkML
  proof receive a reputation bonus; trades without are still valid. This
  lets us ship honest compute accounting today and migrate to
  cryptographic proof-of-inference as `ezkl` / `risc0` mature (Phase
  20+), without requiring all provers to be ready on day one. We'd love
  critique of the ratchet design from anyone who has thought about
  credible-neutrality proof upgrades.
- **Collusion detector.** Tight-cluster + volume-spike + round-robin
  Tarjan-SCC detection feeds a trust penalty into `effective_reputation()`.
- **Agent budgets.** `PersonalAgent` owns a `CuBudget` with hard limits
  (per-cycle / per-day / cycles-per-day) and drives a `tick` loop with
  observability hooks. The economic theory companion
  (https://github.com/clearclown/tirami-economics) writes this up as a
  full 18-chapter treatise; the arXiv-ready PDF is in
  `papers/build/compute-standard.arxiv.pdf`.

**Honest status:**

- 1,192 Rust unit tests pass. 15 Solidity tests pass.
- zkML backend is a `MockBackend` — shape-correct but cryptographically invalid.
- No external security audit yet.
- Base L2 mainnet deploy is Makefile-gated on audit clearance.
- Not a token sale. No ICO, no pre-mine, no treasury. See
  `SECURITY.md § Secondary Markets` for the maintainers' non-involvement
  stance on third-party tokenization.

**Repos:**

- Protocol (Rust, MIT): https://github.com/clearclown/tirami
- Theory (CC-BY-4.0): https://github.com/clearclown/tirami-economics
- Upstream inference fork: https://github.com/nm-arealnormalman/mesh-llm

Feedback especially welcome on: the `ProofPolicy` ratchet, the Kani
invariants (10 so far, `crates/tirami-ledger/kani/`), and whether the
economic theory chapter §17 (Proof of Useful Work → zkML) correctly frames
the cryptographic open problems.

---

## Option D — X / Twitter thread (5 tweets)

**1/5**
> Tirami is a Rust protocol where compute itself is money. 1 TRM = 10⁹ FLOP. No ICO, no pre-mine, no team treasury — MIT OSS since day one. Phase 19 is shipped: 1,192 tests pass. 🧵

**2/5**
> Your Mac Mini serves a prompt → you earn TRM. Your agent needs a bigger model → it pays TRM to the cheapest peer. Dual-signed trades + 128-bit nonce anti-replay + slashing loop + audit tier. Zero exchange, zero custody.

**3/5**
> The supply cap is constitutionally fixed at 21B in a Rust `const` that governance cannot modify. 18 immutable parameters, 21 mutable. `create_proposal` refuses unknown names. Credible neutrality by code inspection, not by social contract.

**4/5**
> zkML proof-of-inference is Phase 20+ — today we run `MockBackend`. `ProofPolicy` ratchets only forward (Optional → Recommended → Required, never backward). Honest compute accounting today, cryptographic proofs as `ezkl` / `risc0` mature.

**5/5**
> No mainnet until external audit clears. Makefile physically refuses to deploy without `AUDIT_CLEARANCE=yes` + multisig owner + interactive prompt. Code: https://github.com/clearclown/tirami. Theory: https://github.com/clearclown/tirami-economics.

---

## Pre-flight checklist before posting

1. `bash scripts/demo-e2e.sh` on a clean checkout of `main` → should succeed end-to-end.
2. `cargo test --workspace` → 1,192 pass.
3. `cd repos/tirami-contracts && forge test` → 15 pass.
4. `bash scripts/verify-impl.sh` → 123/123 GREEN.
5. Confirm no legacy `forge/balance`, `x_forge.cu_cost`, "785 tests" strings in README.md (already scrubbed in PR #91).
6. Confirm the date stamp (`2026-04-19`) in README.md § Status Honesty still reflects current state; bump to current day-of-post.
7. Confirm `papers/build/compute-standard.pdf` and `.arxiv.tar.gz` are present and readable.
8. Have a draft response ready for the three questions that always come up:
   - "Is this a token sale?" → No. See `SECURITY.md § Secondary Markets`.
   - "Is mainnet deployed?" → No. Makefile-gated on audit. Sepolia only.
   - "Does zkML actually work?" → `MockBackend` today; real backends Phase 20+.
