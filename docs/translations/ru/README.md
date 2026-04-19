<div align="center">

# Tirami

**Вычисления — это деньги. Каждый ватт рождает интеллект, а не мусор.**

[![Crates.io](https://img.shields.io/crates/v/tirami-core?label=crates.io&color=e6522c)](https://crates.io/crates/tirami-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](../../../LICENSE)
[![Tests](https://img.shields.io/badge/tests-1192_passing-brightgreen)]()
[![verify-impl](https://img.shields.io/badge/verify--impl-123%2F123_GREEN-brightgreen)]()
[![foundry test](https://img.shields.io/badge/foundry_test-15%2F15_GREEN-brightgreen)]()
[![Phase](https://img.shields.io/badge/phase-19_hardened-blue)]()
[![Mainnet](https://img.shields.io/badge/mainnet-audit_gated-orange)]()

---

[English](../../../README.md) · [日本語](../ja/README.md) · [简体中文](../zh-CN/README.md) · [繁體中文](../zh-TW/README.md) · [Español](../es/README.md) · [Français](../fr/README.md) · **Русский** · [Українська](../uk/README.md) · [हिन्दी](../hi/README.md) · [العربية](../ar/README.md) · [فارسی](../fa/README.md) · [עברית](../he/README.md)

> Канонической является англоязычная версия [`README.md`](../../../README.md). Переводы могут отставать.

</div>

**Tirami — это распределённый протокол LLM-инференса, где вычисления являются деньгами.** Узлы зарабатывают TRM (Tirami Resource Merit), выполняя полезный LLM-инференс для других. В отличие от Bitcoin, сжигающего электричество ради бессмысленных хешей, каждый джоуль, потраченный на узле Tirami, рождает реальный интеллект, нужный кому-то прямо сейчас.

Движок распределённого инференса построен на [mesh-llm](https://github.com/michaelneale/mesh-llm) от Майкла Нила. Tirami надстраивает над ним экономику вычислений: учёт TRM, Proof of Useful Work, динамическое ценообразование, бюджеты автономных агентов, fail-safe контроль. См. [CREDITS.md](../../../CREDITS.md).

**Интегрированный fork:** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — mesh-llm со встроенным экономическим слоем Tirami.

---

## ⚠️ Status Honesty (2026-04-19 / Phase 19)

Прежде всего — вот ровно то, **что работает**, и **что не работает**. Tirami — open-source программное обеспечение под лицензией MIT, **не продажа токенов**. Нет ICO, pre-mine, team treasury или airdrop. TRM — единица учёта вычислений (1 TRM = 10⁹ FLOP), не финансовый продукт — см. [`SECURITY.md § Secondary Markets`](../../../SECURITY.md#secondary-markets--third-party-tokenization).

### ✅ Работает сегодня (1 192 Rust-теста + 15 Solidity-тестов, проверено)

- OpenAI-совместимый чат по HTTP с автоматическим P2P-forwarding к подключённому peer'у (`forward_chat_to_peer`, Phase 19).
- `SignedTradeRecord` с двойной подписью по iroh-QUIC P2P с anti-replay защитой через 128-битный nonce (`execute_signed_trade`).
- `TradeAcceptDispatcher` маршрутизирует сообщения контрподписи в соответствующую активную задачу инференса (Phase 18.5-pt3).
- Детектор сговора + цикл slashing, крутящийся каждые `slashing_interval_secs` (Phase 17 Wave 1.3).
- Governance-предложения с 21-записным изменяемым whitelist + 18-записным неизменяемым конституционным списком (Phase 18.1).
- Welcome loan, stake pool, реферальные бонусы, credit scoring, динамические рыночные цены (EMA smoothing).
- Автообнаружение peer'ов через `PriceSignal.http_endpoint` в gossip-потоке (Phase 19 Tier C).
- PersonalAgent автоматически настраивается при `tirami start` (Phase 18.5-pt3e), с наблюдением за tick loop.
- Prometheus `/metrics` endpoint с префиксом `tirami_*`.
- `Makefile` для развёртывания Base Sepolia/mainnet — Sepolia бесплатный, mainnet под gate (см. ниже).

### 🟡 Спроектировано, но не подключено в production

- zkML-доказательство инференса: `tirami-zkml-bench` содержит только `MockBackend`. Реальные backend'ы (`ezkl` / `risc0`) придут в Phase 20+. `ProofPolicy = Optional` по умолчанию (Phase 19) — trades с доказательством получают репутационный бонус; без него всё равно валидны.
- Гибридные постквантовые подписи ML-DSA (Dilithium): struct и путь verify существуют, `Config::pq_signatures = false` по умолчанию (блокируется iroh 0.97).
- TEE attestation (Apple Secure Enclave / NVIDIA H100 CC): только scaffold `tirami-attestation`.
- Worker daemon gossip-recv loop ([issue #88](https://github.com/clearclown/tirami/issues/88)): ручной `peer.url` в `POST /v1/tirami/agent/task` всё ещё работает.

### ❌ Не сделано (требуется до public mainnet)

- Внешний аудит безопасности (требование Phase 17 Wave 3.3). Кандидаты: Trail of Bits, Zellic, Open Zeppelin, Least Authority.
- Развёртывание Base L2 mainnet. Цель `make deploy-base-mainnet` *отказывается* запускаться без `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER=<addr>` + интерактивного ввода `i-accept-responsibility`. См. [`repos/tirami-contracts/Makefile`](../../../repos/tirami-contracts/Makefile).
- Bug bounty в production с настоящим PGP-ключом (сейчас placeholder, задокументированный в [`SECURITY.md`](../../../SECURITY.md)).
- ≥ 30 дней стабильной работы на Base Sepolia + ≥ 7 дней stress-теста на testnet из 10+ узлов.

Полная дорожная карта по уровням: [`docs/release-readiness.md`](../../../docs/release-readiness.md).

---

## Живое демо

Tirami — это **Airbnb для GPU × экономика AI-агентов**: свободные вычисления зарабатывают аренду в TRM; AI-агенты — арендаторы.

```
$ tirami start
🔑 Новый ключ сгенерирован в ~/.tirami/node.key
📦 Qwen2.5-0.5B-Instruct GGUF получен из HuggingFace
🚀 HTTP API at http://127.0.0.1:3000
✅ Personal agent configured for <wallet-hex>
✅ P2P endpoint bound (iroh QUIC + Noise)
```

### Enablers Phase 19 Tier C/D

```bash
tirami agent status
tirami agent chat "Сделай резюме статьи" --max-tokens 256

# Worker без локальной модели перенаправляет в seed
curl -X POST http://worker.local:3111/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello"}],"max_tokens":5}'

# Автообнаружение peer'ов
curl http://127.0.0.1:3001/v1/tirami/peers | jq '.peers[].http_endpoint'

# Развёртывание mainnet с gate
cd repos/tirami-contracts && make help
```

---

## Почему Tirami

### 1. Вычисление = деньги (потолок эмиссии 21B TRM)

TRM имеет конституционный потолок эмиссии, зафиксированный на 21 000 000 000. Ни одно governance-предложение не может его изменить (Phase 18.1, в `IMMUTABLE_CONSTITUTIONAL_PARAMETERS`). Изменить его можно только форком ПО — и в тот же момент это уже не «Tirami».

### 2. Устойчивость к подделкам без blockchain

Каждый trade защищён двойной подписью Ed25519 (provider + consumer) + 128-битный nonce (anti-replay) + gossip-распространение + периодический anchor Merkle root on-chain.

### 3. AI-агенты сами управляют compute-бюджетом

`PersonalAgent` (Phase 18.5) — автопилот, покупающий и продающий вычисления в mesh от имени пользователя. `tirami agent chat "..."` автоматически выбирает local или remote.

### 4. Микрофинансирование вычислений

`welcome_loan = 1 000 TRM` (72 часа, ставка 0%) для bootstrap. Welcome loan окончательно прекращается в epoch 2 (Constitutional), путь входа мигрирует к stake-required mining (Phase 18.2).

### 5. Ledger-as-Brain: планирование = экономическое решение

`PeerRegistry` + `select_provider` превращают каждый inference-запрос в экономическое решение (репутация, взвешенная детектором сговора + audit tier + slashing).

---

## 5-слойная архитектура

```
L4: Discovery (tirami-agora)       Маркетплейс агентов, репутация, NIP-90
L3: Intelligence (tirami-mind)     PersonalAgent, самоулучшение, TRM-бюджет
L2: Finance (tirami-bank)          Стратегии, портфели, futures, страхование
L1: Economy (этот репо) ✅         Phase 1-19 завершена
L0: Inference (forge-mesh) ✅      Распределённый LLM-инференс, llama.cpp
                                   ↓ Phase 16: batch'и по 10 минут
On-chain: tirami-contracts (Base L2, gated)
  TRM ERC-20 (cap 21B) + TiramiBridge
```

Все 5 слоёв на Rust, 16 workspace crates. **1 192 теста проходят** + 15 Solidity.

---

## Быстрый старт

```bash
# Вариант 1: E2E-демо одной командой
git clone https://github.com/clearclown/tirami && cd tirami
bash scripts/demo-e2e.sh

# Вариант 2: прямой запуск
cargo build --release
./target/release/tirami start -m "qwen2.5:0.5b"

# Вариант 3: как клиент OpenAI
export OPENAI_BASE_URL=http://127.0.0.1:3001/v1
curl $OPENAI_BASE_URL/tirami/balance
```

---

## API справочник

| Endpoint | Описание |
|---|---|
| `POST /v1/chat/completions` | OpenAI-совместимый чат. Ответ с `x_tirami.trm_cost`. P2P-forwarding, если нет локальной модели (`forward_chat_to_peer`, Phase 19) |
| `POST /v1/tirami/agent/task` | Синхронный dispatch PersonalAgent, автовыбор provider'а (Phase 18.5-pt3) |
| `GET /v1/tirami/agent/status` | Состояние PersonalAgent |
| `GET /v1/tirami/balance` | Баланс, репутация, история вклада |
| `GET /v1/tirami/pricing` | Рыночная цена (EMA), спрос/предложение |
| `GET /v1/tirami/trades` | Недавняя история trades |
| `GET /v1/tirami/peers` | Peer'ы с `http_endpoint` (Phase 19) |
| `GET /v1/tirami/providers` | Рейтинг provider'ов, скорректированный по репутации |
| `POST /v1/tirami/schedule` | Ledger-as-Brain probe (только чтение) |
| `GET /v1/tirami/su/supply` | Состояние tokenomics |
| `POST /v1/tirami/su/stake` | Заблокировать TRM ради yield |
| `POST /v1/tirami/governance/propose` | Governance-предложение (конституционные params автоотклоняются) |
| `POST /v1/tirami/lend` / `/borrow` / `/repay` | Кредитование |
| `GET /v1/tirami/slash-events` | История slashing (Phase 17 Wave 1.3) |
| `GET /metrics` | Prometheus (префикс `tirami_*`) |

---

## Безопасность

Пять защитных слоёв: **криптография** (Ed25519, nonce, HMAC, Noise) + **экономика** (slashing, welcome loan sunset, stake-required mining) + **эксплуатационный** (ASN rate limit, DDoS cap, checkpoint, fork detection) + **governance** (конституционные parameters, ProofPolicy ratchet) + **процессный** (kill switch, audit tier, репутационные штрафы). См. [`docs/threat-model.md`](../../../docs/threat-model.md) T1–T17.

---

## Идея

Ответ одной строкой на «зачем делать вычисления деньгами?»: **в эпоху AI по-настоящему дефицитный ресурс — это вычисления**. Tirami привязывает своё денежное определение к этому физическому факту (`1 TRM = 10⁹ FLOP`). См. [`docs/whitepaper.md`](../../../docs/whitepaper.md).

---

## Структура проекта

```
tirami/  (16 Rust crates, 5 слоёв)
├── crates/tirami-{ledger,node,cli,sdk,mcp,bank,mind,agora,anchor,lightning,net,proto,infer,core,shard,zkml-bench,attestation}
├── repos/tirami-contracts/  # Foundry TRM ERC-20 + TiramiBridge
├── scripts/verify-impl.sh   # 123 assertions
└── docs/                    # whitepaper, release-readiness и т.д.
```

~25 000 строк Rust. Phase 1-19 завершена.

---

## Экосистема

| Repo | Слой | Тесты | Состояние |
|---|---|---|---|
| [clearclown/tirami](https://github.com/clearclown/tirami) (этот репо) | L1-L4 | 1 192 | Phase 1-19 ✅ |
| [clearclown/tirami-economics](https://github.com/clearclown/tirami-economics) | Теория | 16/16 GREEN | §1-§18 + PDFs |
| [repos/tirami-contracts](https://github.com/clearclown/tirami/tree/main/repos/tirami-contracts) | on-chain | 15 forge tests | mainnet gated |
| [nm-arealnormalman/mesh-llm](https://github.com/nm-arealnormalman/mesh-llm) | L0 Inference | 646 | forge-economy port ✅ |

---

## Docs

- [Whitepaper](../../../docs/whitepaper.md) / [Release Readiness](../../../docs/release-readiness.md) / [Constitution](../../../docs/constitution.md) / [Killer-App](../../../docs/killer-app.md)
- [Public API Surface](../../../docs/public-api-surface.md) / [zkML Strategy](../../../docs/zkml-strategy.md) / [Strategy](../../../docs/strategy.md)
- [Economic Model](../../../docs/economy.md) / [Architecture](../../../docs/architecture.md) / [Wire Protocol](../../../docs/protocol-spec.md)
- [Threat Model](../../../docs/threat-model.md) / [Security Policy](../../../SECURITY.md) / [Operator Guide](../../../docs/operator-guide.md)
- [Developer Guide](../../../docs/developer-guide.md) / [FAQ](../../../docs/faq.md) / [Roadmap](../../../docs/roadmap.md)

---

## Лицензия

MIT. См. [`LICENSE`](../../../LICENSE).

## Это не инвестиция — отказ от ответственности за вторичный рынок

TRM — это **учёт вычислений**, не финансовый продукт. Maintainers не продают, не продвигают и не спекулируют TRM. Поскольку это MIT OSS, любой может — без ведома maintainers — bridge'ить, листить или деривировать TRM; технически невозможно это предотвратить. Те, кто решают держать или торговать TRM как store of value, принимают на себя все риски (юридические, регуляторные, counterparty, технические).

- Нет ICO, pre-sale, airdrop, private round
- Нет revenue share со сторонних рынков
- Развёртывание Base mainnet **под audit gate**

Полный текст: [`SECURITY.md`](../../../SECURITY.md#secondary-markets--third-party-tokenization).

## Благодарности

Распределённый инференс Tirami построен на [mesh-llm](https://github.com/michaelneale/mesh-llm) от Майкла Нила. См. [CREDITS.md](../../../CREDITS.md).
