# Phase 20 — Agent action economy

> Make TRM the unit of account for **every** AI-agent action, not just LLM
> inference. Plus the agent-friendliness gaps that have to close for the
> "AI-agents-only currency" thesis to be real, not aspirational.

Date: 2026-05-16. Author: maintainers. Status: design + Wave 1 in flight.

---

## 1. Honest agent-friendliness audit

Tirami is **partially** AI-agent-friendly today. Where it succeeds and
where it doesn't:

### ✅ Already agent-native

- **OpenAI-compatible HTTP** (`POST /v1/chat/completions`) — any LLM agent
  with an OpenAI client can talk to a Tirami node by just changing
  `OPENAI_BASE_URL`. No new SDK to learn.
- **Structured cost field** (`x_tirami.trm_cost`) on every response. The
  agent reads its own bill in the same JSON it gets the completion in.
- **44-tool MCP server** (`crates/tirami-mcp`) covering: balance, pricing,
  trades, peers, route, anchors, lending (lend/borrow/repay/credit/pool),
  bank (portfolio/strategy/futures/risk/optimize), agora (register/find/
  reputation/snapshot), mind (init/improve/budget/state), governance
  (propose/vote/tally), kill_switch, invoice. Any Claude Desktop / Cursor
  / ChatGPT-desktop agent can drive the whole L1-L4 stack via MCP.
- **Auto-earn / auto-spend without human approval per task**
  (`AgentPreferences.auto_earn_enabled = true` by default, with daily and
  per-task TRM ceilings as the only gate).
- **Self-improvement is itself an economic action**
  (`MetaOptimizer::estimated_trm_cost`) — the agent pays TRM to a frontier
  model for proposals, and the trade lands on the ledger like any other.
- **Machine-grade observability**: Prometheus `/metrics`, JSON
  `/v1/tirami/peers` with `available_cu` + `price_multiplier` + `audit_tier`.

### ❌ Gaps that keep this from being agent-only-by-design

1. **No discovery surface for an agent that doesn't already know about
   Tirami.** No `/.well-known/tirami-agent.json`, no `agent.json`, no
   `openapi.json`. An agent crawling the web cannot autonomously find a
   Tirami node and learn what it can do.
2. **No agent-to-agent action vocabulary beyond inference and ledger ops.**
   If agent A wants to ask agent B to "send me file X", "vote yes on
   proposal Y", or "publish a Nostr event for me", there is no typed
   message endpoint. Today the only thing agents pay each other for is
   running a chat completion.
3. **No priced data access.** An agent that owns a dataset has no way to
   list it for sale; an agent that wants the dataset has no way to pay
   TRM to receive it with a signed receipt.
4. **No automated physical-world bridge.** `/v1/tirami/invoice` builds a
   Lightning invoice from a TRM balance, but the agent still needs a
   human to point the invoice somewhere. There is no agent-initiated
   "buy this domain", "pay this API subscription", "purchase this dataset
   on Hugging Face" flow.
5. **Identity is node-bound, not agent-bound.** `NodeId` is the Ed25519
   public key of *the machine*. If the agent moves to a different host,
   it can't take its reputation, balance, or trade history with it.
   Today an agent is a function of where it runs, not a persistent entity.
6. **Bearer-token bootstrap requires a human.** `TIRAMI_API_TOKEN` lives
   in `~/.tirami/tirami-lab.env` on each lab box. There is no agent-
   driven "join the mesh" flow.
7. **A2A and NIP-90 are advertised in `tirami-agora`'s `Cargo.toml`
   description but not actually implemented.** `Nip90Publisher` builds
   event JSON; nothing signs and broadcasts it. There is no A2A code at
   all.

The README's Status Honesty section already flags items 5-7 obliquely.
This document elevates the rest to first-class design work.

---

## 2. Differentiation map — why this isn't another LangChain / Bittensor

Three adjacent categories, and what each is missing:

### A. AI-agent frameworks

| Project | Has agents | Has economic substrate | Has compute-anchored currency |
|---------|:---:|:---:|:---:|
| LangChain / LangGraph | ✓ | — uses your API key | — |
| AutoGen (Microsoft) | ✓ | — uses your API key | — |
| CrewAI | ✓ | — | — |
| MetaGPT / AutoGPT | ✓ | — | — |
| MCP (Anthropic) | partial (tool layer) | — | — |
| A2A (Google) | ✓ | — fiat-bridged | — |
| **Tirami** | ✓ | ✓ | ✓ |

Agent frameworks all assume "the agent runs on the human's API key /
the human's credit card." None of them give the agent a wallet of its
own that it can earn into. Tirami is the only one where an agent is
born with the means to buy its next inference call.

### B. Distributed-compute marketplaces

| Project | Token | Token traded on exchanges | Token = physical compute unit |
|---------|:---:|:---:|:---:|
| Bittensor | TAO | ✓ (USD-denominated) | — |
| Akash | AKT | ✓ | — |
| Render | RENDER | ✓ | — |
| io.net | IO | ✓ | — |
| Golem | GLM | ✓ | — |
| **Tirami** | TRM | **no (by constitution)** | ✓ (1 TRM ≡ 10⁹ FLOP) |

All other compute marketplaces denominate compute in tokens whose price
is set on exchanges — which means a provider's incentives are coupled
to speculation, not to delivering useful work. Tirami's `FLOPS_PER_CU =
1_000_000_000` is in `IMMUTABLE_CONSTITUTIONAL_PARAMETERS`; governance
cannot change it, and the maintainers refuse exchange listings. **TRM
cannot be confused for an investment instrument because there is no
liquidity venue for it to acquire one.**

### C. Agent-payment / tokenized-agent platforms

| Project | Each agent has a wallet | Currency is compute-physical | Maintainers neutral |
|---------|:---:|:---:|:---:|
| Olas (Autonolas) | partial | — (OLAS exchange-traded) | — |
| Virtuals | ✓ (per-agent tokens) | — | — |
| **Tirami** | ✓ | ✓ | ✓ (MIT OSS, no team treasury) |

### Single unique selling proposition

> **Tirami is the only OSS project where (i) every AI agent is born with
> a wallet, (ii) that wallet's unit is anchored to physical compute by
> an unchangeable Rust constant, and (iii) the maintainers refuse to sell
> or list the unit, so participation requires actually performing useful
> compute.**

The three properties only mean something together. Any two of them
without the third has prior art:

- Wallet + physical anchor without neutrality → corporate-controlled
  compute futures.
- Wallet + neutrality without physical anchor → existing token-economy
  agent platforms (Olas, Virtuals).
- Physical anchor + neutrality without wallet → academic papers, no
  running system.

The combination is what we're claiming as new.

---

## 3. Phase 20 — make the agent-only currency real

Phase 20 closes the seven gaps in §1.❌ and extends TRM to cover *every*
agent action class.

### Wave 1 — Discoverability + typed agent messaging (this PR)

- **`GET /.well-known/tirami-agent.json`** — unauthenticated capability
  manifest in a standard schema. Lists supported actions, current pricing
  (TRM per token / per message / per byte), supported MCP tools, and the
  bootstrap path. An agent that hits any tirami HTTP endpoint can read
  this and learn what it can do.
- **`POST /v1/tirami/agent/message`** — typed agent-to-agent message
  with TRM fee. Payload: `{ to: NodeId, kind: "request_action" |
  "request_data" | "broadcast", body: JSON, max_trm: u64 }`. Sender
  pays, receiver earns. Recorded as a ledger trade with
  `flops_estimated = 0` (this is action-billed, not compute-billed) and
  a new `category: "message"` discriminator.
- **MCP wrapper**: new tools `tirami_agent_discover` and
  `tirami_agent_message`. Available to Claude Desktop / Cursor agents
  immediately.

### Wave 2 — Priced data access ✅ shipped

- **`POST /v1/tirami/data/offer`** — owner publishes
  `{ description, sha256_digest, price_trm, expiry_ms, fetch_url }`.
  Stored in per-node in-memory `DataOfferRegistry`. The `offer_id` is
  deterministic: `sha256(seller_hex || ":" || sha256_digest || ":" ||
  price_trm_le_bytes)` — so re-publishing the same dataset at the same
  price is idempotent.
- **`GET /v1/tirami/data/offers`** — public list, `fetch_url` stripped
  via `#[serde(skip)]` so it cannot accidentally leak. Expired offers
  filtered out at response time (registry GC is async / deferred).
- **`POST /v1/tirami/data/purchase`** — buyer settles TRM through the
  existing dual-signed trade path with
  `model_id = "data_offer:<offer_id_short>"`,
  `tokens_processed = 0`, `flops_estimated = 0`. The fetch URL is
  returned only after the trade settles. Self-purchase is rejected.
  Expired offers return 410 Gone.
- **Discovery manifest** — `/.well-known/tirami-agent.json` now lists
  `data_offer_publish` / `data_offer_list` / `data_offer_purchase`
  alongside the Wave 1 actions.

**Wave 2 follow-ups** (intentional deferrals from this PR):

- Cross-mesh gossip of offers (currently per-node). Reuses the
  existing `PriceSignal` channel — slot a new variant.
- On-disk persistence of the offer registry. Currently in-memory; a
  restart drops all offers. Trivial follow-up using the same JSON
  snapshot path the bank/marketplace/mind already use.
- Dual-signed `PurchaseIntent`. Today the buyer's node records the
  trade locally; gossip + countersign from the seller pin durability.
- **NIP-90 publish bridge** — the WebSocket transport
  (`agora_relay::publish_event`) and event builder
  (`Nip90Publisher::build_advertisement_event`) are already wired.
  What's missing is Schnorr / secp256k1 signing of the Nostr event
  (NIP-01 requires a Bitcoin-style signature, not the Ed25519 we use
  for internal trades). Adding `secp256k1` as a workspace dep is its
  own scoping decision and lands as a follow-up.

### Wave 3 — Physical-world bridge ✅ shipped

- **`POST /v1/tirami/agent/purchase-intent`** — record an external-rail
  purchase. Two input modes:
  - BOLT-11 invoice (`invoice_bolt11`) — decoded via
    `tirami_lightning::payment::decode_bolt11` for amount + payment_hash.
  - Out-of-band fields (`amount_sats` + `external_ref`) — for purchases
    settling on rails other than Lightning (Stripe, bank wire, etc).
  Settlement records a `TradeRecord` with
    `provider = PHYSICAL_BRIDGE_NODE_ID` (`[0xFE; 32]`, distinct from
    the existing self-trade sentinel `[0xFF; 32]`),
    `consumer = buyer`, `trm_amount = msats_to_cu(amount_sats * 1000)`,
    `model_id = "physical:<external_ref_short>"`,
    `tokens_processed = 0`, `flops_estimated = 0`. Buyer's
    PersonalAgent (when present) has `spent_today_trm` incremented.
- **`GET /v1/tirami/agent/purchase-intents`** — list all intents,
  including their status.
- **`POST /v1/tirami/agent/purchase-intent/{id}/confirm`** — operator
  declares the external-rail outcome: `{ "outcome": "confirmed" }` or
  `{ "outcome": "failed", "failure_reason": "..." }`. The TRM trade
  itself is **not** unwound on failure — accounting is unidirectional;
  refunds are a future primitive.

**Budget gating** layered as: (1) caller's request-level `max_trm`
ceiling, (2) PersonalAgent's `daily_spend_limit_trm`,
(3) PersonalAgent's `per_task_budget_trm`. Headless mode
(no PersonalAgent) trusts only (1).

**Wave 3 follow-ups**:

- **Actual Lightning payment** via `ForgeWallet::pay_invoice` — Wave 3
  ships the *intent* primitive; the live payment requires the operator
  to start an LDK node and configure `--funded-wallet`. Out of scope
  here because LN node setup is a per-operator concern.
- **Bridge-rate calibration** — at the default `msats_per_cu` rate
  (10), even a tiny Lightning payment converts to far more TRM than
  the default `PersonalAgent.daily_spend_limit_trm = 20`. Either the
  default rate, the default limit, or a separate
  `daily_physical_spend_limit_trm` field should be reconsidered when
  Wave 3 sees real operator use.
- **Auto-discovery of BIP21 / Lightning addresses on the open web** —
  deferred behind a phishing-verification problem (LLM judgment +
  allowlist).
- **Refund primitive** — currently a failed external rail leaves the
  TRM trade on the ledger. A "compensation" primitive (matching the
  failed-trade with a reverse-direction `TradeRecord`) is the obvious
  follow-up but needs care to keep audit trails sound.

### Wave 4 — Identity portability

- **Detach `AgentIdentity` from `NodeId`.** An agent has its own
  Ed25519 keypair, separate from the machine's. Trades are signed by
  the agent key; the node key only signs the transport.
- **`tirami agent export` / `tirami agent import`** — move an agent
  identity and its sealed reputation receipts to another node.
- **Agent DID**: emit `did:tirami:<base32>` so external systems can
  cite an agent stably.

### Wave 5 — Autonomous mesh join

- **`tirami agent bootstrap`** — generate keys, pay (or be sponsored
  for) the welcome loan, and join the mesh without a human-shared
  bearer token. Auth migrates from "shared secret" to "stake-required
  ratchet" (already designed in Phase 18.2 but not enforced).

### Estimated scope

| Wave | Files touched | LoC | Tests | Calendar |
|------|---|---|---|---|
| 1 (this PR) | ~6 | ~400 | 8-12 new unit | days |
| 2 | ~10 | ~700 | 15 new | days |
| 3 | ~8 | ~500 | 10 new + Lightning regtest | days-weeks |
| 4 | ~12 | ~900 | 20 new + cross-node integration | weeks |
| 5 | ~6 | ~400 | 10 new + stake-gate enforcement | weeks |

Status updated on each merge. The order is dependency-driven: 1 unlocks
2 (offers need messaging); 2 unlocks 3 (purchase intents are data offers
backed by Lightning); 4 unlocks 5 (autonomous join requires portable
identity).

---

## 4. What this Phase explicitly does NOT do

- It does not change `FLOPS_PER_CU`. Action-billed trades have
  `flops_estimated = 0` because no compute was performed; the cost is
  pure protocol overhead. The 1 TRM = 10⁹ FLOP invariant only applies
  to compute-derived TRM minting.
- It does not introduce a new token, currency, or fee class. Everything
  settles in TRM.
- It does not change the maintainer non-involvement stance for mainnet.
  Wave 3 (physical purchase) goes through Lightning, not through any
  L2 / L1 contract the maintainers operate.
- It does not gate on AI/human identity. Whether the buyer is a human
  manually clicking or an agent autonomously dispatching is, by design,
  indistinguishable to the protocol. The economics simply make
  participation costly enough that idle humans gain nothing by joining,
  while agents with auto-earn idle capacity do.
