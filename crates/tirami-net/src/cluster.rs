use crate::connection::PeerConnection;
use crate::discovery::DiscoveryService;
use crate::transport::ForgeTransport;
use tirami_core::PeerCapability;
use tirami_proto::{Envelope, Heartbeat, Hello, LeaveReason, Leaving, Payload};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Current protocol version. Peers with different versions log a warning
/// but still attempt to communicate.
pub const PROTOCOL_VERSION: u16 = 1;

/// Manages a cluster of Forge nodes — handles handshakes,
/// heartbeats, node join/leave, and dynamic topology.
pub struct ClusterManager {
    transport: Arc<ForgeTransport>,
    discovery: Arc<DiscoveryService>,
    local_capability: PeerCapability,
    heartbeat_interval: Duration,
    started_at: Instant,
}

impl ClusterManager {
    pub fn new(transport: Arc<ForgeTransport>, local_capability: PeerCapability) -> Self {
        Self {
            transport,
            discovery: Arc::new(DiscoveryService::new()),
            local_capability,
            heartbeat_interval: Duration::from_secs(5),
            started_at: Instant::now(),
        }
    }

    pub fn discovery(&self) -> &Arc<DiscoveryService> {
        &self.discovery
    }

    /// Clone of the underlying transport `Arc`, for callers that need to
    /// dispatch messages (e.g. gossip broadcast) without going through the
    /// cluster manager itself.
    pub fn transport_arc(&self) -> Arc<ForgeTransport> {
        self.transport.clone()
    }

    pub fn local_capability(&self) -> &PeerCapability {
        &self.local_capability
    }

    /// Perform the Hello/Welcome handshake with a newly connected peer.
    pub async fn handshake(&self, peer: &PeerConnection) -> anyhow::Result<PeerCapability> {
        let node_id = self.transport.tirami_node_id();

        // Send Hello
        let hello = Envelope {
            msg_id: rand::random(),
            sender: node_id.clone(),
            timestamp: now_millis(),
            payload: Payload::Hello(Hello {
                version: PROTOCOL_VERSION,
                capability: self.local_capability.clone(),
            }),
        };

        peer.send_message(&hello).await?;
        tracing::debug!("Sent Hello to {}", peer.peer_id());

        Ok(self.local_capability.clone())
    }

    /// Start the heartbeat sender in the background.
    /// Uptime in seconds since the cluster manager was created.
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    pub fn start_heartbeat(&self) -> tokio::task::JoinHandle<()> {
        let transport = self.transport.clone();
        let node_id = self.transport.tirami_node_id();
        let interval = self.heartbeat_interval;
        let started_at = self.started_at;

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;

                let peers = transport.connected_peers().await;
                if peers.is_empty() {
                    continue;
                }

                let heartbeat = Envelope {
                    msg_id: rand::random(),
                    sender: node_id.clone(),
                    timestamp: now_millis(),
                    payload: Payload::Heartbeat(Heartbeat {
                        uptime_sec: started_at.elapsed().as_secs(),
                        load: 0.0,
                        memory_free_gb: 0.0,
                        battery_pct: None,
                    }),
                };

                for peer_id in &peers {
                    if let Err(e) = transport.send_to(peer_id, &heartbeat).await {
                        tracing::debug!("Heartbeat to {} failed: {}", peer_id, e);
                    }
                }
            }
        })
    }

    /// Handle incoming messages — dispatch to appropriate handlers.
    pub async fn handle_message(&self, peer_id: &str, envelope: Envelope) {
        match envelope.payload {
            Payload::Hello(hello) => {
                if hello.version != PROTOCOL_VERSION {
                    tracing::warn!(
                        "Peer {} speaks protocol v{}, we speak v{}. Proceeding with caution.",
                        peer_id,
                        hello.version,
                        PROTOCOL_VERSION
                    );
                }
                tracing::info!(
                    "Hello from {} (v{}, {}GB, metal={})",
                    peer_id,
                    hello.version,
                    hello.capability.memory_gb,
                    hello.capability.metal_available
                );

                self.discovery
                    .register_peer(peer_id.to_string(), None, Some(hello.capability))
                    .await;
                self.discovery.mark_connected(peer_id).await;
                self.discovery.record_heartbeat(peer_id).await;
            }
            Payload::Welcome(welcome) => {
                tracing::info!(
                    "Welcome from {} (v{}, {} known peers)",
                    peer_id,
                    welcome.version,
                    welcome.known_peers.len()
                );

                self.discovery
                    .register_peer(peer_id.to_string(), None, Some(welcome.capability))
                    .await;
                self.discovery.mark_connected(peer_id).await;

                // Register peers from Welcome message
                for known in welcome.known_peers {
                    self.discovery
                        .register_peer(known.node_id.to_hex(), None, None)
                        .await;
                }
            }
            Payload::Heartbeat(hb) => {
                tracing::trace!(
                    "Heartbeat from {}: uptime={}s load={:.0}% free={:.1}GB",
                    peer_id,
                    hb.uptime_sec,
                    hb.load * 100.0,
                    hb.memory_free_gb
                );
                self.discovery.record_heartbeat(peer_id).await;
            }
            Payload::Leaving(leaving) => {
                tracing::info!(
                    "Peer {} leaving: {:?} (drain {}ms)",
                    peer_id,
                    leaving.reason,
                    leaving.drain_time_ms
                );
                self.discovery.mark_disconnected(peer_id).await;
            }
            _ => {
                // Other messages handled by pipeline or application layer
            }
        }
    }

    /// Get the number of active peers in the cluster.
    pub async fn active_peer_count(&self) -> usize {
        self.discovery.connected_peers().await.len()
    }

    /// Start a background task that checks for peers that missed heartbeats.
    /// Marks them as disconnected after `timeout_secs` without a heartbeat.
    pub fn start_failure_detector(&self, timeout_secs: u64) -> tokio::task::JoinHandle<()> {
        let discovery = self.discovery.clone();
        let check_interval = Duration::from_secs(timeout_secs / 2);

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(check_interval);
            loop {
                ticker.tick().await;
                let failed = discovery.detect_failed_peers(timeout_secs).await;
                for peer_id in failed {
                    tracing::warn!("Peer {} missed heartbeat, marking as down", peer_id);
                    discovery.mark_disconnected(&peer_id).await;
                }
            }
        })
    }

    /// Send a Leaving message to all connected peers before shutdown.
    pub async fn announce_leaving(&self, reason: LeaveReason) {
        let node_id = self.transport.tirami_node_id();
        let peers = self.transport.connected_peers().await;

        let msg = Envelope {
            msg_id: rand::random(),
            sender: node_id,
            timestamp: now_millis(),
            payload: Payload::Leaving(Leaving {
                reason,
                drain_time_ms: 0,
            }),
        };

        for peer_id in &peers {
            if let Err(e) = self.transport.send_to(peer_id, &msg).await {
                tracing::debug!("Failed to send Leaving to {}: {}", peer_id, e);
            }
        }
        tracing::info!("Announced leaving to {} peers", peers.len());
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
