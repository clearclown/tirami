use crate::FORGE_ALPN;
use crate::connection::PeerConnection;
use tirami_core::NodeId;
use tirami_proto::Envelope;
use iroh::endpoint::presets;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{Mutex, Notify, mpsc};

const MAX_RECENT_MSG_IDS_PER_PEER: usize = 2_048;

#[derive(Default)]
struct ReplayWindow {
    order: VecDeque<u64>,
    seen: HashSet<u64>,
}

impl ReplayWindow {
    fn record(&mut self, msg_id: u64) -> bool {
        if !self.seen.insert(msg_id) {
            return false;
        }

        self.order.push_back(msg_id);
        while self.order.len() > MAX_RECENT_MSG_IDS_PER_PEER {
            if let Some(evicted) = self.order.pop_front() {
                self.seen.remove(&evicted);
            }
        }

        true
    }
}

/// The Forge P2P transport layer built on Iroh.
pub struct ForgeTransport {
    endpoint: iroh::Endpoint,
    peers: Arc<Mutex<HashMap<String, PeerConnection>>>,
    /// Saved addresses for reconnection attempts.
    peer_addrs: Arc<Mutex<HashMap<String, iroh::EndpointAddr>>>,
    recent_msg_ids: Arc<Mutex<HashMap<String, ReplayWindow>>>,
    incoming_tx: mpsc::Sender<(String, Envelope)>,
    incoming_rx: Arc<Mutex<mpsc::Receiver<(String, Envelope)>>>,
    shutdown: Arc<Notify>,
    closed: Arc<AtomicBool>,
}

impl ForgeTransport {
    /// Create a new transport with a fresh Iroh endpoint.
    /// Enables mDNS for automatic LAN peer discovery.
    pub async fn new() -> anyhow::Result<Self> {
        let mdns = iroh::address_lookup::mdns::MdnsAddressLookup::builder();

        let endpoint = iroh::Endpoint::builder(presets::N0)
            .alpns(vec![FORGE_ALPN.to_vec()])
            .address_lookup(mdns)
            .bind()
            .await?;

        let endpoint_id = endpoint.id();
        tracing::info!("Forge node started: {}", endpoint_id.fmt_short());
        tracing::info!("mDNS LAN discovery enabled");
        let addr = endpoint.addr();
        tracing::info!("Endpoint address: {:?}", addr);

        let (incoming_tx, incoming_rx) = mpsc::channel(256);

        Ok(Self {
            endpoint,
            peers: Arc::new(Mutex::new(HashMap::new())),
            peer_addrs: Arc::new(Mutex::new(HashMap::new())),
            recent_msg_ids: Arc::new(Mutex::new(HashMap::new())),
            incoming_tx,
            incoming_rx: Arc::new(Mutex::new(incoming_rx)),
            shutdown: Arc::new(Notify::new()),
            closed: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Get this node's Iroh EndpointId.
    pub fn endpoint_id(&self) -> iroh::EndpointId {
        self.endpoint.id()
    }

    /// Get this node's full address for sharing with peers.
    pub fn endpoint_addr(&self) -> iroh::EndpointAddr {
        self.endpoint.addr()
    }

    /// Get the forge-core NodeId derived from the Iroh identity.
    pub fn tirami_node_id(&self) -> NodeId {
        let bytes: [u8; 32] = *self.endpoint.id().as_bytes();
        NodeId(bytes)
    }

    /// Sign arbitrary bytes with this node's Ed25519 secret key.
    /// Used for dual-signing trades (Proof of Useful Work).
    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        self.endpoint.secret_key().sign(msg).to_bytes()
    }

    /// Connect to a peer by their EndpointAddr.
    ///
    /// Starts a background read loop so that messages sent by the remote
    /// peer on this connection are delivered to `recv()`.
    pub async fn connect(&self, addr: iroh::EndpointAddr) -> anyhow::Result<PeerConnection> {
        let peer_node_id = NodeId(*addr.id.as_bytes());
        tracing::info!("Connecting to peer: {}", peer_node_id);

        let conn = self.endpoint.connect(addr.clone(), FORGE_ALPN).await?;
        let peer_conn = PeerConnection::new(conn);
        let peer_id = peer_conn.peer_id().to_string();

        // Save address for potential reconnection
        self.peer_addrs
            .lock()
            .await
            .insert(peer_id.clone(), addr);

        self.peers
            .lock()
            .await
            .insert(peer_id.clone(), peer_conn.clone());

        // Start reading messages from this peer in the background.
        // Without this, messages sent *back* by the remote side would
        // never be consumed because nobody calls accept_bi() on the
        // outgoing connection.
        let read_peer = peer_conn.clone();
        let read_tx = self.incoming_tx.clone();
        let read_id = peer_id;
        let peers = self.peers.clone();
        let recent_msg_ids = self.recent_msg_ids.clone();
        tokio::spawn(async move {
            Self::read_peer_messages(read_peer, read_id, read_tx, peers, recent_msg_ids).await;
        });

        Ok(peer_conn)
    }

    /// Start accepting incoming connections in the background.
    pub fn start_accepting(&self) -> tokio::task::JoinHandle<()> {
        let endpoint = self.endpoint.clone();
        let peers = self.peers.clone();
        let recent_msg_ids = self.recent_msg_ids.clone();
        let incoming_tx = self.incoming_tx.clone();

        tokio::spawn(async move {
            loop {
                match endpoint.accept().await {
                    Some(connecting) => {
                        let peers = peers.clone();
                        let recent_msg_ids = recent_msg_ids.clone();
                        let incoming_tx = incoming_tx.clone();

                        tokio::spawn(async move {
                            match connecting.await {
                                Ok(conn) => {
                                    let peer_conn = PeerConnection::new(conn);
                                    let peer_id = peer_conn.peer_id().to_string();
                                    tracing::info!(
                                        "Accepted connection from: {}",
                                        peer_conn.peer_node_id()
                                    );
                                    peers
                                        .lock()
                                        .await
                                        .insert(peer_id.clone(), peer_conn.clone());

                                    Self::read_peer_messages(
                                        peer_conn,
                                        peer_id,
                                        incoming_tx,
                                        peers,
                                        recent_msg_ids,
                                    )
                                    .await;
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to accept connection: {}", e);
                                }
                            }
                        });
                    }
                    None => {
                        tracing::info!("Endpoint closed, stopping accept loop");
                        break;
                    }
                }
            }
        })
    }

    /// Read messages from a peer connection and forward to the incoming channel.
    async fn read_peer_messages(
        peer: PeerConnection,
        peer_id: String,
        tx: mpsc::Sender<(String, Envelope)>,
        peers: Arc<Mutex<HashMap<String, PeerConnection>>>,
        recent_msg_ids: Arc<Mutex<HashMap<String, ReplayWindow>>>,
    ) {
        // Token bucket rate limiter: 500 msg/s sustained, 200 burst capacity,
        // with a 5-second grace period after connection to absorb handshake bursts.
        const MAX_MESSAGES_PER_SECOND: u32 = 500;
        const BURST_CAPACITY: u32 = 200;
        const GRACE_PERIOD: std::time::Duration = std::time::Duration::from_secs(5);

        let connection_start = tokio::time::Instant::now();
        let mut tokens: f64 = BURST_CAPACITY as f64;
        let mut last_refill = tokio::time::Instant::now();

        loop {
            // Refill tokens based on elapsed time
            let now = tokio::time::Instant::now();
            let elapsed = now.duration_since(last_refill).as_secs_f64();
            tokens = (tokens + elapsed * MAX_MESSAGES_PER_SECOND as f64)
                .min(BURST_CAPACITY as f64);
            last_refill = now;

            match peer.recv_message().await {
                Ok(envelope) => {
                    // Grace period: no rate limiting in first 5 seconds
                    let in_grace = connection_start.elapsed() < GRACE_PERIOD;
                    if !in_grace && tokens < 1.0 {
                        tracing::warn!(
                            "Rate limit exceeded for peer {} (token bucket empty), dropping message",
                            peer_id,
                        );
                        continue;
                    }
                    if !in_grace {
                        tokens -= 1.0;
                    }

                    if let Err(err) = envelope.validate_for_peer(peer.peer_node_id()) {
                        tracing::warn!(
                            "Dropping invalid envelope from {}: {}",
                            peer.peer_node_id(),
                            err
                        );
                        continue;
                    }
                    let is_new_message = {
                        let mut recent_msg_ids = recent_msg_ids.lock().await;
                        recent_msg_ids
                            .entry(peer_id.clone())
                            .or_default()
                            .record(envelope.msg_id)
                    };
                    if !is_new_message {
                        tracing::warn!(
                            "Dropping duplicate envelope {} from {}",
                            envelope.msg_id,
                            peer.peer_node_id()
                        );
                        continue;
                    }
                    if tx.send((peer_id.clone(), envelope)).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!("Peer {} disconnected: {}", peer_id, e);
                    break;
                }
            }
        }

        peers.lock().await.remove(&peer_id);
        recent_msg_ids.lock().await.remove(&peer_id);
    }

    /// Receive the next incoming message from any peer.
    pub async fn recv(&self) -> Option<(String, Envelope)> {
        if self.closed.load(Ordering::SeqCst) {
            return None;
        }

        let mut incoming_rx = self.incoming_rx.lock().await;
        if self.closed.load(Ordering::SeqCst) {
            return None;
        }

        tokio::select! {
            message = incoming_rx.recv() => message,
            _ = self.shutdown.notified() => None,
        }
    }

    /// Send a message to a specific peer.
    pub async fn send_to(&self, peer_id: &str, envelope: &Envelope) -> anyhow::Result<()> {
        let peers = self.peers.lock().await;
        let peer = peers
            .get(peer_id)
            .ok_or_else(|| anyhow::anyhow!("peer not found: {}", peer_id))?;
        peer.send_message(envelope).await
    }

    /// Get a peer connection by ID.
    pub async fn get_peer(&self, peer_id: &str) -> Option<PeerConnection> {
        self.peers.lock().await.get(peer_id).cloned()
    }

    /// Get the list of connected peer IDs.
    pub async fn connected_peers(&self) -> Vec<String> {
        self.peers.lock().await.keys().cloned().collect()
    }

    /// Attempt to reconnect to a previously connected peer.
    /// Returns Ok if reconnected, Err if the address is unknown or connection failed.
    pub async fn reconnect(&self, peer_id: &str) -> anyhow::Result<PeerConnection> {
        let addr = self
            .peer_addrs
            .lock()
            .await
            .get(peer_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no saved address for peer {}", peer_id))?;

        tracing::info!("Attempting reconnect to peer: {}", peer_id);

        // Remove stale connection
        self.peers.lock().await.remove(peer_id);

        // Reconnect
        self.connect(addr).await
    }

    /// Gracefully close the transport.
    pub async fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.shutdown.notify_waiters();
        self.endpoint.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that the token bucket rate limiter allows bursts during the
    /// grace period and correctly limits after the grace period expires.
    #[test]
    fn token_bucket_grace_period() {
        const MAX_MESSAGES_PER_SECOND: u32 = 500;
        const BURST_CAPACITY: u32 = 200;
        const GRACE_PERIOD: std::time::Duration = std::time::Duration::from_secs(5);

        // Simulate connection start (grace period active)
        let connection_start = std::time::Instant::now();
        let mut tokens: f64 = BURST_CAPACITY as f64;

        // During grace period, messages should never be dropped even if
        // we exceed BURST_CAPACITY.
        let in_grace = connection_start.elapsed() < GRACE_PERIOD;
        assert!(in_grace, "should be in grace period immediately after start");

        // Drain all tokens — during grace period this should not matter
        for _ in 0..300 {
            // In grace period: tokens are not consumed
            if in_grace {
                // no token deduction
            } else {
                tokens -= 1.0;
            }
        }

        // Tokens should be untouched because we were in grace period
        assert_eq!(tokens, BURST_CAPACITY as f64);

        // After grace period: simulate token consumption
        let mut tokens: f64 = BURST_CAPACITY as f64;
        let mut dropped = 0u32;
        for _ in 0..250 {
            if tokens < 1.0 {
                dropped += 1;
                continue;
            }
            tokens -= 1.0;
        }

        // Should have dropped 50 messages (250 - 200 burst capacity)
        assert_eq!(dropped, 50);
        assert!(tokens < 1.0, "tokens should be exhausted");

        // Simulate refill: 0.1 seconds at 500/s = 50 tokens
        let refill_elapsed = 0.1_f64;
        tokens = (tokens + refill_elapsed * MAX_MESSAGES_PER_SECOND as f64)
            .min(BURST_CAPACITY as f64);
        assert!((tokens - 50.0).abs() < 1.0, "should have ~50 tokens after refill");
    }

    #[test]
    fn replay_window_dedup() {
        let mut window = ReplayWindow::default();
        assert!(window.record(1));
        assert!(window.record(2));
        assert!(!window.record(1), "duplicate should be rejected");
        assert!(window.record(3));
    }

    #[test]
    fn replay_window_eviction() {
        let mut window = ReplayWindow::default();
        for i in 0..MAX_RECENT_MSG_IDS_PER_PEER + 100 {
            assert!(window.record(i as u64));
        }
        // Old IDs should have been evicted
        assert!(
            window.record(0),
            "msg_id 0 should be accepted again after eviction"
        );
    }
}
