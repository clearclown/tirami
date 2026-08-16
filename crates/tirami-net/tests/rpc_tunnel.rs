//! Integration test: the llama.cpp RPC tunnel actually carries bytes.
//!
//! The tunnel it replaces (#163) had four independent defects. These tests
//! target the two that silently corrupted data rather than merely failing:
//! ending a response after 10 ms of silence, and dropping up to 64 KB per
//! read iteration.

use std::collections::HashMap;
use std::sync::Arc;
use tirami_net::ForgeTransport;
use tirami_net::tcp_tunnel::{SessionPorts, start_peer_tunnel, start_seed_tunnel};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

fn is_socket_bind_denied(err: &anyhow::Error) -> bool {
    let message = format!("{err:#}");
    message.contains("Operation not permitted") || message.contains("Permission denied")
}

async fn transport_or_skip(label: &str) -> Option<ForgeTransport> {
    match ForgeTransport::new().await {
        Ok(transport) => Some(transport),
        Err(err) if is_socket_bind_denied(&err) => {
            eprintln!("skipping RPC tunnel test: {label}: {err:#}");
            None
        }
        Err(err) => panic!("{label}: {err:#}"),
    }
}

/// Stand-in for `rpc-server`: echoes every byte back on the same connection,
/// and — like the real thing — keeps per-connection state. Returns the port
/// plus a handle to how many connections it saw.
async fn spawn_echo_server() -> anyhow::Result<(u16, Arc<Mutex<usize>>)> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    let connections = Arc::new(Mutex::new(0usize));
    let counter = connections.clone();

    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            *counter.lock().await += 1;
            tokio::spawn(async move {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    match sock.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if sock.write_all(&buf[..n]).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        }
    });

    Ok((port, connections))
}

/// Wire a seed tunnel and a peer tunnel together over a real QUIC connection.
/// Returns the local port a client should connect to.
/// `seed_session` is what the seed stamps on its streams; `registered_session`
/// is what the serving side knows about. They differ only in the negative test.
async fn setup_tunnel(
    seed: &ForgeTransport,
    peer: Arc<ForgeTransport>,
    seed_session: u64,
    registered_session: u64,
    rpc_port: u16,
) -> anyhow::Result<u16> {
    let _accept = peer.start_accepting();

    let sessions: SessionPorts = Arc::new(Mutex::new(HashMap::new()));
    sessions.lock().await.insert(registered_session, rpc_port);
    let session_id = seed_session;

    // Serving side: take the inbound tunnel connection and pipe its streams
    // to the rpc-server registered for the session.
    let sessions_for_peer = sessions.clone();
    let peer_addr = peer.endpoint_addr();
    let accept_task = {
        let peer = Arc::clone(&peer);
        tokio::spawn(async move {
            if let Some(conn) = peer.accept_rpc_tunnel().await {
                let _ = start_peer_tunnel(conn, sessions_for_peer).await;
                // Hold the connection open for the duration of the test.
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        })
    };

    let conn = seed.connect_rpc_tunnel(peer_addr).await?;

    // Bind an ephemeral local port for the seed side.
    let probe = TcpListener::bind(("127.0.0.1", 0)).await?;
    let local_port = probe.local_addr()?.port();
    drop(probe);

    start_seed_tunnel(local_port, session_id, conn).await?;
    // Let both sides settle before the client dials.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    std::mem::forget(accept_task);
    Ok(local_port)
}

/// The headline case: a payload larger than the old 64 KB buffer, sent in
/// chunks separated by pauses longer than the old 10 ms idle cutoff.
///
/// Under the previous tunnel this would have been truncated at the first
/// pause, and each non-idle read would have silently discarded a buffer's
/// worth of bytes. Every byte must come back, in order.
#[tokio::test]
async fn chunked_payload_with_idle_gaps_survives_intact() {
    let Some(seed) = transport_or_skip("seed").await else {
        return;
    };
    let Some(peer) = transport_or_skip("peer").await else {
        seed.close().await;
        return;
    };
    let peer = Arc::new(peer);

    let (rpc_port, connections) = spawn_echo_server().await.expect("echo server");
    let local_port = match setup_tunnel(&seed, Arc::clone(&peer), 42, 42, rpc_port).await {
        Ok(port) => port,
        Err(err) if is_socket_bind_denied(&err) => {
            eprintln!("skipping RPC tunnel test: {err:#}");
            return;
        }
        Err(err) => panic!("tunnel setup: {err:#}"),
    };

    let mut client = TcpStream::connect(("127.0.0.1", local_port))
        .await
        .expect("connect through tunnel");

    // 6 chunks × 32 KiB = 192 KiB, well past the old 64 KiB buffer, with a
    // 25 ms gap between them — more than twice the old cutoff.
    const CHUNK: usize = 32 * 1024;
    const CHUNKS: usize = 6;
    let mut expected = Vec::with_capacity(CHUNK * CHUNKS);

    let (mut client_read, mut client_write) = client.split();
    let writer = async {
        for i in 0..CHUNKS {
            let chunk = vec![i as u8; CHUNK];
            client_write.write_all(&chunk).await?;
            client_write.flush().await?;
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        Ok::<(), std::io::Error>(())
    };
    for i in 0..CHUNKS {
        expected.extend(std::iter::repeat_n(i as u8, CHUNK));
    }

    let mut received = vec![0u8; CHUNK * CHUNKS];
    let reader = client_read.read_exact(&mut received);

    let (w, r) = tokio::join!(writer, reader);
    w.expect("write through tunnel");
    r.expect("read back every byte");

    assert_eq!(
        received, expected,
        "tunnel must deliver every byte in order across idle gaps"
    );

    // One TCP connection to the rpc-server for the whole session — the old
    // tunnel opened a fresh one per frame, which loses rpc-server state.
    assert_eq!(
        *connections.lock().await,
        1,
        "tunnel must hold a single rpc-server connection per session"
    );

    drop(client);
    seed.close().await;
    peer.close().await;
}

/// A stream naming a session the serving side does not know must be refused,
/// not quietly connected to whatever happens to be listening.
#[tokio::test]
async fn unknown_session_does_not_reach_a_port() {
    let Some(seed) = transport_or_skip("seed").await else {
        return;
    };
    let Some(peer) = transport_or_skip("peer").await else {
        seed.close().await;
        return;
    };
    let peer = Arc::new(peer);

    let (rpc_port, connections) = spawn_echo_server().await.expect("echo server");
    // The seed stamps session 999; the serving side only knows about 1.
    let local_port = match setup_tunnel(&seed, Arc::clone(&peer), 999, 1, rpc_port).await {
        Ok(port) => port,
        Err(err) if is_socket_bind_denied(&err) => {
            eprintln!("skipping RPC tunnel test: {err:#}");
            return;
        }
        Err(err) => panic!("tunnel setup: {err:#}"),
    };

    let mut client = TcpStream::connect(("127.0.0.1", local_port))
        .await
        .expect("connect through tunnel");
    client.write_all(b"hello").await.expect("write");

    // A known session would echo "hello" straight back. This one must not.
    let mut buf = [0u8; 8];
    let read =
        tokio::time::timeout(std::time::Duration::from_millis(750), client.read(&mut buf)).await;

    match read {
        // EOF, error, or nothing at all are all correct outcomes.
        Ok(Ok(0)) | Ok(Err(_)) | Err(_) => {}
        Ok(Ok(n)) => panic!("unknown session returned {n} bytes: {:?}", &buf[..n]),
    }

    assert_eq!(
        *connections.lock().await,
        0,
        "an unregistered session must never open a connection to the rpc-server"
    );

    seed.close().await;
    peer.close().await;
}
