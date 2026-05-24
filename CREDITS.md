# Credits & Acknowledgements

## mesh-llm

Tirami's inference foundation comes from [mesh-llm](https://github.com/Mesh-LLM/mesh-llm), originally by **Michael Neale** and now maintained under the Mesh-LLM organization.

mesh-llm solved the hard problems of distributed LLM inference:

- Pipeline parallelism for dense models across multiple machines
- Expert sharding for Mixture-of-Experts architectures (Qwen3, Mixtral, DeepSeek)
- iroh-based mesh networking with Nostr discovery
- OpenAI-compatible API with multi-model routing
- Web management console with live topology visualization
- Plugin system with Blackboard agent coordination

Tirami does not claim credit for any of this engineering. What Tirami adds is an economic layer — TRM accounting, Proof of Useful Work, dynamic pricing, and autonomous agent budgets — on top of the mesh-llm inference foundation.

The current upstream local/distributed LLM runtime is [Mesh-LLM/mesh-llm](https://github.com/Mesh-LLM/mesh-llm). Tirami also has a historical integration fork at [nm-arealnormalman/mesh-llm](https://github.com/nm-arealnormalman/mesh-llm), but the recommended entry point for the Tirami protocol and economy is the `clearclown/tirami` workspace.

The relationship is straightforward: mesh-llm makes distributed inference work. Tirami makes it worth doing.

## Other Influences

- **Bitcoin** — Proved that `electricity → computation → monetary value` is a viable economic model. Tirami inherits the insight but replaces useless PoW with useful inference.
- **BitTorrent** — Reciprocity-based resource sharing without central coordination. Tirami's TRM economy is a formalized version of BitTorrent's tit-for-tat.
- **llama.cpp** by Georgi Gerganov — The inference engine that makes local LLM execution practical on consumer hardware.
- **iroh** by n0 — The QUIC-based networking library that handles NAT traversal, relay fallback, and peer-to-peer connections.
- **LDK (Lightning Dev Kit)** — The embedded Lightning node that enables optional TRM↔Bitcoin settlement.

## Philosophy

Sam Altman's thesis — intelligence scales with compute — is the economic foundation of Tirami. If more electricity + more silicon = smarter AI, then compute is the most valuable commodity of the AI era. Tirami creates a market for it.
