//! Per-connection state: address, GUID, MTU, sequence numbers,
//! ACK/NACK queues, and the connection state machine.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::Instant;

/// Connection lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Waiting for Open Connection Request 1.
    Unconnected,
    /// Received Request 1, sent Reply 1 — awaiting Request 2.
    Handshaking,
    /// Request 2 received, Reply 2 sent — connection established.
    Connected,
    /// Connection has been closed or timed out.
    Disconnected,
}

/// Per-connection data tracked by the server.
#[derive(Debug)]
pub struct Connection {
    /// Client's socket address.
    pub addr: SocketAddr,
    /// Client GUID (sent in UnconnectedPing / OpenConnectionRequest2).
    pub client_guid: i64,
    /// Negotiated MTU for this connection.
    pub mtu: u16,
    /// Current connection state.
    pub state: ConnectionState,
    /// When this connection was created.
    pub created_at: Instant,
    /// When the last packet was received from this client.
    pub last_recv: Instant,
    /// Outgoing datagram sequence number (monotonically increasing).
    pub send_seq: u32,
    /// Next reliable message index.
    pub reliable_index: u32,
    /// Next ordered message index per channel.
    pub order_indices: [u32; 32],
    /// Next sequenced message index per channel.
    pub sequence_indices: [u32; 32],
    /// Tracked outgoing frames awaiting ACK.
    pub pending_frames: VecDeque<PendingFrame>,
    /// Received datagram sequence numbers (ACK tracking).
    pub recv_queue: RecvQueue,
    /// MTU size confirmed at connection establishment.
    pub mtu_size: u16,
}

/// A frame sent to the client that needs acknowledgment.
#[derive(Debug, Clone)]
pub struct PendingFrame {
    pub sequence_number: u32,
    pub reliable_index: u32,
    pub raw_data: Vec<u8>,
    pub send_time: Instant,
    pub retries: u32,
}

/// Tracks which datagram sequence numbers have been received.
#[derive(Debug)]
pub struct RecvQueue {
    /// Received sequence numbers, stored as ranges (start, end).
    ranges: Vec<(u32, u32)>,
}

impl RecvQueue {
    pub fn new() -> Self {
        Self { ranges: Vec::new() }
    }

    /// Record that `seq` has been received.
    pub fn push(&mut self, seq: u32) {
        if let Some(last) = self.ranges.last_mut() {
            if seq == last.1 + 1 {
                last.1 = seq;
                return;
            }
        }
        self.ranges.push((seq, seq));
    }

    /// Check whether `seq` has already been received.
    pub fn contains(&self, seq: u32) -> bool {
        self.ranges.iter().any(|&(s, e)| seq >= s && seq <= e)
    }

    /// Drain the queue and return the ranges, resetting internal state.
    pub fn drain(&mut self) -> Vec<(u32, u32)> {
        std::mem::take(&mut self.ranges)
    }
}

impl Connection {
    /// Create a new connection in `Unconnected` state.
    pub fn new(addr: SocketAddr, client_guid: i64, mtu: u16) -> Self {
        let now = Instant::now();
        Self {
            addr,
            client_guid,
            mtu,
            state: ConnectionState::Unconnected,
            created_at: now,
            last_recv: now,
            send_seq: 0,
            reliable_index: 0,
            order_indices: [0; 32],
            sequence_indices: [0; 32],
            pending_frames: VecDeque::new(),
            recv_queue: RecvQueue::new(),
            mtu_size: mtu,
        }
    }
}
