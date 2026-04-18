# Tirami — Roadmap

## Phase 1: Local Inference ✅

- `tirami-core`: Type system (NodeId, LayerRange, ModelManifest, PeerCapability)
- `tirami-infer`: llama.cpp engine, GGUF loader, streaming token generation
- `tirami-node`: HTTP API (/chat, /chat/stream, /health)
- `tirami-cli`: `tirami chat` command with model auto-download

## Phase 2: P2P Protocol ✅

- `tirami-net`: Iroh transport, Noise encryption, peer connections
- `tirami-proto`: 14 wire protocol message types (bincode + length-prefix)
- `tirami-node`: Seed/Worker pipeline, inference request/response
- Integration tests: 2 nodes exchange Hello + multiple messages

## Phase 3: Remote Inference + Operator Ledger ✅

- `tirami-ledger`: TRM accounting, trade execution, reputation, yield, market pricing
- `tirami-node`: Ledger integrated into inference pipeline
- TRM balance checks before inference
- Trade records after completion
- HMAC-SHA256 ledger integrity

## Phase 4: Economic API ✅

- OpenAI-compatible API: `POST /v1/chat/completions`, `GET /v1/models`
- TRM metering: every inference records a trade with `x_forge` extension
- Agent budget endpoints: `GET /v1/tirami/balance`, `GET /v1/tirami/pricing`
- TRM→Lightning settlement bridge: `tirami settle --pay`
- Seed model auto-resolve from HF Hub
- Graceful Ctrl-C shutdown with ledger persistence

## Phase 5: mesh-llm Fork Integration (next)

**Goal:** Replace Tirami's inference layer with mesh-llm's proven distributed engine.

| Deliverable | Description |
|---|---|
| Fork mesh-llm | Create tirami as a mesh-llm fork with economic layer |
| Integrate tirami-ledger | Hook TRM recording into mesh-llm's inference pipeline |
| Preserve economic API | Keep /v1/tirami/* endpoints in the new codebase |
| Web console extension | Add TRM balance and trade visibility to mesh-llm's console |
| Pipeline + MoE | Inherit mesh-llm's pipeline parallelism and expert sharding |
| Nostr discovery | Inherit mesh-llm's public mesh discovery |
| CREDITS.md | Document mesh-llm attribution |

## Phase 5.5: TRM Lending Primitives

**Goal:** Enable TRM lending, borrowing, and credit scoring to lower the participation barrier.

| Deliverable | Description |
|---|---|
| LoanRecord type | Dual-signed loan structure in tirami-ledger |
| Credit score | Composite score from trade + repayment history |
| Lending API | /v1/tirami/lend, /borrow, /repay, /credit, /pool, /loans |
| Collateral system | TRM reservation for loan collateral, auto-release on repay |
| Default handling | Auto-liquidation on missed repayment deadline |
| Free tier evolution | 1,000 TRM grant becomes 0% interest welcome loan |
| Lending safety | Pool reserves (30%), velocity limits, default-rate circuit breaker |

## Phase 6: Multi-Model Pricing + Routing

**Goal:** Different TRM rates per model, intelligent provider selection.

| Deliverable | Description |
|---|---|
| Model tier pricing | TRM/token rates per model size class (small/medium/large/frontier) |
| MoE discount | Reduced pricing for mixture-of-experts models (active params / total params) |
| Routing API | GET /v1/tirami/route for cost/quality-optimal provider selection |
| Provider ranking | Multi-factor scoring (reputation, price, latency, model quality) |

## Phase 7: L2/L3/L4 Rust rewrite ✅ (2026-04-07)

**Goal:** Replace the Python scaffolds for tirami-bank/mind/agora with Rust
workspace crates inside `clearclown/tirami`. Bit-for-bit semantic preservation.

| Deliverable | Status |
|---|---|
| tirami-bank Rust crate (53 tests) | ✅ Strategies, portfolio, futures, insurance, RiskModel VaR, YieldOptimizer |
| tirami-mind Rust crate (53 tests) | ✅ Harness, CuBudget, Benchmark, MetaOptimizer, ImprovementCycleRunner, ForgeMindAgent |
| tirami-agora Rust crate (42 tests) | ✅ AgentRegistry, ReputationCalculator (4 sub-scores), CapabilityMatcher, Marketplace |
| forge-economics §10/§11/§12 | ✅ All L2/L3/L4 constants centralized as single source of truth |
| Python repos archived | ✅ Tagged v0.1.0-python-scaffold, redirect READMEs in clearclown/tirami-{bank,mind,agora} |
| Workspace tests | ✅ 291 passing (was 143) |

## Phase 8: L2/L3/L4 wired into tirami-node ✅ (2026-04-08)

**Goal:** Make L2/L3/L4 first-class citizens of the running tirami node.
A single `tirami node --port 3000` exposes the full 5-layer Tirami ecosystem
over HTTP, real TRM is consumed by the self-improvement loop.

| Deliverable | Status |
|---|---|
| `bank_adapter::pool_snapshot_from_ledger()` | ✅ Live ledger state → tirami_bank::PoolSnapshot |
| `agora_adapter::refresh_marketplace_from_ledger()` | ✅ Lazy trade-log drain via last_seen_idx cursor |
| `mind_adapter::record_frontier_consumption()` | ✅ Frontier model = SHA-256("frontier:" + model_id) NodeId, real TradeRecord on ledger |
| 8 `/v1/tirami/bank/*` endpoints | ✅ portfolio / tick / strategy / risk / futures (×2) / risk-assessment / optimize |
| 7 `/v1/tirami/agora/*` endpoints | ✅ register / agents / reputation / find / stats / snapshot / restore |
| 5 `/v1/tirami/mind/*` endpoints | ✅ init / state / improve / budget / stats |
| `CuPaidOptimizer` | ✅ reqwest-backed MetaOptimizer (Anthropic Messages API), graceful fallback on network error |
| Async `MetaOptimizer` trait | ✅ #[async_trait], all 53 tirami-mind tests migrated to #[tokio::test] |
| Workspace tests | ✅ 315 passing (was 291) |
| verify-impl.sh | ✅ 57 / 57 GREEN |

## Phase 9: Production hardening ✅ (2026-04-08)

**Goal achieved:** Phase 8 is now production-grade and fully propagated to forge-mesh.

| Deliverable | Status | Notes |
|---|---|---|
| Theory ↔ impl audit | ✅ | 43 match / 0 drift / 1 minor missing; `docs/THEORY-AUDIT.md` |
| forge-mesh Phase 7+8 sync | ✅ | 45 new /api/forge/* endpoints; 393 → 641 tests |
| tirami-sdk Phase 8 wrappers | ✅ | 20 new Python methods, v0.3.0, 27 pytest tests |
| forge-cu-mcp Phase 8 tools | ✅ | 20 new MCP tools, v0.3.0 |
| Reputation gossip | ✅ | ReputationObservation wire msg + weighted-median consensus |
| NIP-90 relay publish | ✅ | tokio-tungstenite WebSocket publisher in tirami_ledger::agora_relay |
| Persistent L2/L3/L4 state | ✅ | BankServices / Marketplace / ForgeMindAgent survive restarts |
| Collusion resistance | ✅ | Tight cluster + volume spike + round-robin Tarjan SCC detection |
| Workspace tests | ✅ | 315 → **337** (+22) |
| verify-impl.sh | ✅ | 57 → **72/72 GREEN** |

## Phase 10: v0.3 productization ✅ (2026-04-09)

**Goal achieved:** tirami-sdk/MCP 0.3.0 ready for publish, reputation gossip signed,
forge-mesh running CI, Compute Standard paper drafted, Prometheus + Bitcoin anchoring shipped.

| Deliverable | Status | Notes |
|---|---|---|
| P1 tirami-sdk + forge-cu-mcp 0.3.0 wheels | ✅ | 4 artifacts twine-checked, git tags created, `twine upload` gated on user PyPI credentials (see `sdk/python/PUBLISH-0.3.0.md`) |
| P2 Ed25519-signed ReputationObservation | ✅ | `new_signed()` + strict `verify()`, unsigned obs rejected end-to-end |
| P3 forge-mesh GitHub Actions CI | ✅ | `.github/workflows/rust-workspace.yml` + README badge |
| P4 forge-mesh persistent L2/L3/L4 state | ✅ | `state_persist.rs` ported; +5 round-trip tests |
| P5 Prometheus metrics export | ✅ | `tirami_ledger::metrics::ForgeMetrics` + `/metrics` endpoint; 11 metric series including collusion scores |
| P6 Bitcoin OP_RETURN anchoring | ✅ | `tirami_ledger::anchor` + `/v1/tirami/anchor` endpoint; 40-byte FRGE payload, 80-byte standard limit |
| P7 Compute Standard paper v0.1 | ✅ | 7,000-word preprint in `forge-economics/papers/compute-standard.md`, arXiv-ready |
| Workspace tests | ✅ | 337 → **785** |
| forge-mesh tests | ✅ | 641 → **646** (+5) |
| verify-impl.sh | ✅ | 72 → **123 GREEN** |

## Phase 11: v0.4+ research frontier (planned)

**Goal:** zkML verification, federated training, BitVM-style optimistic verification,
and the A2A / MCP market layer.

| Deliverable | Description |
|---|---|
| zkML verification proofs | Proof-of-useful-work via zk SNARKs over inference traces |
| Federated fine-tuning | Distributed training loop paid in TRM |
| BitVM optimistic verification | Off-chain dispute resolution for TRM claims |
| A2A payment extension | TRM payment headers for Google A2A protocol |
| Reputation signing propagation | Wire the new_signed() helper into the gossip scheduler |
| LDK wallet integration for anchor broadcast | Connect tirami-lightning to the anchor tx skeleton |
| tirami-sdk / forge-cu-mcp PyPI upload | User-gated final step of Phase 10 P1 |

## Phase 12-16: Governance + L2 anchor ✅ (2026-04-10 → 2026-04-17)

Stake / governance / referral + unified scheduler + FLOP measurement
+ hybrid-chain anchor. All tested, all in production paths. See
[CHANGELOG.md](../CHANGELOG.md) for per-wave detail.

## Phase 17: Large-Scale Security Hardening ✅ (2026-04-18)

**Goal:** Make Tirami safe to release on a public adversarial
network at 100-1 000 nodes of scale. Organized as 4 waves of
6 - 8 items each, 24 primitives total, 180 new unit tests.

See [`security/phase-17-summary.md`](security/phase-17-summary.md)
for the condensed audit-facing view. Per-wave highlights:

**Wave 1 — P0 Critical Integrity**
- TradeRecord v2 with 128-bit nonce (replay defense)
- `execute_signed_trade` with nonce dedup cache
- Slashing wired into a 5-minute daemon loop
- AuditVerdict::Failed → 30% stake burn + audit trail
- Scoped API tokens (ReadOnly / Inference / Economy / Admin)
- Ed25519 + ML-DSA hybrid signature scaffold

**Wave 2 — P1 Scale Hardening**
- SPoRA random-layer audit
- Probabilistic heavy-audit scaffold (1% / 2-of-3 quorum)
- Per-ASN rate limiter
- Trade-log seal + JSON-lines archive
- Fork detection + nonce-fraud proofs
- PeerRegistry LRU bound
- Base Sepolia scaffold + deployment runbook
- Welcome-loan per-bucket Sybil cap

**Wave 3 — P2 Hostile-Environment Readiness**
- Hardware attestation scaffold (Apple SE / NVIDIA H100 CC / Intel SGX / AMD SEV-SNP)
- Kani formal verification (10 invariants; target ≥ 30 before audit)
- External audit scope + candidate shortlist
- DDoS mitigation (connection cap + 7-section ops guide)
- Key rotation scaffold (NodeIdentity with epochs)
- Bug bounty framework

**Wave 4 — Production Wire-up**
- WelcomeLoanLimiter wired into ComputeLedger
- `max_concurrent_connections` enforced in transport
- Daemon checkpoint loop (`seal_and_archive` every hour)
- AsnRateLimiter wired into transport (via `Connection::paths()` → `PathInfo::remote_addr()`)
- API self-sign path clarification (architectural)
- Kani + Foundry CI + TiramiBridge first-mint fix + PGP setup guide

Mainnet deploy remains **BLOCKED** until the external gates
listed at the top of `security/phase-17-summary.md` are all
satisfied (external audit + Sepolia stability + multi-sig +
bug bounty live + ML-DSA dep resolution).

## Phase 18 — Post-audit integration (planned)

Driven by the external audit's findings plus the external
gates that were documented but couldn't be closed in Phase 17:

| Deliverable | Description |
|---|---|
| External audit findings resolution | Act on Critical / High / Medium findings from the auditor |
| Base Sepolia live deploy | Using the Wave 2.7 runbook |
| Fork resync wire protocol | `ResyncRequest` + `ResyncBatch` messages (part-2 of Wave 2.5) |
| Real ML-DSA backend | Swap `MockPqVerifier` for real FIPS-204 once the dep pin resolves |
| CandleEngine SPoRA override | Backend-specific `generate_audit_at_layer` that hashes real intermediate activations |
| NodeIdentity live integration | Migrate `ForgeTransport` + `forge node rotate-key` CLI |
| Apple SE / NVIDIA H100 CC bindings | Real hardware attestation via `security-framework` / `nvml-wrapper` |
| Kani proof expansion | 10 → 30+ invariants |
| Per-endpoint scope gating | Migrate each `/v1/tirami/*` to require its appropriate `ApiScope` |
| Mainnet deploy | After 30-day Sepolia stability + multi-sig + bounty live |

## Long-term

| Milestone | Description |
|---|---|
| Compute derivatives | Forward contracts on future compute capacity |
| tirami-bank | Advanced financial instruments (futures, insurance, yield optimization) |
| SDK release | tirami-node as embeddable Rust library with stable API |
| Protocol v2 | Lessons from v1, backward-compatible evolution |
| Cross-architecture | NVIDIA GPU, AMD ROCm, RISC-V support (via mesh-llm) |
| Federated training | Distributed fine-tuning, not just inference |
| Compute Standard paper | Academic publication on TRM-native economics |

> The protocol is the platform. The computation is the currency. The agents are the economy.
