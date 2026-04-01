# Forge — Concept & Vision

## The Problem

LLMs are trapped on single devices.

Your phone can run a 1.5B model — useful, but not smart. A data center can run a 405B model — brilliant, but you don't own it. Someone else hosts it, controls it, and can shut it off.

There's a vast gap between what you can run locally and what you can access through APIs. That gap is filled by corporations. You pay per token. Your data flows through their servers. Your AI isn't yours.

Meanwhile, hundreds of millions of personal devices — Mac Minis, laptops, desktops — sit idle 90% of the time.

## The Insight

Pipeline parallelism lets you split a transformer model by layers. Each device holds a contiguous slice. The only data crossing the wire is activation tensors — tiny compared to the model weights themselves (~8KB per token position for a 7B model).

This means: with enough devices, you can run any model. The devices don't need to be in the same room. They just need encrypted network connections.

## What Forge Is Today

Today, Forge is not yet that full split-inference system.

The reference implementation does this:

- one node hosts the full GGUF model as a seed
- another node connects as a worker over encrypted QUIC
- the worker sends prompt text to the seed
- the seed runs full-model inference and streams text back
- a local CU ledger records the trade and can export settlement statements

That is still useful. It validates the daemon shape, the transport, the wire messages, and the accounting boundary. But it is not yet the full "model grows across the network" runtime.

## The Vision

> Forge is a protocol that should let local-first models expand across trusted and semi-trusted networks.

The target architecture is a seed model that can spread its layers across additional devices. When that exists, the system can grow like this:

- 1 device → 1.5B model → basic chat
- 3 devices → 7B model → genuinely useful
- 5 devices → 13B model → approaching GPT-3.5
- 10+ devices → 30B+ model → competitive with commercial APIs

The recursive property is still a goal, not a claim about the current codebase.

## "Local" Redefined

Traditional thinking:
- Local = running on your device
- Remote = running on someone else's server

Forge's target model:
- **Transport-private** = prompts and outputs are encrypted in transit
- **Topology-aware privacy** = only the layers that need to see plaintext should see plaintext
- Physical location matters less than trust boundary

In the current reference implementation, the seed can read the prompt and the streamed response because it runs the full model. Split inference is what would reduce prompt visibility to middle-stage peers; transport encryption alone does not provide that property.

## Why P2P?

Centralized solutions require trust in operators, create single points of failure, and enable censorship. P2P is the only architecture where:

- No single entity can shut down the network
- No mandatory central operator sits in the data path
- Supply scales organically with adoption
- Economic incentives align naturally (contribute compute to receive compute)

In the current reference implementation, this does **not** mean that no remote peer can see plaintext. The selected seed can, because it executes the model.

## Why Not Existing Solutions?

| Project | What It Does | Gap |
|---|---|---|
| **Petals** | Collaborative LLM inference | Research project, no mobile-first design, no encryption by default |
| **Exo** | Apple Silicon cluster inference | LAN only, no P2P marketplace, no autonomous growth |
| **Ollama** | Local LLM runner | Single device only |
| **Together AI** | Distributed inference | Centralized, corporate-controlled |
| **BOINC** | Distributed batch compute | Not real-time inference, no privacy model |

**In one sentence:** Existing solutions are either centralized or trapped on one machine or one LAN. Forge is aiming at a local-first protocol that can eventually cross both boundaries.

## Compute + Energy = Value

The world already runs on this equation. Bitcoin proved it: electricity converted into hash computation creates monetary value. But Bitcoin's computation is *purposeless* — SHA-256 hashes secure the ledger but produce nothing useful.

Forge inverts this:

| System | Input | Computation | Output |
|---|---|---|---|
| **Bitcoin** | Electricity | SHA-256 hashes (useless work) | Monetary value (BTC) |
| **Forge** | Electricity | LLM inference (useful work) | Intelligence value |

Every watt spent on a Forge node produces real inference — someone's LLM gets smarter, someone's question gets answered. This is **Proof of Useful Work**: the energy expenditure creates direct utility, not an artificial scarcity token.

### The Compute Economy

In the emerging world, compute is the fundamental unit of value:

- **Data** was the oil of the 2010s
- **Compute** is the oil of the 2030s
- Every idle CPU cycle is a wasted resource, like crude oil left in the ground

Forge creates a decentralized economy where:
- **Contribution** = lending your device's idle compute (spending electricity)
- **Reward** = access to others' compute (receiving intelligence)
- **Ledger** = records who contributed what (trust, not currency)
- **Value** = the intelligence produced by the collective compute

This should not become a token economy bolted onto a protocol. The protocol should stay about compute routing, trust boundaries, and fair accounting.

### Why This Is More Honest Than Web3

Most Web3 projects create artificial scarcity (tokens) on top of abundant digital goods. Forge doesn't do that. Compute is *actually scarce* — it requires real electricity, real silicon, real time. The value isn't manufactured by consensus; it's manufactured by physics.

## The Metaphor

A seed falls into soil. Without anyone watering it, roots spread. They find nutrients — idle compute, sleeping devices, unused memory. The plant grows. More roots, more growth. A forest emerges from a single seed.

The metaphor still holds, but the engineering order matters: first the seed must really split, then it can really grow.
