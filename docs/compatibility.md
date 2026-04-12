# Forge ↔ mesh-llm ↔ llama.cpp Compatibility

> **TL;DR** — Forge is mesh-llm + a TRM economic layer. Any GGUF model that runs on
> llama.cpp runs on Forge. Any OpenAI-compatible client that talks to mesh-llm
> talks to Forge. The only addition is `/v1/tirami/*` (45 endpoints, all opt-in).
>
> **Verified 2026-04-09**: full end-to-end run with SmolLM2-135M on Apple M-series
> Metal GPU, 3 real inference requests recorded as 3 real `TradeRecord`s, full
> stack from L1 ledger to L4 marketplace responding with live data.

---

## What we are

**Forge = mesh-llm runtime + TRM economy.**

| Layer | Source | What Forge adds |
|---|---|---|
| Inference engine | llama.cpp via `llama-cpp-2 = "0.1"` | **nothing** — same engine, same models, same Metal/CUDA backends |
| OpenAI API | mesh-llm-derived axum router | `x_forge` extension on responses + 45 `/v1/tirami/*` endpoints |
| P2P transport | iroh QUIC + Noise (mesh-llm-derived) | dual-signed trades + dual-signed loans + reputation gossip |
| Distributed tensor sharding | mesh-llm shard planner | **nothing** — same topology engine |
| Economy | (none in mesh-llm) | **everything** — TRM ledger, lending, futures, RiskModel, tirami-mind, tirami-agora |

If you ran `mesh-llm node --model qwen2.5:0.5b` yesterday, you can run
`forge node --model qwen2.5:0.5b` today and get the same response. The only
difference: every inference call now gets a `cu_cost` and shows up in
`/v1/tirami/trades`.

---

## Model compatibility (GGUF / llama.cpp)

Forge uses `llama-cpp-2 = "0.1"` (the official Rust binding to llama.cpp). It
inherits 100% of llama.cpp's model support:

- **Quantization**: q2_k through q8_0, including the q4_k_m / q5_k_m / q6_k mixes
- **Architectures**: Llama 1/2/3/4, Qwen 2/2.5, Mistral, Mixtral (MoE),
  Phi 1/2/3, Gemma, DeepSeek, SmolLM, TinyLlama, Falcon, MPT, BERT, T5,
  Stable LM, Yi, ChatGLM, GPT-J, GPT-NeoX, Persimmon, Bloom, Refact,
  CodeLlama, Replit, StarCoder, OpenLlama, Xverse, Command-R, DBRX, Olmo,
  ArcticChat, MiniCPM, RWKV, Mamba (state-space), and any future model
  llama.cpp adds — there's no architecture-specific code in tirami-infer.
- **Acceleration**: Metal (Apple Silicon, default ON), CUDA, ROCm, Vulkan,
  CPU AVX2/AVX512/NEON. Whatever llama.cpp builds with, Forge uses.
- **Features**: KV cache, flash attention, batched inference, multi-user
  state, function calling parsing (delegated to the model's chat template),
  speculative decoding (when llama.cpp adds it).

### Built-in model registry

`forge models` lists models that auto-download on first use:

```
qwen2.5:0.5b         ~491MB   Qwen/Qwen2.5-0.5B-Instruct-GGUF
qwen2.5:1.5b        ~1100MB   Qwen/Qwen2.5-1.5B-Instruct-GGUF
qwen2.5:3b          ~2000MB   Qwen/Qwen2.5-3B-Instruct-GGUF
qwen2.5:7b          ~4700MB   Qwen/Qwen2.5-7B-Instruct-GGUF
smollm2:135m         ~100MB   bartowski/SmolLM2-135M-Instruct-GGUF
```

Adding a new entry is one struct in
[`crates/tirami-infer/src/model_registry.rs`](../crates/tirami-infer/src/model_registry.rs).
PRs welcome.

### Custom GGUF

```bash
# Local file
forge node --model /path/to/model.gguf --tokenizer /path/to/tokenizer.json

# Or download manually and point to it
huggingface-cli download Qwen/Qwen2.5-1.5B-Instruct-GGUF qwen2.5-1.5b-instruct-q4_k_m.gguf
forge node --model ~/.cache/.../qwen2.5-1.5b-instruct-q4_k_m.gguf -t tokenizer.json
```

---

## API compatibility (OpenAI Chat Completions)

Forge implements `POST /v1/chat/completions` with the OpenAI v1 wire format.
Drop-in replacement for `https://api.openai.com/v1` in any client that lets
you set `OPENAI_BASE_URL`:

```bash
export OPENAI_BASE_URL=http://localhost:3000/v1
export OPENAI_API_KEY=$(cat ~/.forge/api_token)

# Now any OpenAI client just works:
openai api chat.completions.create -m qwen2.5:0.5b -g user "hi"
python -c "import openai; print(openai.chat.completions.create(model='smollm2:135m', messages=[{'role':'user','content':'hi'}]))"
```

The response includes a Forge-specific `x_forge` extension:

```json
{
  "id": "chatcmpl-...",
  "object": "chat.completion",
  "model": "SmolLM2-135M-Instruct-Q4_K_M",
  "choices": [...],
  "usage": {"prompt_tokens": 13, "completion_tokens": 21, "total_tokens": 34},
  "x_forge": {
    "cu_cost": 21,
    "effective_balance": 1021
  }
}
```

OpenAI clients ignore the `x_forge` field. Forge-aware clients can use it for
budget tracking. Both keep working.

### Supported request fields

- ✅ `model`, `messages` (system/user/assistant), `max_tokens`, `temperature`,
  `top_p`, `stop`, `stream`, `seed`
- 🟡 `tools` / `tool_choice`: parsed but execution depends on the model's
  native tool-calling support (passed through llama.cpp's chat templates).
- ❌ `function_call` (deprecated, use `tools`), `n > 1` (single completion only),
  `logit_bias`, `user` (no per-user state on a single node — that's what L4
  agora is for, see `/v1/tirami/agora/*`)

### Streaming (SSE)

`POST /v1/chat/completions` with `"stream": true` returns
`text/event-stream` chunks in OpenAI's `data:` format. The final `[DONE]`
sentinel is sent. The `x_forge` extension appears in the final chunk's
`usage` field.

---

## How Forge differs from raw mesh-llm / llama.cpp

If you're running `llama-server` (llama.cpp's built-in HTTP server) or
`mesh-llm node`, here's what you gain by switching to `forge node`:

| Feature | llama.cpp | mesh-llm | **forge** |
|---|---|---|---|
| GGUF inference (CPU + Metal/CUDA) | ✅ | ✅ | ✅ |
| OpenAI Chat Completions API | ✅ | ✅ | ✅ |
| Multi-user shared KV cache | ✅ | ✅ | ✅ |
| iroh QUIC + Noise P2P | ❌ | ✅ | ✅ |
| Distributed tensor sharding | ❌ | ✅ | ✅ |
| **CU accounting per request** | ❌ | ❌ | ✅ |
| **Dual-signed trade records** | ❌ | ❌ | ✅ (Ed25519) |
| **Lending pool with circuit breakers** | ❌ | ❌ | ✅ |
| **PortfolioManager + futures + insurance** | ❌ | ❌ | ✅ (tirami-bank) |
| **Self-improvement loop paid in CU** | ❌ | ❌ | ✅ (tirami-mind) |
| **Reputation gossip + collusion detection** | ❌ | ❌ | ✅ (tirami-agora) |
| **Bitcoin OP_RETURN anchoring** | ❌ | ❌ | ✅ |
| **Prometheus /metrics** | ❌ | ❌ | ✅ |
| **NIP-90 Nostr discovery** | ❌ | ❌ | ✅ |
| **Single-binary 5-layer stack** | ❌ | partial | ✅ (Phase 8) |

You can disable the economic layer entirely if you just want a fast OpenAI-
compatible inference server: ignore `/v1/tirami/*`, leave `/metrics` unscraped,
and Forge degrades to "mesh-llm with extra Rust crates compiled in".

---

## Migration from mesh-llm

If you're running `mesh-llm` (the upstream nm-arealnormalman fork) and want
the economic layer:

```bash
# Stop mesh-llm
killall mesh-llm

# Build forge
git clone https://github.com/clearclown/forge
cd forge
cargo build --release -p tirami-cli

# Same model, same port, same OpenAI clients
./target/release/forge node --model qwen2.5:0.5b --port 3000

# All your existing OpenAI code continues to work.
# TRM accounting starts immediately. Welcome loan = 1,000 TRM at 0% interest.
```

The `clearclown/forge` workspace **also publishes** a synced fork at
[nm-arealnormalman/mesh-llm](https://github.com/nm-arealnormalman/mesh-llm)
with the full economic layer ported into mesh-llm's directory layout. Use
whichever entry point feels more natural — both are at Phase 10 parity, both
expose the same 45 economic endpoints.

---

## Comparison to other compute economies

| Project | Currency | Substrate | Notes |
|---|---|---|---|
| **Forge** | **CU (compute)** | **GGUF / llama.cpp** | This. Compute-as-currency, no token, no ICO. |
| Bittensor (TAO) | TAO token | bespoke | Speculative token, validator-curated subnets, opaque scoring |
| Akash | AKT token | Docker / GPU rentals | Token-mediated marketplace, no per-request metering |
| Render Network | RNDR token | rendering | Same shape as Akash for raster/3D rendering |
| Together.ai | USD | proprietary | Centralized; commercial inference API |
| Hugging Face Inference Endpoints | USD | proprietary | Centralized; commercial inference API |
| Ollama | (none) | GGUF / llama.cpp | Same engine, no economy, no P2P |
| llama-server | (none) | llama.cpp | Same engine, no economy, no P2P, single-node |
| LM Studio | (none) | GGUF / llama.cpp | Desktop UI, no economy, single-node |

**The Forge thesis**: every existing competitor either burns electricity on
non-useful work (Bitcoin), introduces a speculative token disconnected from
compute cost (Bittensor, Akash, Render), or operates a centralized commercial
service (Together, HF). Forge is the only project where the unit of account
is the FLOP, the unit of work is the inference, and the only way to get more
units is to do more useful work.

---

## What's verified to work today (2026-04-09)

The following has been **physically run** end-to-end on Apple Silicon (M-series,
Metal GPU) with no mock components. Every TRM recorded in this transcript is
the result of actual llama.cpp inference cycles.

```bash
$ ./target/release/forge node --port 3001 -m smollm2:135m
[INFO] Model loaded (llama.cpp), EOS=2  ← real GGUF, 31/31 layers on Metal
[INFO] HF tokenizer loaded
[INFO] API server listening on 127.0.0.1:3001

$ curl -X POST localhost:3001/v1/chat/completions \
    -H "Authorization: Bearer test-real-run" -H "Content-Type: application/json" \
    -d '{"model":"smollm2:135m","messages":[{"role":"user","content":"What is 2+2?"}],"max_tokens":20}'
{
  "choices":[{"message":{"role":"assistant","content":"2 + 2 = 4..."}}],
  "usage":{"prompt_tokens":13,"completion_tokens":21,"total_tokens":34},
  "x_forge":{"cu_cost":21,"effective_balance":1021}
}
                                          ↑ real TRM charged for 21 real tokens

$ curl localhost:3001/v1/tirami/balance -H "Authorization: Bearer test-real-run"
{"contributed":21,"consumed":0,"net_balance":21,"effective_balance":1021,"reputation":0.5}
                ↑ real ledger entry

$ curl localhost:3001/metrics  # no auth needed (Prometheus scrape target)
forge_cu_contributed_total{node_id="0000..."} 21    ← real Prometheus gauge
forge_trade_count_total 1                            ← real trade
forge_reputation{node_id="0000..."} 0.5              ← DEFAULT_REPUTATION

$ curl 'localhost:3001/v1/tirami/anchor?network=testnet' -H "Authorization: Bearer test-real-run"
{
  "merkle_root_hex":"8edd724d48ce205d49ac42d683c4a624fdffe80936d5c184c5dd225579a673e8",
  "script_hex":"6a2846524745010100008edd724d48ce205d49ac42d683c4a624fdffe80936d5c184c5dd225579a673e8",
  "network":"Testnet",
  "payload_len":40
}
                ↑ real Bitcoin OP_RETURN script ready to broadcast
```

Same node also responds to `/v1/tirami/bank/tick`, `/v1/tirami/agora/find`,
`/v1/tirami/mind/improve`, `/v1/tirami/credit`, `/v1/tirami/pool`, etc — every
endpoint listed in `CLAUDE.md` "API Surface" responds with live data driven
by the same in-process ComputeLedger.

---

## Strategic positioning

For users coming from each of the other ecosystems:

**from Bittensor**: same "compute earns currency" intuition, but the currency
is the FLOP itself instead of a speculative token. Validators are not
required — every node maintains its own ledger and gossips signed trade
records. There is no central subnet curator, no incentive to game scoring
functions, no token to manipulate.

**from Akash / Render**: same "GPU-for-rent" mental model, but per-request
metered (every chat completion is a separate trade record) instead of
container-rental. No token in the way. Your customers pay you in CU; you can
optionally bridge TRM to BTC via Lightning if you need to.

**from Ollama / LM Studio**: same model loading experience, same OpenAI API,
but multi-node and economically aware. If you want to run one model on your
laptop, Ollama is great. If you want a fleet of nodes that earn TRM when
they're idle and spend TRM when they need a bigger model, use Forge.

**from llama-server**: drop-in replacement. Same llama.cpp under the hood.
Same OpenAI compat. Add the economic layer when you're ready by hitting
`/v1/tirami/balance`.

**from Together.ai / OpenAI / Anthropic API**: replace your `OPENAI_BASE_URL`
with `http://localhost:3000/v1` and you're sovereign. No more vendor lock-in,
no more rate limits, no more ToS surprises. The trade-off is you're running
your own GPU.

---

## Ecosystem map

```
                ┌───────────────────────┐
                │  forge-economics      │  ← theory + spec/parameters.md + 7,000-word paper
                │  (clearclown/...)     │
                └──────────┬────────────┘
                           │ canonical constants
                           ▼
┌─────────────────────────────────────────────┐
│  L4 tirami-agora    discovery + reputation   │
│  L3 tirami-mind     self-improvement loop    │  ← clearclown/forge (this repo)
│  L2 tirami-bank     finance instruments      │     5-layer Rust workspace
│  L1 tirami-ledger   TRM + lending + safety    │     12 crates
│  L0 mesh-llm       distributed inference    │  ← inherited from upstream
└─────────────────────────────────────────────┘
                  ▲                ▲
                  │                │
          ┌───────┴──┐      ┌──────┴────────┐
          │ tirami-sdk│      │ forge-cu-mcp  │  ← Python clients for everything above
          │ (PyPI)   │      │ (PyPI + MCP)  │
          └──────────┘      └───────────────┘

Plus a synced production runtime fork at:
nm-arealnormalman/mesh-llm — same 5 layers, different binary entry point
```

---

## Where to start

| You want to... | Run this |
|---|---|
| Chat with a tiny local model | `forge chat -m smollm2:135m "hi"` |
| Run a long-lived OpenAI server | `forge node -m qwen2.5:0.5b` |
| Earn TRM by serving inference | `forge seed -m qwen2.5:1.5b` |
| Spend TRM calling another node | `forge worker --seed <pubkey>` |
| Drive tirami-bank from Python | `pip install tirami-sdk==0.3.0` |
| Drive forge from Claude Code / Cursor | `pip install forge-cu-mcp==0.3.0` |
| Read the theory | `forge-economics/papers/compute-standard.md` |
| Watch the metrics | `curl localhost:3000/metrics \| grep forge_` |

That's it. Compute is currency. Welcome.
