//! Phase 20 Wave 4 — HTTP surface for portable agent identity.
//!
//! Four endpoints:
//!
//! - `GET  /v1/tirami/agent/identity` — return the local agent's DID
//!   + public key (never the private key).
//! - `POST /v1/tirami/agent/identity/init` — bootstrap a fresh
//!   identity if none exists. Idempotent: if one is already loaded
//!   the existing one is returned untouched.
//! - `POST /v1/tirami/agent/identity/export` — `{ passphrase }` →
//!   encrypted [`AgentIdentityBundle`] suitable for sending to another
//!   node.
//! - `POST /v1/tirami/agent/identity/import` — `{ passphrase, bundle }`
//!   → load the imported identity into this node's slot, replacing
//!   any previously-loaded identity.
//!
//! Storage in [`AppState`]: `Arc<Mutex<Option<AgentIdentity>>>`. The
//! identity is in-memory only for Wave 4; on-disk persistence is a
//! separate concern that will reuse the existing
//! `personal_agent_state_path` snapshot path.

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use tirami_mind::{AgentIdentity, AgentIdentityBundle, AgentIdentityError};

use crate::api::{AppState, check_forge_rate_limit, now_millis_pub};

pub type AgentIdentityState = Arc<Mutex<Option<AgentIdentity>>>;

// ---------------------------------------------------------------------------
// Response shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct IdentityPublicView {
    pub did: String,
    pub public_key_hex: String,
    pub display_name: Option<String>,
    pub created_at_ms: u64,
}

impl From<&AgentIdentity> for IdentityPublicView {
    fn from(id: &AgentIdentity) -> Self {
        Self {
            did: id.did(),
            public_key_hex: id.public_key_hex(),
            display_name: id.display_name.clone(),
            created_at_ms: id.created_at_ms,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct InitRequest {
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    pub passphrase: String,
}

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub passphrase: String,
    pub bundle: AgentIdentityBundle,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

fn map_err(e: AgentIdentityError) -> (StatusCode, String) {
    match e {
        AgentIdentityError::PassphraseTooShort => {
            (StatusCode::BAD_REQUEST, e.to_string())
        }
        AgentIdentityError::Aead(_) => {
            // AEAD decrypt failures during import are 400-class —
            // most plausibly the passphrase is wrong.
            (StatusCode::BAD_REQUEST, e.to_string())
        }
        AgentIdentityError::BundleSchema(_) | AgentIdentityError::DidFormat(_) => {
            (StatusCode::BAD_REQUEST, e.to_string())
        }
        AgentIdentityError::SignatureInvalid(_) => {
            (StatusCode::BAD_REQUEST, e.to_string())
        }
        AgentIdentityError::Kdf(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    }
}

/// `GET /v1/tirami/agent/identity`
pub(crate) async fn get_identity(
    State(state): State<AppState>,
) -> Result<Json<IdentityPublicView>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let guard = state.agent_identity.lock().await;
    match guard.as_ref() {
        Some(id) => Ok(Json(IdentityPublicView::from(id))),
        None => Err((
            StatusCode::NOT_FOUND,
            "no agent identity configured; POST /v1/tirami/agent/identity/init to bootstrap".into(),
        )),
    }
}

/// `POST /v1/tirami/agent/identity/init`
///
/// Bootstrap a fresh identity. Idempotent: if one already exists the
/// existing one is returned untouched and the request is otherwise
/// a no-op.
pub(crate) async fn init_identity(
    State(state): State<AppState>,
    Json(req): Json<InitRequest>,
) -> Result<Json<IdentityPublicView>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let mut guard = state.agent_identity.lock().await;
    if let Some(existing) = guard.as_ref() {
        return Ok(Json(IdentityPublicView::from(existing)));
    }
    // Display-name sanity caps.
    if let Some(ref name) = req.display_name {
        if name.len() > 128 {
            return Err((
                StatusCode::BAD_REQUEST,
                "display_name must be ≤ 128 characters".into(),
            ));
        }
    }
    let id = AgentIdentity::generate(now_millis_pub(), req.display_name);
    let view = IdentityPublicView::from(&id);
    *guard = Some(id);
    Ok(Json(view))
}

/// `POST /v1/tirami/agent/identity/export` — `{ passphrase }` →
/// encrypted bundle.
pub(crate) async fn export_identity(
    State(state): State<AppState>,
    Json(req): Json<ExportRequest>,
) -> Result<Json<AgentIdentityBundle>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let guard = state.agent_identity.lock().await;
    let id = guard.as_ref().ok_or((
        StatusCode::NOT_FOUND,
        "no agent identity to export; init one first".into(),
    ))?;
    let bundle = id.export(&req.passphrase).map_err(map_err)?;
    Ok(Json(bundle))
}

/// `POST /v1/tirami/agent/identity/import`
///
/// Loads the imported identity into the node's slot, **replacing**
/// any previously-loaded identity. The caller is presumed to know
/// they're switching identities; we do not require a confirm flag
/// because the passphrase already gates this.
pub(crate) async fn import_identity(
    State(state): State<AppState>,
    Json(req): Json<ImportRequest>,
) -> Result<Json<IdentityPublicView>, (StatusCode, String)> {
    check_forge_rate_limit(&state).await?;
    let imported = AgentIdentity::import(&req.bundle, &req.passphrase).map_err(map_err)?;
    let view = IdentityPublicView::from(&imported);
    let mut guard = state.agent_identity.lock().await;
    *guard = Some(imported);
    Ok(Json(view))
}
