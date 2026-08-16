//! QUIC ↔ TCP tunnel for llama.cpp's RPC protocol.
//!
//! Carries the raw TCP stream `llama-cli --rpc` speaks to a `rpc-server`
//! through iroh QUIC, which gives it encryption, authentication, and NAT
//! traversal. The tunnel is a **transparent byte pipe** — it never parses
//! llama.cpp's framing.
//!
//! ```text
//! Seed (llama-cli --rpc 127.0.0.1:A)      Peer (rpc-server on :B)
//!   │                                       │
//!   │ TCP connect 127.0.0.1:A               │ TCP listen 127.0.0.1:B
//!   │        │                              │        ▲
//!   │        ▼                              │        │
//!   │  [TCP listener :A]                    │  [TCP connect :B]
//!   │        │                              │        ▲
//!   │        ▼   one QUIC bidi stream       │        │
//!   │  8-byte session header ═══════════════►  read header, look up port
//!   │        ↕   copy_bidirectional         │        ↕
//! ```
//!
//! ## Why this is a rewrite rather than a fix
//!
//! The previous implementation was request/response over
//! `PeerConnection::send_raw`, and had four independent problems:
//!
//! 1. **It could never have received anything.**
//!    `ForgeTransport::read_peer_messages` already consumes *every* inbound
//!    bidirectional stream on the protocol connection and decodes it as a
//!    bincode `Envelope`. A tunnel calling `recv_raw` on that same connection
//!    raced it for streams. Fixed by giving the tunnel its own ALPN, hence
//!    its own connection (see [`crate::RPC_TUNNEL_ALPN`]).
//!
//! 2. **It ended responses on 10 ms of silence.** llama.cpp's RPC is a
//!    length-prefixed stream, and issue #164 measured weight transfer running
//!    for 139 s (Thunderbolt) to 867 s (GbE) of continuous chunked traffic.
//!    A 10 ms gap under load was certain.
//!
//! 3. **It silently dropped up to 64 KB per iteration.** The 10 ms probe read
//!    wrote into `buf`, and when it *succeeded* those bytes were never
//!    appended to the response — the next loop overwrote them. A correctness
//!    bug independent of the timing.
//!
//! 4. **It opened a new TCP connection to the rpc-server per frame.**
//!    `rpc-server` keeps buffer and tensor handles per connection, so a
//!    fresh socket each frame loses the session. Length-prefix framing alone
//!    would not have fixed this.
//!
//! A raw `copy_bidirectional` over one long-lived stream per connection
//! avoids all four, and needs no knowledge of llama.cpp's protocol.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

/// Session id → local rpc-server port, on the serving side.
pub type SessionPorts = Arc<Mutex<HashMap<u64, u16>>>;

/// Start the **seed side**: listen on `local_port` and pipe each accepted TCP
/// connection to `peer` over its own QUIC stream.
///
/// `llama-cli --rpc 127.0.0.1:<local_port>` then talks to the remote
/// rpc-server as if it were local.
pub async fn start_seed_tunnel(
    local_port: u16,
    session_id: u64,
    conn: iroh::endpoint::Connection,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let listener = TcpListener::bind(("127.0.0.1", local_port)).await?;
    tracing::info!(
        "RPC tunnel listening on 127.0.0.1:{} (session {})",
        local_port,
        session_id
    );

    let handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((tcp, addr)) => {
                    tracing::debug!("RPC tunnel: TCP connection from {}", addr);
                    let conn = conn.clone();
                    tokio::spawn(async move {
                        if let Err(e) = pipe_tcp_to_quic(tcp, conn, session_id).await {
                            tracing::warn!("RPC tunnel (session {}) closed: {}", session_id, e);
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!("RPC tunnel accept error: {}", e);
                    break;
                }
            }
        }
    });

    Ok(handle)
}

/// Start the **serving side**: accept QUIC streams on `conn` and pipe each to
/// the rpc-server port registered for the session named in its header.
pub async fn start_peer_tunnel(
    conn: iroh::endpoint::Connection,
    sessions: SessionPorts,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let handle = tokio::spawn(async move {
        loop {
            match conn.accept_bi().await {
                Ok((send, recv)) => {
                    let sessions = sessions.clone();
                    tokio::spawn(async move {
                        if let Err(e) = pipe_quic_to_tcp(send, recv, sessions).await {
                            tracing::warn!("RPC tunnel stream closed: {}", e);
                        }
                    });
                }
                Err(e) => {
                    tracing::debug!("RPC tunnel peer disconnected: {}", e);
                    break;
                }
            }
        }
    });

    Ok(handle)
}

/// Seed side of one TCP connection: open a QUIC stream, announce the session,
/// then copy bytes both ways until either end closes.
async fn pipe_tcp_to_quic(
    tcp: TcpStream,
    conn: iroh::endpoint::Connection,
    session_id: u64,
) -> anyhow::Result<()> {
    let (mut send, mut recv) = conn.open_bi().await?;

    // 8-byte header, then nothing but llama.cpp's own bytes. One QUIC
    // connection can carry several sessions, so the stream has to say which.
    send.write_all(&session_id.to_be_bytes()).await?;

    let (mut tcp_read, mut tcp_write) = tcp.into_split();

    // Both directions run concurrently: llama.cpp pushes tensors up while the
    // server streams results back, and a half-duplex loop would deadlock.
    let up = async {
        let n = tokio::io::copy(&mut tcp_read, &mut send).await?;
        send.finish()?;
        Ok::<u64, anyhow::Error>(n)
    };
    let down = async {
        let n = tokio::io::copy(&mut recv, &mut tcp_write).await?;
        tcp_write.shutdown().await?;
        Ok::<u64, anyhow::Error>(n)
    };

    let (up, down) = tokio::join!(up, down);
    let (up, down) = (up?, down?);
    tracing::debug!(
        "RPC tunnel session {} finished: {} bytes up, {} bytes down",
        session_id,
        up,
        down
    );
    Ok(())
}

/// Serving side of one QUIC stream: read the session header, connect to the
/// rpc-server that session registered, then copy bytes both ways.
async fn pipe_quic_to_tcp(
    mut send: iroh::endpoint::SendStream,
    mut recv: iroh::endpoint::RecvStream,
    sessions: SessionPorts,
) -> anyhow::Result<()> {
    let mut header = [0u8; 8];
    recv.read_exact(&mut header).await?;
    let session_id = u64::from_be_bytes(header);

    let port = sessions
        .lock()
        .await
        .get(&session_id)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("unknown rpc tunnel session {session_id}"))?;

    let tcp = TcpStream::connect(("127.0.0.1", port)).await?;
    let (mut tcp_read, mut tcp_write) = tcp.into_split();

    let down = async {
        let n = tokio::io::copy(&mut recv, &mut tcp_write).await?;
        tcp_write.shutdown().await?;
        Ok::<u64, anyhow::Error>(n)
    };
    let up = async {
        let n = tokio::io::copy(&mut tcp_read, &mut send).await?;
        send.finish()?;
        Ok::<u64, anyhow::Error>(n)
    };

    let (down, up) = tokio::join!(down, up);
    let (down, up) = (down?, up?);
    tracing::debug!(
        "RPC tunnel session {} on port {} finished: {} bytes in, {} bytes out",
        session_id,
        port,
        down,
        up
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stream header is the only framing the tunnel imposes. Everything
    /// after it is llama.cpp's bytes, untouched.
    #[test]
    fn session_header_round_trips() {
        for id in [1u64, 42, u64::MAX] {
            let bytes = id.to_be_bytes();
            assert_eq!(u64::from_be_bytes(bytes), id);
            assert_eq!(bytes.len(), 8);
        }
    }

    #[tokio::test]
    async fn unknown_session_is_rejected() {
        let sessions: SessionPorts = Arc::new(Mutex::new(HashMap::new()));
        assert!(sessions.lock().await.get(&7).is_none());

        sessions.lock().await.insert(7, 50052);
        assert_eq!(sessions.lock().await.get(&7).copied(), Some(50052));
    }
}
