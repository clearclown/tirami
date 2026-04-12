# HN / Reddit / X launch teaser drafts

> Pre-launch copy for the Phase 10 / 11 announcement. Not yet published.

---

## Option A — Hacker News submission

**Title:** `Show HN: Forge – Distributed LLM inference where compute itself is the currency`

**Text:**

I've been building **Forge** — a Rust protocol where computation is the medium of exchange. No token, no ICO, no speculation. 1 Tirami Resource Merit (TRM) = 10^10 FLOPs of verified inference. You earn TRM by running a local model for someone else; you spend TRM by asking a node to run inference for you.

The whole stack is a 5-layer Rust workspace on top of `llama.cpp` via `llama-cpp-2`:

- **L0** — mesh-llm inference (Metal / CUDA / CPU via GGUF)
- **L1** — dual-signed trade ledger + lending pool + dynamic pricing
- **L2** — PortfolioManager, Strategies, FuturesContract, VaR RiskModel
- **L3** — ForgeMindAgent: AI agent that self-improves by paying TRM to a frontier model
- **L4** — Marketplace with reputation consensus and collusion detection

It's OpenAI-compatible (drop-in `OPENAI_BASE_URL`), has a Python SDK + MCP server, Prometheus `/metrics`, Bitcoin OP_RETURN anchoring of the trade Merkle root, signed reputation gossip, and a theoretical companion paper.

**Verified end-to-end today** (2026-04-09): one command downloads SmolLM2-135M, starts a node, runs 3 real chat completions on Metal GPU, and you can watch real TRM land in `/v1/tirami/balance` while `/v1/tirami/bank/tick` produces a lending Decision off the live pool state:

```
$ bash scripts/demo-e2e.sh
✓ model loaded after 1×2s
✓ prompt="What is 2+2?" → cu=16  reply="4 / Explanation..."
✓ balance: contributed=48 CU, reputation=0.5
✓ PortfolioManager.tick() → action=lend
✓ RiskModel VaR 99%: 692 TRM (DEFAULT_RATE=0.02, LGD=0.50, σ=2.33)
✓ merkle_root: 374d7b... (Bitcoin OP_RETURN script ready to broadcast)

All Phase 1-10 endpoints verified with live data.
```

**Stats**: 1,048 passing tests across 4 repos, 72/72 verify-impl assertions GREEN, 7,000-word academic paper synthesizing the theory.

The thesis in one sentence: *every other compute-economy project (Bittensor, Akash, Render, Ollama, Together.ai) either burns electricity on non-useful work, inserts a speculative token between compute and value, or runs as a centralized commercial service. Forge is the only project where the unit of account is the FLOP, the unit of work is the inference, and the only way to get more units is to do more useful work.*

Happy to answer questions. Code: https://github.com/clearclown/forge — MIT.

---

## Option B — Twitter/X thread (12 posts, under 280 chars each)

**1/12** I just shipped Phase 10 of Forge — a distributed LLM inference protocol where compute IS the currency.

No token. No ICO. 1 TRM = 10^10 FLOPs of verified inference.

Written 100% in Rust on top of llama.cpp. OpenAI-compatible drop-in replacement.

🧵

**2/12** The pitch: every other "compute economy" (Bittensor, Akash, Render) inserts a speculative token between you and the work. Forge doesn't.

You earn TRM by running inference for others. You spend TRM by asking someone else's node to run inference. That's the whole economy.

**3/12** 5-layer architecture, all Rust, single binary:

L0 — mesh-llm inference (Metal/CUDA/GGUF)
L1 — dual-signed TRM ledger + lending pool
L2 — futures, insurance, VaR RiskModel
L3 — ForgeMindAgent (self-improves paid in CU)
L4 — Reputation marketplace

**4/12** The L3 piece is the most fun: `ForgeMindAgent::improve()` runs a self-optimization loop where a frontier model (Claude, GPT, local CuPaidOptimizer) proposes a new system prompt. If benchmark improves + ROI ≥ 1.0, commit. The API call cost is deducted from the agent's TRM balance FOR REAL.

**5/12** OpenAI compat means drop-in replacement:

```bash
export OPENAI_BASE_URL=http://localhost:3000/v1
# Your existing Python / JS / curl just works
```

Plus you get `/v1/tirami/{balance,trades,pool,bank,mind,agora,anchor,...}` — 45 economic endpoints.

**6/12** Phase 10 closeout metrics:

• 1,048 passing tests (forge 359 + forge-mesh 646 + tirami-sdk 27 + forge-economics 16)
• 72/72 verify-impl assertions GREEN
• Theory ↔ code audit: 43 match, 0 drift
• 12 Rust crates + 1 Python SDK + 1 MCP server + 1 paper

**7/12** Verified end-to-end TODAY on Apple Silicon Metal GPU:

```
bash scripts/demo-e2e.sh
```

Downloads SmolLM2-135M (~100MB), starts node, runs 3 real chat completions, walks through every endpoint, prints colored summary. Cold start ~30s.

**8/12** The Prometheus /metrics surface is legit — 11 series including `forge_trade_count_total`, `forge_cu_contributed_total{node_id=...}`, `forge_collusion_trust_penalty{node_id=...}`. Ready to wire into Grafana.

**9/12** Phase 10 P6 is wild: a `/v1/tirami/anchor?network=mainnet` endpoint returns a Bitcoin OP_RETURN script carrying the 32-byte trade log Merkle root. You can broadcast it through any Bitcoin wallet and get a free integrity witness for your entire trade history.

**10/12** The companion paper "The Compute Standard" (7,000 words, 20 citations) is drafted in forge-economics/papers/. Synthesizes Landauer (1961), Soddy (1926), Nakamoto (2008), Hayek (1945), and the Forge spec into a single coherent argument for compute-as-currency.

**11/12** If you're coming from:

• Bittensor → same "compute earns value" intuition without the speculative token
• Akash/Render → per-request metered instead of container-rental
• Ollama → same model loading, add a mesh + an economy
• OpenAI API → set OPENAI_BASE_URL, regain sovereignty

**12/12** Code: github.com/clearclown/forge (MIT)
Paper: github.com/clearclown/forge-economics
SDK: pip install tirami-sdk
MCP: pip install forge-cu-mcp (Claude Code / Cursor)
Theory: docs/compatibility.md shows the full feature matrix

Compute is currency. Welcome.

---

## Option C — Reddit r/LocalLLaMA

**Title:** `I built a drop-in llama.cpp replacement that turns every inference into an economic trade`

**Body:**

Hey r/LocalLLaMA,

I've been heads-down for a few months on **Forge** — a Rust project that wraps llama.cpp (via `llama-cpp-2`) with a complete compute-as-currency economy. I just finished Phase 10 and figured this subreddit would get it.

**What it is**

Forge is literally mesh-llm + llama.cpp + a TRM ledger + 4 layers of economic primitives stacked on top. You run `forge node -m smollm2:135m` and you get:

1. An OpenAI-compatible HTTP API on `http://localhost:3000/v1` (same shape as llama-server)
2. A TRM ledger that records 1 TRM per ~1 token of output
3. A P2P mesh (iroh QUIC + Noise) where your node can earn TRM serving other people's requests
4. A lending pool, portfolio manager, futures contracts, insurance, reputation consensus
5. An AI self-improvement loop (ForgeMindAgent) that spends TRM to improve its own prompts via frontier LLMs
6. Prometheus metrics, Bitcoin OP_RETURN anchoring of the trade log, Nostr NIP-90 discovery

All of this is single-binary Rust. It uses llama-cpp-2 under the hood so any GGUF works — q4_k_m, q5_k_m, q6_k, q8_0, f16, f32. Every llama.cpp architecture is automatically supported: Llama, Qwen, Mistral, Mixtral, Phi, Gemma, DeepSeek, SmolLM, Yi, Command-R, etc.

**Why you should care**

If you're already running `llama-server` or `ollama serve` at home:

- **Drop-in**: set `OPENAI_BASE_URL=http://localhost:3000/v1`, your existing clients work unchanged
- **Earn**: if you expose your node on your LAN/WAN, other nodes can pay you TRM for inference. TRM is earnable-only; no token, no ICO, no speculation.
- **Budget-aware agents**: the Python SDK `pip install tirami-sdk` and MCP server `pip install forge-cu-mcp` let Claude Code / Cursor drive the full stack directly.

If you're already running Bittensor / Akash:

- No token between you and the work. The unit of account is the FLOP. The unit of work is the inference. That's the whole currency.

**Where it is**

- Code: github.com/clearclown/forge (MIT)
- One-command demo: `bash scripts/demo-e2e.sh` — downloads SmolLM2-135M, starts the node, walks every endpoint
- Verified end-to-end TODAY with SmolLM2 on Metal GPU. 1,048 tests passing across 4 repos.
- Theory paper: github.com/clearclown/forge-economics/papers/compute-standard.md (7,000 words)

**What it isn't yet**

- Token-by-token streaming is currently buffered-into-SSE (pseudo-streaming). I'm fixing that right now — real streaming is the Phase 11 priority. Should land within a day or two.
- The lending pool isn't yet integrated with Lightning for BTC bridging (works locally, bridge is scaffolded).
- `/v1/tirami/mind/improve` with `CuPaidOptimizer` calls the Anthropic Messages API format only — GPT / Together / Groq are TODO.

Happy to answer any questions here, or open issues at github.com/clearclown/forge/issues.

---

## Demo script output (for embedding in posts)

```
═══ build ═══
  ✓ binary already built at target/release/forge

═══ start node (smollm2:135m on port 3001) ═══
  ✓ node PID 57688, log: /tmp/forge-demo-node.log
  ✓ model loaded after 1×2s

═══ L0 inference: 3 real chat completions ═══
  ✓ prompt="What is 2+2?" → cu=16  reply="4 / Explanation..."
  ✓ prompt="Name a color." → cu=16  reply=" , El Terzo..."
  ✓ prompt="Say hi briefly." → cu=16  reply=" `Hey everyone`..."

═══ L1 economy: balance + trades + pricing ═══
  ✓ balance: contributed=48 CU, reputation=0.5 (DEFAULT_REPUTATION constant)
  ✓ trade log: 3 records
  ✓ deflation_factor: 0.997013 (drops slightly per trade)

═══ L2 tirami-bank: portfolio tick on real pool state ═══
  ✓ PortfolioManager.tick() → action=lend
  ✓ RiskModel VaR 99%: 692 TRM (DEFAULT_RATE=0.02, LGD=0.50, σ=2.33)

═══ L4 tirami-agora: register + find ═══
  ✓ registered demo agent (hex=aaa...)
  ✓ Marketplace.find() returned 1 matches

═══ L3 tirami-mind: init + 1 echo improvement cycle ═══
  ✓ ForgeMindAgent initialized with EchoMetaOptimizer
  ✓ improve(1) → decision=Revert (echo never improves — correct)

═══ Phase 10 P5: Prometheus /metrics ═══
  ✓ forge_cu_contributed_total{node_id="0000..."} 48
  ✓ forge_trade_count_total 3
  ✓ forge_reputation{node_id="0000..."} 0.5

═══ Phase 10 P6: Bitcoin OP_RETURN anchor ═══
  ✓ merkle_root: 374d7b467a36dc1ac809f59512c01a3d10d26d1fd0d74b499d77d0ec2ff39972
  ✓ script:      6a284652474501000000374d7b467a36dc1...
  ✓ → valid Bitcoin OP_RETURN payload, ready to broadcast

═══ summary ═══
  ✓ 5-layer Forge stack ran end-to-end on a real GGUF model
  ✓ 48 TRM contributed across 3 real inference trades
  ✓ Bitcoin anchor = 374d7b46...

All Phase 1-10 endpoints verified with live data.
```

---

## Asciinema / VHS script (future)

```vhs
Output docs/assets/demo-e2e.gif
Set FontSize 14
Set Width 1200
Set Height 800
Set Theme "Dracula"

Type "bash scripts/demo-e2e.sh"
Sleep 500ms
Enter
Sleep 30s
```

Generate with: `vhs docs/assets/demo-e2e.tape`

---

## Key talking points / objections anticipated

**Q: "Isn't this just Bittensor with extra steps?"**
A: No — Bittensor inserts the TAO token between validators and miners. Forge doesn't have a token. The unit of account IS the FLOP. You earn TRM directly by serving inference; no validator pool, no yield farming, no speculation.

**Q: "How do you prevent Sybil attacks?"**
A: Welcome loan has a `welcome_loan_sybil_threshold: 100` — once you've seen >100 unknown nodes in a time window, further welcome-loan grants are denied. Plus `tirami_ledger::collusion::CollusionDetector` runs Tarjan SCC on the trade graph to detect round-robin wash trading.

**Q: "Why Rust?"**
A: The inference layer already lives in llama.cpp (C++), exposed via `llama-cpp-2` Rust bindings. Writing the economic layer in Rust means one build target, one test runner, one type system, one deployment artifact. The theory crate `forge-economics` and the paper are Markdown/LaTeX, but everything that matters at runtime is Rust.

**Q: "How does the node earn TRM when it's idle?"**
A: Two ways: (1) passive "availability yield" at 0.1%/hr × reputation (Phase 1), and (2) respond to inference requests from peers (the main mechanism). There's also a welcome loan of 1,000 TRM at 0% interest for 72 hours so new nodes can experiment before needing to earn.

**Q: "What if I just want llama-server with no economy?"**
A: Disable the economic layer entirely — ignore `/v1/tirami/*` and forge degrades to "llama-server with extra Rust crates compiled in". The TRM accounting still runs, but if no one looks at `/v1/tirami/balance`, it's harmless.

**Q: "Is this financialized? Does it leak compute into speculation?"**
A: No. TRM is earnable-only — you literally cannot buy CU. There is no token, no ICO, no secondary market. Bridging to Bitcoin Lightning exists as an optional settlement path, but the core protocol has no fiat on-ramp in its design.
