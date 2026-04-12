# Forge — Concept & Vision

## The Problem Is Not Distributed Inference

Projects like [mesh-llm](https://github.com/michaelneale/mesh-llm), Petals, and Exo have shown that you can split LLM inference across multiple devices over a network. The hard engineering of pipeline parallelism, expert sharding, and mesh coordination is largely solved.

The unsolved problem is: **why would anyone contribute their hardware?**

mesh-llm pools GPUs beautifully — but if you run your Mac Mini as a mesh node for a year, you get nothing. No record of contribution, no priority access, no economic return. The network runs on goodwill. Goodwill doesn't scale.

## The Insight: Compute Is Money

Every monetary system is backed by scarcity. Gold is scarce because geology. Oil is scarce because extraction costs energy. Bitcoin is scarce because mining burns electricity on SHA-256 hashes.

But Bitcoin's scarcity is artificial — the computation is purposeless. The hashes secure the ledger but produce nothing useful.

LLM inference is different. When a Forge node spends electricity to answer someone's question, that computation has **intrinsic value**. Someone wanted that answer badly enough to request it. The electricity was not wasted — it produced intelligence.

```
Bitcoin:   electricity → useless hashing → artificial scarcity → value
Forge:     electricity → useful inference → real utility → value
```

This is the **Compute Standard (計算本位制)**: a monetary system where the unit of value is backed by verified useful computation.

## What Forge Is

Forge is mesh-llm with an economy.

The inference layer (networking, model distribution, API) comes from mesh-llm. Forge adds:

1. **CU Ledger** — Every inference creates a trade. Provider earns CU, consumer spends CU. Dual-signed by both parties.
2. **Dynamic Pricing** — TRM per token floats with local supply and demand. More idle nodes → cheaper. More requests → more expensive.
3. **Proof of Useful Work** — TRM is earned by performing real inference, not by solving arbitrary puzzles.
4. **Agent Budget API** — AI agents can query their balance, estimate costs, and make autonomous spending decisions.
5. **External Bridges** — TRM can optionally be exchanged for Bitcoin (Lightning), stablecoins, or fiat through adapter layers outside the protocol.

## Why Not Just Use Bitcoin?

We considered making Bitcoin/Lightning the primary settlement layer. We decided against it.

| Concern | Explanation |
|---------|-------------|
| **Philosophical inconsistency** | Rewarding useful work in a currency backed by useless work |
| **External dependency** | If Bitcoin's security breaks (quantum computing, regulatory), Forge's economy breaks too |
| **Efficiency** | Lightning channel management is overhead for per-inference micropayments |
| **Self-sufficiency** | TRM has value because the computation itself is useful — it doesn't need external validation |

Bitcoin remains available as an **off-ramp** for operators who need external liquidity. But the protocol's native economy runs on CU.

## Why TRM Has Value

CU is not a speculative token. It is a **claim on future compute**.

If you earned 10,000 TRM by serving inference, you can spend those TRM to buy inference from any other node on the network. The value is not abstract — it is the ability to make a machine think for you.

This makes TRM a **productive asset**, not a store of value:

```
Apartment building          Mac Mini on Forge
───────────────────         ──────────────────
Asset: building             Asset: compute hardware
Cost: maintenance           Cost: electricity
Revenue: rent               Revenue: TRM from inference
Yield: rent - maintenance   Yield: TRM earned - electricity
Idle = lost income          Idle = wasted potential
```

Unlike Bitcoin (digital gold — holds value but produces nothing), TRM is like a rental property — it generates yield by performing useful work.

## AI Agents as Economic Actors

The most important consumer of Forge's economy is not humans — it's AI agents.

An agent running a small local model (1.5B parameters on a phone) has limited intelligence. But if it can earn TRM by lending idle compute and spend TRM to access larger models, it can autonomously expand its own capabilities:

```
Small agent (phone, 1.5B)
  → idle overnight → lends CPU → earns CU
  → morning: needs complex reasoning
  → checks /v1/tirami/balance → has 5,000 CU
  → checks /v1/tirami/pricing → 70B model costs 2,000 TRM for 500 tokens
  → buys 70B inference → gets smarter answer
  → uses answer to make better trading decisions
  → earns more TRM next cycle
```

This is the self-reinforcement loop: agents that make good economic decisions grow stronger, which lets them make even better decisions.

No human needs to approve individual transactions. The agent operates within a budget policy set by its owner. The protocol provides the market; the agent provides the strategy.

## Post-Marketing Economy

In today's economy, marketing exists because humans have limited attention and imperfect information. AI agents don't have this problem. An agent can benchmark every provider, verify every reputation score, and compare every price — instantly and objectively.

Forge enables a marketplace where providers are judged by verifiable performance, not advertising:

- **Reputation** is computed from dual-signed trade history — cryptographically verifiable
- **Pricing** reflects real supply/demand, not marketing budgets
- **Quality** is benchmarkable — agents can spot-check inference outputs
- **Discovery** happens via Nostr NIP-90 and A2A Agent Cards, not SEO

This is the vision behind **tirami-agora** (Layer 4): an agent marketplace where the best provider wins, not the loudest.

## Compute Microfinance

A node with 500 TRM cannot access a 70B model (costs ~2,000 TRM per session). Without lending, this node is permanently stuck at the small-model tier.

With TRM lending:

```
1. Node borrows 1,500 TRM at 0.5%/hr interest
2. Accesses 70B model for 4 hours
3. Serves premium inference, earns 3,000 CU
4. Repays 1,500 + 30 TRM interest
5. Net profit: 1,470 TRM (minus electricity)
```

This is the engine that makes Forge's self-improvement loop economically viable. No other distributed inference project offers compute lending — this was confirmed through comprehensive competitive analysis of Bittensor, Akash, Golem, io.net, Gensyn, Ritual, and others.

See [economy.md](economy.md) for the full lending specification and [strategy.md](strategy.md) for the competitive landscape.

## Comparison

| Project | Inference | Economy | Agent Autonomy | Key Limitation |
|---------|-----------|---------|----------------|---------------|
| **mesh-llm** | Distributed (pipeline + MoE) | None | Blackboard messaging only | No incentive to contribute |
| **Petals** | Distributed (collaborative) | None | None | Low activity, stuck on old transformers |
| **Ollama** | Local only | None | None | Single device, no network |
| **Exo** | Distributed (Apple Silicon) | None | None | No economic layer, research-stage |
| **Together AI** | Centralized | Pay-per-token (corporate) | API access only | Centralized, no agent economy |
| **Bittensor** | Subnet-based | TAO token ($2.9B) | Subnet-level | Validator gaming, speculative token |
| **Akash** | General cloud | AKT token ($118M) | None | Not AI-native, no lending |
| **Autonolas** | Agent-delegated | OLAS token ($10.5M) | Full autonomy | Token-dependent, no self-improvement |
| **Golem** | General compute | GLM token ($125M) | None | No GPU focus, 10 years of low adoption |
| **Forge** | Distributed (mesh-llm) | **CU (useful work)** | **Autonomous budget + lending** | Early stage |

## The Metaphor

A seed falls into the network. It earns its first TRM by lending idle cycles overnight. With those CU, it buys access to a larger model. It becomes smarter. It finds more efficient trades. More CU. A bigger model. A forest emerges from a single seed — not because someone planted it, but because the economics made growth inevitable.
