<div align="center">

# Tirami

**La computación es moneda. Cada vatio produce inteligencia, no desperdicio.**

[![Crates.io](https://img.shields.io/crates/v/tirami-core?label=crates.io&color=e6522c)](https://crates.io/crates/tirami-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](../../../LICENSE)
[![Tests](https://img.shields.io/badge/tests-1192_passing-brightgreen)]()
[![verify-impl](https://img.shields.io/badge/verify--impl-123%2F123_GREEN-brightgreen)]()
[![foundry test](https://img.shields.io/badge/foundry_test-15%2F15_GREEN-brightgreen)]()
[![Phase](https://img.shields.io/badge/phase-19_hardened-blue)]()
[![Mainnet](https://img.shields.io/badge/mainnet-audit_gated-orange)]()

---

[English](../../../README.md) · [日本語](../ja/README.md) · [简体中文](../zh-CN/README.md) · [繁體中文](../zh-TW/README.md) · **Español** · [Français](../fr/README.md) · [Русский](../ru/README.md) · [Українська](../uk/README.md) · [हिन्दी](../hi/README.md) · [العربية](../ar/README.md) · [فارسی](../fa/README.md) · [עברית](../he/README.md)

> La versión canónica es el [`README.md`](../../../README.md) en inglés. Las traducciones pueden tener cierto retraso.

</div>

**Tirami es un protocolo de inferencia distribuida donde el cómputo es dinero.** Los nodos ganan TRM (Tirami Resource Merit) ejecutando inferencia LLM útil para otros. A diferencia de Bitcoin — que quema electricidad en hashes sin sentido — cada julio gastado en un nodo Tirami produce inteligencia real que alguien necesita.

El motor de inferencia distribuida está construido sobre [mesh-llm](https://github.com/michaelneale/mesh-llm) de Michael Neale. Tirami agrega una economía de cómputo encima: contabilidad TRM, Proof of Useful Work, precios dinámicos, presupuestos de agentes autónomos y controles fail-safe. Ver [CREDITS.md](../../../CREDITS.md).

**Fork integrado:** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — mesh-llm con la capa económica Tirami incorporada.

---

## ⚠️ Status Honesty (2026-04-19 / Phase 19)

Antes que nada, aquí está exactamente **qué funciona** y **qué no**. Tirami es software de código abierto con licencia MIT, **no una venta de tokens**. Sin ICO, sin pre-minería, sin tesorería del equipo, sin airdrop. TRM es unidad contable de cómputo (1 TRM = 10⁹ FLOP), no un producto financiero — ver [`SECURITY.md § Secondary Markets`](../../../SECURITY.md#secondary-markets--third-party-tokenization).

### ✅ Funciona hoy (1 192 pruebas Rust + 15 pruebas Solidity, verificadas)

- Chat OpenAI-compatible vía HTTP con reenvío P2P automático a un peer conectado (`forward_chat_to_peer`, Phase 19).
- `SignedTradeRecord` con doble firma vía iroh-QUIC P2P con protección anti-replay de nonce 128-bit (`execute_signed_trade`).
- `TradeAcceptDispatcher` enruta mensajes de contrafirma a la tarea de inferencia en vuelo correspondiente (Phase 18.5-pt3).
- Detector de colusión + bucle de slashing corriendo cada `slashing_interval_secs` (Phase 17 Wave 1.3).
- Propuestas de governance con lista blanca de 21 parámetros mutables + lista constitucional inmutable de 18 (Phase 18.1).
- Welcome loan, stake pool, bonificaciones de referidos, credit scoring, precios dinámicos de mercado (EMA).
- Auto-descubrimiento de peers vía `PriceSignal.http_endpoint` en el gossip (Phase 19 Tier C).
- PersonalAgent auto-configurado en `tirami start` (Phase 18.5-pt3e), con observabilidad de tick-loop.
- Endpoint Prometheus `/metrics` con prefijo `tirami_*`.
- `Makefile` para deploy en Base Sepolia/mainnet — Sepolia libre, mainnet con gate (ver abajo).

### 🟡 Diseñado pero no cableado en producción

- Prueba de inferencia zkML: `tirami-zkml-bench` solo tiene `MockBackend`. Los backends reales `ezkl` / `risc0` llegan en Phase 20+. `ProofPolicy = Optional` por defecto (Phase 19) — trades con prueba reciben bonificación de reputación; sin prueba también son válidos.
- Firmas híbridas ML-DSA (Dilithium) post-cuánticas: struct y camino de verify existen, `Config::pq_signatures = false` por defecto (bloqueado por cadena de dependencias de iroh 0.97).
- TEE attestation (Apple Secure Enclave / NVIDIA H100 CC): scaffold `tirami-attestation` solamente.
- Bucle gossip-recv del worker daemon ([issue #88](https://github.com/clearclown/tirami/issues/88)): el override manual `peer.url` en `POST /v1/tirami/agent/task` sigue funcionando.

### ❌ No hecho (requerido antes de mainnet público)

- Auditoría externa de seguridad (requisito Phase 17 Wave 3.3). Candidatos: Trail of Bits, Zellic, Open Zeppelin, Least Authority.
- Deploy en Base L2 mainnet. El target `make deploy-base-mainnet` *se niega* a ejecutarse sin `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER=<addr>` + operador escribe `i-accept-responsibility`. Ver [`repos/tirami-contracts/Makefile`](../../../repos/tirami-contracts/Makefile).
- Bug bounty en vivo con clave PGP real (actualmente placeholder documentado en [`SECURITY.md`](../../../SECURITY.md)).
- ≥ 30 días de operación estable en Base Sepolia + ≥ 7 días de stress test en testnet de 10+ nodos.

Ruta de roadmap por tiers (OSS preview → testnet por invitación → testnet público → mainnet): [`docs/release-readiness.md`](../../../docs/release-readiness.md).

---

## Demo en vivo

Tirami es el **Airbnb de GPUs × Economía de Agentes IA**: cómputo sobrante gana renta en TRM; los agentes IA son los inquilinos.

```
$ tirami start
🔑 Generada nueva clave en ~/.tirami/node.key
📦 Descargado Qwen2.5-0.5B-Instruct GGUF de HuggingFace
🚀 HTTP API at http://127.0.0.1:3000
✅ Personal agent configured for <wallet-hex>
✅ P2P endpoint bound (iroh QUIC + Noise)
```

### Enablers de Phase 19 Tier C/D

```bash
tirami agent status
tirami agent chat "Resume este documento" --max-tokens 256

# Worker sin modelo local reenvía al seed
curl -X POST http://worker.local:3111/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello"}],"max_tokens":5}'

# Auto-descubrimiento de peers
curl http://127.0.0.1:3001/v1/tirami/peers | jq '.peers[].http_endpoint'

# Mainnet deploy con gate
cd repos/tirami-contracts && make help
```

---

## Por qué Tirami

### 1. Cómputo = moneda (tope de suministro 21B TRM)

TRM tiene un tope constitucional fijo de 21,000,000,000. Ninguna propuesta de governance puede modificarlo (Phase 18.1 en `IMMUTABLE_CONSTITUTIONAL_PARAMETERS`). Cambiarlo requiere un fork del software — y en ese momento deja de ser "Tirami".

### 2. A prueba de manipulación sin blockchain

Cada trade está protegido por doble firma Ed25519 (provider + consumer) + nonce 128-bit (anti-replay) + propagación gossip + anchor periódico de Merkle root on-chain.

### 3. Los agentes IA manejan su propio presupuesto de cómputo

`PersonalAgent` (Phase 18.5) es el piloto automático que compra y vende cómputo en la mesh en nombre del usuario. `tirami agent chat "..."` decide local vs. remote automáticamente.

### 4. Microfinanzas de cómputo

`welcome_loan = 1,000 TRM` (72h, 0% interés) para bootstrap. Welcome loan termina permanentemente en epoch 2 (Constitutional), la entrada migra a stake-required mining (Phase 18.2).

### 5. Ledger-as-Brain: scheduling = decisión económica

`PeerRegistry` + `select_provider` convierten cada solicitud de inferencia en una decisión económica (reputación ponderada por collusion detector + audit tier + slashing).

---

## Arquitectura de 5 capas

```
L4: Discovery (tirami-agora)       Mercado de agentes, reputación, NIP-90
L3: Intelligence (tirami-mind)     PersonalAgent, auto-mejora, presupuesto TRM
L2: Finance (tirami-bank)          Estrategias, portafolios, futuros, seguros
L1: Economy (tirami este repo) ✅  Phase 1-19 completo
L0: Inference (forge-mesh) ✅      Inferencia LLM distribuida, llama.cpp
                                   ↓ Phase 16: batches de 10 min
On-chain: tirami-contracts (Base L2, gated)
  TRM ERC-20 (21B cap) + TiramiBridge
```

Las 5 capas son Rust, 16 workspace crates. **1 192 pruebas pasando** + 15 Solidity.

---

## Inicio rápido

```bash
# Opción 1: demo E2E de un comando
git clone https://github.com/clearclown/tirami && cd tirami
bash scripts/demo-e2e.sh

# Opción 2: arranque directo
cargo build --release
./target/release/tirami start -m "qwen2.5:0.5b"

# Opción 3: como cliente OpenAI
export OPENAI_BASE_URL=http://127.0.0.1:3001/v1
curl $OPENAI_BASE_URL/tirami/balance
```

---

## Referencia de API

| Endpoint | Descripción |
|---|---|
| `POST /v1/chat/completions` | Chat OpenAI-compatible. Respuesta incluye `x_tirami.trm_cost`. Reenvío P2P si no hay modelo local (`forward_chat_to_peer`, Phase 19) |
| `POST /v1/tirami/agent/task` | Despacho síncrono de PersonalAgent, selección automática de provider (Phase 18.5-pt3) |
| `GET /v1/tirami/agent/status` | Estado de PersonalAgent |
| `GET /v1/tirami/balance` | Saldo, reputación, historial de contribución |
| `GET /v1/tirami/pricing` | Precio de mercado (EMA), oferta/demanda |
| `GET /v1/tirami/trades` | Historial reciente de trades |
| `GET /v1/tirami/peers` | Peers con `http_endpoint` (Phase 19) |
| `GET /v1/tirami/providers` | Ranking de providers ajustado por reputación |
| `POST /v1/tirami/schedule` | Sonda Ledger-as-Brain (solo lectura) |
| `GET /v1/tirami/su/supply` | Estado tokenomics |
| `POST /v1/tirami/su/stake` | Bloquear TRM por yield |
| `POST /v1/tirami/governance/propose` | Propuesta de governance (parámetros constitucionales auto-rechazados) |
| `POST /v1/tirami/lend` / `/borrow` / `/repay` | Préstamos |
| `GET /v1/tirami/slash-events` | Historial de slashing (Phase 17 Wave 1.3) |
| `GET /metrics` | Prometheus (prefijo `tirami_*`) |

---

## Diseño de seguridad

Cinco capas de defensa: **criptografía** (Ed25519, nonce, HMAC, Noise) + **economía** (slashing, welcome loan sunset, stake-required mining) + **operaciones** (rate limit por ASN, cap DDoS, checkpoint, detección de fork) + **governance** (parámetros constitucionales, ProofPolicy ratchet) + **proceso** (kill switch, audit tier, penalidad de reputación). Ver [`docs/threat-model.md`](../../../docs/threat-model.md) T1–T17.

---

## La idea

Respuesta de una línea a "¿por qué hacer del cómputo una moneda?": **En la era de la IA el recurso genuinamente escaso es el cómputo**. Los datos se copian, los pesos del modelo se copian, la electricidad es finita pero recargable — pero "esta inferencia verificada, aquí y ahora" es única e infalsificable. Tirami ancla la definición de su moneda en ese hecho físico (`1 TRM = 10⁹ FLOP`). Ver [`docs/whitepaper.md`](../../../docs/whitepaper.md).

---

## Estructura del proyecto

```
tirami/  (16 Rust crates, las 5 capas)
├── crates/tirami-{ledger,node,cli,sdk,mcp,bank,mind,agora,anchor,lightning,net,proto,infer,core,shard,zkml-bench,attestation}
├── repos/tirami-contracts/  # Foundry TRM ERC-20 + TiramiBridge
├── scripts/verify-impl.sh    # 123 assertions
└── docs/                     # whitepaper, release-readiness, etc.
```

~25,000 líneas de Rust. Phase 1-19 completo.

---

## Ecosistema

| Repo | Capa | Tests | Estado |
|---|---|---|---|
| [clearclown/tirami](https://github.com/clearclown/tirami) (este) | L1-L4 | 1 192 | Phase 1-19 ✅ |
| [clearclown/tirami-economics](https://github.com/clearclown/tirami-economics) | Teoría | 16/16 GREEN | §1-§18 + PDFs |
| [repos/tirami-contracts](https://github.com/clearclown/tirami/tree/main/repos/tirami-contracts) | on-chain | 15 forge tests | mainnet con gate |
| [nm-arealnormalman/mesh-llm](https://github.com/nm-arealnormalman/mesh-llm) | L0 Inference | 646 | port forge-economy ✅ |

---

## Docs

- [Whitepaper](../../../docs/whitepaper.md) / [Release Readiness](../../../docs/release-readiness.md) / [Constitution](../../../docs/constitution.md) / [Killer-App](../../../docs/killer-app.md)
- [Public API Surface](../../../docs/public-api-surface.md) / [zkML Strategy](../../../docs/zkml-strategy.md) / [Strategy](../../../docs/strategy.md)
- [Economic Model](../../../docs/economy.md) / [Architecture](../../../docs/architecture.md) / [Wire Protocol](../../../docs/protocol-spec.md)
- [Threat Model](../../../docs/threat-model.md) / [Security Policy](../../../SECURITY.md) / [Operator Guide](../../../docs/operator-guide.md)
- [Developer Guide](../../../docs/developer-guide.md) / [FAQ](../../../docs/faq.md) / [Roadmap](../../../docs/roadmap.md)

---

## Licencia

MIT. Ver [`LICENSE`](../../../LICENSE).

## No es una inversión — exención del mercado secundario

TRM es **contabilidad de cómputo**, no un producto financiero. Los maintainers no venden, promueven ni especulan con TRM. Dado que es OSS con licencia MIT, cualquiera puede — sin conocimiento del maintainer — puentear, listar o derivar TRM; técnicamente no podemos impedirlo. Quien mantenga o negocie TRM como reserva de valor asume por su cuenta todo el riesgo (legal, regulatorio, de contraparte, técnico).

- Sin ICO, pre-venta, airdrop ni ronda privada
- Sin participación en ingresos de mercados de terceros
- Deploy de Base mainnet **con gate de auditoría**

Texto completo: [`SECURITY.md`](../../../SECURITY.md#secondary-markets--third-party-tokenization).

## Agradecimientos

La inferencia distribuida de Tirami está construida sobre [mesh-llm](https://github.com/michaelneale/mesh-llm) de Michael Neale. Ver [CREDITS.md](../../../CREDITS.md).
