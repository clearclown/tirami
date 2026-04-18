pub mod asn_rate_limit;
pub mod cluster;
pub mod connection;
pub mod discovery;
pub mod gossip;
pub mod tcp_tunnel;
pub mod transport;

pub use cluster::{ClusterManager, PROTOCOL_VERSION};
pub use connection::PeerConnection;
pub use discovery::DiscoveryService;
pub use gossip::GossipState;
pub use transport::ForgeTransport;

/// ALPN protocol identifier for Forge P2P communication.
pub const FORGE_ALPN: &[u8] = b"forge/1";
