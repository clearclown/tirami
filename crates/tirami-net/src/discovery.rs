use tirami_core::PeerCapability;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Node discovery service — finds peers on LAN and WAN.
///
/// Discovery sources:
/// 1. Direct address (user provides peer address)
/// 2. Known peers (received from connected peers' Welcome messages)
/// 3. Future: mDNS for LAN, DHT for WAN
pub struct DiscoveryService {
    /// Known peers and their capabilities.
    known_peers: Arc<Mutex<HashMap<String, PeerRecord>>>,
}

/// A record of a discovered peer.
#[derive(Debug, Clone)]
pub struct PeerRecord {
    pub peer_id: String,
    pub endpoint_addr: Option<iroh::EndpointAddr>,
    pub capability: Option<PeerCapability>,
    pub last_seen: u64,
    pub last_heartbeat: u64,
    pub connected: bool,
}

impl DiscoveryService {
    pub fn new() -> Self {
        Self {
            known_peers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a peer we've learned about (from Welcome messages, etc.).
    pub async fn register_peer(
        &self,
        peer_id: String,
        addr: Option<iroh::EndpointAddr>,
        capability: Option<PeerCapability>,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut peers = self.known_peers.lock().await;
        let record = peers.entry(peer_id.clone()).or_insert(PeerRecord {
            peer_id,
            endpoint_addr: None,
            capability: None,
            last_seen: 0,
            last_heartbeat: 0,
            connected: false,
        });

        if let Some(a) = addr {
            record.endpoint_addr = Some(a);
        }
        if let Some(c) = capability {
            record.capability = Some(c);
        }
        record.last_seen = now;
    }

    /// Mark a peer as connected.
    pub async fn mark_connected(&self, peer_id: &str) {
        if let Some(record) = self.known_peers.lock().await.get_mut(peer_id) {
            record.connected = true;
        }
    }

    /// Mark a peer as disconnected.
    pub async fn mark_disconnected(&self, peer_id: &str) {
        if let Some(record) = self.known_peers.lock().await.get_mut(peer_id) {
            record.connected = false;
        }
    }

    /// Get all known peers that are not yet connected and have addresses.
    pub async fn unconnected_peers(&self) -> Vec<PeerRecord> {
        self.known_peers
            .lock()
            .await
            .values()
            .filter(|r| !r.connected && r.endpoint_addr.is_some())
            .cloned()
            .collect()
    }

    /// Get all connected peers.
    pub async fn connected_peers(&self) -> Vec<PeerRecord> {
        self.known_peers
            .lock()
            .await
            .values()
            .filter(|r| r.connected)
            .cloned()
            .collect()
    }

    /// Get peers sorted by capability (most powerful first).
    pub async fn peers_by_capability(&self) -> Vec<PeerRecord> {
        let mut peers: Vec<_> = self
            .known_peers
            .lock()
            .await
            .values()
            .filter(|r| r.connected && r.capability.is_some())
            .cloned()
            .collect();

        peers.sort_by(|a, b| {
            let a_mem = a.capability.as_ref().map_or(0.0, |c| c.available_memory_gb);
            let b_mem = b.capability.as_ref().map_or(0.0, |c| c.available_memory_gb);
            b_mem
                .partial_cmp(&a_mem)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        peers
    }

    /// Record a heartbeat from a peer.
    pub async fn record_heartbeat(&self, peer_id: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(record) = self.known_peers.lock().await.get_mut(peer_id) {
            record.last_heartbeat = now;
            record.last_seen = now;
        }
    }

    /// Get peers that have not sent a heartbeat within the timeout.
    /// Returns peer IDs of timed-out peers.
    pub async fn detect_failed_peers(&self, heartbeat_timeout_secs: u64) -> Vec<String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.known_peers
            .lock()
            .await
            .values()
            .filter(|r| {
                r.connected
                    && r.last_heartbeat > 0
                    && now - r.last_heartbeat > heartbeat_timeout_secs
            })
            .map(|r| r.peer_id.clone())
            .collect()
    }

    /// Remove stale peers (not seen within timeout).
    pub async fn prune_stale(&self, timeout_secs: u64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.known_peers
            .lock()
            .await
            .retain(|_, r| now - r.last_seen < timeout_secs || r.connected);
    }

    /// Total count of known peers.
    pub async fn peer_count(&self) -> usize {
        self.known_peers.lock().await.len()
    }
}

impl Default for DiscoveryService {
    fn default() -> Self {
        Self::new()
    }
}
