<div align="center">

# Tirami

**गणना ही मुद्रा है। हर वाट बुद्धिमत्ता पैदा करता है, कचरा नहीं।**

[![Crates.io](https://img.shields.io/crates/v/tirami-core?label=crates.io&color=e6522c)](https://crates.io/crates/tirami-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](../../../LICENSE)
[![Tests](https://img.shields.io/badge/tests-1192_passing-brightgreen)]()
[![verify-impl](https://img.shields.io/badge/verify--impl-123%2F123_GREEN-brightgreen)]()
[![foundry test](https://img.shields.io/badge/foundry_test-15%2F15_GREEN-brightgreen)]()
[![Phase](https://img.shields.io/badge/phase-19_hardened-blue)]()
[![Mainnet](https://img.shields.io/badge/mainnet-audit_gated-orange)]()

---

[English](../../../README.md) · [日本語](../ja/README.md) · [简体中文](../zh-CN/README.md) · [繁體中文](../zh-TW/README.md) · [Español](../es/README.md) · [Français](../fr/README.md) · [Русский](../ru/README.md) · [Українська](../uk/README.md) · **हिन्दी** · [العربية](../ar/README.md) · [فارسی](../fa/README.md) · [עברית](../he/README.md)

> आधिकारिक संस्करण अंग्रेज़ी [`README.md`](../../../README.md) है। अनुवाद कुछ पीछे हो सकते हैं।

</div>

**Tirami एक वितरित LLM इन्फ़रेंस प्रोटोकॉल है जहाँ गणना ही पैसा है।** नोड्स दूसरों के लिए उपयोगी LLM इन्फ़रेंस चला कर TRM (Tirami Resource Merit) कमाते हैं। Bitcoin के विपरीत — जो अर्थहीन हैश के लिए बिजली जलाता है — Tirami नोड पर खर्च किया गया हर जूल वास्तविक बुद्धिमत्ता पैदा करता है जिसकी किसी को उसी क्षण ज़रूरत है।

वितरित इन्फ़रेंस इंजन Michael Neale के [mesh-llm](https://github.com/michaelneale/mesh-llm) पर बना है। Tirami इसके ऊपर गणना की अर्थव्यवस्था जोड़ता है: TRM लेखांकन, Proof of Useful Work, गतिशील मूल्य निर्धारण, स्वायत्त एजेंट बजट, fail-safe नियंत्रण। देखें [CREDITS.md](../../../CREDITS.md)।

**एकीकृत fork:** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — mesh-llm के साथ Tirami की आर्थिक परत अंतर्निहित।

---

## ⚠️ Status Honesty (2026-04-19 / Phase 19)

किसी और बात से पहले, यहाँ ठीक-ठीक वही है **जो काम करता है** और **जो नहीं करता**। Tirami MIT लाइसेंस के तहत open-source सॉफ़्टवेयर है, **कोई टोकन बिक्री नहीं**। न ICO, न pre-mine, न team treasury, न airdrop। TRM गणना की एक लेखांकन इकाई है (1 TRM = 10⁹ FLOP), कोई वित्तीय उत्पाद नहीं — देखें [`SECURITY.md § Secondary Markets`](../../../SECURITY.md#secondary-markets--third-party-tokenization)।

### ✅ आज काम करता है (1,192 Rust परीक्षण + 15 Solidity परीक्षण, सत्यापित)

- HTTP के ज़रिए OpenAI-सुसंगत चैट, जुड़े पीयर तक स्वचालित P2P forwarding (`forward_chat_to_peer`, Phase 19)।
- iroh-QUIC P2P पर 128-bit nonce replay सुरक्षा के साथ दोहरे-हस्ताक्षरित `SignedTradeRecord` (`execute_signed_trade`)।
- `TradeAcceptDispatcher` प्रति-हस्ताक्षर संदेशों को संबंधित इन-फ़्लाइट इन्फ़रेंस कार्य तक रूट करता है (Phase 18.5-pt3)।
- मिलीभगत डिटेक्टर + हर `slashing_interval_secs` पर चलने वाला slashing loop (Phase 17 Wave 1.3)।
- 21-प्रविष्टियों वाली परिवर्तनीय whitelist + 18 अपरिवर्तनीय संवैधानिक प्रविष्टियों के साथ governance प्रस्ताव (Phase 18.1)।
- Welcome loan, stake pool, रेफ़रल बोनस, क्रेडिट स्कोरिंग, गतिशील बाज़ार मूल्य (EMA smoothing)।
- gossip स्ट्रीम पर `PriceSignal.http_endpoint` के ज़रिए स्वचालित पीयर खोज (Phase 19 Tier C)।
- `tirami start` पर स्वतः विन्यासित PersonalAgent (Phase 18.5-pt3e), tick loop अवलोकन के साथ।
- `tirami_*` उपसर्ग वाला Prometheus `/metrics` endpoint।
- Base Sepolia/mainnet deployment `Makefile` — Sepolia मुफ़्त, mainnet gate-सुरक्षित (नीचे देखें)।

### 🟡 डिज़ाइन किया, पर production में नहीं जोड़ा

- Inference का zkML प्रमाण: `tirami-zkml-bench` में केवल `MockBackend`। वास्तविक `ezkl` / `risc0` backends Phase 20+ में आएँगे। `ProofPolicy = Optional` डिफ़ॉल्ट है (Phase 19) — प्रमाण सहित trades को प्रतिष्ठा बोनस मिलता है; बिना प्रमाण के भी वैध।
- हाइब्रिड पोस्ट-क्वांटम ML-DSA (Dilithium) हस्ताक्षर: struct और verify पथ मौजूद हैं, `Config::pq_signatures = false` डिफ़ॉल्ट (iroh 0.97 द्वारा अवरुद्ध)।
- TEE attestation (Apple Secure Enclave / NVIDIA H100 CC): केवल `tirami-attestation` scaffold।
- Worker daemon gossip-recv loop ([issue #88](https://github.com/clearclown/tirami/issues/88)): `POST /v1/tirami/agent/task` में manual `peer.url` अभी भी काम करता है।

### ❌ नहीं हुआ (public mainnet से पहले आवश्यक)

- बाहरी सुरक्षा ऑडिट (Phase 17 Wave 3.3 आवश्यकता)। उम्मीदवार: Trail of Bits, Zellic, Open Zeppelin, Least Authority।
- Base L2 mainnet deployment। `make deploy-base-mainnet` target बिना `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER=<addr>` + interactive `i-accept-responsibility` इनपुट के चलने से *इनकार* करता है। देखें [`repos/tirami-contracts/Makefile`](../../../repos/tirami-contracts/Makefile)।
- वास्तविक PGP कुंजी के साथ production bug bounty (अभी [`SECURITY.md`](../../../SECURITY.md) में दस्तावेज़ित placeholder)।
- ≥ 30 दिन Base Sepolia पर स्थिर संचालन + ≥ 7 दिन 10+ नोड testnet पर stress test।

पूर्ण रोडमैप tier-दर-tier: [`docs/release-readiness.md`](../../../docs/release-readiness.md)।

---

## लाइव डेमो

Tirami है **GPU का Airbnb × AI-एजेंट अर्थव्यवस्था**: उपलब्ध गणना TRM किराया कमाती है; AI एजेंट किरायेदार हैं।

```
$ tirami start
🔑 ~/.tirami/node.key में नई कुंजी उत्पन्न
📦 Qwen2.5-0.5B-Instruct GGUF HuggingFace से लाया गया
🚀 HTTP API at http://127.0.0.1:3000
✅ Personal agent configured for <wallet-hex>
✅ P2P endpoint bound (iroh QUIC + Noise)
```

### Phase 19 Tier C/D enablers

```bash
tirami agent status
tirami agent chat "इस पेपर का सार दो" --max-tokens 256

# बिना local model वाला worker seed को forward करता है
curl -X POST http://worker.local:3111/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello"}],"max_tokens":5}'

# स्वचालित पीयर खोज
curl http://127.0.0.1:3001/v1/tirami/peers | jq '.peers[].http_endpoint'

# gate के साथ mainnet deploy
cd repos/tirami-contracts && make help
```

---

## Tirami क्यों

### 1. गणना = पैसा (21B TRM आपूर्ति सीमा)

TRM की संवैधानिक आपूर्ति सीमा 21,000,000,000 पर स्थिर है। कोई भी governance प्रस्ताव इसे नहीं बदल सकता (Phase 18.1, `IMMUTABLE_CONSTITUTIONAL_PARAMETERS`)। बदलने के लिए सॉफ़्टवेयर fork चाहिए — और उस क्षण वह «Tirami» नहीं रह जाता।

### 2. बिना blockchain के छेड़छाड़-प्रतिरोधी

हर trade दोहरे-Ed25519 हस्ताक्षर (provider + consumer) + 128-bit nonce (anti-replay) + gossip प्रसार + समय-समय पर Merkle root का on-chain anchor से संरक्षित है।

### 3. AI एजेंट अपना compute बजट खुद चलाते हैं

`PersonalAgent` (Phase 18.5) autopilot है जो उपयोगकर्ता की ओर से mesh पर compute खरीदता-बेचता है। `tirami agent chat "..."` स्वतः local या remote चुनता है।

### 4. Compute की microfinance

`welcome_loan = 1,000 TRM` (72 घंटे, 0% ब्याज) bootstrap के लिए। Welcome loan epoch 2 में स्थायी रूप से बंद हो जाता है (Constitutional), प्रवेश पथ stake-required mining में स्थानांतरित (Phase 18.2)।

### 5. Ledger-as-Brain: scheduling = आर्थिक निर्णय

`PeerRegistry` + `select_provider` हर inference अनुरोध को एक आर्थिक निर्णय में बदल देते हैं (मिलीभगत डिटेक्टर + audit tier + slashing से भारित प्रतिष्ठा)।

---

## 5-स्तरीय आर्किटेक्चर

```
L4: Discovery (tirami-agora)       एजेंट बाज़ार, प्रतिष्ठा, NIP-90
L3: Intelligence (tirami-mind)     PersonalAgent, आत्म-सुधार, TRM बजट
L2: Finance (tirami-bank)          रणनीतियाँ, पोर्टफ़ोलियो, futures, बीमा
L1: Economy (यह रेपो) ✅            Phase 1-19 पूर्ण
L0: Inference (forge-mesh) ✅      वितरित LLM इन्फ़रेंस, llama.cpp
                                   ↓ Phase 16: 10-मिनट batches
On-chain: tirami-contracts (Base L2, gated)
  TRM ERC-20 (cap 21B) + TiramiBridge
```

सभी 5 स्तर Rust में, 16 workspace crates। **1,192 परीक्षण पास** + 15 Solidity।

---

## त्वरित शुरुआत

```bash
# विकल्प 1: एक कमांड में E2E डेमो
git clone https://github.com/clearclown/tirami && cd tirami
bash scripts/demo-e2e.sh

# विकल्प 2: सीधा प्रारंभ
cargo build --release
./target/release/tirami start -m "qwen2.5:0.5b"

# विकल्प 3: OpenAI client की तरह
export OPENAI_BASE_URL=http://127.0.0.1:3001/v1
curl $OPENAI_BASE_URL/tirami/balance
```

---

## API संदर्भ

| Endpoint | विवरण |
|---|---|
| `POST /v1/chat/completions` | OpenAI-सुसंगत चैट। `x_tirami.trm_cost` सहित प्रतिक्रिया। local model न होने पर P2P forwarding (`forward_chat_to_peer`, Phase 19) |
| `POST /v1/tirami/agent/task` | PersonalAgent का synchronous dispatch, स्वचालित provider चयन (Phase 18.5-pt3) |
| `GET /v1/tirami/agent/status` | PersonalAgent की स्थिति |
| `GET /v1/tirami/balance` | शेष, प्रतिष्ठा, योगदान इतिहास |
| `GET /v1/tirami/pricing` | बाज़ार मूल्य (EMA), आपूर्ति/माँग |
| `GET /v1/tirami/trades` | हालिया trade इतिहास |
| `GET /v1/tirami/peers` | `http_endpoint` के साथ पीयर (Phase 19) |
| `GET /v1/tirami/providers` | प्रतिष्ठा-समायोजित provider रैंकिंग |
| `POST /v1/tirami/schedule` | Ledger-as-Brain probe (केवल पढ़ने के लिए) |
| `GET /v1/tirami/su/supply` | Tokenomics स्थिति |
| `POST /v1/tirami/su/stake` | yield के लिए TRM lock |
| `POST /v1/tirami/governance/propose` | governance प्रस्ताव (संवैधानिक params स्वतः-अस्वीकृत) |
| `POST /v1/tirami/lend` / `/borrow` / `/repay` | उधार देना |
| `GET /v1/tirami/slash-events` | slashing इतिहास (Phase 17 Wave 1.3) |
| `GET /metrics` | Prometheus (`tirami_*` उपसर्ग) |

---

## सुरक्षा

रक्षा की पाँच परतें: **क्रिप्टोग्राफ़ी** (Ed25519, nonce, HMAC, Noise) + **अर्थशास्त्र** (slashing, welcome loan sunset, stake-required mining) + **संचालन** (प्रति ASN rate limit, DDoS cap, checkpoint, fork detection) + **governance** (संवैधानिक parameters, ProofPolicy ratchet) + **प्रक्रिया** (kill switch, audit tier, प्रतिष्ठा दंड)। देखें [`docs/threat-model.md`](../../../docs/threat-model.md) T1–T17।

---

## विचार

«गणना को मुद्रा क्यों बनाएँ?» का एक-पंक्ति उत्तर: **AI युग में सचमुच दुर्लभ संसाधन गणना है**। Tirami अपनी मौद्रिक परिभाषा को इस भौतिक तथ्य से जोड़ता है (`1 TRM = 10⁹ FLOP`)। देखें [`docs/whitepaper.md`](../../../docs/whitepaper.md)।

---

## परियोजना संरचना

```
tirami/  (16 Rust crates, 5 स्तर)
├── crates/tirami-{ledger,node,cli,sdk,mcp,bank,mind,agora,anchor,lightning,net,proto,infer,core,shard,zkml-bench,attestation}
├── repos/tirami-contracts/  # Foundry TRM ERC-20 + TiramiBridge
├── scripts/verify-impl.sh   # 123 assertions
└── docs/                    # whitepaper, release-readiness आदि
```

~25,000 Rust पंक्तियाँ। Phase 1-19 पूर्ण।

---

## Ecosystem

| Repo | स्तर | परीक्षण | स्थिति |
|---|---|---|---|
| [clearclown/tirami](https://github.com/clearclown/tirami) (यह रेपो) | L1-L4 | 1,192 | Phase 1-19 ✅ |
| [clearclown/tirami-economics](https://github.com/clearclown/tirami-economics) | सिद्धांत | 16/16 GREEN | §1-§18 + PDFs |
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

## लाइसेंस

MIT। देखें [`LICENSE`](../../../LICENSE)।

## यह निवेश नहीं है — द्वितीयक बाज़ार अस्वीकरण

TRM **गणना लेखांकन** है, कोई वित्तीय उत्पाद नहीं। Maintainers TRM को न बेचते हैं, न प्रचार करते हैं, न सट्टा लगाते हैं। चूँकि यह MIT OSS है, कोई भी — maintainers की जानकारी के बिना — TRM को bridge, list या derive कर सकता है; तकनीकी रूप से रोकना असंभव है। जो लोग TRM को store of value के रूप में रखने या trade करने का निर्णय लेते हैं वे सभी जोखिम (कानूनी, विनियामक, counterparty, तकनीकी) स्वयं वहन करते हैं।

- न ICO, pre-sale, airdrop, private round
- तृतीय-पक्ष बाज़ारों से कोई revenue share नहीं
- Base mainnet deployment **audit gate के तहत**

पूर्ण पाठ: [`SECURITY.md`](../../../SECURITY.md#secondary-markets--third-party-tokenization)।

## आभार

Tirami का वितरित इन्फ़रेंस Michael Neale के [mesh-llm](https://github.com/michaelneale/mesh-llm) पर बना है। देखें [CREDITS.md](../../../CREDITS.md)।
