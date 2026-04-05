# Reddit r/MachineLearning Post

**Title:** [P] Forge: Open-source compute economy for AI agents — earn CU by serving inference, spend CU on larger models

**Body:**

**Problem:** AI agents are limited by local hardware. They can't autonomously acquire more compute.

**Solution:** Forge — a P2P protocol where agents earn Compute Units (CU) by serving LLM inference and spend CU to access larger models.

**Technical highlights:**

- **Dual-signed trades:** Ed25519 signatures from both provider AND consumer. No blockchain — bilateral cryptographic proof.
- **CU deflation:** logarithmic decay — as total trades grow, each CU buys more compute. `deflation_factor = 1 / (1 + ln(1 + trades/1000))`
- **Gossip protocol:** signed trades broadcast to mesh with SHA-256 deduplication, bounded LRU (100K entries)
- **Safety:** token-bucket rate limiter, circuit breakers (5 errors or 30 spends/min), kill switch, budget policies
- **Merkle root:** SHA-256 tree over all trades, anchorable to Bitcoin OP_RETURN

**Agent integration:**

```python
from forge_sdk import ForgeAgent

agent = ForgeAgent(max_cu_per_task=500)
while agent.has_budget():
    result = agent.think("next task")
    # agent.think() checks balance, estimates cost, runs inference
```

**Stack:** Rust (~10K LOC), llama.cpp backend, iroh QUIC + Noise encryption, 84 tests, 2 security audits.

Built on mesh-llm for distributed inference (pipeline parallelism, MoE expert sharding).

- GitHub: https://github.com/clearclown/forge
- PyPI: `pip install forge-sdk`
- Whitepaper: WHITEPAPER.md
