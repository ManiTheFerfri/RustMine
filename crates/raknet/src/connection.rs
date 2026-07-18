//! Per-connection state: address, GUID, MTU, sequence numbers,
//! ACK/NACK queues, split-packet reassembly, and the connection state machine.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

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

/// How long a partial split packet is kept before being dropped.
const SPLIT_PACKET_TIMEOUT: Duration = Duration::from_secs(5);

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
    /// Next ordered message index per channel (outgoing).
    pub order_indices: [u32; 32],
    /// Next sequenced message index per channel (outgoing).
    pub sequence_indices: [u32; 32],
    /// Tracked outgoing frames awaiting ACK.
    pub pending_frames: VecDeque<PendingFrame>,
    /// Received datagram sequence numbers (ACK tracking).
    pub recv_queue: RecvQueue,
    /// MTU size confirmed at connection establishment.
    pub mtu_size: u16,
    /// In-flight split packets being reassembled, keyed by split_id.
    pub(crate) split_packets: HashMap<u16, SplitPacket>,
    /// Counter for outgoing split-packet IDs.
    pub(crate) next_split_id: u16,
    /// Next ordered message index expected on each channel (incoming).
    pub(crate) next_order_index: [u32; 32],
    /// Reordering buffer for ordered reliable frames, keyed by (channel, order_index).
    pub(crate) ordered_backlog: HashMap<(u8, u32), Vec<u8>>,
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

/// Partially reassembled split-packet.
#[derive(Debug, Clone)]
pub(crate) struct SplitPacket {
    pub split_count: u32,
    pub parts: HashMap<u32, Vec<u8>>,
    pub created_at: Instant,
}

impl SplitPacket {
    pub fn new(split_count: u32) -> Self {
        Self {
            split_count,
            parts: HashMap::new(),
            created_at: Instant::now(),
        }
    }

    /// Insert a piece; returns the reassembled body if all parts have arrived.
    pub fn insert(&mut self, split_index: u32, data: Vec<u8>) -> Option<Vec<u8>> {
        if split_index >= self.split_count {
            return None;
        }
        self.parts.insert(split_index, data);
        if self.parts.len() as u32 == self.split_count {
            let mut out = Vec::new();
            for i in 0..self.split_count {
                if let Some(part) = self.parts.get(&i) {
                    out.extend_from_slice(part);
                } else {
                    return None;
                }
            }
            Some(out)
        } else {
            None
        }
    }
}

/// Tracks which datagram sequence numbers have been received.
#[derive(Debug, Default)]
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

    /// Coalesce adjacent ranges into a minimal list.
    pub fn ranges(&self) -> &[(u32, u32)] {
        &self.ranges
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
            sequence_indices: [u32; 32],
            pending_frames: VecDeque::new(),
            recv_queue: RecvQueue::new(),
            mtu_size: mtu,
            split_packets: HashMap::new(),
            next_split_id: 0,
            next_order_index: [0; 32],
            ordered_backlog: HashMap::new(),
        }
    }

    /// Feed a split chunk into reassembly. Returns `Some(reassembled_body)` when
    /// all parts for this split_id have arrived.
    pub(crate) fn feed_split(
        &mut self,
        split_id: u16,
        split_index: u32,
        split_count: u32,
        data: Vec<u8>,
    ) -> Option<Vec<u8>> {
        let entry = self
            .split_packets
            .entry(split_id)
            .or_insert_with(|| SplitPacket::new(split_count));
        if entry.split_count != split_count {
            // Mismatched counts — reset with the latest info.
            *entry = SplitPacket::new(split_count);
        }
        let result = entry.insert(split_index, data);
        if result.is_some() {
            self.split_packets.remove(&split_id);
        }
        result
    }

    /// Drop any split packets that have been incomplete for too long.
    pub(crate) fn expire_split_packets(&mut self) {
        let now = Instant::now();
        self.split_packets
            .retain(|_, sp| now.duration_since(sp.created_at) < SPLIT_PACKET_TIMEOUT);
    }
}
