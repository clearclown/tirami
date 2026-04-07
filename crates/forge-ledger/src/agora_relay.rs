//! Nostr relay publisher — real WebSocket transport for NIP-90 events built by
//! `agora::Nip90Publisher`.
//!
//! This module is the minimal bridge from "we have well-formed Nostr JSON" to
//! "the event is actually on a public relay". It does NOT implement signing
//! (that's the responsibility of `agora::Nip90Publisher` upstream) and does NOT
//! manage long-lived subscriptions — just one-shot publish.

use crate::agora::AgoraError;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Default public Nostr relay used if the caller doesn't specify one.
pub const DEFAULT_RELAY_URL: &str = "wss://relay.damus.io";

/// Publish one signed Nostr event to a relay and wait for the OK response.
///
/// Returns `Ok(())` on relay accept, or `Err(AgoraError::RelayError(msg))`
/// on connection failure, timeout, or explicit relay rejection.
///
/// The `event` must already be a complete signed Nostr event object (with
/// `id`, `pubkey`, `created_at`, `kind`, `tags`, `content`, `sig`).
pub async fn publish_event(
    relay_url: &str,
    event: &Value,
    timeout_sec: u64,
) -> Result<(), AgoraError> {
    let dur = Duration::from_secs(timeout_sec.max(1));

    let connect_fut = connect_async(relay_url);
    let (ws, _resp) = timeout(dur, connect_fut)
        .await
        .map_err(|_| AgoraError::RelayError("connect timeout".into()))?
        .map_err(|e| AgoraError::RelayError(format!("connect error: {e}")))?;

    let (mut write, mut read) = ws.split();

    // Build ["EVENT", <event>]
    let req = serde_json::json!(["EVENT", event]);
    let req_str = req.to_string();

    timeout(dur, write.send(Message::Text(req_str.into())))
        .await
        .map_err(|_| AgoraError::RelayError("send timeout".into()))?
        .map_err(|e| AgoraError::RelayError(format!("send error: {e}")))?;

    // Read until we see an OK with matching event id, or timeout.
    let event_id = event
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let read_fut = async {
        while let Some(msg) = read.next().await {
            let msg = msg.map_err(|e| AgoraError::RelayError(format!("read error: {e}")))?;
            if let Message::Text(text) = msg {
                if let Ok(resp) = serde_json::from_str::<Value>(&text) {
                    if resp.get(0).and_then(|v| v.as_str()) == Some("OK")
                        && resp.get(1).and_then(|v| v.as_str()) == Some(event_id.as_str())
                    {
                        let accepted =
                            resp.get(2).and_then(|v| v.as_bool()).unwrap_or(false);
                        if accepted {
                            return Ok(());
                        } else {
                            let msg = resp
                                .get(3)
                                .and_then(|v| v.as_str())
                                .unwrap_or("rejected");
                            return Err(AgoraError::RelayError(format!(
                                "relay rejected: {msg}"
                            )));
                        }
                    }
                    // NOTICE / EOSE / other — ignore and keep reading
                }
            }
        }
        Err(AgoraError::RelayError("relay closed without OK".into()))
    };

    timeout(dur, read_fut)
        .await
        .map_err(|_| AgoraError::RelayError("ack timeout".into()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_publish_event_connect_failure_returns_relay_error() {
        // Point at a port nothing listens on — connect must fail fast.
        let fake_event = serde_json::json!({
            "id": "0".repeat(64),
            "pubkey": "0".repeat(64),
            "created_at": 1_700_000_000u64,
            "kind": 31990,
            "tags": [],
            "content": "",
            "sig": "0".repeat(128),
        });
        let result = publish_event("ws://127.0.0.1:1", &fake_event, 2).await;
        assert!(result.is_err(), "expected Err from unreachable port");
        if let Err(AgoraError::RelayError(msg)) = result {
            // Accept any of: "connect", "refused", "timeout", "error"
            let lower = msg.to_lowercase();
            assert!(
                lower.contains("connect") || lower.contains("refused") || lower.contains("error"),
                "unexpected error message: {msg}"
            );
        } else {
            panic!("expected AgoraError::RelayError, got a different variant");
        }
    }

    #[tokio::test]
    async fn test_publish_event_invalid_url_returns_relay_error() {
        let fake_event = serde_json::json!({ "id": "x" });
        let result = publish_event("not-a-url", &fake_event, 2).await;
        assert!(result.is_err(), "expected Err from invalid URL");
        assert!(
            matches!(result, Err(AgoraError::RelayError(_))),
            "expected AgoraError::RelayError"
        );
    }
}
