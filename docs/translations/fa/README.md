<div align="center" dir="rtl">

# Tirami

**محاسبات همان پول است. هر وات به جای اتلاف، هوش تولید می‌کند.**

[![Crates.io](https://img.shields.io/crates/v/tirami-core?label=crates.io&color=e6522c)](https://crates.io/crates/tirami-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](../../../LICENSE)
[![Tests](https://img.shields.io/badge/tests-1192_passing-brightgreen)]()
[![verify-impl](https://img.shields.io/badge/verify--impl-123%2F123_GREEN-brightgreen)]()
[![foundry test](https://img.shields.io/badge/foundry_test-15%2F15_GREEN-brightgreen)]()
[![Phase](https://img.shields.io/badge/phase-19_hardened-blue)]()
[![Mainnet](https://img.shields.io/badge/mainnet-audit_gated-orange)]()

---

[English](../../../README.md) · [日本語](../ja/README.md) · [简体中文](../zh-CN/README.md) · [繁體中文](../zh-TW/README.md) · [Español](../es/README.md) · [Français](../fr/README.md) · [Русский](../ru/README.md) · [Українська](../uk/README.md) · [हिन्दी](../hi/README.md) · [العربية](../ar/README.md) · **فارسی** · [עברית](../he/README.md)

> نسخهٔ قانونی، [`README.md`](../../../README.md) انگلیسی است. ترجمه‌ها ممکن است عقب باشند.

</div>

<div dir="rtl">

**Tirami یک پروتکل استنتاج LLM توزیع‌شده است که در آن محاسبه همان پول است.** گره‌ها با اجرای استنتاج LLM مفید برای دیگران، TRM (Tirami Resource Merit) کسب می‌کنند. برخلاف Bitcoin — که برق را برای hashهای بی‌معنا می‌سوزاند — هر ژولی که روی یک گرهٔ Tirami خرج می‌شود، هوش واقعی تولید می‌کند که کسی در همان لحظه به آن نیاز دارد.

موتور استنتاج توزیع‌شده روی [mesh-llm](https://github.com/michaelneale/mesh-llm) از Michael Neale ساخته شده است. Tirami روی آن اقتصاد محاسبات را می‌افزاید: حسابداری TRM، Proof of Useful Work، قیمت‌گذاری پویا، بودجهٔ عامل‌های خودمختار، کنترل‌های fail-safe. به [CREDITS.md](../../../CREDITS.md) مراجعه کنید.

**Fork یکپارچه:** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — mesh-llm با لایهٔ اقتصادی Tirami تعبیه‌شده.

---

## ⚠️ Status Honesty (2026-04-19 / Phase 19)

پیش از هر چیز، این دقیقاً همان چیزی است که **کار می‌کند** و **کار نمی‌کند**. Tirami نرم‌افزار open-source تحت مجوز MIT است، **نه فروش توکن**. نه ICO، نه pre-mine، نه خزانهٔ تیمی، نه airdrop. TRM یک واحد حسابداری محاسبات است (1 TRM = 10⁹ FLOP)، نه محصول مالی — به [`SECURITY.md § Secondary Markets`](../../../SECURITY.md#secondary-markets--third-party-tokenization) مراجعه کنید.

### ✅ امروز کار می‌کند (1,192 تست Rust + 15 تست Solidity، تأیید شده)

- چت سازگار با OpenAI روی HTTP با forwarding P2P خودکار به peerِ متصل (`forward_chat_to_peer`، Phase 19).
- `SignedTradeRecord` با امضای دوگانه روی iroh-QUIC P2P با محافظت anti-replay با nonce ۱۲۸ بیتی (`execute_signed_trade`).
- `TradeAcceptDispatcher` پیام‌های ضدامضا را به وظیفهٔ استنتاج در حال اجرای متناظر می‌رساند (Phase 18.5-pt3).
- آشکارساز تبانی + حلقهٔ slashing که هر `slashing_interval_secs` اجرا می‌شود (Phase 17 Wave 1.3).
- پیشنهادهای governance با whitelist قابل‌تغییر ۲۱ ورودی + فهرست قانون‌اساسی تغییرناپذیر ۱۸ ورودی (Phase 18.1).
- Welcome loan، stake pool، پاداش‌های ارجاع، credit scoring، قیمت‌های بازار پویا (EMA smoothing).
- کشف خودکار peerها از طریق `PriceSignal.http_endpoint` در جریان gossip (Phase 19 Tier C).
- PersonalAgent به‌طور خودکار در `tirami start` پیکربندی می‌شود (Phase 18.5-pt3e)، همراه با رصد tick loop.
- endpoint `/metrics` Prometheus با پیشوند `tirami_*`.
- `Makefile` برای استقرار Base Sepolia/mainnet — Sepolia رایگان، mainnet gate-شده (پایین‌تر ببینید).

### 🟡 طراحی‌شده اما در تولید سیم‌کشی نشده

- اثبات zkML استنتاج: `tirami-zkml-bench` تنها `MockBackend` دارد. backendهای واقعی `ezkl` / `risc0` در Phase 20+ خواهند آمد. `ProofPolicy = Optional` به‌صورت پیش‌فرض (Phase 19) — tradeهای دارای اثبات پاداش reputation می‌گیرند؛ بدون اثبات همچنان معتبرند.
- امضاهای hybrid پساکوانتومی ML-DSA (Dilithium): struct و مسیر verify وجود دارند، `Config::pq_signatures = false` به‌صورت پیش‌فرض (به‌خاطر iroh 0.97 مسدود).
- TEE attestation (Apple Secure Enclave / NVIDIA H100 CC): تنها scaffold `tirami-attestation`.
- Worker daemon gossip-recv loop ([issue #88](https://github.com/clearclown/tirami/issues/88)): `peer.url` دستی در `POST /v1/tirami/agent/task` همچنان کار می‌کند.

### ❌ انجام نشده (پیش از public mainnet لازم)

- ممیزی امنیتی خارجی (الزام Phase 17 Wave 3.3). نامزدها: Trail of Bits، Zellic، Open Zeppelin، Least Authority.
- استقرار Base L2 mainnet. هدف `make deploy-base-mainnet` بدون `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER=<addr>` + ورودی تعاملی `i-accept-responsibility` از اجرا *سر باز می‌زند*. به [`repos/tirami-contracts/Makefile`](../../../repos/tirami-contracts/Makefile) مراجعه کنید.
- bug bounty تولیدی با کلید PGP واقعی (در حال حاضر placeholder مستند در [`SECURITY.md`](../../../SECURITY.md)).
- ≥ ۳۰ روز عملیات پایدار روی Base Sepolia + ≥ ۷ روز stress test روی testnet با ۱۰+ گره.

نقشه‌راه کامل بر اساس tier: [`docs/release-readiness.md`](../../../docs/release-readiness.md).

---

## دموی زنده

Tirami یعنی **Airbnb برای GPU × اقتصاد عامل‌های AI**: محاسبات در دسترس اجارهٔ TRM می‌گیرند؛ عامل‌های AI مستأجرند.

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
tirami agent chat "این مقاله را خلاصه کن" --max-tokens 256

# Worker بدون مدل محلی به seed فوروارد می‌کند
curl -X POST http://worker.local:3111/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello"}],"max_tokens":5}'

# کشف خودکار peerها
curl http://127.0.0.1:3001/v1/tirami/peers | jq '.peers[].http_endpoint'

# استقرار mainnet با gate
cd repos/tirami-contracts && make help
```

<div dir="rtl">

---

## چرا Tirami

### 1. محاسبه = پول (سقف عرضهٔ 21B TRM)

TRM سقف عرضهٔ قانون‌اساسی ثابتی در 21,000,000,000 دارد. هیچ پیشنهاد governance نمی‌تواند آن را تغییر دهد (Phase 18.1، در `IMMUTABLE_CONSTITUTIONAL_PARAMETERS`). تغییر آن نیازمند fork نرم‌افزاری است — و در همان لحظه دیگر «Tirami» نیست.

### 2. مقاوم در برابر دستکاری بدون blockchain

هر trade با امضای دوگانهٔ Ed25519 (provider + consumer) + nonce ۱۲۸ بیتی (anti-replay) + انتشار gossip + anchor دوره‌ای Merkle root on-chain محافظت می‌شود.

### 3. عامل‌های AI بودجهٔ محاسباتی خود را مدیریت می‌کنند

`PersonalAgent` (Phase 18.5) یک خلبان خودکار است که به نمایندگی از کاربر در mesh محاسبه می‌خرد و می‌فروشد. `tirami agent chat "..."` به‌طور خودکار local یا remote را انتخاب می‌کند.

### 4. مایکروفاینانس محاسبات

`welcome_loan = 1,000 TRM` (۷۲ ساعت، نرخ ۰٪) برای bootstrap. Welcome loan در epoch 2 به‌طور دائمی متوقف می‌شود (Constitutional)، مسیر ورود به stake-required mining مهاجرت می‌کند (Phase 18.2).

### 5. Ledger-as-Brain: زمان‌بندی = تصمیم اقتصادی

`PeerRegistry` + `select_provider` هر درخواست استنتاج را به یک تصمیم اقتصادی تبدیل می‌کنند (reputation وزن‌دار با آشکارساز تبانی + audit tier + slashing).

---

## معماری ۵ لایه

</div>

```
L4: Discovery (tirami-agora)       بازار عامل‌ها، reputation، NIP-90
L3: Intelligence (tirami-mind)     PersonalAgent، بهبود خودکار، بودجهٔ TRM
L2: Finance (tirami-bank)          استراتژی‌ها، پورتفولیو، futures، بیمه
L1: Economy (این مخزن) ✅           Phase 1-19 کامل
L0: Inference (forge-mesh) ✅      استنتاج LLM توزیع‌شده، llama.cpp
                                   ↓ Phase 16: batchهای ۱۰ دقیقه‌ای
On-chain: tirami-contracts (Base L2, gated)
  TRM ERC-20 (cap 21B) + TiramiBridge
```

<div dir="rtl">

هر ۵ لایه در Rust، ۱۶ workspace crate. **۱٬۱۹۲ تست پاس** + ۱۵ Solidity.

---

## شروع سریع

</div>

```bash
# گزینه ۱: دموی E2E در یک فرمان
git clone https://github.com/clearclown/tirami && cd tirami
bash scripts/demo-e2e.sh

# گزینه ۲: راه‌اندازی مستقیم
cargo build --release
./target/release/tirami start -m "qwen2.5:0.5b"

# گزینه ۳: به‌عنوان کلاینت OpenAI
export OPENAI_BASE_URL=http://127.0.0.1:3001/v1
curl $OPENAI_BASE_URL/tirami/balance
```

<div dir="rtl">

---

## مرجع API

| Endpoint | توضیح |
|---|---|
| `POST /v1/chat/completions` | چت سازگار با OpenAI. پاسخ شامل `x_tirami.trm_cost`. forwarding P2P در صورت نبود مدل محلی (`forward_chat_to_peer`، Phase 19) |
| `POST /v1/tirami/agent/task` | dispatch همگام PersonalAgent، انتخاب خودکار provider (Phase 18.5-pt3) |
| `GET /v1/tirami/agent/status` | وضعیت PersonalAgent |
| `GET /v1/tirami/balance` | موجودی، reputation، تاریخچهٔ مشارکت |
| `GET /v1/tirami/pricing` | قیمت بازار (EMA)، عرضه/تقاضا |
| `GET /v1/tirami/trades` | تاریخچهٔ اخیر tradeها |
| `GET /v1/tirami/peers` | peerها با `http_endpoint` (Phase 19) |
| `GET /v1/tirami/providers` | رتبه‌بندی providerها تعدیل‌شده با reputation |
| `POST /v1/tirami/schedule` | probe Ledger-as-Brain (فقط خواندنی) |
| `GET /v1/tirami/su/supply` | وضعیت tokenomics |
| `POST /v1/tirami/su/stake` | قفل‌کردن TRM برای yield |
| `POST /v1/tirami/governance/propose` | پیشنهاد governance (paramهای قانون‌اساسی خودکار رد می‌شوند) |
| `POST /v1/tirami/lend` / `/borrow` / `/repay` | وام‌دهی |
| `GET /v1/tirami/slash-events` | تاریخچهٔ slashing (Phase 17 Wave 1.3) |
| `GET /metrics` | Prometheus (پیشوند `tirami_*`) |

---

## امنیت

پنج لایهٔ دفاعی: **رمزنگاری** (Ed25519، nonce، HMAC، Noise) + **اقتصاد** (slashing، welcome loan sunset، stake-required mining) + **عملیاتی** (rate limit به ازای ASN، DDoS cap، checkpoint، fork detection) + **governance** (parameterهای قانون‌اساسی، ratchet ProofPolicy) + **فرآیندی** (kill switch، audit tier، جریمه‌های reputation). به [`docs/threat-model.md`](../../../docs/threat-model.md) T1–T17 مراجعه کنید.

---

## ایده

پاسخ یک‌خطی به «چرا محاسبه را پول کنیم؟»: **در عصر AI، منبع واقعاً کمیاب محاسبه است**. Tirami تعریف پولی خود را به این واقعیت فیزیکی گره می‌زند (`1 TRM = 10⁹ FLOP`). به [`docs/whitepaper.md`](../../../docs/whitepaper.md) مراجعه کنید.

---

## ساختار پروژه

</div>

```
tirami/  (16 Rust crates, ۵ لایه)
├── crates/tirami-{ledger,node,cli,sdk,mcp,bank,mind,agora,anchor,lightning,net,proto,infer,core,shard,zkml-bench,attestation}
├── repos/tirami-contracts/  # Foundry TRM ERC-20 + TiramiBridge
├── scripts/verify-impl.sh   # 123 assertions
└── docs/                    # whitepaper، release-readiness و غیره
```

<div dir="rtl">

~۲۵٬۰۰۰ سطر Rust. Phase 1-19 کامل.

---

## اکوسیستم

| Repo | لایه | تست‌ها | وضعیت |
|---|---|---|---|
| [clearclown/tirami](https://github.com/clearclown/tirami) (این مخزن) | L1-L4 | ۱٬۱۹۲ | Phase 1-19 ✅ |
| [clearclown/tirami-economics](https://github.com/clearclown/tirami-economics) | نظری | 16/16 GREEN | §1-§18 + PDFs |
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

## مجوز

MIT. به [`LICENSE`](../../../LICENSE) مراجعه کنید.

## این سرمایه‌گذاری نیست — سلب مسئولیت بازار ثانویه

TRM **حسابداری محاسبات** است، نه محصول مالی. Maintainerها TRM را نمی‌فروشند، تبلیغ نمی‌کنند و روی آن سفته‌بازی نمی‌کنند. چون MIT OSS است، هرکس می‌تواند — بدون اطلاع maintainerها — TRM را bridge، list یا مشتق‌گیری کند؛ از نظر فنی جلوگیری ناممکن است. کسانی که انتخاب می‌کنند TRM را به‌عنوان ذخیرهٔ ارزش نگه دارند یا معامله کنند، تمام ریسک‌ها (حقوقی، قانونی، counterparty، فنی) را خود بر عهده می‌گیرند.

- نه ICO، pre-sale، airdrop، private round
- نه revenue share از بازارهای شخص ثالث
- استقرار Base mainnet **تحت audit gate**

متن کامل: [`SECURITY.md`](../../../SECURITY.md#secondary-markets--third-party-tokenization).

## سپاسگزاری

استنتاج توزیع‌شدهٔ Tirami روی [mesh-llm](https://github.com/michaelneale/mesh-llm) از Michael Neale ساخته شده است. به [CREDITS.md](../../../CREDITS.md) مراجعه کنید.

</div>
