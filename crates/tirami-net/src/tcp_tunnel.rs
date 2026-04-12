//! QUIC ↔ TCP bidirectional tunnel.
//!
//! Bridges llama.cpp's raw TCP RPC protocol through Forge's encrypted
//! Iroh QUIC transport. This gives the RPC protocol encryption,
//! authentication, and NAT traversal for free.
//!
//! Architecture:
//! ```text
//! Seed (llama-cpp-2)           Peer (rpc-server)
//!   │                            │
//!   │ TCP connect localhost:A    │ TCP listen localhost:B
//!   │         │                  │         ▲
//!   │         ▼                  │         │
//!   │  [TCP Listener :A]        │  [TCP Connector :B]
//!   │         │                  │         ▲
//!   │         ▼                  │         │
//!   │  QUIC stream ═══════════►  QUIC stream
//!   │     (encrypted)            │  (encrypted)
//! ```

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Start a TCP-to-QUIC tunnel on the **seed side**.
///
/// Listens on `local_port` for TCP connections (from llama-cpp-2).
/// For each TCP connection, opens a QUIC stream to the peer and
/// relays bytes bidirectionally.
pub async fn start_seed_tunnel(
    local_port: u16,
    peer_conn: crate::connection::PeerConnection,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", local_port)).await?;
    tracing::info!("RPC tunnel listening on 127.0.0.1:{}", local_port);

    let handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((tcp_stream, addr)) => {
                    tracing::debug!("RPC tunnel: TCP connection from {}", addr);
                    let peer = peer_conn.clone();

                    tokio::spawn(async move {
                        if let Err(e) = relay_tcp_to_quic(tcp_stream, peer).await {
                            tracing::debug!("RPC tunnel relay error: {}", e);
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

/// Start a QUIC-to-TCP tunnel on the **peer side**.
///
/// Accepts raw byte streams from the QUIC connection and forwards
/// them to the local rpc-server TCP port.
pub async fn start_peer_tunnel(
    rpc_server_port: u16,
    peer_conn: crate::connection::PeerConnection,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let rpc_addr = format!("127.0.0.1:{}", rpc_server_port);
    tracing::info!("RPC tunnel forwarding to {}", rpc_addr);

    let handle = tokio::spawn(async move {
        loop {
            match peer_conn.recv_raw().await {
                Ok((data, responder)) => {
                    let rpc_addr = rpc_addr.clone();

                    tokio::spawn(async move {
                        match relay_to_rpc_server(&rpc_addr, &data).await {
                            Ok(response) => {
                                if let Err(e) = responder.respond(&response).await {
                                    tracing::debug!("RPC tunnel respond error: {}", e);
                                }
                            }
                            Err(e) => {
                                tracing::warn!("RPC tunnel forward error: {}", e);
                                let _ = responder.respond(b"").await;
                            }
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

/// Relay a single TCP connection through a QUIC peer connection.
async fn relay_tcp_to_quic(
    mut tcp: TcpStream,
    peer: crate::connection::PeerConnection,
) -> anyhow::Result<()> {
    let mut buf = vec![0u8; 64 * 1024]; // 64KB buffer

    loop {
        let n = tcp.read(&mut buf).await?;
        if n == 0 {
            break; // TCP connection closed
        }

        // Send through QUIC and get response
        let response = peer.send_raw(&buf[..n]).await?;

        if !response.is_empty() {
            tcp.write_all(&response).await?;
        }
    }

    Ok(())
}

/// Forward data to a local rpc-server and return the response.
async fn relay_to_rpc_server(rpc_addr: &str, data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut stream = TcpStream::connect(rpc_addr).await?;
    stream.write_all(data).await?;

    let mut response = Vec::new();
    let mut buf = [0u8; 64 * 1024];

    // Read response with a short timeout (RPC responses are fast)
    match tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            match stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => response.extend_from_slice(&buf[..n]),
                Err(e) => return Err(e),
            }
            // RPC protocol is request-response, so we read until
            // the server has sent its response. We use a small delay
            // to detect end-of-response.
            if tokio::time::timeout(std::time::Duration::from_millis(10), stream.read(&mut buf))
                .await
                .is_err()
            {
                break;
            }
        }
        Ok(())
    })
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e.into()),
        Err(_) => anyhow::bail!("RPC server response timeout"),
    }

    Ok(response)
}
