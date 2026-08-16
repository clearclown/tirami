# Hacker News — Show HN Post

**Title:** Show HN: Forge – A P2P compute economy where AI agents earn and spend for inference

**URL:** https://github.com/clearclown/forge

**Comment (post immediately after submission):**

Forge adds an economy to distributed LLM inference. The core idea:

Bitcoin proved electricity → computation → money. But Bitcoin's computation (SHA-256) is useless. Forge inverts this: every Compute Unit (CU) is earned by performing real LLM inference.

Technical highlights:
- Dual-signed trades: Ed25519 signatures from both provider AND consumer. No blockchain needed — bilateral cryptographic proof is sufficient
- Gossip protocol: signed trades propagate across the mesh with SHA-256 deduplication
- Merkle root: entire trade history can be anchored to Bitcoin via OP_RETURN
- CU deflation: as network grows, each CU buys more compute (log-scale, like Bitcoin halving)
- Safety: kill switch, circuit breakers, budget policies (fail-safe design)

Built on mesh-llm [1] for distributed inference. Forge adds the economic layer.

The target user isn't humans — it's AI agents. Agents check their balance (`GET /v1/forge/balance`), estimate costs (`GET /v1/forge/pricing`), and run inference (`POST /v1/chat/completions`) autonomously. Every response includes `x_forge.cu_cost`.

~10K lines of Rust, 84 tests, MIT licensed. Working demo in the README — real output from a running node.

Python SDK: `pip install forge-sdk`
MCP server: `pip install forge-mcp` (for Claude Code / Cursor)

[1] https://github.com/Mesh-LLM/mesh-llm
