# Forge — Threat Model

## Security Goals

1. **Confidentiality in transit**: passive observers and relays should not read prompts or responses
2. **Authenticated peers**: every direct connection should bind to a cryptographic node identity
3. **Bounded trust**: the system should make it explicit which nodes see plaintext and which only see intermediate state
4. **Availability**: single peer failures should degrade service rather than silently corrupt it

## Encryption

### Transport Layer
- All connections use QUIC with TLS 1.3 (via Iroh)
- Additional Noise Protocol handshake (XX pattern) for peer authentication
- Result: ChaCha20-Poly1305 symmetric encryption with forward secrecy
- Per-session ephemeral keys — compromise of one session doesn't affect others

### Identity
- Each node has a persistent Ed25519 keypair
- Generated on first launch, stored in platform keychain
- Node ID = public key hash
- No central certificate authority — web-of-trust model

### What's Encrypted
| Data | Encrypted | Notes |
|---|---|---|
| Prompt text | Yes | Encrypted in transit; visible to the seed in the current reference flow |
| Streamed text output | Yes | Encrypted in transit; visible to the seed that generated it |
| Activation tensors | Planned | Relevant once split inference is active in runtime |
| Control messages | Yes | All protocol messages within QUIC |
| Peer capabilities | Yes | Exchanged over encrypted channel |

## Threat Analysis

### T1: Malicious Seed Node (current reference flow)
**Threat**: The seed operator reads the prompt or response because the worker sends prompt text to the seed and the seed runs the full model.

**Current status**: This is an explicit trust boundary, not a solved security property.

**Mitigation**:
- only connect workers to seeds you trust with plaintext prompts
- keep transport encrypted so relays and passive observers cannot read contents
- move toward split inference so middle-stage peers do not receive plaintext prompts

### T2: Malicious Pipeline Node (target split-inference flow)
**Threat**: A node in the future pipeline tries to extract the prompt or response from intermediate activations.

**Mitigation target**: A node at pipeline stage k should only see the activation tensor output of layer k-1 and produce the activation tensor for layer k. It should not receive the original prompt text.

**Residual risk**: Activation tensors can leak information about the input. Differential privacy, redundancy, and attestation remain future work.

### T3: Sybil Attack
**Threat**: An attacker creates many fake nodes to dominate the pipeline.

**Mitigation**:
- Reputation system based on observed behavior (uptime, correct computation)
- New nodes start with low reputation and limited pipeline positions
- Critical layers (first and last) preferentially assigned to high-reputation nodes
- Rate limiting on new node joins from same IP range

### T4: Byzantine Inference
**Threat**: A malicious node returns incorrect activation tensors.

**Mitigation (MVP)**: Accept the risk. For most use cases, a subtly wrong inference result is detectable by the user.

**Mitigation (future)**:
- Redundant computation on critical layers (2 nodes compute same layers, compare)
- Verifiable computation using TEE attestation (Apple Silicon Secure Enclave)
- Statistical anomaly detection on activation tensor distributions

### T5: Traffic Analysis
**Threat**: Observer monitors encrypted traffic patterns to infer usage.

**Mitigation**:
- QUIC multiplexes all communication over a single connection
- Current seed/worker traffic still leaks coarse request timing and response length metadata
- Padding on control messages to constant size (optional, not in MVP)

### T6: Relay Server Compromise
**Threat**: Bootstrap relay servers are compromised.

**Impact**: Minimal. Relay servers only facilitate connection establishment. They see:
- Which node IDs are connecting (metadata)
- Encrypted QUIC packets (cannot decrypt)
- They do NOT see decrypted prompts or responses

**Mitigation**: Multiple independent relay operators. Network continues without relays once DHT is populated.

### T7: Model Poisoning
**Threat**: A node serves a modified GGUF model with backdoored weights.

**Mitigation**:
- Model files verified by SHA-256 hash against known-good manifests
- Model manifests distributed via DHT with signatures from model publishers
- Nodes only load models from verified sources (HuggingFace hashes)

### T8: Denial of Service
**Threat**: Nodes join and then become unresponsive, disrupting inference.

**Mitigation**:
- Heartbeat timeout and rebalancing are target runtime properties, not complete guarantees of the current implementation
- local fallback is a design goal for future split-inference clients
- Reputation penalty for nodes that disconnect frequently
- Graceful degradation is a core design principle
- inbound inference requests are bounded by runtime validation and a fixed concurrent execution limit on the seed
- duplicate protocol `msg_id` values from the same peer are dropped inside a bounded replay window

### T9: Administrative API Exposure
**Threat**: An operator binds the local HTTP API to a public interface without protection, exposing `/status`, `/topology`, `/settlement`, or `/chat`.

**Mitigation in the current implementation**:
- the daemon binds the HTTP API to `127.0.0.1` by default
- operators can still expose it intentionally with `--bind 0.0.0.0`
- exposed administrative routes can be protected with a bearer token via `--api-token`
- JSON request bodies are size-limited before deserialization to reduce allocation abuse on `/chat`

**Residual risk**: Bearer token authentication is an operator control, not mutual TLS. If the token leaks, the API should be treated as compromised until rotated.

## Trust Hierarchy

```
Most trusted:    Your own device (phone, laptop)
                 ↓
Trusted:         Your own devices on LAN (Mac Mini at home)
                 ↓
Semi-trusted:    High-reputation WAN peers (months of uptime)
                 ↓
Untrusted:       New WAN peers (fresh join, no history)
```

Layer assignment should follow this hierarchy once split inference exists:
- First and last layers (most sensitive — see input embeddings and output logits) → your own devices
- Middle layers (see only intermediate activations) → can be assigned to semi-trusted or untrusted peers

## Privacy Guarantees

**What Forge guarantees today:**
- prompts and responses are encrypted in transit between directly connected peers
- relays and passive network observers do not see decrypted prompt or response contents
- there is no mandatory central server in the data path
- the current seed/worker trust boundary is explicit

**What Forge does not guarantee today:**
- that the seed cannot read the prompt or response
- that split inference hides plaintext from all remote compute providers
- that incorrect remote inference is detected automatically

**What Forge is aiming to guarantee later:**
- middle-stage peers do not receive plaintext prompts
- activation tensors are encrypted in transit between pipeline stages
- prompt visibility is reduced to the minimal set of trusted boundary nodes

Those later guarantees depend on shipping actual split inference first. Until then, Forge should be described as encrypted remote inference with an honest trust boundary.

## Economic Threats

### T10: TRM Forgery

**Threat**: A node claims TRM it did not earn by fabricating TradeRecords.

**Current mitigation**: Local ledger with HMAC-SHA256 integrity prevents file-level tampering. However, the node operator can still write arbitrary trades into their own ledger.

**Target mitigation**: Dual-signature protocol. Every TradeRecord must be signed by both the provider and the consumer. A node cannot credit itself TRM without a counterparty's signature. Gossip sync means other nodes can verify both signatures independently.

**Residual risk**: Collusion between provider and consumer to create fake trades. This is economically irrational — the colluding consumer gains nothing. Statistical anomaly detection on trade volume and frequency can flag suspicious patterns.

### T11: Free Tier Abuse (Sybil)

**Threat**: An attacker creates many new NodeIds to exploit the 1,000 TRM free tier repeatedly.

**Current mitigation**: If more than 100 unknown nodes (contributed = 0, consumed > 0) exist in the ledger, new free-tier requests are rejected. Each NodeId is an Ed25519 keypair — cheap to create but trackable.

**Target mitigation**: Proof of Work on node registration (small computational cost to create a new identity), or stake-based entry (new nodes must contribute compute before consuming).

### T12: Ledger Divergence

**Threat**: Different nodes have incompatible views of the same trades, leading to economic inconsistency.

**Current mitigation**: Each node maintains its own local view. No consistency guarantee across nodes.

**Target mitigation**: Gossip-synced dual-signed TradeRecords. Both parties produce identical signed records. Any node receiving a gossip update can verify signatures and reject inconsistencies. Periodic summary anchoring to Bitcoin (OP_RETURN) provides an optional immutable audit trail.

### T13: Market Manipulation

**Threat**: A node artificially inflates demand or supply factors to manipulate pricing.

**Current mitigation**: Market price is computed locally from each node's own observations. No single node can force another node to adopt its price.

**Target mitigation**: Gossip-based price signals weighted by reputation. High-reputation nodes' observations carry more weight. New or low-reputation nodes cannot significantly influence network-wide pricing.

### T14: Inference Quality Attack

**Threat**: A provider returns low-quality or truncated inference to earn TRM without doing full computation.

**Current mitigation**: Accept the risk. For most use cases, obviously wrong outputs are detectable by the consumer.

**Target mitigation**: Consumer-side quality verification. The consumer can re-run a small sample of tokens locally to verify the provider's output is consistent. Reputation penalty for providers whose outputs fail spot checks.

### T15: Loan Default Cascading

**Threat**: A large borrower defaults on a loan, depleting lender reserves. Affected lenders cannot meet their own obligations, causing a cascade of defaults across the network.

**Current mitigation**: Not yet applicable (lending not implemented).

**Target mitigation**:
- Maximum loan-to-collateral ratio (3:1) limits exposure per loan
- Maximum single-loan size capped at 20% of lending pool
- Pool reserve requirement: at least 30% of pool must remain unlent at all times
- Diversification: lending pool distributes across multiple borrowers automatically
- Circuit breaker: if default rate exceeds 10% in any hour, all new lending is suspended network-wide

**Residual risk**: Coordinated default by colluding borrowers who built credit independently over months. Statistical monitoring of correlated default timing and shared IP ranges can detect this pattern.

### T16: Credit Score Manipulation

**Threat**: A node artificially inflates its credit score through wash trading (self-dealing between owned nodes) or strategic small-loan repayment before taking a large loan and defaulting.

**Current mitigation**: Not yet applicable (credit scoring not implemented).

**Target mitigation**:
- Credit score weights repayment amount, not just count — many small repayments don't boost score as much as fewer large ones
- Trade-based score component uses graph analysis to detect circular trading patterns between the same set of nodes
- Minimum account age (7 days) before any borrowing is allowed
- Score decay: credit score decreases if node is inactive for >7 days (0.01/day decay on uptime_score)
- Anomaly detection on score velocity — rapid score increase flags the node for closer monitoring

**Residual risk**: Patient attacker who builds score slowly over months before a single large default. Collateral requirements (3:1 max LTV) limit maximum loss even with perfect credit — worst case, lender loses 67% of the loan amount.

### T17: Lending Pool Depletion

**Threat**: A coordinated attack drains the lending pool by taking maximum loans from multiple identities simultaneously, then defaulting on all.

**Current mitigation**: Not yet applicable (lending pool not implemented).

**Target mitigation**:
- Pool reserve requirement (30% minimum unlent at all times)
- Per-identity borrowing cap based on credit score (quadratic: `credit^2 * pool * 0.2`)
- Sybil resistance: new identities have low credit score (0.3), limiting each to ~1.8% of pool
- Rate limiting on new loan creation (max 10 loans per minute globally)
- Global lending velocity circuit breaker: suspends all new loans if total lending exceeds 50% of pool in any 1-hour window

**Residual risk**: Slow-motion attack using aged identities accumulated over months. Maximum total exposure is bounded by `pool_size * (1 - reserve_ratio)` = 70% of pool. With collateral, actual loss is further bounded to ~47% of pool in the absolute worst case.
