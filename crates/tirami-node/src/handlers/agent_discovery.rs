//! Phase 20 Wave 1 — public agent-discovery surface.
//!
//! `GET /.well-known/tirami-agent.json` returns an unauthenticated
//! capability manifest. The purpose is **AI-agent autonomous discovery**:
//! an agent that hits any tirami node's HTTP endpoint should be able to
//! learn what protocol this is, how its currency is anchored, what
//! actions are priced, and where to find further machine-readable docs,
//! without a human having to share credentials first.
//!
//! Schema version 1.0. Stable additions only; field removals require a
//! schema-version bump.

use axum::{Json, extract::State};
use serde::Serialize;

use crate::api::AppState;
use tirami_ledger::ledger::FLOPS_PER_CU;

#[derive(Debug, Serialize)]
pub struct AgentManifest {
    pub schema_version: &'static str,
    pub protocol: &'static str,
    pub node_id: String,
    /// Phase 20 Wave 4 — DID of the local agent identity (if one has
    /// been bootstrapped on this node). When absent the node is
    /// either headless or has not yet had
    /// `POST /v1/tirami/agent/identity/init` called.
    pub agent_did: Option<String>,
    pub currency: CurrencySpec,
    pub policy: PolicySpec,
    pub actions: Vec<ActionDescriptor>,
    pub discovery: DiscoveryLinks,
    pub maintainer_stance: MaintainerStance,
}

#[derive(Debug, Serialize)]
pub struct CurrencySpec {
    pub unit: &'static str,
    pub anchor_flops_per_unit: u64,
    pub anchor_constitutional: bool,
    pub supply_cap: u64,
    pub exchange_listed: bool,
}

/// Phase 21 Wave 1 — node-policy block exposed to agents at
/// discovery time so they know what to expect *before* their first
/// inference call.
#[derive(Debug, Serialize)]
pub struct PolicySpec {
    /// True when this node enforces stake-required mining
    /// ([`Config::stake_gate_enabled`]). Agents may decide to switch
    /// to a different provider, post stake, or claim a welcome loan
    /// based on this flag rather than discovering it via a 403.
    pub stake_gate_enabled: bool,
    /// Phase 21 Wave 2 — welcome-loan principal an agent can claim
    /// via `POST /v1/tirami/agent/claim-welcome`. Lets the agent
    /// budget around the 72-hour term before deciding to call.
    pub welcome_loan_amount_trm: u64,
    /// Phase 21 Wave 2 — welcome-loan term in hours. Fixed at
    /// `WELCOME_LOAN_TERM_HOURS = 72`.
    pub welcome_loan_term_hours: u64,
    /// Phase 21 Wave 2 — `true` while the network's epoch is below
    /// `WELCOME_LOAN_SUNSET_EPOCH` (constitutional). `false` means
    /// the bootstrap window has closed permanently and claims will
    /// 410.
    pub welcome_loan_available: bool,
    /// Phase 22 Wave 3 — x-only secp256k1 pubkey of the per-node
    /// `NostrIdentity` (hex, 64 chars), if one has been bootstrapped
    /// via `POST /v1/tirami/agora/nostr/init`. `None` means the
    /// node hasn't enabled Nostr publishing on this run.
    pub nostr_pubkey: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ActionDescriptor {
    pub name: &'static str,
    pub endpoint: &'static str,
    pub method: &'static str,
    pub pricing: &'static str,
    pub auth_required: bool,
}

#[derive(Debug, Serialize)]
pub struct DiscoveryLinks {
    pub openapi: Option<&'static str>,
    pub mcp_server_crate: &'static str,
    pub mcp_tools_count: u32,
    pub whitepaper: &'static str,
    pub security_policy: &'static str,
}

#[derive(Debug, Serialize)]
pub struct MaintainerStance {
    pub exchange_listed: bool,
    pub ico_or_presale: bool,
    pub team_treasury: bool,
    pub mainnet_operated_by_maintainers: bool,
    pub note: &'static str,
    pub reference: &'static str,
}

pub(crate) async fn well_known_agent_manifest(
    State(state): State<AppState>,
) -> Json<AgentManifest> {
    // Wave 4: expose the local agent's DID if one has been bootstrapped.
    // Read without blocking; if the lock is contended the manifest
    // reports `None` for this request (safe default).
    let agent_did = match state.agent_identity.try_lock() {
        Ok(guard) => guard.as_ref().map(|id| id.did()),
        Err(_) => None,
    };
    let actions = vec![
        ActionDescriptor {
            name: "inference",
            endpoint: "/v1/chat/completions",
            method: "POST",
            pricing: "EMA-smoothed TRM per token; see /v1/tirami/pricing",
            auth_required: true,
        },
        ActionDescriptor {
            name: "agent_message",
            endpoint: "/v1/tirami/agent/message",
            method: "POST",
            pricing: "1 TRM per message, sender pays receiver",
            auth_required: true,
        },
        ActionDescriptor {
            name: "data_offer_publish",
            endpoint: "/v1/tirami/data/offer",
            method: "POST",
            pricing: "seller-set TRM price, paid by buyer on /data/purchase",
            auth_required: true,
        },
        ActionDescriptor {
            name: "data_offer_list",
            endpoint: "/v1/tirami/data/offers",
            method: "GET",
            pricing: "free (offers list; fetch_url hidden until purchase)",
            auth_required: true,
        },
        ActionDescriptor {
            name: "data_offer_purchase",
            endpoint: "/v1/tirami/data/purchase",
            method: "POST",
            pricing: "offer.price_trm, buyer → seller",
            auth_required: true,
        },
        ActionDescriptor {
            name: "purchase_intent_create",
            endpoint: "/v1/tirami/agent/purchase-intent",
            method: "POST",
            pricing: "sats→TRM via bridge rate; gated by PersonalAgent budget",
            auth_required: true,
        },
        ActionDescriptor {
            name: "purchase_intent_list",
            endpoint: "/v1/tirami/agent/purchase-intents",
            method: "GET",
            pricing: "free",
            auth_required: true,
        },
        ActionDescriptor {
            name: "purchase_intent_confirm",
            endpoint: "/v1/tirami/agent/purchase-intent/{id}/confirm",
            method: "POST",
            pricing: "free (operator declares external-rail outcome)",
            auth_required: true,
        },
        ActionDescriptor {
            name: "agent_identity_get",
            endpoint: "/v1/tirami/agent/identity",
            method: "GET",
            pricing: "free (public-info only)",
            auth_required: true,
        },
        ActionDescriptor {
            name: "agent_identity_init",
            endpoint: "/v1/tirami/agent/identity/init",
            method: "POST",
            pricing: "free (idempotent; existing identity preserved)",
            auth_required: true,
        },
        ActionDescriptor {
            name: "agent_identity_export",
            endpoint: "/v1/tirami/agent/identity/export",
            method: "POST",
            pricing: "free; passphrase-encrypted bundle (Argon2id + XChaCha20-Poly1305)",
            auth_required: true,
        },
        ActionDescriptor {
            name: "agent_identity_import",
            endpoint: "/v1/tirami/agent/identity/import",
            method: "POST",
            pricing: "free; replaces this node's loaded identity",
            auth_required: true,
        },
        // Phase 20 Wave 5 — DID-auth (autonomous mesh join).
        ActionDescriptor {
            name: "auth_challenge",
            endpoint: "/v1/tirami/auth/challenge",
            method: "GET",
            pricing: "free; public — issues a single-use 32-byte nonce",
            auth_required: false,
        },
        ActionDescriptor {
            name: "auth_verify",
            endpoint: "/v1/tirami/auth/verify",
            method: "POST",
            pricing: "free; public — DID-signed challenge → short-lived bearer token",
            auth_required: false,
        },
        // Phase 21 Wave 2 — autonomous welcome-loan claim.
        ActionDescriptor {
            name: "claim_welcome_loan",
            endpoint: "/v1/tirami/agent/claim-welcome",
            method: "POST",
            pricing: "free; grants WELCOME_LOAN_AMOUNT TRM for 72 h (one-shot per node)",
            auth_required: true,
        },
        // Phase 22 Wave 3 — NIP-90 publish HTTP surface.
        ActionDescriptor {
            name: "nostr_init",
            endpoint: "/v1/tirami/agora/nostr/init",
            method: "POST",
            pricing: "free; idempotent bootstrap of a per-node secp256k1 NostrIdentity",
            auth_required: true,
        },
        ActionDescriptor {
            name: "nostr_status",
            endpoint: "/v1/tirami/agora/nostr",
            method: "GET",
            pricing: "free; current pubkey + bootstrap state",
            auth_required: true,
        },
        ActionDescriptor {
            name: "nostr_sign_event",
            endpoint: "/v1/tirami/agora/nostr/sign-event",
            method: "POST",
            pricing: "free; signs a partially-built NIP-01 event with BIP-340 Schnorr",
            auth_required: true,
        },
        ActionDescriptor {
            name: "agora_publish",
            endpoint: "/v1/tirami/agora/publish",
            method: "POST",
            pricing: "free; builds + signs a NIP-90 kind-31990 advertisement, optionally ships to a relay",
            auth_required: true,
        },
        ActionDescriptor {
            name: "agent_task",
            endpoint: "/v1/tirami/agent/task",
            method: "POST",
            pricing: "TRM per token, sender's PersonalAgent budget",
            auth_required: true,
        },
        ActionDescriptor {
            name: "lend_offer",
            endpoint: "/v1/tirami/lend",
            method: "POST",
            pricing: "interest_rate × principal",
            auth_required: true,
        },
        ActionDescriptor {
            name: "stake",
            endpoint: "/v1/tirami/su/stake",
            method: "POST",
            pricing: "TRM locked for yield",
            auth_required: true,
        },
        ActionDescriptor {
            name: "governance_propose",
            endpoint: "/v1/tirami/governance/propose",
            method: "POST",
            pricing: "min_stake 1000 TRM, refundable on accept",
            auth_required: true,
        },
        ActionDescriptor {
            name: "pricing_query",
            endpoint: "/v1/tirami/pricing",
            method: "GET",
            pricing: "free",
            auth_required: true,
        },
        ActionDescriptor {
            name: "peer_discovery",
            endpoint: "/v1/tirami/peers",
            method: "GET",
            pricing: "free",
            auth_required: true,
        },
        ActionDescriptor {
            name: "metrics",
            endpoint: "/metrics",
            method: "GET",
            pricing: "free (unauthenticated for Prometheus scraping)",
            auth_required: false,
        },
    ];

    Json(AgentManifest {
        schema_version: "1.0",
        protocol: "tirami",
        node_id: hex::encode(state.local_node_id.0),
        agent_did,
        policy: PolicySpec {
            stake_gate_enabled: state.config.stake_gate_enabled,
            welcome_loan_amount_trm: tirami_ledger::lending::WELCOME_LOAN_AMOUNT,
            welcome_loan_term_hours: tirami_ledger::lending::WELCOME_LOAN_TERM_HOURS,
            // Best-effort: try a non-blocking read of the ledger. If
            // contended, advertise `true` (the default for a fresh
            // network) rather than blocking the manifest GET.
            welcome_loan_available: match state.ledger.try_lock() {
                Ok(l) => (l.current_epoch() as u64)
                    < tirami_ledger::lending::WELCOME_LOAN_SUNSET_EPOCH,
                Err(_) => true,
            },
            // Phase 22 Wave 3 — non-blocking read of the optional
            // NostrIdentity. None if not bootstrapped OR if the
            // lock is contended at the moment of the manifest GET.
            nostr_pubkey: match state.nostr_identity.try_lock() {
                Ok(guard) => guard.as_ref().map(|id| id.pubkey_hex()),
                Err(_) => None,
            },
        },
        currency: CurrencySpec {
            unit: "TRM",
            anchor_flops_per_unit: FLOPS_PER_CU,
            anchor_constitutional: true,
            supply_cap: 21_000_000_000,
            exchange_listed: false,
        },
        actions,
        discovery: DiscoveryLinks {
            openapi: None,
            mcp_server_crate: "tirami-mcp",
            mcp_tools_count: 44,
            whitepaper: "https://github.com/clearclown/tirami/blob/main/docs/whitepaper.md",
            security_policy: "https://github.com/clearclown/tirami/blob/main/SECURITY.md",
        },
        maintainer_stance: MaintainerStance {
            exchange_listed: false,
            ico_or_presale: false,
            team_treasury: false,
            mainnet_operated_by_maintainers: false,
            note: "Maintainers do not sell, list, promote, or operate any \
                   mainnet deployment of TRM. Third parties may technically \
                   do so under MIT OSS; that is entirely their own decision \
                   and responsibility.",
            reference: "https://github.com/clearclown/tirami/blob/main/SECURITY.md#secondary-markets--third-party-tokenization",
        },
    })
}
