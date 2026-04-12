use tirami_core::NodeId;
use tirami_proto::{Envelope, MAX_PROTOCOL_MESSAGE_BYTES};
use iroh::endpoint::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A connection to a single peer, wrapping an Iroh QUIC connection.
#[derive(Clone)]
pub struct PeerConnection {
    conn: Connection,
    peer_node_id: NodeId,
    peer_id: String,
    write_lock: Arc<Mutex<()>>,
}

impl PeerConnection {
    pub fn new(conn: Connection) -> Self {
        let peer_node_id = NodeId(*conn.remote_id().as_bytes());
        Self {
            conn,
            peer_id: peer_node_id.to_hex(),
            peer_node_id,
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    pub fn peer_node_id(&self) -> &NodeId {
        &self.peer_node_id
    }

    /// Send a protocol message to this peer.
    pub async fn send_message(&self, envelope: &Envelope) -> anyhow::Result<()> {
        let _lock = self.write_lock.lock().await;

        let (mut send, _recv) = self.conn.open_bi().await?;

        let data = bincode::serialize(envelope)?;

        // Length-prefixed framing: 4 bytes big-endian length + payload
        let len = (data.len() as u32).to_be_bytes();
        send.write_all(&len).await?;
        send.write_all(&data).await?;
        send.finish()?;

        Ok(())
    }

    /// Receive a protocol message from this peer.
    pub async fn recv_message(&self) -> anyhow::Result<Envelope> {
        let (_send, mut recv) = self.conn.accept_bi().await?;

        // Read 4-byte length prefix
        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > MAX_PROTOCOL_MESSAGE_BYTES {
            anyhow::bail!("message too large: {} bytes", len);
        }

        let mut data = vec![0u8; len];
        recv.read_exact(&mut data).await?;

        let envelope: Envelope = bincode::deserialize(&data)?;
        Ok(envelope)
    }

    /// Send raw bytes and receive a response (for activation tensors).
    pub async fn send_raw(&self, data: &[u8]) -> anyhow::Result<Vec<u8>> {
        let (mut send, mut recv) = self.conn.open_bi().await?;

        let len = (data.len() as u32).to_be_bytes();
        send.write_all(&len).await?;
        send.write_all(data).await?;
        send.finish()?;

        // Read response
        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let resp_len = u32::from_be_bytes(len_buf) as usize;

        let mut resp = vec![0u8; resp_len];
        recv.read_exact(&mut resp).await?;

        Ok(resp)
    }

    /// Accept raw bytes from a peer and provide a responder handle.
    pub async fn recv_raw(&self) -> anyhow::Result<(Vec<u8>, RawResponder)> {
        let (send, mut recv) = self.conn.accept_bi().await?;

        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > MAX_PROTOCOL_MESSAGE_BYTES {
            anyhow::bail!("raw message too large: {} bytes", len);
        }

        let mut data = vec![0u8; len];
        recv.read_exact(&mut data).await?;

        Ok((data, RawResponder { send }))
    }

    pub fn is_alive(&self) -> bool {
        self.conn.close_reason().is_none()
    }
}

/// Handle for responding to a raw byte request.
pub struct RawResponder {
    send: iroh::endpoint::SendStream,
}

impl RawResponder {
    pub async fn respond(mut self, data: &[u8]) -> anyhow::Result<()> {
        let len = (data.len() as u32).to_be_bytes();
        self.send.write_all(&len).await?;
        self.send.write_all(data).await?;
        self.send.finish()?;
        Ok(())
    }
}
