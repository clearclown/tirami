# Forge — Architecture

## Aperçu

Forge est un système à deux couches : **l'inférence** et **l'économie**.

La couche d'inférence gère la distribution des modèles, le réseau maillé (mesh) et le service API. Elle est basée sur [mesh-llm](https://github.com/Mesh-LLM/mesh-llm).

La couche économique gère la comptabilité des CU, l'enregistrement des transactions, la tarification et les budgets des agents. C'est la contribution originale de Forge.

```
┌─────────────────────────────────────────────────┐
│  SDK / Limite d'Intégration                     │
│  Tout client peut intégrer forge-node en tant   │
│  que bibliothèque. Agents tiers, tableaux de    │
│  bord, adaptateurs.                             │
└──────────────────┬──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  Couche Économique (Original Forge)              │
│                                                  │
│  ┌──────────────┐ ┌──────────┐ ┌─────────────┐ │
│  │ forge-ledger │ │ prix     │ │ budgets     │ │
│  │ échanges CU  │ │ offre /  │ │ agents      │ │
│  │ réputation   │ │ demande  │ │ /v1/forge/* │ │
│  │ rendement    │ │          │ │             │ │
│  └──────────────┘ └──────────┘ └─────────────┘ │
│                                                  │
│  ┌──────────────┐ ┌──────────────────────────┐  │
│  │ forge-verify │ │ forge-bridge (optionnel)  │  │
│  │ double-sign  │ │ CU ↔ BTC Lightning      │  │
│  │ sync gossip  │ │ CU ↔ stablecoin         │  │
└──────────────────┴──────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────┐
│  Couche d'Inférence (Dérivée de mesh-llm)       │
│                                                  │
│  ┌────────────┐ ┌───────────┐ ┌──────────────┐ │
│  │ maillage   │ │ llama.cpp │ │ API OpenAI   │ │
│  │ iroh       │ │ pipeline  │ │ /v1/chat/    │ │
│  │ QUIC+Noise │ │ shard MoE │ │ completions  │ │
└───────────────┘ └───────────┘ └──────────────┘ │
└─────────────────────────────────────────────────┘
```

## Couche d'Inférence (mesh-llm)

La couche d'inférence est responsable de :

- **Réseau maillé** : Connexions QUIC basées sur iroh avec chiffrement Noise.
- **Découverte de pairs** : Relais Nostr pour les réseaux publics, mDNS pour le LAN.
- **Distribution des modèles** : Parallélisme de pipeline pour les modèles denses, sharding d'experts pour les MoE.
- **Exécution de l'inférence** : llama.cpp via les sous-processus llama-server et rpc-server.
- **Service API** : `/v1/chat/completions` et `/v1/models` compatibles OpenAI.

Forge hérite de tout cela de mesh-llm. La couche d'inférence ne connaît pas les CU, les échanges ou les prix.

## Couche Économique (Forge)

La couche économique se situe au-dessus de l'inférence et est responsable de :

### forge-ledger — Le Moteur Économique

```rust
pub struct ComputeLedger {
    balances: HashMap<NodeId, NodeBalance>,
    work_log: Vec<WorkUnit>,
    trade_log: Vec<TradeRecord>,
    price: MarketPrice,
}
```

Responsabilités principales :
- Suivre le solde CU par nœud (contribué, consommé, réservé).
- Enregistrer chaque échange d'inférence (fournisseur, consommateur, montant CU, jetons).
- Calculer les prix du marché dynamiques à partir de l'offre et de la demande.
- Appliquer le rendement (yield) aux nœuds contributeurs.
- Exporter les relevés de règlement pour les ponts hors protocole.
- Persister les instantanés (snapshots) sur disque avec intégrité HMAC-SHA256.

### forge-verify — Preuve de Travail Utile (cible)

Garantit que les réclamations de CU sont légitimes :
- Protocole de double signature : le fournisseur et le consommateur signent chaque TradeRecord.
- Synchronisation par rumeur (gossip) : les échanges signés se propagent sur le réseau.
- Vérification : n'importe quel nœud peut valider les deux signatures.
- Détection de fraude : les échanges non correspondants ou non signés sont rejetés.

### forge-bridge — Règlement Externe (optionnel)

Convertit la valeur CU en valeur externe pour les opérateurs qui en ont besoin :
- Bitcoin Lightning : CU → msats via un taux de change configurable.
- Stablecoin : CU → USDC/USDT via un adaptateur.
- Fiat : CU → virement bancaire via le tableau de bord de l'opérateur.

La couche de pont est en dehors du protocole de base. Différents opérateurs peuvent utiliser différents ponts.

### Surface API

| Route | Couche | Description |
|-------|-------|-------------|
| `POST /v1/chat/completions` | Inférence + Économie | Exécuter l'inférence, enregistrer l'échange CU |
| `GET /v1/models` | Inférence | Liste des modèles chargés |
| `GET /v1/forge/balance` | Économie | Solde CU, réputation |
| `GET /v1/forge/pricing` | Économie | Prix du marché, estimations de coûts |
| `GET /status` | Économie | Prix du marché, stats réseau, échanges récents |
| `GET /topology` | Inférence | Manifeste du modèle, pairs, plan de shard |
| `GET /settlement` | Économie | Historique des échanges exportable |
| `GET /health` | Inférence | Vérification de santé de base |

## Flux de Données

### Inférence avec Comptabilité CU

```
Le consommateur envoie une requête
    ↓
L'API reçoit POST /v1/chat/completions
    ↓
Le Ledger vérifie : peut_payer(consommateur, coût_estimé) ?
    ↓ oui
La couche d'inférence s'exécute (llama-server / rpc-server)
    ↓
Les jetons (tokens) sont renvoyés au consommateur
    ↓
Le Ledger enregistre l'échange :
  - fournisseur.contribué += coût_cu
  - consommateur.consommé += coût_cu
  - trade_log.push(TradeRecord)
    ↓
La réponse inclut x_forge : { coût_cu, solde_effectif }
```

### Export de Règlement

```
L'opérateur lance : forge settle --hours 24
    ↓
L'API lit le trade_log pour la période
    ↓
Agrège par nœud : gain_brut, dépense_brute, cu_net
    ↓
Exporte un relevé JSON avec prix de référence optionnel
    ↓
L'opérateur utilise l'adaptateur de pont pour convertir les CU nets en BTC/fiat
```

## Modèle de Sécurité

```
Couche 0 : Mainchain Bitcoin      ← Ancrage optionnel (futur)
Couche 1 : Doubles signatures     ← Le fournisseur + le consommateur signent chaque échange
Couche 2 : Registre HMAC-SHA256   ← Protection d'intégrité locale
Couche 3 : iroh (QUIC + Noise)    ← Chiffrement du transport
Couche 4 : Exécution d'inférence  ← Le modèle tourne localement chez le fournisseur
```

Chaque couche protège contre des menaces différentes :
- Couche 4 : Intégrité du modèle (vérification du hash GGUF)
- Couche 3 : Confidentialité du transport (écoute clandestine)
- Couche 2 : Altération locale (modification de fichiers)
- Couche 1 : Fraude réseau (fausses réclamations CU)
- Couche 0 : Immuabilité historique (ancrage Bitcoin optionnel)

## Dépendances des Crates

```
forge-core ← types partagés (NodeId, CU, Config)
    ↑
forge-ledger ← moteur économique (échanges, prix, rendement)
    ↑
forge-lightning ← pont externe (portefeuille LDK, CU↔sats)
    ↑
forge-node ← orchestrateur (API HTTP, pipeline, intégration ledger)
    ↑
forge-cli ← CLI de référence (chat, seed, worker, règlement)

forge-net ← transport P2P (iroh, QUIC, Noise, mDNS)
forge-proto ← messages filaires (bincode, 14 types de payloads)
forge-infer ← moteur d'inférence (llama.cpp, chargeur GGUF)
forge-shard ← planificateur de topologie (assignation des couches, rééquilibrage)
```
