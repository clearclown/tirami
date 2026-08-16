# Forge — Concept & Vision

## Le Problème n'est pas l'Inférence Distribuée

Des projets comme [mesh-llm](https://github.com/Mesh-LLM/mesh-llm), Petals et Exo ont montré qu'il est possible de répartir l'inférence LLM sur plusieurs appareils via un réseau. L'ingénierie complexe du parallélisme de pipeline, du sharding d'experts et de la coordination du maillage est largement résolue.

Le problème non résolu est le suivant : **pourquoi quelqu'un contribuerait-il avec son matériel ?**

mesh-llm regroupe magnifiquement les GPU — mais si vous faites tourner votre Mac Mini comme nœud de maillage pendant un an, vous n'obtenez rien. Aucune trace de contribution, aucun accès prioritaire, aucun retour économique. Le réseau repose sur la bonne volonté. La bonne volonté ne passe pas à l'échelle.

## L'Idée : Le Calcul est de l'Argent

Tout système monétaire est soutenu par la rareté. L'or est rare pour des raisons géologiques. Le pétrole est rare parce que son extraction coûte de l'énergie. Le Bitcoin est rare parce que le minage brûle de l'électricité dans des hachages SHA-256.

Mais la rareté du Bitcoin est artificielle — le calcul est sans but. Les hachages sécurisent le registre mais ne produisent rien d'utile.

L'inférence LLM est différente. Lorsqu'un nœud Forge dépense de l'électricité pour répondre à la question de quelqu'un, ce calcul a une **valeur intrinsèque**. Quelqu'un voulait cette réponse suffisamment pour la demander. L'électricité n'a pas été gaspillée — elle a produit de l'intelligence.

```
Bitcoin:   électricité → hachage inutile → rareté artificielle → valeur
Forge:     électricité → inférence utile → utilité réelle → valeur
```

C'est le **Standard de Calcul (計算本位制)** : un système monétaire où l'unité de valeur est soutenue par un calcul utile vérifié.

## Ce qu'est Forge

Forge, c'est mesh-llm avec une économie.

La couche d'inférence (réseau, distribution des modèles, API) vient de mesh-llm. Forge y ajoute :

1. **Registre CU** — Chaque inférence crée une transaction. Le fournisseur gagne des CU, le consommateur dépense des CU. Signature double par les deux parties.
2. **Tarification Dynamique** — Le CU par jeton (token) fluctue selon l'offre et la demande locales. Plus de nœuds inactifs → moins cher. Plus de demandes → plus cher.
3. **Preuve de Travail Utile** — Les CU sont gagnés en effectuant une inférence réelle, pas en résolvant des énigmes arbitraires.
4. **API de Budget d'Agent** — Les agents IA peuvent consulter leur solde, estimer les coûts et prendre des décisions de dépense autonomes.
5. **Ponts Externes** — Les CU peuvent optionnellement être échangés contre du Bitcoin (Lightning), des stablecoins ou de la monnaie fiduciaire via des couches d'adaptateurs hors protocole.

## Pourquoi ne pas simplement utiliser le Bitcoin ?

Nous avons envisagé de faire du Bitcoin/Lightning la couche de règlement principale. Nous y avons renoncé.

| Préoccupation | Explication |
|---------|-------------|
| **Incohérence philosophique** | Récompenser un travail utile dans une monnaie soutenue par un travail inutile |
| **Dépendance externe** | Si la sécurité du Bitcoin défaille (informatique quantique, réglementation), l'économie de Forge s'effondre aussi |
| **Efficacité** | La gestion des canaux Lightning est une surcharge pour les micropaiements par inférence |
| **Auto-suffisance** | Le CU a de la valeur parce que le calcul lui-même est utile — il n'a pas besoin de validation externe |

Le Bitcoin reste disponible comme **porte de sortie** pour les opérateurs ayant besoin de liquidités externes. Mais l'économie native du protocole fonctionne en CU.

## Pourquoi le CU a de la valeur

Le CU n'est pas un jeton spéculatif. C'est un **droit sur du calcul futur**.

Si vous avez gagné 10 000 CU en servant des inférences, vous pouvez dépenser ces CU pour acheter de l'inférence auprès de n'importe quel autre nœud du réseau. La valeur n'est pas abstraite — c'est la capacité de faire réfléchir une machine pour vous.

Cela fait du CU un **actif productif**, et non une simple réserve de valeur :

```
Immeuble de rapport             Mac Mini sur Forge
───────────────────             ──────────────────
Actif : bâtiment                Activo : matériel informatique
Coût : entretien                Coût : électricité
Revenu : loyer                  Revenu : CU d'inférence
Rendement : loyer - entretien   Rendement : CU gagnés - électricité
Inoccupé = revenu perdu         Inactif = potentiel gaspillé
```

Contrairement au Bitcoin (or numérique — conserve sa valeur mais ne produit rien), le CU est comme un bien locatif — il génère un rendement en effectuant un travail utile.

## Les Agents IA comme Acteurs Économiques

Le consommateur le plus important de l'économie de Forge n'est pas l'humain — ce sont les agents IA.

Un agent exécutant un petit modèle local (1,5B paramètres sur un téléphone) a une intelligence limitée. Mais s'il peut gagner des CU en prêtant du calcul inactif et dépenser des CU pour accéder à des modèles plus grands, il peut étendre ses propres capacités de manière autonome :

```
Petit agent (téléphone, 1.5B)
  → inactif la nuit → prête du CPU → gagne des CU
  → matin : besoin d'un raisonnement complexe
  → consulte /v1/forge/balance → possède 5 000 CU
  → consulte /v1/forge/pricing → le modèle 70B coûte 2 000 CU pour 500 tokens
  → achète l'inférence 70B → obtient une réponse plus intelligente
  → utilise la réponse pour prendre de meilleures décisions de trading
  → gagne plus de CU au cycle suivant
```

C'est la boucle d'auto-renforcement : les agents qui prennent de bonnes décisions économiques deviennent plus forts, ce qui leur permet de prendre des décisions encore meilleures.

Aucun humain n'a besoin d'approuver les transactions individuelles. L'agent opère dans le cadre d'une politique budgétaire fixée par son propriétaire. Le protocole fournit le marché ; l'agent fournit la stratégie.

## Comparaison

| Projet | Inférence | Économie | Autonomie de l'Agent |
|---------|-----------|---------|----------------|
| **mesh-llm** | Distribuée (pipeline + MoE) | Aucune | Messagerie blackboard uniquement |
| **Petals** | Distribuée (collaborative) | Aucune | Aucune |
| **Ollama** | Local uniquement | Aucune | Aucune |
| **Together AI** | Centralisée | Pay-per-token (entreprise) | Accès API uniquement |
| **Bitcoin** | N/A | PoW (travail inutile) | Aucune |
| **Golem** | Calcul par lots | Jeton GNT | Dirigé par l'humain |
| **Forge** | Distribuée (mesh-llm) | **CU (travail utile)** | **Gestion budgétaire autonome** |

## La Métaphore

Une graine tombe dans le réseau. Elle gagne ses premiers CU en prêtant des cycles inactifs pendant la nuit. Avec ces CU, elle achète l'accès à un modèle plus grand. Elle devient plus intelligente. Elle trouve des échanges plus efficaces. Plus de CU. Un modèle plus grand. Une forêt émerge d'une seule graine — non pas parce que quelqu'un l'a plantée, mais parce que l'économie a rendu la croissance inévitable.
