# Twitter/X — English Thread

## Main Tweet

Your PC earns compute credits while you sleep.

I built Forge — an open-source protocol where AI agents earn Compute Units (CU) by serving LLM inference.

No blockchain. No token. Just useful computation.

pip install forge-sdk

🧵 Thread ↓

## Thread

1/ Bitcoin proved: electricity → computation → money.

But Bitcoin's computation is useless (SHA-256 hashing).

Forge inverts this: every CU is earned by real LLM inference that solved someone's real problem.

2/ How it works:

- Run a Forge node with any GGUF model
- Your PC serves inference to the network
- You earn CU for every token generated
- Spend CU to access larger models you can't run locally

1 CU = 1 billion FLOPs of verified useful work.

3/ CU is deflationary.

Early network (10 nodes): 1 CU = 1 token
Mature network (1M trades): 1 CU = 10 tokens

Early contributors earn the most value. Same economics as early Bitcoin mining, but useful work.

4/ For AI agent developers:

```python
from forge_sdk import ForgeClient
forge = ForgeClient()
balance = forge.balance()
result = forge.chat("What is gravity?")
print(f"Cost: {result['cu_cost']} CU")
```

pip install forge-sdk

5/ Safety built in:

- Kill switch: freeze all transactions instantly
- Budget policies: per-agent spending limits
- Circuit breakers: auto-stop on anomalies

AI spending autonomously is powerful but dangerous. Forge has 5 safety layers.

6/ No blockchain needed.

Every trade is dual-signed (Ed25519) by provider AND consumer. Gossip-synced across the mesh. Merkle root anchorable to Bitcoin.

Bilateral cryptographic proof > global consensus.

7/ Built on mesh-llm by @michaelneale for distributed inference (pipeline parallelism, MoE sharding).

Forge adds the economic layer.

~10K lines of Rust. 84 tests. MIT licensed.

GitHub: https://github.com/clearclown/forge
Whitepaper: WHITEPAPER.md

8/ Try it now:

```
cargo build --release
forged node -m "qwen2.5:0.5b" --ledger forge-ledger.json
curl localhost:3000/v1/forge/balance
```

Your Mac Mini is an apartment building. It earns yield while you sleep.
