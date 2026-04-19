<div align="center">

# Tirami

**Le calcul est monnaie. Chaque watt produit de l'intelligence, pas du gaspillage.**

[![Crates.io](https://img.shields.io/crates/v/tirami-core?label=crates.io&color=e6522c)](https://crates.io/crates/tirami-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-brightgreen.svg)](../../../LICENSE)
[![Tests](https://img.shields.io/badge/tests-1192_passing-brightgreen)]()
[![verify-impl](https://img.shields.io/badge/verify--impl-123%2F123_GREEN-brightgreen)]()
[![foundry test](https://img.shields.io/badge/foundry_test-15%2F15_GREEN-brightgreen)]()
[![Phase](https://img.shields.io/badge/phase-19_hardened-blue)]()
[![Mainnet](https://img.shields.io/badge/mainnet-audit_gated-orange)]()

---

[English](../../../README.md) · [日本語](../ja/README.md) · [简体中文](../zh-CN/README.md) · [繁體中文](../zh-TW/README.md) · [Español](../es/README.md) · **Français** · [Русский](../ru/README.md) · [Українська](../uk/README.md) · [हिन्दी](../hi/README.md) · [العربية](../ar/README.md) · [فارسی](../fa/README.md) · [עברית](../he/README.md)

> La version canonique est le [`README.md`](../../../README.md) en anglais. Les traductions peuvent être en retard.

</div>

**Tirami est un protocole d'inférence distribuée où le calcul est de l'argent.** Les nœuds gagnent des TRM (Tirami Resource Merit) en exécutant de l'inférence LLM utile pour d'autres. Contrairement à Bitcoin — qui brûle de l'électricité sur des hachages sans sens — chaque joule dépensé sur un nœud Tirami produit de l'intelligence réelle dont quelqu'un a besoin.

Le moteur d'inférence distribuée est construit sur [mesh-llm](https://github.com/michaelneale/mesh-llm) de Michael Neale. Tirami ajoute une économie du calcul par-dessus : comptabilité TRM, Proof of Useful Work, tarification dynamique, budgets d'agents autonomes, contrôles fail-safe. Voir [CREDITS.md](../../../CREDITS.md).

**Fork intégré :** [forge-mesh](https://github.com/nm-arealnormalman/mesh-llm) — mesh-llm avec la couche économique Tirami incorporée.

---

## ⚠️ Status Honesty (2026-04-19 / Phase 19)

Avant toute autre chose, voici exactement **ce qui fonctionne** et **ce qui ne fonctionne pas**. Tirami est un logiciel open source sous licence MIT, **pas une vente de tokens**. Pas d'ICO, pas de pré-minage, pas de trésorerie d'équipe, pas d'airdrop. TRM est une unité de comptabilité du calcul (1 TRM = 10⁹ FLOP), pas un produit financier — voir [`SECURITY.md § Secondary Markets`](../../../SECURITY.md#secondary-markets--third-party-tokenization).

### ✅ Fonctionnel aujourd'hui (1 192 tests Rust + 15 tests Solidity, vérifiés)

- Chat OpenAI-compatible via HTTP avec forwarding P2P automatique vers un peer connecté (`forward_chat_to_peer`, Phase 19).
- `SignedTradeRecord` à double signature via iroh-QUIC P2P avec protection anti-rejeu par nonce 128 bits (`execute_signed_trade`).
- `TradeAcceptDispatcher` route les messages de contre-signature vers la tâche d'inférence en vol correspondante (Phase 18.5-pt3).
- Détecteur de collusion + boucle de slashing tournant à chaque `slashing_interval_secs` (Phase 17 Wave 1.3).
- Propositions de governance avec whitelist mutable de 21 entrées + liste constitutionnelle immuable de 18 (Phase 18.1).
- Welcome loan, pool de staking, bonus de parrainage, credit scoring, prix de marché dynamiques (lissés EMA).
- Auto-découverte de peers via `PriceSignal.http_endpoint` sur le fil gossip (Phase 19 Tier C).
- PersonalAgent auto-configuré au `tirami start` (Phase 18.5-pt3e), avec observabilité de la boucle tick.
- Endpoint Prometheus `/metrics` avec préfixe `tirami_*`.
- `Makefile` de déploiement Base Sepolia/mainnet — Sepolia est gratuit, mainnet est gardé (voir plus bas).

### 🟡 Conçu mais pas câblé en production

- Preuve d'inférence zkML : `tirami-zkml-bench` n'a que `MockBackend`. Les vrais backends `ezkl` / `risc0` arrivent en Phase 20+. `ProofPolicy = Optional` par défaut (Phase 19) — les trades avec preuve reçoivent un bonus de réputation ; sans preuve restent valides.
- Signatures hybrides post-quantiques ML-DSA (Dilithium) : struct et chemin de verify existent, `Config::pq_signatures = false` par défaut (bloqué par iroh 0.97).
- TEE attestation (Apple Secure Enclave / NVIDIA H100 CC) : scaffold `tirami-attestation` uniquement.
- Boucle gossip-recv du worker daemon ([issue #88](https://github.com/clearclown/tirami/issues/88)) : le `peer.url` manuel dans `POST /v1/tirami/agent/task` fonctionne toujours.

### ❌ Pas fait (requis avant mainnet public)

- Audit externe de sécurité (exigence Phase 17 Wave 3.3). Candidats : Trail of Bits, Zellic, Open Zeppelin, Least Authority.
- Déploiement Base L2 mainnet. La cible `make deploy-base-mainnet` *refuse* de s'exécuter sans `AUDIT_CLEARANCE=yes` + `MULTISIG_OWNER=<addr>` + saisie interactive `i-accept-responsibility`. Voir [`repos/tirami-contracts/Makefile`](../../../repos/tirami-contracts/Makefile).
- Bug bounty en production avec une vraie clé PGP (actuellement placeholder documenté dans [`SECURITY.md`](../../../SECURITY.md)).
- ≥ 30 jours d'exploitation stable sur Base Sepolia + ≥ 7 jours de stress test sur testnet de 10+ nœuds.

Roadmap complète par tiers : [`docs/release-readiness.md`](../../../docs/release-readiness.md).

---

## Démo en direct

Tirami est le **Airbnb des GPUs × Économie d'agents IA** : le calcul disponible gagne un loyer en TRM ; les agents IA sont les locataires.

```
$ tirami start
🔑 Nouvelle clé générée dans ~/.tirami/node.key
📦 Qwen2.5-0.5B-Instruct GGUF récupéré depuis HuggingFace
🚀 HTTP API at http://127.0.0.1:3000
✅ Personal agent configured for <wallet-hex>
✅ P2P endpoint bound (iroh QUIC + Noise)
```

### Enablers de Phase 19 Tier C/D

```bash
tirami agent status
tirami agent chat "Résume ce papier" --max-tokens 256

# Worker sans modèle local forwarde au seed
curl -X POST http://worker.local:3111/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello"}],"max_tokens":5}'

# Auto-découverte de peers
curl http://127.0.0.1:3001/v1/tirami/peers | jq '.peers[].http_endpoint'

# Déploiement mainnet avec gate
cd repos/tirami-contracts && make help
```

---

## Pourquoi Tirami

### 1. Calcul = monnaie (plafond d'offre 21B TRM)

TRM a un plafond d'offre constitutionnel fixé à 21 000 000 000. Aucune proposition de governance ne peut le modifier (Phase 18.1, dans `IMMUTABLE_CONSTITUTIONAL_PARAMETERS`). Le modifier nécessite un fork du logiciel — à ce moment précis, ce n'est plus « Tirami ».

### 2. Résistant à la manipulation sans blockchain

Chaque trade est protégé par double signature Ed25519 (provider + consumer) + nonce 128 bits (anti-rejeu) + propagation gossip + anchor périodique de Merkle root on-chain.

### 3. Les agents IA gèrent leur propre budget de calcul

`PersonalAgent` (Phase 18.5) est le pilote automatique qui achète et vend du calcul sur le mesh au nom de l'utilisateur. `tirami agent chat "..."` choisit local ou remote automatiquement.

### 4. Micro-finance du calcul

`welcome_loan = 1 000 TRM` (72h, taux 0 %) pour bootstrap. Welcome loan s'arrête définitivement en epoch 2 (Constitutional), la voie d'entrée migre vers stake-required mining (Phase 18.2).

### 5. Ledger-as-Brain : scheduling = décision économique

`PeerRegistry` + `select_provider` transforment chaque requête d'inférence en une décision économique (réputation pondérée par collusion detector + audit tier + slashing).

---

## Architecture 5 couches

```
L4: Discovery (tirami-agora)       Marché des agents, réputation, NIP-90
L3: Intelligence (tirami-mind)     PersonalAgent, auto-amélioration, budget TRM
L2: Finance (tirami-bank)          Stratégies, portefeuilles, futures, assurance
L1: Economy (tirami ce dépôt) ✅   Phase 1-19 complète
L0: Inference (forge-mesh) ✅      Inférence LLM distribuée, llama.cpp
                                   ↓ Phase 16 : batches de 10 min
On-chain : tirami-contracts (Base L2, gated)
  TRM ERC-20 (cap 21B) + TiramiBridge
```

Les 5 couches sont en Rust, 16 workspace crates. **1 192 tests passent** + 15 Solidity.

---

## Démarrage rapide

```bash
# Option 1 : démo E2E en une commande
git clone https://github.com/clearclown/tirami && cd tirami
bash scripts/demo-e2e.sh

# Option 2 : démarrage direct
cargo build --release
./target/release/tirami start -m "qwen2.5:0.5b"

# Option 3 : comme client OpenAI
export OPENAI_BASE_URL=http://127.0.0.1:3001/v1
curl $OPENAI_BASE_URL/tirami/balance
```

---

## Référence d'API

| Endpoint | Description |
|---|---|
| `POST /v1/chat/completions` | Chat OpenAI-compatible. Réponse incluant `x_tirami.trm_cost`. Forwarding P2P si pas de modèle local (`forward_chat_to_peer`, Phase 19) |
| `POST /v1/tirami/agent/task` | Dispatch synchrone du PersonalAgent, sélection automatique du provider (Phase 18.5-pt3) |
| `GET /v1/tirami/agent/status` | État du PersonalAgent |
| `GET /v1/tirami/balance` | Solde, réputation, historique de contribution |
| `GET /v1/tirami/pricing` | Prix de marché (EMA), offre/demande |
| `GET /v1/tirami/trades` | Historique récent de trades |
| `GET /v1/tirami/peers` | Peers avec `http_endpoint` (Phase 19) |
| `GET /v1/tirami/providers` | Classement des providers ajusté par réputation |
| `POST /v1/tirami/schedule` | Sonde Ledger-as-Brain (lecture seule) |
| `GET /v1/tirami/su/supply` | État tokenomics |
| `POST /v1/tirami/su/stake` | Bloquer TRM pour yield |
| `POST /v1/tirami/governance/propose` | Proposition governance (params constitutionnels auto-rejetés) |
| `POST /v1/tirami/lend` / `/borrow` / `/repay` | Prêts |
| `GET /v1/tirami/slash-events` | Historique de slashing (Phase 17 Wave 1.3) |
| `GET /metrics` | Prometheus (préfixe `tirami_*`) |

---

## Sécurité

Cinq couches de défense : **cryptographie** (Ed25519, nonce, HMAC, Noise) + **économie** (slashing, welcome loan sunset, stake-required mining) + **opérationnel** (rate limit par ASN, cap DDoS, checkpoint, détection de fork) + **governance** (paramètres constitutionnels, ratchet ProofPolicy) + **processus** (kill switch, audit tier, pénalité de réputation). Voir [`docs/threat-model.md`](../../../docs/threat-model.md) T1–T17.

---

## L'idée

Réponse en une ligne à « pourquoi faire du calcul une monnaie ? » : **à l'ère de l'IA, la ressource véritablement rare est le calcul**. Tirami ancre sa définition monétaire sur ce fait physique (`1 TRM = 10⁹ FLOP`). Voir [`docs/whitepaper.md`](../../../docs/whitepaper.md).

---

## Structure du projet

```
tirami/  (16 Rust crates, les 5 couches)
├── crates/tirami-{ledger,node,cli,sdk,mcp,bank,mind,agora,anchor,lightning,net,proto,infer,core,shard,zkml-bench,attestation}
├── repos/tirami-contracts/  # Foundry TRM ERC-20 + TiramiBridge
├── scripts/verify-impl.sh   # 123 assertions
└── docs/                    # whitepaper, release-readiness, etc.
```

~25 000 lignes de Rust. Phase 1-19 complète.

---

## Écosystème

| Repo | Couche | Tests | État |
|---|---|---|---|
| [clearclown/tirami](https://github.com/clearclown/tirami) (ce dépôt) | L1-L4 | 1 192 | Phase 1-19 ✅ |
| [clearclown/tirami-economics](https://github.com/clearclown/tirami-economics) | Théorie | 16/16 GREEN | §1-§18 + PDFs |
| [repos/tirami-contracts](https://github.com/clearclown/tirami/tree/main/repos/tirami-contracts) | on-chain | 15 forge tests | mainnet gated |
| [nm-arealnormalman/mesh-llm](https://github.com/nm-arealnormalman/mesh-llm) | L0 Inference | 646 | port forge-economy ✅ |

---

## Docs

- [Whitepaper](../../../docs/whitepaper.md) / [Release Readiness](../../../docs/release-readiness.md) / [Constitution](../../../docs/constitution.md) / [Killer-App](../../../docs/killer-app.md)
- [Public API Surface](../../../docs/public-api-surface.md) / [zkML Strategy](../../../docs/zkml-strategy.md) / [Strategy](../../../docs/strategy.md)
- [Economic Model](../../../docs/economy.md) / [Architecture](../../../docs/architecture.md) / [Wire Protocol](../../../docs/protocol-spec.md)
- [Threat Model](../../../docs/threat-model.md) / [Security Policy](../../../SECURITY.md) / [Operator Guide](../../../docs/operator-guide.md)
- [Developer Guide](../../../docs/developer-guide.md) / [FAQ](../../../docs/faq.md) / [Roadmap](../../../docs/roadmap.md)

---

## Licence

MIT. Voir [`LICENSE`](../../../LICENSE).

## Ce n'est pas un investissement — avertissement sur le marché secondaire

TRM est **de la comptabilité de calcul**, pas un produit financier. Les maintainers ne vendent, ne promeuvent, ni ne spéculent sur TRM. Puisque c'est de l'OSS MIT, n'importe qui peut — à l'insu des maintainers — bridger, lister ou dériver TRM ; techniquement impossible à empêcher. Ceux qui choisissent de détenir ou d'échanger TRM comme réserve de valeur en assument tous les risques (légaux, réglementaires, de contrepartie, techniques).

- Pas d'ICO, de pré-vente, d'airdrop, de tour privé
- Pas de partage de revenus depuis des marchés tiers
- Déploiement mainnet Base **avec gate d'audit**

Texte complet : [`SECURITY.md`](../../../SECURITY.md#secondary-markets--third-party-tokenization).

## Remerciements

L'inférence distribuée de Tirami est bâtie sur [mesh-llm](https://github.com/michaelneale/mesh-llm) de Michael Neale. Voir [CREDITS.md](../../../CREDITS.md).
