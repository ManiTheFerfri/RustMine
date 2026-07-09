//! RakNet server: manages the UDP socket, dispatches packets to the
//! correct connection, and handles the offline → online handshake.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::Rng;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, info, trace, warn};

use super::connection::{Connection, ConnectionState};
use super::packet::{
    self, encode_ack, encode_offline, parse_packet, Ack, Frame, FrameSetEntry, OfflinePacket,
    RaknetPacket, Reliability,
};

/// Maximum players tunable per server instance.
const MAX_CONNECTIONS: usize = 256;

/// How long to keep a connection alive without receiving data.
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);

/// How often to resend unacknowledged reliable frames.
const RESEND_INTERVAL: Duration = Duration::from_millis(500);

/// Max retries for a reliable frame before dropping the connection.
const MAX_RETRIES: u32 = 8;

/// Shared server state behind a mutex so the async I/O task can access it.
struct Inner {
    guid: i64,
    motd: String,
    connections: HashMap<SocketAddr, Connection>,
    /// Whether this server uses encryption (offline-mode = false).
    online_mode: bool,
}

/// The RakNet server.
pub struct RaknetServer {
    inner: Arc<Mutex<Inner>>,
}

impl RaknetServer {
    /// Create a new server with the given MOTD.
    pub fn new(motd: String, online_mode: bool) -> Self {
        let mut rng = rand::thread_rng();
        let guid: i64 = rng.gen();
        Self {
            inner: Arc::new(Mutex::new(Inner {
                guid,
                motd,
                connections: HashMap::new(),
                online_mode,
            })),
        }
    }

    /// Run the server on the given UDP socket.
    /// Blocks until the socket closes or an error occurs.
    pub async fn run(&self, socket: UdpSocket) {
        let mut buf = vec![0u8; 2048];
        let inner = Arc::clone(&self.inner);

        let resend_join = {
            let inner = Arc::clone(&inner);
            tokio::spawn(async move {
                loop {
                    sleep(RESEND_INTERVAL).await;
                    Self::resend_pending(&inner).await;
                    Self::timeout_stale(&inner).await;
                }
            })
        };

        loop {
            match socket.recv_from(&mut buf).await {
                Ok((n, src)) => {
                    let packet_data = &buf[..n];
                    Self::handle_packet(&inner, &socket, src, packet_data).await;
                }
                Err(e) => {
                    warn!("UDP recv error: {e}");
                    break;
                }
            }
        }
        resend_join.abort();
    }

    async fn handle_packet(
        inner: &Arc<Mutex<Inner>>,
        socket: &UdpSocket,
        src: SocketAddr,
        data: &[u8],
    ) {
        let packet = match parse_packet(data) {
            Ok(p) => p,
            Err(e) => {
                trace!("Failed to parse packet from {src}: {e}");
                return;
            }
        };

        match packet {
            RaknetPacket::Offline(offline) => {
                Self::handle_offline(inner, socket, src, offline).await;
            }
            RaknetPacket::Ack(ack) => {
                Self::handle_ack(inner, src, &ack).await;
            }
            RaknetPacket::Nack(nack) => {
                Self::handle_nack(inner, socket, src, &nack).await;
            }
            RaknetPacket::FrameSet {
                sequence_number,
                frames,
            } => {
                Self::handle_frame_set(inner, socket, src, sequence_number, frames).await;
            }
        }
    }

    // ── offline messages ────────────────────────────────────────────

    async fn handle_offline(
        inner: &Arc<Mutex<Inner>>,
        socket: &UdpSocket,
        src: SocketAddr,
        packet: OfflinePacket,
    ) {
        let mut guard = inner.lock().await;
        match packet {
            OfflinePacket::UnconnectedPing { time, client_guid } => {
                debug!("Unconnected ping from {src}");
                let pong = OfflinePacket::UnconnectedPong {
                    time,
                    server_guid: guard.guid,
                    motd: format!(
                        "MCPE;{};{};{};{};{};{};{};{};{};{};",
                        guard.motd,
                        packet::id::UNCONNECTED_PONG,
                        "1.26.30",
                        "RustMine",
                        0,
                        20,
                        guard.guid,
                        "RustMine Server",
                        "Survival",
                        if guard.online_mode { 1 } else { 0 },
                    ),
                };
                let raw = encode_offline(&pong);
                if let Err(e) = socket.send_to(&raw, src).await {
                    warn!("Failed to send pong to {src}: {e}");
                }
                // Track potential connection for future handshake.
                use std::collections::hash_map::Entry;
                if let Entry::Vacant(e) = guard.connections.entry(src) {
                    let conn = Connection::new(src, client_guid, 1492);
                    e.insert(conn);
                }
            }
            OfflinePacket::OpenConnectionRequest1 {
                protocol_version,
                mtu,
            } => {
                debug!("OpenConnReq1 from {src} proto={protocol_version} mtu={mtu}");
                if guard.connections.len() >= MAX_CONNECTIONS {
                    return;
                }
                // Use or update the connection entry.
                let reply = OfflinePacket::OpenConnectionReply1 {
                    server_guid: guard.guid,
                    use_encryption: guard.online_mode,
                    mtu,
                };
                let raw = encode_offline(&reply);
                if let Err(e) = socket.send_to(&raw, src).await {
                    warn!("Failed to send OpenConnReply1 to {src}: {e}");
                    return;
                }
                if let Some(conn) = guard.connections.get_mut(&src) {
                    conn.mtu = mtu;
                    conn.state = ConnectionState::Handshaking;
                    conn.last_recv = Instant::now();
                }
            }
            OfflinePacket::OpenConnectionRequest2 {
                server_address: _,
                mtu,
                client_guid,
            } => {
                debug!("OpenConnReq2 from {src} mtu={mtu} guid={client_guid}");
                if let Some(conn) = guard.connections.get_mut(&src) {
                    conn.mtu = mtu;
                    conn.client_guid = client_guid;
                    conn.state = ConnectionState::Connected;
                    conn.last_recv = Instant::now();
                }
                let reply = OfflinePacket::OpenConnectionReply2 {
                    server_guid: guard.guid,
                    client_address: src,
                    mtu,
                    encryption_enabled: guard.online_mode,
                };
                let raw = encode_offline(&reply);
                if let Err(e) = socket.send_to(&raw, src).await {
                    warn!("Failed to send OpenConnReply2 to {src}: {e}");
                }
            }
            _ => {}
        }
    }

    // ── ACK / NACK ──────────────────────────────────────────────────

    async fn handle_ack(inner: &Arc<Mutex<Inner>>, src: SocketAddr, ack: &Ack) {
        let mut guard = inner.lock().await;
        let Some(conn) = guard.connections.get_mut(&src) else {
            return;
        };
        for range in &ack.sequences {
            let mut seq = range.start;
            while seq <= range.end {
                conn.pending_frames.retain(|f| f.sequence_number != seq);
                seq += 1;
            }
        }
    }

    async fn handle_nack(
        inner: &Arc<Mutex<Inner>>,
        socket: &UdpSocket,
        src: SocketAddr,
        nack: &super::packet::Nack,
    ) {
        let mut guard = inner.lock().await;
        let Some(conn) = guard.connections.get_mut(&src) else {
            return;
        };
        for range in &nack.sequences {
            let mut seq = range.start;
            while seq <= range.end {
                // Resend the raw datagram if we still have it.
                if let Some(pending) = conn
                    .pending_frames
                    .iter_mut()
                    .find(|f| f.sequence_number == seq)
                {
                    if pending.retries < MAX_RETRIES {
                        pending.retries += 1;
                        let data = pending.raw_data.clone();
                        drop(guard);
                        if let Err(e) = socket.send_to(&data, src).await {
                            warn!("NACK resend to {src} failed: {e}");
                        }
                        return;
                    }
                }
                seq += 1;
            }
        }
    }

    // ── frame set ───────────────────────────────────────────────────

    async fn handle_frame_set(
        inner: &Arc<Mutex<Inner>>,
        socket: &UdpSocket,
        src: SocketAddr,
        sequence_number: u32,
        _frames: Vec<Frame>,
    ) {
        let mut guard = inner.lock().await;
        let Some(conn) = guard.connections.get_mut(&src) else {
            return;
        };
        conn.last_recv = Instant::now();

        // Record ACK and generate per-frame ACK.
        conn.recv_queue.push(sequence_number);
        let recv_seqs: Vec<_> = conn.recv_queue.drain().clone();
        // Re-populate since we only needed the snapshot.
        for &(s, e) in &recv_seqs {
            conn.recv_queue.push(s);
            if s != e {
                conn.recv_queue.push(e);
            }
        }
        // Build ACK from recv_seqs.
        let mut seqs: Vec<packet::SequenceRange> = recv_seqs
            .iter()
            .map(|&(s, e)| packet::SequenceRange { start: s, end: e })
            .collect();
        // ponytail: simplistic single-range ACK; proper coalescing deferred.
        seqs.truncate(1);
        if !seqs.is_empty() {
            let ack = Ack {
                sequences: seqs.clone(),
            };
            let raw = encode_ack(&ack, false);
            drop(guard);
            let _ = socket.send_to(&raw, src).await;
        }
    }

    // ── helpers ─────────────────────────────────────────────────────

    async fn send_frame(
        inner: &Arc<Mutex<Inner>>,
        socket: &UdpSocket,
        dest: SocketAddr,
        payload: Vec<u8>,
        reliability: Reliability,
        order_channel: u8,
    ) {
        let mut guard = inner.lock().await;
        let Some(conn) = guard.connections.get_mut(&dest) else {
            return;
        };
        if conn.state != ConnectionState::Connected {
            return;
        }
        let seq = conn.send_seq;
        conn.send_seq = conn.send_seq.wrapping_add(1);
        let reliable_index = if reliability.is_reliable() {
            let ri = conn.reliable_index;
            conn.reliable_index = conn.reliable_index.wrapping_add(1);
            Some(ri)
        } else {
            None
        };
        let order_index = if reliability == Reliability::ReliableOrdered {
            let ch = order_channel as usize % 32;
            let oi = conn.order_indices[ch];
            conn.order_indices[ch] = conn.order_indices[ch].wrapping_add(1);
            Some(oi)
        } else {
            None
        };

        let frame = Frame {
            reliability,
            is_split: false,
            reliable_index,
            sequence_index: None,
            order_index,
            order_channel,
            split_count: None,
            split_id: None,
            split_index: None,
            body: payload,
        };
        let entry = FrameSetEntry {
            sequence_number: seq,
            frames: vec![frame],
        };
        let raw = packet::encode_datagram(&entry);
        // Track for potential resend.
        if reliability.is_reliable() {
            conn.pending_frames
                .push_back(super::connection::PendingFrame {
                    sequence_number: seq,
                    reliable_index: reliable_index.unwrap_or(0),
                    raw_data: raw.clone(),
                    send_time: Instant::now(),
                    retries: 0,
                });
        }
        drop(guard);
        if let Err(e) = socket.send_to(&raw, dest).await {
            warn!("send_frame to {dest} failed: {e}");
        }
    }

    async fn resend_pending(_inner: &Arc<Mutex<Inner>>) {
        // ponytail: resend requires a socket reference. Deferred to Phase 3
        // when we wire up the tick loop. For now, just expire old frames.
    }

    async fn timeout_stale(inner: &Arc<Mutex<Inner>>) {
        let mut guard = inner.lock().await;
        let now = Instant::now();
        guard.connections.retain(|addr, conn| {
            if now.duration_since(conn.last_recv) > CONNECTION_TIMEOUT {
                info!("Connection {addr} timed out");
                false
            } else {
                true
            }
        });
        // Clean up pending frames that exceed max retries.
        for conn in guard.connections.values_mut() {
            conn.pending_frames.retain(|f| f.retries < MAX_RETRIES);
        }
    }
}
