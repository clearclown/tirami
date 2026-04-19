<div align="center" dir="rtl">

# Tirami

**الحوسبة هي المال. كل واط يُنتج ذكاءً لا نفايات.**

[![Crates.io](https://img.shields.io/crates/v/tirami-core?label=crates.io&color=e6522c)](https://crates.io/crates/tirami-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](../../../LICENSE)
[![Tests](https://img.shields.io/badge/tests-1192_passing-brightgreen)]()
[![verify-impl](https://img.shields.io/badge/verify--impl-123%2F123_GREEN-brightgreen)]()
[![foundry test](https://img.shields.io/badge/foundry_test-15%2F15_GREEN-brightgreen)]()
[![Phase](https://img.shields.io/badge/phase-19_hardened-blue)]()
[![Mainnet](https://img.shields.io/badge/mainnet-audit_gated-orange)]()

---

[English](../../../README.md) · [日本語](../ja/README.md) · [简体中文](../zh-CN/README.md) · [繁體中文](../zh-TW/README.md) · [Español](../es/README.md) · [Français](../fr/README.md) · [Русский](../ru/README.md) · [Українська](../uk/README.md) · [हिन्दी](../hi/README.md) · **العربية** · [فارسی](../fa/README.md) · [עברית](../he/README.md)

> النسخة القانونية هي [`README.md`](../../../README.md) باللغة الإنجليزية. قد تتأخر الترجمات.

</div>

<div dir="rtl">

**Tirami بروتوكول استدلال LLM موزّع حيث الحوسبة هي المال.** تكسب العقد TRM (Tirami Resource Merit) بتشغيل استدلال LLM مفيد لغيرها. على خلاف Bitcoin — الذي يحرق الكهرباء من أجل hashes بلا معنى — كل جول يُنفَق على عقدة Tirami يُنتج ذكاءً حقيقيًا يحتاجه شخص ما في تلك اللحظة.

محرك الاستدلال الموزّع مبني على [mesh-llm](https://github.com/michaelneale/mesh-llm) لـ Michael Neale. يضيف Tirami فوقه اقتصاد الحوسبة: محاسبة TRM، Proof of Useful Work، التسعير الديناميكي، ميزانيات الوكلاء المستقلين، ضوابط fail-safe. انظر [CREDITS.md](../../../CREDITS.md).

**Fork مُدمَج:** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — mesh-llm مع دمج طبقة Tirami الاقتصادية.

---

## ⚠️ Status Honesty (2026-04-19 / Phase 19)

قبل أي شيء آخر، هذا بالضبط **ما يعمل** و**ما لا يعمل**. Tirami برنامج مفتوح المصدر برخصة MIT، **ليس بيع رموز**. لا ICO، لا pre-mine، لا خزينة فريق، لا airdrop. TRM وحدة محاسبة للحوسبة (1 TRM = 10⁹ FLOP)، وليس منتجًا ماليًا — انظر [`SECURITY.md § Secondary Markets`](../../../SECURITY.md#secondary-markets--third-party-tokenization).

### ✅ يعمل اليوم (1,192 اختبار Rust + 15 اختبار Solidity، متحقَّق منها)

- دردشة متوافقة مع OpenAI عبر HTTP مع توجيه P2P تلقائي إلى peer متصل (`forward_chat_to_peer`، Phase 19).
- `SignedTradeRecord` مزدوج التوقيع عبر iroh-QUIC P2P مع حماية من إعادة التشغيل بـ nonce من 128 bit (`execute_signed_trade`).
- `TradeAcceptDispatcher` يوجّه رسائل التوقيع المقابل إلى مهمة الاستدلال قيد التنفيذ المقابلة (Phase 18.5-pt3).
- كاشف التواطؤ + حلقة slashing تعمل كل `slashing_interval_secs` (Phase 17 Wave 1.3).
- مقترحات الحوكمة مع whitelist قابلة للتعديل من 21 مدخلًا + قائمة دستورية ثابتة من 18 (Phase 18.1).
- Welcome loan، stake pool، مكافآت الإحالة، credit scoring، أسعار سوق ديناميكية (EMA smoothing).
- اكتشاف أقران تلقائي عبر `PriceSignal.http_endpoint` في تدفق gossip (Phase 19 Tier C).
- PersonalAgent يُهيَّأ تلقائيًا عند `tirami start` (Phase 18.5-pt3e)، مع مراقبة حلقة tick.
- Prometheus `/metrics` endpoint ببادئة `tirami_*`.
- `Makefile` لنشر Base Sepolia/mainnet — Sepolia مجاني، mainnet محمي ببوابة (انظر أدناه).

### 🟡 مُصمَّم لكنه غير موصول في الإنتاج

- إثبات zkML للاستدلال: `tirami-zkml-bench` يحوي `MockBackend` فقط. backends حقيقية (`ezkl` / `risc0`) في Phase 20+. `ProofPolicy = Optional` افتراضيًا (Phase 19) — trades بإثبات تحصل على مكافأة سمعة؛ بدونه تبقى صالحة.
- توقيعات ML-DSA (Dilithium) الهجينة ما بعد الكم: الـ struct ومسار verify موجودان، `Config::pq_signatures = false` افتراضيًا (محظور بسبب iroh 0.97).
- TEE attestation (Apple Secure Enclave / NVIDIA H100 CC): scaffold `tirami-attestation` فقط.
- Worker daemon gossip-recv loop ([issue #88](https://github.com/clearclown/tirami/issues/88)): `peer.url` اليدوي في `POST /v1/tirami/agent/task` لا يزال يعمل.

### ❌ لم يُنجَز (مطلوب قبل public mainnet)

- تدقيق أمني خارجي (متطلب Phase 17 Wave 3.3). المرشحون: Trail of Bits، Zellic، Open Zeppelin، Least Authority.
- نشر Base L2 mainnet. هدف `make deploy-base-mainnet` *يرفض* التنفيذ بدون `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER=<addr>` + إدخال `i-accept-responsibility` تفاعلي. انظر [`repos/tirami-contracts/Makefile`](../../../repos/tirami-contracts/Makefile).
- bug bounty إنتاجي بمفتاح PGP حقيقي (حاليًا placeholder موثق في [`SECURITY.md`](../../../SECURITY.md)).
- ≥ 30 يومًا تشغيل مستقر على Base Sepolia + ≥ 7 أيام stress test على testnet من 10+ عقد.

خريطة الطريق الكاملة حسب الـ tier: [`docs/release-readiness.md`](../../../docs/release-readiness.md).

---

## عرض حي

Tirami هو **Airbnb للـ GPU × اقتصاد وكلاء الذكاء الاصطناعي**: الحوسبة المتاحة تكسب إيجارًا بـ TRM؛ وكلاء الذكاء الاصطناعي هم المستأجرون.

</div>

```
$ tirami start
🔑 New key generated in ~/.tirami/node.key
📦 Qwen2.5-0.5B-Instruct GGUF fetched from HuggingFace
🚀 HTTP API at http://127.0.0.1:3000
✅ Personal agent configured for <wallet-hex>
✅ P2P endpoint bound (iroh QUIC + Noise)
```

<div dir="rtl">

### Phase 19 Tier C/D enablers

</div>

```bash
tirami agent status
tirami agent chat "لخّص هذه الورقة" --max-tokens 256

# Worker بدون نموذج محلي يُحوّل إلى seed
curl -X POST http://worker.local:3111/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello"}],"max_tokens":5}'

# اكتشاف الأقران التلقائي
curl http://127.0.0.1:3001/v1/tirami/peers | jq '.peers[].http_endpoint'

# نشر mainnet بالبوابة
cd repos/tirami-contracts && make help
```

<div dir="rtl">

---

## لماذا Tirami

### 1. الحوسبة = المال (سقف إمداد 21B TRM)

لـ TRM سقف إمداد دستوري ثابت عند 21,000,000,000. لا يمكن لأي مقترح حوكمة تغييره (Phase 18.1، في `IMMUTABLE_CONSTITUTIONAL_PARAMETERS`). تغييره يتطلب fork للبرنامج — وفي تلك اللحظة لم يعد «Tirami».

### 2. مقاومة العبث بدون blockchain

كل trade محمي بتوقيع Ed25519 مزدوج (provider + consumer) + nonce من 128 bit (anti-replay) + نشر gossip + anchor دوري لـ Merkle root على السلسلة.

### 3. وكلاء الذكاء الاصطناعي يديرون ميزانية حوسبتهم

`PersonalAgent` (Phase 18.5) هو الطيار الآلي الذي يشتري ويبيع الحوسبة في mesh بالنيابة عن المستخدم. `tirami agent chat "..."` يختار تلقائيًا local أو remote.

### 4. تمويل أصغر للحوسبة

`welcome_loan = 1,000 TRM` (72 ساعة، معدل 0٪) للـ bootstrap. ينتهي welcome loan نهائيًا في epoch 2 (دستوري)، ويهاجر مسار الدخول إلى stake-required mining (Phase 18.2).

### 5. Ledger-as-Brain: الجدولة = قرار اقتصادي

`PeerRegistry` + `select_provider` يحوّلان كل طلب استدلال إلى قرار اقتصادي (سمعة موزونة بكاشف التواطؤ + audit tier + slashing).

---

## معمارية من 5 طبقات

</div>

```
L4: Discovery (tirami-agora)       سوق الوكلاء، السمعة، NIP-90
L3: Intelligence (tirami-mind)     PersonalAgent، التحسّن الذاتي، ميزانية TRM
L2: Finance (tirami-bank)          استراتيجيات، محافظ، futures، تأمين
L1: Economy (هذا المستودع) ✅      Phase 1-19 مكتملة
L0: Inference (forge-mesh) ✅      استدلال LLM موزّع، llama.cpp
                                   ↓ Phase 16: batches من 10 دقائق
On-chain: tirami-contracts (Base L2, gated)
  TRM ERC-20 (cap 21B) + TiramiBridge
```

<div dir="rtl">

جميع الطبقات الخمس بـ Rust، 16 workspace crates. **1,192 اختبار ينجح** + 15 Solidity.

---

## بداية سريعة

</div>

```bash
# الخيار 1: عرض E2E بأمر واحد
git clone https://github.com/clearclown/tirami && cd tirami
bash scripts/demo-e2e.sh

# الخيار 2: تشغيل مباشر
cargo build --release
./target/release/tirami start -m "qwen2.5:0.5b"

# الخيار 3: كعميل OpenAI
export OPENAI_BASE_URL=http://127.0.0.1:3001/v1
curl $OPENAI_BASE_URL/tirami/balance
```

<div dir="rtl">

---

## مرجع API

| Endpoint | الوصف |
|---|---|
| `POST /v1/chat/completions` | دردشة متوافقة مع OpenAI. رد يتضمن `x_tirami.trm_cost`. توجيه P2P إن لم يوجد نموذج محلي (`forward_chat_to_peer`، Phase 19) |
| `POST /v1/tirami/agent/task` | dispatch متزامن لـ PersonalAgent، اختيار provider تلقائي (Phase 18.5-pt3) |
| `GET /v1/tirami/agent/status` | حالة PersonalAgent |
| `GET /v1/tirami/balance` | الرصيد، السمعة، سجل المساهمة |
| `GET /v1/tirami/pricing` | سعر السوق (EMA)، العرض/الطلب |
| `GET /v1/tirami/trades` | سجل trades حديث |
| `GET /v1/tirami/peers` | الأقران مع `http_endpoint` (Phase 19) |
| `GET /v1/tirami/providers` | ترتيب providers معدَّل بالسمعة |
| `POST /v1/tirami/schedule` | فحص Ledger-as-Brain (قراءة فقط) |
| `GET /v1/tirami/su/supply` | حالة الـ tokenomics |
| `POST /v1/tirami/su/stake` | قفل TRM لأجل yield |
| `POST /v1/tirami/governance/propose` | مقترح حوكمة (params دستورية ترفض تلقائيًا) |
| `POST /v1/tirami/lend` / `/borrow` / `/repay` | الإقراض |
| `GET /v1/tirami/slash-events` | سجل slashing (Phase 17 Wave 1.3) |
| `GET /metrics` | Prometheus (بادئة `tirami_*`) |

---

## الأمن

خمس طبقات دفاع: **تشفير** (Ed25519، nonce، HMAC، Noise) + **اقتصاد** (slashing، welcome loan sunset، stake-required mining) + **تشغيلي** (rate limit لكل ASN، DDoS cap، checkpoint، fork detection) + **حوكمة** (parameters دستورية، ProofPolicy ratchet) + **عملية** (kill switch، audit tier، عقوبات السمعة). انظر [`docs/threat-model.md`](../../../docs/threat-model.md) T1–T17.

---

## الفكرة

جواب من سطر واحد على «لماذا نجعل الحوسبة عملة؟»: **في عصر الذكاء الاصطناعي، المورد النادر حقًا هو الحوسبة**. يربط Tirami تعريفه النقدي بهذه الحقيقة الفيزيائية (`1 TRM = 10⁹ FLOP`). انظر [`docs/whitepaper.md`](../../../docs/whitepaper.md).

---

## بنية المشروع

</div>

```
tirami/  (16 Rust crates, 5 طبقات)
├── crates/tirami-{ledger,node,cli,sdk,mcp,bank,mind,agora,anchor,lightning,net,proto,infer,core,shard,zkml-bench,attestation}
├── repos/tirami-contracts/  # Foundry TRM ERC-20 + TiramiBridge
├── scripts/verify-impl.sh   # 123 assertions
└── docs/                    # whitepaper, release-readiness, إلخ
```

<div dir="rtl">

~25,000 سطر Rust. Phase 1-19 مكتملة.

---

## النظام البيئي

| Repo | الطبقة | اختبارات | الحالة |
|---|---|---|---|
| [clearclown/tirami](https://github.com/clearclown/tirami) (هذا المستودع) | L1-L4 | 1,192 | Phase 1-19 ✅ |
| [clearclown/tirami-economics](https://github.com/clearclown/tirami-economics) | نظرية | 16/16 GREEN | §1-§18 + PDFs |
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

## الترخيص

MIT. انظر [`LICENSE`](../../../LICENSE).

## هذا ليس استثمارًا — إخلاء مسؤولية عن السوق الثانوية

TRM هو **محاسبة حوسبة**، وليس منتجًا ماليًا. المشرفون لا يبيعون TRM ولا يروّجون له ولا يضاربون عليه. بما أنه MIT OSS، يمكن لأي شخص — بدون علم المشرفين — bridge أو list أو اشتقاق TRM؛ من المستحيل تقنيًا منع ذلك. من يختار الاحتفاظ بـ TRM أو تداوله كمخزن للقيمة يتحمل بنفسه جميع المخاطر (قانونية، تنظيمية، طرف مقابل، تقنية).

- لا ICO، pre-sale، airdrop، private round
- لا revenue share من أسواق طرف ثالث
- نشر Base mainnet **تحت audit gate**

النص الكامل: [`SECURITY.md`](../../../SECURITY.md#secondary-markets--third-party-tokenization).

## شكر وتقدير

استدلال Tirami الموزّع مبني على [mesh-llm](https://github.com/michaelneale/mesh-llm) لـ Michael Neale. انظر [CREDITS.md](../../../CREDITS.md).

</div>
