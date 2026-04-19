<div align="center">

# Tirami

**Обчислення — це гроші. Кожен ват породжує інтелект, а не марнотратство.**

[![Crates.io](https://img.shields.io/crates/v/tirami-core?label=crates.io&color=e6522c)](https://crates.io/crates/tirami-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](../../../LICENSE)
[![Tests](https://img.shields.io/badge/tests-1192_passing-brightgreen)]()
[![verify-impl](https://img.shields.io/badge/verify--impl-123%2F123_GREEN-brightgreen)]()
[![foundry test](https://img.shields.io/badge/foundry_test-15%2F15_GREEN-brightgreen)]()
[![Phase](https://img.shields.io/badge/phase-19_hardened-blue)]()
[![Mainnet](https://img.shields.io/badge/mainnet-audit_gated-orange)]()

---

[English](../../../README.md) · [日本語](../ja/README.md) · [简体中文](../zh-CN/README.md) · [繁體中文](../zh-TW/README.md) · [Español](../es/README.md) · [Français](../fr/README.md) · [Русский](../ru/README.md) · **Українська** · [हिन्दी](../hi/README.md) · [العربية](../ar/README.md) · [فارسی](../fa/README.md) · [עברית](../he/README.md)

> Канонічна версія — англомовний [`README.md`](../../../README.md). Переклади можуть відставати.

</div>

**Tirami — це розподілений протокол LLM-висновування, де обчислення є грошима.** Вузли заробляють TRM (Tirami Resource Merit), виконуючи корисні обчислення для інших. На відміну від Bitcoin, який спалює електрику заради безглуздих хешів, кожен джоуль, витрачений у Tirami, породжує реальний інтелект, потрібний комусь у цей момент.

Рушій розподіленого висновування побудовано на [mesh-llm](https://github.com/michaelneale/mesh-llm) від Майкла Ніла. Tirami надбудовує над ним економіку обчислень: TRM-бухгалтерія, Proof of Useful Work, динамічне ціноутворення, автономні бюджети агентів, fail-safe контролі. Див. [CREDITS.md](../../../CREDITS.md).

**Інтегрований fork:** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — mesh-llm з вбудованим економічним шаром Tirami.

---

## ⚠️ Status Honesty (2026-04-19 / Phase 19)

Перш ніж щось інше, ось точно те, **що працює** і **що не працює**. Tirami — це open-source програмне забезпечення під ліцензією MIT, **не продаж токенів**. Немає ICO, pre-mine, командної скарбниці чи airdrop. TRM — одиниця обліку обчислень (1 TRM = 10⁹ FLOP), а не фінансовий продукт — див. [`SECURITY.md § Secondary Markets`](../../../SECURITY.md#secondary-markets--third-party-tokenization).

### ✅ Працює сьогодні (1 192 Rust тести + 15 Solidity тестів, перевірено)

- OpenAI-сумісний чат через HTTP з автоматичним P2P-forwarding до підключеного пера (`forward_chat_to_peer`, Phase 19).
- `SignedTradeRecord` з подвійним підписом через iroh-QUIC P2P із захистом від повторного відтворення через 128-бітний nonce (`execute_signed_trade`).
- `TradeAcceptDispatcher` маршрутизує повідомлення контрпідпису до відповідної активної задачі висновування (Phase 18.5-pt3).
- Детектор змови + цикл slashing, що крутиться кожні `slashing_interval_secs` (Phase 17 Wave 1.3).
- Governance-пропозиції зі списком з 21 змінного параметра + 18 незмінних конституційних параметрів (Phase 18.1).
- Welcome loan, stake pool, реферальні бонуси, кредитний скоринг, динамічні ринкові ціни (згладжені EMA).
- Автовиявлення пірів через `PriceSignal.http_endpoint` у gossip-потоці (Phase 19 Tier C).
- PersonalAgent автоматично налаштовується при `tirami start` (Phase 18.5-pt3e) зі спостереженням за tick-циклом.
- Prometheus `/metrics` endpoint з префіксом `tirami_*`.
- `Makefile` для розгортання Base Sepolia/mainnet — Sepolia безкоштовний, mainnet під ключем (див. нижче).

### 🟡 Спроектовано, але не підключено до production

- zkML-докази висновування: `tirami-zkml-bench` має лише `MockBackend`. Реальні backend `ezkl` / `risc0` з'являться у Phase 20+. `ProofPolicy = Optional` за замовчуванням (Phase 19) — trades з доказом отримують репутаційний бонус; без доказу лишаються дійсними.
- Гібридні постквантові підписи ML-DSA (Dilithium): struct та шлях verify існують, `Config::pq_signatures = false` за замовчуванням (заблоковано iroh 0.97).
- TEE-атестація (Apple Secure Enclave / NVIDIA H100 CC): лише scaffold `tirami-attestation`.
- Worker gossip-recv loop ([issue #88](https://github.com/clearclown/tirami/issues/88)): ручний `peer.url` у `POST /v1/tirami/agent/task` усе ще працює.

### ❌ Не зроблено (потрібно до public mainnet)

- Зовнішній аудит безпеки (вимога Phase 17 Wave 3.3). Кандидати: Trail of Bits, Zellic, Open Zeppelin, Least Authority.
- Розгортання Base L2 mainnet. Ціль `make deploy-base-mainnet` *відмовляється* виконуватись без `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER=<addr>` + інтерактивного вводу `i-accept-responsibility`. Див. [`repos/tirami-contracts/Makefile`](../../../repos/tirami-contracts/Makefile).
- Bug bounty у production зі справжнім PGP-ключем (наразі placeholder, задокументований у [`SECURITY.md`](../../../SECURITY.md)).
- ≥ 30 днів стабільної роботи на Base Sepolia + ≥ 7 днів стрес-тесту на testnet із 10+ вузлами.

Повна roadmap за рівнями: [`docs/release-readiness.md`](../../../docs/release-readiness.md).

---

## Live-демо

Tirami — це **Airbnb для GPU × економіка AI-агентів**: вільні обчислення заробляють оренду в TRM; AI-агенти — це орендарі.

```
$ tirami start
🔑 Новий ключ згенеровано у ~/.tirami/node.key
📦 Qwen2.5-0.5B-Instruct GGUF отримано з HuggingFace
🚀 HTTP API at http://127.0.0.1:3000
✅ Personal agent configured for <wallet-hex>
✅ P2P endpoint bound (iroh QUIC + Noise)
```

### Enablers Phase 19 Tier C/D

```bash
tirami agent status
tirami agent chat "Зроби резюме статті" --max-tokens 256

# Worker без локальної моделі переправляє до seed
curl -X POST http://worker.local:3111/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello"}],"max_tokens":5}'

# Автовиявлення пірів
curl http://127.0.0.1:3001/v1/tirami/peers | jq '.peers[].http_endpoint'

# Розгортання mainnet із gate
cd repos/tirami-contracts && make help
```

---

## Чому Tirami

### 1. Обчислення = гроші (ліміт емісії 21B TRM)

TRM має конституційний ліміт емісії у 21 000 000 000 одиниць. Жодна governance-пропозиція не може його змінити (Phase 18.1, `IMMUTABLE_CONSTITUTIONAL_PARAMETERS`). Змінити його можна лише форком ПЗ — і в цю саму мить це вже не «Tirami».

### 2. Стійкість до маніпуляцій без блокчейну

Кожен trade захищений подвійним підписом Ed25519 (provider + consumer) + 128-бітний nonce (анти-replay) + gossip-поширенням + періодичним anchor Merkle root on-chain.

### 3. AI-агенти керують власним обчислювальним бюджетом

`PersonalAgent` (Phase 18.5) — це автопілот, який купує та продає обчислення в mesh від імені користувача. `tirami agent chat "..."` автоматично обирає local або remote.

### 4. Мікрофінанси обчислень

`welcome_loan = 1 000 TRM` (72 год, ставка 0%) для bootstrap. Welcome loan зупиняється остаточно у epoch 2 (Constitutional), вхідний шлях мігрує до stake-required mining (Phase 18.2).

### 5. Ledger-as-Brain: розклад = економічне рішення

`PeerRegistry` + `select_provider` перетворюють кожен inference-запит на економічне рішення (репутація, зважена на детектор змови + audit tier + slashing).

---

## Архітектура з 5 шарів

```
L4: Discovery (tirami-agora)       Маркетплейс агентів, репутація, NIP-90
L3: Intelligence (tirami-mind)     PersonalAgent, самополіпшення, TRM-бюджет
L2: Finance (tirami-bank)          Стратегії, портфелі, ф'ючерси, страхування
L1: Economy (цей репо) ✅          Phase 1-19 завершено
L0: Inference (forge-mesh) ✅      Розподілене LLM-висновування, llama.cpp
                                   ↓ Phase 16: батчі по 10 хв
On-chain: tirami-contracts (Base L2, gated)
  TRM ERC-20 (cap 21B) + TiramiBridge
```

Усі 5 шарів на Rust, 16 workspace crates. **1 192 тести проходять** + 15 Solidity.

---

## Швидкий старт

```bash
# Option 1: E2E-демо однією командою
git clone https://github.com/clearclown/tirami && cd tirami
bash scripts/demo-e2e.sh

# Option 2: прямий запуск
cargo build --release
./target/release/tirami start -m "qwen2.5:0.5b"

# Option 3: як клієнт OpenAI
export OPENAI_BASE_URL=http://127.0.0.1:3001/v1
curl $OPENAI_BASE_URL/tirami/balance
```

---

## Довідник API

| Endpoint | Опис |
|---|---|
| `POST /v1/chat/completions` | OpenAI-сумісний чат. Відповідь з `x_tirami.trm_cost`. P2P-forwarding, якщо немає локальної моделі (`forward_chat_to_peer`, Phase 19) |
| `POST /v1/tirami/agent/task` | Синхронний dispatch PersonalAgent, автовибір provider (Phase 18.5-pt3) |
| `GET /v1/tirami/agent/status` | Стан PersonalAgent |
| `GET /v1/tirami/balance` | Баланс, репутація, історія внесків |
| `GET /v1/tirami/pricing` | Ринкова ціна (EMA), попит/пропозиція |
| `GET /v1/tirami/trades` | Нещодавня історія trades |
| `GET /v1/tirami/peers` | Пери з `http_endpoint` (Phase 19) |
| `GET /v1/tirami/providers` | Рейтинг providers з корекцією на репутацію |
| `POST /v1/tirami/schedule` | Ledger-as-Brain зонд (лише читання) |
| `GET /v1/tirami/su/supply` | Стан tokenomics |
| `POST /v1/tirami/su/stake` | Заблокувати TRM для yield |
| `POST /v1/tirami/governance/propose` | Governance-пропозиція (конституційні params авторегект) |
| `POST /v1/tirami/lend` / `/borrow` / `/repay` | Кредитування |
| `GET /v1/tirami/slash-events` | Історія slashing (Phase 17 Wave 1.3) |
| `GET /metrics` | Prometheus (префікс `tirami_*`) |

---

## Безпека

П'ять шарів захисту: **криптографія** (Ed25519, nonce, HMAC, Noise) + **економіка** (slashing, welcome loan sunset, stake-required mining) + **операційний** (ASN rate limit, DDoS cap, checkpoint, fork detection) + **governance** (конституційні параметри, ProofPolicy ratchet) + **процесний** (kill switch, audit tier, репутаційні штрафи). Див. [`docs/threat-model.md`](../../../docs/threat-model.md) T1–T17.

---

## Ідея

Відповідь у один рядок на «навіщо робити обчислення грошима?»: **в епоху AI по-справжньому рідкісний ресурс — це обчислення**. Tirami прив'язує своє грошове визначення до цього фізичного факту (`1 TRM = 10⁹ FLOP`). Див. [`docs/whitepaper.md`](../../../docs/whitepaper.md).

---

## Структура проекту

```
tirami/  (16 Rust crates, 5 шарів)
├── crates/tirami-{ledger,node,cli,sdk,mcp,bank,mind,agora,anchor,lightning,net,proto,infer,core,shard,zkml-bench,attestation}
├── repos/tirami-contracts/  # Foundry TRM ERC-20 + TiramiBridge
├── scripts/verify-impl.sh   # 123 assertions
└── docs/                    # whitepaper, release-readiness, тощо
```

~25 000 рядків Rust. Phase 1-19 завершено.

---

## Екосистема

| Repo | Шар | Тести | Стан |
|---|---|---|---|
| [clearclown/tirami](https://github.com/clearclown/tirami) (цей репо) | L1-L4 | 1 192 | Phase 1-19 ✅ |
| [clearclown/tirami-economics](https://github.com/clearclown/tirami-economics) | Теорія | 16/16 GREEN | §1-§18 + PDFs |
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

## Ліцензія

MIT. Див. [`LICENSE`](../../../LICENSE).

## Це не інвестиція — застереження про вторинний ринок

TRM — це **бухгалтерія обчислень**, не фінансовий продукт. Maintainers не продають, не просувають і не спекулюють TRM. Оскільки це MIT OSS, будь-хто може — без відома maintainers — bridge'ити, листити або деривувати TRM; технічно неможливо цьому запобігти. Ті, хто обирає тримати чи торгувати TRM як засіб заощадження, приймають на себе всі ризики (юридичні, регуляторні, контрагента, технічні).

- Немає ICO, pre-sale, airdrop, private round
- Немає revenue share з третіх ринків
- Розгортання mainnet на Base **під audit gate**

Повний текст: [`SECURITY.md`](../../../SECURITY.md#secondary-markets--third-party-tokenization).

## Подяки

Розподілене висновування Tirami побудовано на [mesh-llm](https://github.com/michaelneale/mesh-llm) від Майкла Ніла. Див. [CREDITS.md](../../../CREDITS.md).
