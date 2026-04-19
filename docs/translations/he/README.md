<div align="center" dir="rtl">

# Tirami

**חישוב הוא כסף. כל ואט מייצר בינה, לא פסולת.**

[![Crates.io](https://img.shields.io/crates/v/tirami-core?label=crates.io&color=e6522c)](https://crates.io/crates/tirami-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](../../../LICENSE)
[![Tests](https://img.shields.io/badge/tests-1192_passing-brightgreen)]()
[![verify-impl](https://img.shields.io/badge/verify--impl-123%2F123_GREEN-brightgreen)]()
[![foundry test](https://img.shields.io/badge/foundry_test-15%2F15_GREEN-brightgreen)]()
[![Phase](https://img.shields.io/badge/phase-19_hardened-blue)]()
[![Mainnet](https://img.shields.io/badge/mainnet-audit_gated-orange)]()

---

[English](../../../README.md) · [日本語](../ja/README.md) · [简体中文](../zh-CN/README.md) · [繁體中文](../zh-TW/README.md) · [Español](../es/README.md) · [Français](../fr/README.md) · [Русский](../ru/README.md) · [Українська](../uk/README.md) · [हिन्दी](../hi/README.md) · [العربية](../ar/README.md) · [فارسی](../fa/README.md) · **עברית**

> הגרסה הקנונית היא [`README.md`](../../../README.md) באנגלית. ייתכן שתרגומים מתעכבים.

</div>

<div dir="rtl">

**Tirami הוא פרוטוקול היסק LLM מבוזר שבו חישוב הוא כסף.** צמתים מרוויחים TRM (Tirami Resource Merit) בהרצת היסק LLM מועיל עבור אחרים. בניגוד ל-Bitcoin — השורף חשמל עבור hashes חסרי משמעות — כל ג'אול שמוצא על צומת Tirami מייצר בינה אמיתית שמישהו זקוק לה באותו רגע.

מנוע ההיסק המבוזר בנוי על [mesh-llm](https://github.com/michaelneale/mesh-llm) מאת Michael Neale. Tirami מוסיף מעליו כלכלת חישוב: הנהלת חשבונות TRM, Proof of Useful Work, תמחור דינמי, תקציבי סוכנים אוטונומיים, בקרי fail-safe. ראה [CREDITS.md](../../../CREDITS.md).

**Fork משולב:** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — mesh-llm עם שכבת הכלכלה של Tirami משובצת.

---

## ⚠️ Status Honesty (2026-04-19 / Phase 19)

לפני כל דבר אחר, הנה בדיוק **מה שעובד** ו**מה שלא**. Tirami היא תוכנת קוד פתוח ברישיון MIT, **לא מכירת טוקנים**. אין ICO, אין pre-mine, אין team treasury, אין airdrop. TRM היא יחידת חשבונאות חישוב (1 TRM = 10⁹ FLOP), לא מוצר פיננסי — ראה [`SECURITY.md § Secondary Markets`](../../../SECURITY.md#secondary-markets--third-party-tokenization).

### ✅ עובד היום (1,192 מבחני Rust + 15 מבחני Solidity, מאומתים)

- צ'אט תואם OpenAI ב-HTTP עם forwarding אוטומטי ב-P2P אל peer מחובר (`forward_chat_to_peer`, Phase 19).
- `SignedTradeRecord` עם חתימה כפולה על iroh-QUIC P2P עם הגנת anti-replay על ידי nonce של 128 ביט (`execute_signed_trade`).
- `TradeAcceptDispatcher` מנתב הודעות חתימה-נגדית למשימת ההיסק הפעילה המתאימה (Phase 18.5-pt3).
- מזהה קנוניה + לולאת slashing שרצה כל `slashing_interval_secs` (Phase 17 Wave 1.3).
- הצעות governance עם whitelist ניתנת לשינוי בת 21 ערכים + רשימה חוקתית קבועה בת 18 (Phase 18.1).
- Welcome loan, stake pool, בונוסי הפניה, credit scoring, מחירי שוק דינמיים (EMA smoothing).
- גילוי peers אוטומטי דרך `PriceSignal.http_endpoint` בזרם gossip (Phase 19 Tier C).
- PersonalAgent מוגדר אוטומטית ב-`tirami start` (Phase 18.5-pt3e), עם תצפית על tick loop.
- Endpoint `/metrics` של Prometheus עם תחילית `tirami_*`.
- `Makefile` לפריסה ב-Base Sepolia/mainnet — Sepolia חינמי, mainnet gate-מוגן (ראה למטה).

### 🟡 תוכנן אך לא חובר בייצור

- הוכחת zkML להיסק: `tirami-zkml-bench` כולל רק `MockBackend`. Backends אמיתיים (`ezkl` / `risc0`) יגיעו ב-Phase 20+. `ProofPolicy = Optional` כברירת מחדל (Phase 19) — trades עם הוכחה מקבלים בונוס reputation; בלעדיה עדיין תקפים.
- חתימות ML-DSA (Dilithium) hybrid פוסט-קוונטיות: struct ומסלול verify קיימים, `Config::pq_signatures = false` כברירת מחדל (חסום ב-iroh 0.97).
- TEE attestation (Apple Secure Enclave / NVIDIA H100 CC): scaffold בלבד ב-`tirami-attestation`.
- Worker daemon gossip-recv loop ([issue #88](https://github.com/clearclown/tirami/issues/88)): `peer.url` ידני ב-`POST /v1/tirami/agent/task` עדיין עובד.

### ❌ לא הושלם (נדרש לפני public mainnet)

- ביקורת אבטחה חיצונית (דרישת Phase 17 Wave 3.3). מועמדים: Trail of Bits, Zellic, Open Zeppelin, Least Authority.
- פריסת Base L2 mainnet. היעד `make deploy-base-mainnet` *מסרב* לרוץ ללא `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER=<addr>` + קלט אינטראקטיבי `i-accept-responsibility`. ראה [`repos/tirami-contracts/Makefile`](../../../repos/tirami-contracts/Makefile).
- bug bounty ייצורי עם מפתח PGP אמיתי (כרגע placeholder מתועד ב-[`SECURITY.md`](../../../SECURITY.md)).
- ≥ 30 ימים הפעלה יציבה ב-Base Sepolia + ≥ 7 ימים stress test ב-testnet של 10+ צמתים.

מפת דרכים מלאה לפי tier: [`docs/release-readiness.md`](../../../docs/release-readiness.md).

---

## הדגמה חיה

Tirami הוא **Airbnb ל-GPU × כלכלת סוכני AI**: חישוב זמין מרוויח שכירות ב-TRM; סוכני AI הם השוכרים.

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
tirami agent chat "סכם את המאמר הזה" --max-tokens 256

# Worker ללא מודל מקומי מעביר אל seed
curl -X POST http://worker.local:3111/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello"}],"max_tokens":5}'

# גילוי peers אוטומטי
curl http://127.0.0.1:3001/v1/tirami/peers | jq '.peers[].http_endpoint'

# פריסת mainnet עם gate
cd repos/tirami-contracts && make help
```

<div dir="rtl">

---

## למה Tirami

### 1. חישוב = כסף (תקרת היצע 21B TRM)

ל-TRM תקרת היצע חוקתית קבועה על 21,000,000,000. אף הצעת governance לא יכולה לשנות אותה (Phase 18.1, ב-`IMMUTABLE_CONSTITUTIONAL_PARAMETERS`). שינוי דורש fork של התוכנה — ובאותו רגע זה כבר לא «Tirami».

### 2. עמיד לחבלות ללא blockchain

כל trade מוגן בחתימה כפולה Ed25519 (provider + consumer) + nonce של 128 ביט (anti-replay) + הפצת gossip + עיגון תקופתי של Merkle root on-chain.

### 3. סוכני AI מנהלים את תקציב החישוב של עצמם

`PersonalAgent` (Phase 18.5) הוא טייס אוטומטי שקונה ומוכר חישוב ב-mesh בשם המשתמש. `tirami agent chat "..."` בוחר אוטומטית local או remote.

### 4. מיקרו-פיננסים של חישוב

`welcome_loan = 1,000 TRM` (72 שעות, ריבית 0%) ל-bootstrap. Welcome loan מסתיים סופית ב-epoch 2 (Constitutional), מסלול הכניסה עובר ל-stake-required mining (Phase 18.2).

### 5. Ledger-as-Brain: תזמון = החלטה כלכלית

`PeerRegistry` + `select_provider` הופכים כל בקשת היסק להחלטה כלכלית (reputation משוקלל במזהה קנוניה + audit tier + slashing).

---

## ארכיטקטורה בת 5 שכבות

</div>

```
L4: Discovery (tirami-agora)       שוק סוכנים, reputation, NIP-90
L3: Intelligence (tirami-mind)     PersonalAgent, שיפור עצמי, תקציב TRM
L2: Finance (tirami-bank)          אסטרטגיות, תיקים, futures, ביטוח
L1: Economy (מאגר זה) ✅           Phase 1-19 הושלם
L0: Inference (forge-mesh) ✅      היסק LLM מבוזר, llama.cpp
                                   ↓ Phase 16: batches של 10 דקות
On-chain: tirami-contracts (Base L2, gated)
  TRM ERC-20 (cap 21B) + TiramiBridge
```

<div dir="rtl">

כל 5 השכבות ב-Rust, 16 workspace crates. **1,192 מבחנים עוברים** + 15 Solidity.

---

## התחלה מהירה

</div>

```bash
# אפשרות 1: הדגמת E2E בפקודה אחת
git clone https://github.com/clearclown/tirami && cd tirami
bash scripts/demo-e2e.sh

# אפשרות 2: הפעלה ישירה
cargo build --release
./target/release/tirami start -m "qwen2.5:0.5b"

# אפשרות 3: כ-OpenAI client
export OPENAI_BASE_URL=http://127.0.0.1:3001/v1
curl $OPENAI_BASE_URL/tirami/balance
```

<div dir="rtl">

---

## ממשק API

| Endpoint | תיאור |
|---|---|
| `POST /v1/chat/completions` | צ'אט תואם OpenAI. תגובה כוללת `x_tirami.trm_cost`. P2P forwarding אם אין מודל מקומי (`forward_chat_to_peer`, Phase 19) |
| `POST /v1/tirami/agent/task` | dispatch סינכרוני של PersonalAgent, בחירת provider אוטומטית (Phase 18.5-pt3) |
| `GET /v1/tirami/agent/status` | מצב PersonalAgent |
| `GET /v1/tirami/balance` | יתרה, reputation, היסטוריית תרומה |
| `GET /v1/tirami/pricing` | מחיר שוק (EMA), היצע/ביקוש |
| `GET /v1/tirami/trades` | היסטוריית trades אחרונה |
| `GET /v1/tirami/peers` | peers עם `http_endpoint` (Phase 19) |
| `GET /v1/tirami/providers` | דירוג providers מותאם ל-reputation |
| `POST /v1/tirami/schedule` | probe של Ledger-as-Brain (קריאה בלבד) |
| `GET /v1/tirami/su/supply` | מצב tokenomics |
| `POST /v1/tirami/su/stake` | נעילת TRM עבור yield |
| `POST /v1/tirami/governance/propose` | הצעת governance (params חוקתיים נדחים אוטומטית) |
| `POST /v1/tirami/lend` / `/borrow` / `/repay` | הלוואות |
| `GET /v1/tirami/slash-events` | היסטוריית slashing (Phase 17 Wave 1.3) |
| `GET /metrics` | Prometheus (תחילית `tirami_*`) |

---

## אבטחה

חמש שכבות הגנה: **קריפטוגרפיה** (Ed25519, nonce, HMAC, Noise) + **כלכלה** (slashing, welcome loan sunset, stake-required mining) + **תפעולי** (rate limit לכל ASN, DDoS cap, checkpoint, fork detection) + **governance** (parameters חוקתיים, ProofPolicy ratchet) + **תהליכי** (kill switch, audit tier, עונשי reputation). ראה [`docs/threat-model.md`](../../../docs/threat-model.md) T1–T17.

---

## הרעיון

תשובה בשורה אחת ל-«למה להפוך חישוב למטבע?»: **בעידן ה-AI, המשאב הנדיר באמת הוא חישוב**. Tirami מעגן את ההגדרה הכספית שלו בעובדה הפיזיקלית הזאת (`1 TRM = 10⁹ FLOP`). ראה [`docs/whitepaper.md`](../../../docs/whitepaper.md).

---

## מבנה הפרויקט

</div>

```
tirami/  (16 Rust crates, 5 שכבות)
├── crates/tirami-{ledger,node,cli,sdk,mcp,bank,mind,agora,anchor,lightning,net,proto,infer,core,shard,zkml-bench,attestation}
├── repos/tirami-contracts/  # Foundry TRM ERC-20 + TiramiBridge
├── scripts/verify-impl.sh   # 123 assertions
└── docs/                    # whitepaper, release-readiness וכו'
```

<div dir="rtl">

~25,000 שורות Rust. Phase 1-19 הושלם.

---

## אקוסיסטם

| Repo | שכבה | מבחנים | מצב |
|---|---|---|---|
| [clearclown/tirami](https://github.com/clearclown/tirami) (מאגר זה) | L1-L4 | 1,192 | Phase 1-19 ✅ |
| [clearclown/tirami-economics](https://github.com/clearclown/tirami-economics) | תיאוריה | 16/16 GREEN | §1-§18 + PDFs |
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

## רישיון

MIT. ראה [`LICENSE`](../../../LICENSE).

## זו אינה השקעה — הצהרת פטור לשוק משני

TRM היא **חשבונאות חישוב**, לא מוצר פיננסי. ה-maintainers לא מוכרים, לא מקדמים ולא עוסקים בספקולציה על TRM. מאחר שזהו MIT OSS, כל אחד יכול — ללא ידיעת ה-maintainers — לבצע bridge, list או נגזור של TRM; מבחינה טכנית בלתי אפשרי למנוע זאת. מי שבוחר להחזיק או לסחור ב-TRM כ-store of value נושא בעצמו בכל הסיכונים (משפטיים, רגולטוריים, counterparty, טכניים).

- אין ICO, pre-sale, airdrop, private round
- אין revenue share משווקי צד שלישי
- פריסת Base mainnet **תחת audit gate**

טקסט מלא: [`SECURITY.md`](../../../SECURITY.md#secondary-markets--third-party-tokenization).

## תודות

ההיסק המבוזר של Tirami בנוי על [mesh-llm](https://github.com/michaelneale/mesh-llm) מאת Michael Neale. ראה [CREDITS.md](../../../CREDITS.md).

</div>
