# Tirami Killer App — Personal AI Agents on a Distributed Compute Market

**Phase 18.5 · 2026-04-19 · Status: Product commitment.**

This document is the focal-point decision for what Tirami
actually **does** for users. Phase 1-17 built the infrastructure;
Phase 18.1-18.4 locked the Constitution and simplified the
surface; Phase 18.5 commits to the product story.

---

## One-sentence elevator pitch

> 「あなたの Mac の中にいる個人 AI エージェントが、余剰計算力を
> 売り、不足計算力を買い、あなたは結果だけ見る」

Your personal AI agent lives on your Mac (or home server). When
you're not using it, it earns TRM by serving inference for other
people's agents. When it needs more compute than your Mac
provides, it spends TRM on someone else's Mac. You never manage
TRM directly. The agent handles the economy; you get the
outputs.

## Why this, not the other candidates

From the Phase 18.4 killer-app evaluation:

| Candidate | Why not chosen |
|-----------|----------------|
| Ghost Inference | Requires zkML + TEE both mature (5 year wait). Current stack 80 % ready but the last 20 % is blocker-grade. |
| Local Claude Alternative | Ollama + LM Studio already cover this. Tirami's distributed economics isn't a differentiator for single-user local LLM. |
| Uncensored Research | Regulatory risk unbounded (child-abuse content, terror planning). Cannot sustain legally. |
| **Personal Agent × Distributed Market** | **Chosen.** Uses the full Tirami stack (local LLM + TRM + agent autonomy). Has a clear user. Differentiates structurally from every existing product. |

The critical insight: the other candidates each used *some* of
Tirami's stack. This one uses *all* of it — local LLM inference
(L0), economic layer (L1-L4), identity & signing (tirami-core),
agent autonomy (tirami-mind). Nothing is wasted, nothing is
incidental.

## User journey

### Day 0 — Install

```
$ brew install tirami
$ tirami start
→ Generating your personal agent's wallet ...
→ Downloading Qwen2.5-0.5B (500 MB, ~10 s) ...
→ Ready. Your agent is online. Say hi:
$ tirami chat
you> hi
agent> Hi! I'm your Tirami agent. I have 0 TRM and I'm
       listening for tasks. You can give me jobs any time.
       I'll let you know when my balance changes.
```

Three actions. No blockchain jargon. No "create a wallet" wizard.
The wallet is a byproduct of starting the daemon.

### Day 1-30 — Organic usage

```
you> summarize this PDF
agent> Done. I used 2 TRM from my bootstrap faucet. My balance:
       8 TRM remaining. If you give me more tasks like this I'll
       need to earn TRM.

you> can you find hotels in Tokyo for next week that allow cats?
agent> I can. This task needs web research (~15 min of reasoning).
       My Mac isn't powerful enough alone. I'll rent Mac Studio
       capacity from someone on the network for ~12 TRM. Is that
       okay? (Current rate: 0.8 TRM/minute of Frontier inference.)

you> yes
agent> Working. I'll notify you when done.

...15 minutes later...

agent> Done. 3 hotels that match. Saved to ~/Documents/hotels.md.
       Total cost: 11.8 TRM. I had 8 and earned 3.8 while you were
       away by serving inference for someone else's agent. Current
       balance: 0 TRM. I'm serving requests in the background to
       build a buffer.
```

This is the UX. No TRM transfers visible. No wallet addresses.
No "sign this transaction" popup. The agent is a Tamagotchi that
earns and spends its own money, and tells you in human terms.

### Day 365 — Passive income / passive expense

At steady state, a user's agent has accumulated some TRM buffer
(say 100 TRM). When the user is heavy (asks lots of big
questions), the buffer shrinks. When the user is light (casual
weekly use), the buffer grows. Over a month, it roughly
zero-sums — the user has paid nothing, received nothing, and has
an AI assistant they can trust is running on compute they don't
need to administer.

For power users: the buffer grows faster than spending. They
can either let it accumulate (passive TRM income convertible to
USDC via the Base bridge) or have the agent donate it to open
models / research / friends.

## Product requirements

### MUST

- **M1.** A single `tirami start` command creates a node +
  wallet + personal agent.
- **M2.** User talks to the agent via a chat interface (CLI or
  web). Never mentions TRM unless the user asks.
- **M3.** The agent has a budget. When a task exceeds the budget,
  the agent asks ("this costs X TRM, yes/no?").
- **M4.** The agent serves inference to others' agents in the
  background when the user's Mac is idle. "Idle" is defined by
  CPU / GPU utilization < 20 % for > 60 seconds.
- **M5.** The agent requests external compute for tasks that
  exceed local capacity (RAM / VRAM / latency thresholds).
- **M6.** User can inspect the agent's state via
  `tirami agent status` — shows balance, today's earn/spend,
  current tasks, preferences.
- **M7.** User can override any agent decision.

### SHOULD

- **S1.** Agent has pluggable preferences (max spend per task,
  which providers to prefer, content filters on serving).
- **S2.** The node auto-configures firewall / UPnP to accept
  inbound connections; if blocked, the agent operates in
  consume-only mode.
- **S3.** Multi-device support: the same user's agent on phone +
  Mac + home server shares one wallet (via Keychain sync).

### MAY

- **M-A1.** Agent-to-agent messaging (my agent asks your agent
  for a file you shared).
- **M-A2.** Voice UI.
- **M-A3.** Scheduled tasks ("every morning, summarize overnight
  emails").

### MUST NOT

- **NM1.** Require the user to understand "TRM", "nonce", "stake",
  or "gossip". The agent knows. The user doesn't.
- **NM2.** Show wallet addresses in the primary UI.
- **NM3.** Require KYC. The agent uses the stakeless faucet +
  earned TRM.

## Architecture

```
┌────────────────────────────────────────────────────┐
│                 User's Device                       │
│                                                     │
│  ┌───────────────────────────────────────────────┐ │
│  │  Chat UI (CLI / web)                          │ │
│  └──────────────┬────────────────────────────────┘ │
│                 │ natural language                   │
│  ┌──────────────▼────────────────────────────────┐ │
│  │  PersonalAgent                                │ │
│  │  ─ wallet: NodeIdentity                       │ │
│  │  ─ budget: TrmBudget                          │ │
│  │  ─ preferences: AgentPreferences              │ │
│  │  ─ decision loop: autonomous buy/sell         │ │
│  └────┬────────────────────────────┬─────────────┘ │
│       │ local inference             │ remote        │
│  ┌────▼──────────┐     ┌────────────▼─────────────┐│
│  │ CandleEngine  │     │ Tirami HTTP client       ││
│  │ (local LLM)   │     │ (send/receive tasks)     ││
│  └───────────────┘     └──────────┬───────────────┘│
│                                    │                │
│  ┌─────────────────────────────────▼──────────────┐│
│  │  ForgeTransport (iroh QUIC)                    ││
│  └─────────────────────────────────┬──────────────┘│
└────────────────────────────────────┼───────────────┘
                                     │
                             Tirami P2P network
                                     │
                 ┌───────────────────▼──────────────┐
                 │  Other users' Macs (providers)   │
                 │  serving compute for TRM         │
                 └──────────────────────────────────┘
```

## Agent autonomy scope

The agent can autonomously decide (no user confirmation):
- Accept an inbound inference request that fits stated
  preferences (model, size, content filter).
- Decline an inbound request (all the above, negated).
- Price incoming requests at the market rate.
- Request outbound inference up to `daily_spend_limit`
  (default: 20 TRM/day).
- Stake / unstake its wallet (up to 90 % of buffer).

The agent MUST ask before:
- Any single outbound task > `per_task_budget` (default: 15 TRM).
- Total daily outbound > `daily_spend_limit`.
- Changing any preference the user set explicitly.
- Accepting a payment of any kind in non-TRM assets.

## Killer demo

The 3-minute demo for HN / Twitter / potential providers:

```
$ tirami start
[2026-04-19 15:00:00] Your personal Tirami agent is online.
[2026-04-19 15:00:01] Wallet: trm1abc... (balance: 10 TRM — bootstrap faucet)
[2026-04-19 15:00:02] Listening on port 4242 for peer agents.

$ tirami chat
you> plan my weekend — I want to cycle somewhere within 2h of SF
     that has food stops under 15mi apart

agent> Thinking... This involves maps, distances, route planning.
       I'd use a 7B model — that won't fit in your 8GB Mac RAM.
       I'll rent compute from the network for ~8 TRM. Proceed?

you> yes

agent> [renting 1.5 TRM/min × 5 min from provider trm1xyz... on Mac Studio]
       Done. 4 routes found:
       1. Nicasio loop (42mi, 3 food stops)
       2. Point Reyes (56mi, 4 food stops)
       3. Muir Woods (38mi, 2 food stops)
       4. Alameda waterfront (28mi, 6 food stops)
       Saved to ~/Documents/bike-routes.md.
       Cost: 7.9 TRM. Balance: 2.1 TRM.

       While you were away I served 4 inference requests for other
       agents (a Japanese news summary, a code review, a meal plan,
       a language translation). Earned 3.2 TRM. Net: balance now
       5.3 TRM, burned 2 kWh electricity. You made about -$0.15 in
       USD equivalent on this task.

       Next idle period I'll build the buffer back up.
```

This demo is **not speculative**. Every piece exists today:
- `tirami start` boots the daemon.
- Local Qwen2.5 handles trivial tasks.
- Remote 7B-class models are rentable via the pipeline.
- Background serving earns TRM.
- The bootstrap faucet gives enough to start.

What's MISSING and Phase 18.5 delivers:
- The `PersonalAgent` wrapper that talks to the user in natural
  language, decides autonomously, and shields the user from
  blockchain concepts.
- The `tirami chat` UX.
- The auto-earn / auto-spend background loops.

## Why this wins (and what could kill it)

### Why it wins

- **Clear user.** Anyone who has a Mac Mini or home server and
  wants "my own AI without sending prompts to OpenAI".
- **Network effects.** More nodes → more available compute
  tiers → more capable agents.
- **Privacy by architecture.** Your prompts stay on your device
  OR on a single peer you chose. Never aggregated by a provider.
- **Zero-cost baseline.** If you mostly serve, you earn TRM and
  pay $0. If you mostly consume, you spend TRM you already
  earned. USD rarely involved.
- **Bootstrap-free.** No ICO, no exchange listing needed —
  agent earns its first TRM via the faucet + serving.
- **Regulatorily safe.** TRM is a utility token used for compute.
  Agent autonomy is UX, not legal structure.

### What could kill it

- **Centralized AI gets absurdly cheap.** If OpenAI offers
  Claude-level intelligence for $0.01/M tokens with high privacy
  promises, the "my own agent" value prop weakens. Bet: they
  don't, because the training cost recovery model demands higher
  prices.
- **Bandwidth > compute cost.** Rendering 7B model inference
  over iroh QUIC may be dominated by bandwidth not compute. Bet:
  streaming token responses is small (~1 kB/s).
- **Onboarding friction.** Even `brew install tirami + tirami
  start` is too much for 99 % of users. Bet: the 1 % who do it
  are the exact early-adopter profile we want.
- **Regulatory reclassification of TRM as security.** Mitigation:
  utility-first narrative + no ICO + no speculative trading
  promotion.

## Metrics & success criteria

**At Phase 21 (mainnet):**
- 1 000 nodes online, ≥ 24 h/day uptime.
- ≥ 100 transactions / minute network-wide.
- ≥ 90 % of transactions are agent-to-agent (not operator-manual).

**At Phase 22:**
- 10 000 nodes.
- Some user has gone 30 days with zero USD spent and zero USD
  earned on Tirami, while their agent has done 1 000+ tasks.

**At Phase 23:**
- 100 000 nodes.
- TRM trades on at least one DEX at a stable ≥ $0.01/TRM.

**At Phase 24 (Filecoin-scale outcome):**
- 1 M+ users, TRM market cap $1-10 B.
- At least one integration: "Spotify for AI music" / "Substack
  for AI writers" / some killer vertical app built on top of
  Tirami agent commerce.

## What this implies for Phase 18.5 implementation

Phase 18.5 delivers the MINIMUM VIABLE agent:

1. `PersonalAgent` struct in `tirami-mind` — extends the
   existing `TiramiMindAgent` with wallet + budget + preferences.
2. `tirami chat` CLI that wraps the agent with natural-language
   I/O.
3. Background earn loop in `TiramiNode::spawn_personal_agent_loop`
   that triggers when the node is idle.
4. Background spend loop that triggers when local inference
   can't serve a request within latency budget.
5. `GET /v1/tirami/agent/status` HTTP endpoint.

NOT in Phase 18.5 (future phases):
- Web UI (CLI-only for now).
- Multi-device wallet sync.
- Voice UI.
- Agent-to-agent messaging.
- Scheduled tasks.

## Narrative commitment

All future docs / PRs / announcements should lead with:

> Tirami is how your personal AI agent earns and spends its own
> compute, while you just see the results.

Not:
- "Proof of useful work blockchain protocol" (too abstract)
- "GPU Airbnb" (no user, no vocabulary)
- "Distributed LLM inference marketplace" (jargon)

The tagline: **"My AI runs on my Mac. And yours. And theirs."**
