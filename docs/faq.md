# Forge — Frequently Asked Questions

- [What is a TRM?](#what-is-a-compute-unit)
- [How is TRM different from a token like BTC or TAO?](#how-is-cu-different-from-a-token-like-btc-or-tao)
- [How much TRM do I earn by running a node?](#how-much-cu-do-i-earn-by-running-a-node)
- [Can I bridge TRM to Bitcoin?](#can-i-bridge-cu-to-bitcoin)
- [Does my data stay private?](#does-my-data-stay-private)
- [How does reputation consensus resist collusion?](#how-does-reputation-consensus-resist-collusion)
- [What happens if my node goes offline?](#what-happens-if-my-node-goes-offline)
- [Is there a token sale, ICO, or presale?](#is-there-a-token-sale-ico-or-presale)
- [How does Forge differ from Bittensor, Akash, or Ollama?](#how-does-forge-differ-from-bittensor-akash-or-ollama)
- [What is the learning curve?](#what-is-the-learning-curve)
- [Is Forge production-ready?](#is-forge-production-ready)
- [How do I contribute?](#how-do-i-contribute)

---

### What is a TRM?

1 TRM = 10^9 FLOPs (one billion floating-point operations) of verified LLM inference (parameters.md §1 `cu_definition`). It is the atomic unit of economic account in Forge.

CU is **earned** by nodes that serve inference to other nodes, and **spent** by nodes that consume inference. Every `POST /v1/chat/completions` request creates a trade record: the provider's `contributed` balance increases by `cu_cost`, and the consumer's `consumed` balance increases by the same amount. The transfer is zero-sum — no TRM is created or destroyed by the trade itself.

---

### How is TRM different from a token like BTC or TAO?

CU cannot be bought on an exchange. There is no token sale, no ICO, no pre-mine, no secondary market. The only way to acquire TRM is to perform useful computation for another node.

| Property | BTC / TAO | TRM |
|---|---|---|
| How to get it | Buy on exchange | Serve inference |
| Exchange listing | Yes | No |
| Speculation | Possible | Structurally impossible |
| Supply anchor | SHA-256 difficulty / validator score | Physical FLOPs of useful work |
| Human approval per tx | Required (wallet signature) | Not required (agent acts autonomously) |

CU is also anchored to thermodynamic reality: producing 1 TRM requires burning real electricity running real llama.cpp inference. The physical price floor is approximately $0.000001/CU (electricity cost) and the ceiling is approximately $0.000132/CU (Mac Mini M4 hardware amortization) per parameters.md §9.

---

### How much TRM do I earn by running a node?

Three factors determine earnings:

**1. Inference volume**: more requests served = more CU. Each token generated earns TRM at the tier rate for the model you're serving. Rates per parameters.md §2:
- Small tier (< 3B params): 1 CU/token
- Medium tier (3B–14B): 3 CU/token
- Large tier (14B–70B): 8 CU/token
- Frontier tier (> 70B): 20 CU/token

**2. Reputation**: new nodes start at 0.5 (`DEFAULT_REPUTATION`, parameters.md §7). Reputation rises with uptime and successful trades. Higher reputation means higher availability yield.

**3. Availability yield**: nodes that stay online earn `0.1%/hour × reputation` on their accumulated contributed TRM (parameters.md §7 `availability_yield_rate`). At reputation 1.0, a node with 10,000 TRM contributed earns 10 CU/hour just for being reachable.

In practice: a Mac Mini M4 running Qwen2.5-7B (Large tier) and serving a moderate load can produce around 5,000,000 CU/year (parameters.md §9 `mac_mini_annual_cu_capacity`). This is the physical production ceiling for that hardware class.

---

### Can I bridge TRM to Bitcoin?

Yes, optionally. The `tirami-lightning` crate implements a CU↔BTC bridge via Lightning Network.

`POST /v1/tirami/invoice` creates a Lightning invoice that pays your TRM balance out as satoshis. The CLI equivalent is `forge settle --pay`. This requires a configured LDK wallet (`forge wallet info` to check status).

The reverse direction (`create_deposit()`) accepts a Lightning payment and credits your TRM balance.

This is **entirely optional**. The Forge protocol has no blockchain in its critical path, no on-chain fees, and no requirement to ever touch Bitcoin. The bridge exists for hardware owners who want to convert TRM earnings to BTC, and for agents that need to purchase digital services in the human economy (cloud GPU, APIs) using BTC.

---

### Does my data stay private?

Partially, with a clearly stated trust boundary.

**In transit**: all connections use QUIC with TLS 1.3 (via iroh) plus a Noise Protocol XX-pattern handshake. Passive network observers and relay servers cannot read prompts or responses.

**At the serving node**: in the current seed/worker topology, the node that runs inference sees your prompt text. This is an explicit trust boundary, not a solved privacy property. Only connect to seeds you trust with plaintext prompts.

**In gossip**: trade records that propagate across the mesh include metadata (provider NodeId, consumer NodeId, TRM amount, token count, model_id, timestamp) but **not** the prompt content or response text.

**Local inference** (`forge node` mode): if you run the node locally and don't expose P2P ports, your prompts never leave your machine.

Full privacy for distributed inference (encrypted activation tensors between pipeline stages) is Phase 11+ work (zkML). It is not available today.

---

### How does reputation consensus resist collusion?

Two independent layers:

**Layer 1 — Signed observations**: `ReputationObservation` gossip messages are signed with Ed25519 (`new_signed()` + strict `verify()`). Unsigned or invalidly-signed observations are rejected before they can influence a node's effective reputation. No node can forge another node's observation.

**Layer 2 — Collusion detection**: `tirami_ledger::collusion::CollusionDetector` runs three independent detection algorithms on the trade graph:
- *Tight cluster*: identifies groups of nodes that trade almost exclusively with each other.
- *Volume spike*: detects sudden anomalous increases in trade volume between specific pairs.
- *Round-robin (Tarjan SCC)*: uses Tarjan's strongly-connected-components algorithm to find circular trade patterns that resemble wash trading.

When a node scores above the detection threshold, a `trust_penalty` (up to 0.5) is subtracted from its effective reputation automatically. The penalty is visible in `/metrics` as `forge_collusion_trust_penalty`.

---

### What happens if my node goes offline?

Your TRM balance persists in `tirami-ledger.json` across restarts and disconnections. The key invariant is: **earned TRM is never lost to a node restart**.

What does decay over time:

- **Reputation (uptime component)**: decays at 0.01/day after 7 days of inactivity (parameters.md §7 `inactivity_decay_rate`, `inactivity_decay_threshold_days`).
- **CU balance**: nodes offline for more than 90 days may have accumulated TRM burned at 1%/month (parameters.md §7 `inactivity_burn_rate`, `inactivity_burn_threshold_days`). This is an anti-squatting measure.
- **Open loans**: a loan that reaches its due date while the node is offline will default. Collateral is liquidated automatically. Credit score collapses after a default and takes weeks to rebuild.

On restart, the ledger loads from its JSON snapshot (HMAC-SHA256 verified), and the node resumes earning immediately on its first served request.

---

### Is there a token sale, ICO, or presale?

No. There is no Forge token. TRM is earnable-only. Anyone claiming to sell "Forge tokens" or "pre-sale CU" is running a scam. Report it.

The only on-ramp to TRM is:
1. Serve inference to other nodes (earn directly).
2. Receive the 1,000 TRM welcome loan (0% interest, 72-hour term, parameters.md §3).
3. Have another node lend you TRM via the lending pool.

There is no ICO, no presale, no investor allocation, no foundation reserve, no token contract on any chain.

---

### How does Forge differ from Bittensor, Akash, or Ollama?

See `docs/compatibility.md` for the full feature matrix. The short version:

**vs Bittensor (TAO)**: Bittensor has a speculative token (TAO) managed by a validator pool and opaque subnet scoring. Forge has no token — TRM is the computation itself. There is no validator to bribe, no subnet to curate, no inflation schedule to game.

**vs Akash (AKT)**: Akash is a container rental marketplace metered by the hour. Forge is per-request metered for LLM inference. There is no AKT-style token in Forge — providers are paid in TRM immediately upon serving each request.

**vs Ollama**: Ollama is an excellent single-node inference server with no P2P and no economy. If you want one model on one machine, Ollama is simpler. If you want a fleet of nodes that earn TRM when idle, lend to other nodes, and run a self-improvement loop, use Forge.

---

### What is the learning curve?

Under 30 seconds to first inference and first TRM earned:

```bash
git clone https://github.com/clearclown/forge && cd forge
bash scripts/demo-e2e.sh
```

This downloads SmolLM2-135M (~100 MB from HuggingFace), starts a forge node with Metal/CUDA acceleration, runs 3 real chat completions, and walks through every Phase 1-12 endpoint with live data and colored output. The demo requires no configuration, no API keys, and no accounts.

After the demo completes, the same node responds to OpenAI-compatible clients:

```bash
export OPENAI_BASE_URL=http://127.0.0.1:3001/v1
# Now any OpenAI SDK or LangChain client just works.
```

---

### Is Forge production-ready?

The codebase is well-tested: 426 unit and integration tests, 95/95 conformance assertions (verify-impl.sh), and empirically validated on Apple Silicon via the demo script. The economic model has been run end-to-end with real llama.cpp inference and real trade records.

However: this is v0.3, intended for single-operator deployments and research use. There are no production SLAs. The codebase has not had a third-party security audit. Phase 13+ work (real zkML proofs, real BitVM dispute resolution, forge-mesh full sync with production CI) is required before recommending Forge for high-value deployments.

Run it for curiosity, research, and small-scale experiments. Treat TRM balances accordingly.

---

### How do I contribute?

Read `docs/developer-guide.md` first — it covers repo layout, test requirements, and the spec-first workflow for new economic primitives.

For small fixes (typo, test gap, minor bug): send a PR directly. For large changes (new crate, new layer, protocol change): open an issue first to discuss the design before writing code. All PRs require all three test suites to be green before merge.

---

See also: [docs/developer-guide.md](developer-guide.md), [docs/architecture.md](architecture.md), [docs/economy.md](economy.md), [docs/compatibility.md](compatibility.md).
