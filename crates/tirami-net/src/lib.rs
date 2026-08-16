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

/// ALPN for the llama.cpp RPC tunnel (#163).
///
/// Deliberately a separate ALPN, which means a separate QUIC connection.
/// `ForgeTransport::read_peer_messages` already consumes *every* incoming
/// bidirectional stream on the protocol connection and tries to decode it as
/// a bincode `Envelope`. A tunnel sharing that connection would race it for
/// streams and lose roughly half of them — which is why the previous
/// `recv_raw`-based tunnel could not have worked even with its framing fixed.
///
/// On its own ALPN the tunnel owns all streams on its connection, so no
/// stream-type demultiplexing is needed and the protocol wire format is
/// untouched.
pub const RPC_TUNNEL_ALPN: &[u8] = b"tirami-rpc/1";
