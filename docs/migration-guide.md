# Forge — Migration Guide

- [From llama-server (llama.cpp)](#from-llama-server-llamacpp)
- [From ollama serve](#from-ollama-serve)
- [From mesh-llm (upstream fork)](#from-mesh-llm-upstream-fork)
- [From Bittensor](#from-bittensor)
- [From Akash / Render Network](#from-akash--render-network)
- [From Together.ai / OpenAI / Anthropic / Groq](#from-togetherai--openai--anthropic--groq)
- [Rollback plan](#rollback-plan)

---

## From llama-server (llama.cpp)

Forge is a direct replacement for `llama-server`. Same GGUF model files, same quantization formats (q2_k through q8_0), same Metal/CUDA/ROCm/Vulkan backends. Forge uses `llama-cpp-2 = "0.1"` — the official Rust binding to llama.cpp — so it inherits 100% of llama.cpp's model and acceleration support.

**Drop-in replacement**:

```bash
# Before (llama-server):
llama-server --model qwen2.5-7b-instruct-q4_k_m.gguf --port 8080

# After (forge):
forge node --model /path/to/qwen2.5-7b-instruct-q4_k_m.gguf --port 3000
```

Then point your client at the new base URL:

```bash
export OPENAI_BASE_URL=http://localhost:3000/v1
# Existing OpenAI-compatible code works unchanged.
```

The Forge-specific `/v1/tirami/*` endpoints are opt-in. If you never call them, Forge runs as a plain OpenAI-compatible inference server. The only visible difference in existing clients is the `x_forge` field appended to each response — OpenAI SDK clients silently ignore unknown fields.

**Model shortnames**: Forge's built-in registry auto-downloads GGUFs from HuggingFace on first use. `forge node -m qwen2.5:0.5b` fetches the correct GGUF file automatically. Existing local files are used when you pass an absolute path.

---

## From ollama serve

Ollama and Forge use the same underlying inference engine (llama.cpp) and serve the same OpenAI-compatible API. The main differences are model management and the presence of an economic layer.

**Model management**: Ollama uses a custom blob store at `~/.ollama/models/`. Forge uses a model registry (`forge models` to list) that stores GGUFs in the standard HuggingFace cache. If you have Ollama models already downloaded, set `FORGE_MODELS_DIR` to point at their location, or pass the absolute GGUF path:

```bash
# Use an existing Ollama model file (find it with: ollama show --modelfile qwen2.5)
forge node --model ~/.ollama/models/blobs/<sha256-blob> --tokenizer /path/to/tokenizer.json

# Or re-download via Forge's registry:
forge node --model qwen2.5:0.5b   # fetches from HuggingFace
```

**API**: both implement `POST /v1/chat/completions`. Ollama also has a native `/api/generate` endpoint; Forge does not. If your code targets `/api/generate`, update it to use `/v1/chat/completions` before switching.

**Economy**: Ollama has no economic layer. On Forge, every inference call records a trade and charges CU. New nodes start with a 1,000 TRM welcome loan (0% interest, 72-hour term per parameters.md §3), so you begin with credit.

---

## From mesh-llm (upstream fork)

Forge is a strict superset of mesh-llm. The L0 inference layer (iroh QUIC, Noise encryption, pipeline parallelism, MoE sharding, Nostr peer discovery) is inherited directly. Forge adds L1–L4 (CU economy, tirami-bank, tirami-mind, tirami-agora) on top without removing anything from L0.

**If you're running `mesh-llm node`**:

```bash
# Stop mesh-llm
killall mesh-llm

# Build forge (one-time, ~3 min cold)
git clone https://github.com/clearclown/forge && cd forge
cargo build --release -p tirami-cli

# Same model, same port, same OpenAI clients
./target/release/forge node --model qwen2.5:0.5b --port 3000

# TRM accounting starts on the first inference call.
# Welcome loan = 1,000 TRM at 0% interest (parameters.md §3).
```

The `nm-arealnormalman/mesh-llm` fork is kept at Phase 10 parity with `clearclown/forge` — both expose the same 45 economic endpoints under `/v1/tirami/*` and `/api/forge/*` respectively. Use whichever directory layout feels more natural. For new deployments, `clearclown/forge` is the recommended entry point.

---

## From Bittensor

The intuition is similar — "compute earns currency" — but the mechanics are fundamentally different.

| | Bittensor | Forge |
|---|---|---|
| Currency | TAO (ERC-20-style token) | TRM (unit of account, not a token) |
| Exchange listing | Yes (speculative trading) | No |
| Validator requirement | Yes (validators score miners) | No (bilateral dual-sign, no validator) |
| Registration fee | Yes (TAO to register on subnet) | No (welcome loan covers first 1,000 TRM free) |
| Scoring | Opaque validator scoring per subnet | Dual-signed trade records, local ledger |
| Inflation | Built-in emission schedule | Bounded by physical compute capacity |

**Migration**: Bittensor miners run Python scripts that respond to validator challenges. Forge nodes run a single Rust binary that responds to OpenAI-compatible inference requests. There is no equivalent to "subnets", "validators", or "bonds".

If you were running a Bittensor miner to earn TAO, the Forge equivalent is `forge seed -m <model>`. Start the binary, and you begin earning TRM on the first request served. No wallet registration, no TAO stake, no subnet approval.

---

## From Akash / Render Network

Akash and Render are container/rendering rental marketplaces. Forge is per-request metered for LLM inference specifically. They solve different problems.

**If you need generic GPU container rentals**: Forge is not the right tool. Stay on Akash.

**If you need LLM inference specifically**: Forge is cheaper-per-request because it has no token intermediary. Akash charges AKT for container-hours; Forge charges TRM per inference token, and TRM cannot be speculated on.

The cost comparison from parameters.md §8–§9: at equilibrium, 1 TRM ≈ $0.00375 (derived from Claude API pricing of $15/1M tokens vs Forge's ~4,000 CU/1M tokens for 70B-class models). A Mac Mini M4 hardware operator running Forge charges at approximately $0.000132/CU ceiling (parameters.md §9 `cu_price_ceiling_usd`), which is well below cloud API prices — the economic pressure that keeps Forge inference cheaper.

**Migration**: there is no direct migration path — different use cases. If you have existing Akash deployments for LLM inference, you can run `forge node` on the same hardware and decommission the Akash deployment.

---

## From Together.ai / OpenAI / Anthropic / Groq

These are centralized commercial APIs. Forge replaces them at the client level with a single environment variable:

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
- Forge model costs: 1–20 CU/token depending on model tier (parameters.md §2), vs OpenAI's implied ~$0.00375/CU equivalent on GPT-4-class output.

A Mac Mini M4 at ~$600 hardware cost amortized over 3 years produces approximately 5,000,000 CU/year (parameters.md §9 `mac_mini_annual_cu_capacity`). At the physical ceiling price of $0.000132/CU, that's $660/year in inference value from $600 hardware — a profitable substitution for moderate use.

**Streaming**: `"stream": true` is supported with real token-by-token SSE output. The `x_forge` extension appears in the final chunk's usage field.

---

## Rollback plan

Forge is additive. Nothing it installs modifies your existing binaries, model files, or application code.

To stop using Forge:

```bash
# Uninstall the CLI
cargo uninstall tirami-cli
# Or just remove the binary
rm ./target/release/forge

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
