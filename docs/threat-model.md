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
