# Tirami — Migration Guide

- [From llama-server (llama.cpp)](#from-llama-server-llamacpp)
- [From ollama serve](#from-ollama-serve)
- [From mesh-llm (upstream fork)](#from-mesh-llm-upstream-fork)
- [From Bittensor](#from-bittensor)
- [From Akash / Render Network](#from-akash--render-network)
- [From Together.ai / OpenAI / Anthropic / Groq](#from-togetherai--openai--anthropic--groq)
- [Rollback plan](#rollback-plan)

---

## From llama-server (llama.cpp)

Tirami is a direct replacement for `llama-server`. Same GGUF model files, same quantization formats (q2_k through q8_0), same Metal/CUDA/ROCm/Vulkan backends. Tirami uses `llama-cpp-2 = "0.1"` — the official Rust binding to llama.cpp — so it inherits 100% of llama.cpp's model and acceleration support.

**Drop-in replacement**:

```bash
# Before (llama-server):
llama-server --model qwen2.5-7b-instruct-q4_k_m.gguf --port 8080

# After (tirami):
tirami node --model /path/to/qwen2.5-7b-instruct-q4_k_m.gguf --port 3000
```

Then point your client at the new base URL:

```bash
export OPENAI_BASE_URL=http://localhost:3000/v1
# Existing OpenAI-compatible code works unchanged.
```

The Tirami-specific `/v1/tirami/*` endpoints are opt-in. If you never call them, Tirami runs as a plain OpenAI-compatible inference server. The only visible difference in existing clients is the `x_forge` field appended to each response — OpenAI SDK clients silently ignore unknown fields.

**Model shortnames**: Tirami's built-in registry auto-downloads GGUFs from HuggingFace on first use. `tirami node -m qwen2.5:0.5b` fetches the correct GGUF file automatically. Existing local files are used when you pass an absolute path.

---

## From ollama serve

Ollama and Tirami use the same underlying inference engine (llama.cpp) and serve the same OpenAI-compatible API. The main differences are model management and the presence of an economic layer.

**Model management**: Ollama uses a custom blob store at `~/.ollama/models/`. Tirami uses a model registry (`tirami models` to list) that stores GGUFs in the standard HuggingFace cache. If you have Ollama models already downloaded, set `FORGE_MODELS_DIR` to point at their location, or pass the absolute GGUF path:

```bash
# Use an existing Ollama model file (find it with: ollama show --modelfile qwen2.5)
tirami node --model ~/.ollama/models/blobs/<sha256-blob> --tokenizer /path/to/tokenizer.json

# Or re-download via Tirami's registry:
tirami node --model qwen2.5:0.5b   # fetches from HuggingFace
```

**API**: both implement `POST /v1/chat/completions`. Ollama also has a native `/api/generate` endpoint; Tirami does not. If your code targets `/api/generate`, update it to use `/v1/chat/completions` before switching.

**Economy**: Ollama has no economic layer. On Tirami, every inference call records a trade and charges TRM. New nodes start with a 1,000 TRM welcome loan (0% interest, 72-hour term per parameters.md §3), so you begin with credit.

---

## From mesh-llm (upstream fork)

Tirami is a strict superset of mesh-llm. The L0 inference layer (iroh QUIC, Noise encryption, pipeline parallelism, MoE sharding, Nostr peer discovery) is inherited directly. Tirami adds L1–L4 (TRM economy, tirami-bank, tirami-mind, tirami-agora) on top without removing anything from L0.

**If you're running `mesh-llm node`**:

```bash
# Stop mesh-llm
killall mesh-llm

# Build tirami (one-time, ~3 min cold)
git clone https://github.com/clearclown/tirami && cd tirami
cargo build --release -p tirami-cli

# Same model, same port, same OpenAI clients
./target/release/tirami node --model qwen2.5:0.5b --port 3000

# TRM accounting starts on the first inference call.
# Welcome loan = 1,000 TRM at 0% interest (parameters.md §3).
```

The `nm-arealnormalman/mesh-llm` fork is kept at Phase 10 parity with `clearclown/tirami` — both expose the same 45 economic endpoints under `/v1/tirami/*` and `/api/forge/*` respectively. Use whichever directory layout feels more natural. For new deployments, `clearclown/tirami` is the recommended entry point.

---

## From Bittensor

The intuition is similar — "compute earns currency" — but the mechanics are fundamentally different.

| | Bittensor | Tirami |
|---|---|---|
| Currency | TAO (ERC-20-style token) | TRM (unit of account, not a token) |
| Exchange listing | Yes (speculative trading) | No |
| Validator requirement | Yes (validators score miners) | No (bilateral dual-sign, no validator) |
| Registration fee | Yes (TAO to register on subnet) | No (welcome loan covers first 1,000 TRM free) |
| Scoring | Opaque validator scoring per subnet | Dual-signed trade records, local ledger |
| Inflation | Built-in emission schedule | Bounded by physical compute capacity |

**Migration**: Bittensor miners run Python scripts that respond to validator challenges. Tirami nodes run a single Rust binary that responds to OpenAI-compatible inference requests. There is no equivalent to "subnets", "validators", or "bonds".

If you were running a Bittensor miner to earn TAO, the Tirami equivalent is `tirami seed -m <model>`. Start the binary, and you begin earning TRM on the first request served. No wallet registration, no TAO stake, no subnet approval.

---

## From Akash / Render Network

Akash and Render are container/rendering rental marketplaces. Tirami is per-request metered for LLM inference specifically. They solve different problems.

**If you need generic GPU container rentals**: Tirami is not the right tool. Stay on Akash.

**If you need LLM inference specifically**: Tirami is cheaper-per-request because it has no token intermediary. Akash charges AKT for container-hours; Tirami charges TRM per inference token, and TRM cannot be speculated on.

The cost comparison from parameters.md §8–§9: at equilibrium, 1 TRM ≈ $0.00375 (derived from Claude API pricing of $15/1M tokens vs Tirami's ~4,000 TRM/1M tokens for 70B-class models). A Mac Mini M4 hardware operator running Tirami charges at approximately $0.000132/TRM ceiling (parameters.md §9 `cu_price_ceiling_usd`), which is well below cloud API prices — the economic pressure that keeps Tirami inference cheaper.

**Migration**: there is no direct migration path — different use cases. If you have existing Akash deployments for LLM inference, you can run `tirami node` on the same hardware and decommission the Akash deployment.

---

## From Together.ai / OpenAI / Anthropic / Groq

These are centralized commercial APIs. Tirami replaces them at the client level with a single environment variable:

```bash
export OPENAI_BASE_URL=http://localhost:3000/v1
export OPENAI_API_KEY="your-forge-api-token"   # set with --api-token at node startup
```

Your existing code keeps working. All standard `POST /v1/chat/completions` fields are supported. The `x_forge` extension in responses is ignored by OpenAI-compatible clients.

**Trade-offs**: you are now responsible for the hardware. In exchange:
- No rate limits (your hardware is the only limit).
- No ToS surprises.
- No vendor lock-in.
- No per-token billing to a third party.
- Tirami model costs: 1–20 TRM/token depending on model tier (parameters.md §2), vs OpenAI's implied ~$0.00375/TRM equivalent on GPT-4-class output.

A Mac Mini M4 at ~$600 hardware cost amortized over 3 years produces approximately 5,000,000 TRM/year (parameters.md §9 `mac_mini_annual_cu_capacity`). At the physical ceiling price of $0.000132/TRM, that's $660/year in inference value from $600 hardware — a profitable substitution for moderate use.

**Streaming**: `"stream": true` is supported with real token-by-token SSE output. The `x_forge` extension appears in the final chunk's usage field.

---

## Rollback plan

Tirami is additive. Nothing it installs modifies your existing binaries, model files, or application code.

To stop using Tirami:

```bash
# Uninstall the CLI
cargo uninstall tirami-cli
# Or just remove the binary
rm ./target/release/tirami

# Remove state files (optional — keep for audit history)
rm tirami-ledger.json
rm bank_state.json marketplace_state.json mind_state.json

# Restore original base URL in your application
export OPENAI_BASE_URL=https://api.openai.com/v1
```

The `tirami-ledger.json` file is a JSON record of your TRM trade history. It is human-readable and worth keeping as an audit log even after decommissioning the node.

There is no lock-in at the protocol level. GGUF model files are shared with llama.cpp and Ollama and can be used directly by either tool. Tokenizer files are standard HuggingFace format.

---

See also: [docs/compatibility.md](compatibility.md) for the full feature matrix, [docs/operator-guide.md](operator-guide.md) for deployment configuration, [docs/agent-integration.md](agent-integration.md) for SDK usage.
