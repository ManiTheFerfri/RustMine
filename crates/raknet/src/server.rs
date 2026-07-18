//! RakNet server: manages the UDP socket, dispatches packets to the
//! correct connection, and handles the offline → online handshake.
//!
//! Fully-reassembled game-layer payloads (i.e. a Bedrock 0xfe batch
//! body) are delivered through an mpsc channel, and outbound payloads
//! are accepted through another channel, so the game loop can run on a
//! separate task without blocking UDP I/O.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::Rng;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, info, trace, warn};

use super::connection::{Connection, ConnectionState, PendingFrame};
use super::packet::{
    encode_ack, encode_datagram, encode_frame, encode_offline, parse_packet, Ack, Frame,
    FrameSetEntry, OfflinePacket, RaknetPacket, Reliability,
};

// `split_into_frames` is exported but our internal `split_payload` is used
// because it can populate the shared reliable/order indices directly.

/// Maximum players per server instance.
const MAX_CONNECTIONS: usize = 256;

/// Connection inactivity timeout.
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(20);

/// How often to resend unacknowledged reliable frames.
const RESEND_INTERVAL: Duration = Duration::from_millis(800);

/// Max retries for a reliable frame before dropping the connection.
const MAX_RETRIES: u32 = 10;

/// Messages produced by the RakNet task for the rest of the server.
#[derive(Debug)]
pub enum ServerEvent {
    /// A client just completed the RakNet handshake (OpenConnectionReply2 sent).
    Connected(SocketAddr),
    /// A client timed out or otherwise disconnected.
    Disconnected(SocketAddr),
    /// A fully-reassembled Bedrock batch payload received from a client.
    GamePacket(SocketAddr, Vec<u8>),
}

/// Sender side for queuing outbound reliable-ordered Bedrock batches.
pub type OutboundSender = mpsc::UnboundedSender<(SocketAddr, Vec<u8>)>;
type OutboundReceiver = mpsc::UnboundedReceiver<(SocketAddr, Vec<u8>)>;

/// Event receiver handed out by [`RaknetServer::bind`].
pub type EventReceiver = mpsc::UnboundedReceiver<ServerEvent>;

struct Inner {
    guid: i64,
    motd: String,
    connections: HashMap<SocketAddr, Connection>,
    online_mode: bool,
    next_split_id: u16,
}

/// The RakNet server.
pub struct RaknetServer {
    inner: Arc<Mutex<Inner>>,
}

impl RaknetServer {
    /// Create a new server. Spawns tasks on the current tokio runtime when
    /// [`run`](Self::run) is called.
    pub fn new(motd: String, online_mode: bool) -> Self {
        let mut rng = rand::thread_rng();
        let guid: i64 = rng.gen();
        Self {
            inner: Arc::new(Mutex::new(Inner {
                guid,
                motd,
                connections: HashMap::new(),
                online_mode,
                next_split_id: 0,
            })),
        }
    }

    /// Bind and run the server on `socket`. Returns an event receiver and an
    /// outbound sender once the socket is accepting traffic. The returned
    /// future resolves when the socket closes.
    pub async fn start(
        self,
        socket: Arc<UdpSocket>,
    ) -> (EventReceiver, OutboundSender) {
        let (event_tx, event_rx) = mpsc::unbounded_channel::<ServerEvent>();
        let (out_tx, out_rx) = mpsc::unbounded_channel::<(SocketAddr, Vec<u8>)>();

        let inner = Arc::clone(&self.inner);
        let sock = Arc::clone(&socket);

        // Outbound task — serializes sends per inner state.
        tokio::spawn({
            let inner = Arc::clone(&inner);
            let sock = Arc::clone(&sock);
            async move { Self::outbound_loop(inner, sock, out_rx).await }
        });
        // Resend/timeout task.
        tokio::spawn({
            let inner = Arc::clone(&inner);
            let sock = Arc::clone(&sock);
            async move {
                loop {
                    sleep(RESEND_INTERVAL).await;
                    Self::resend_pending(&inner, &sock).await;
                    let dropped = Self::timeout_stale(&inner).await;
                    for addr in dropped {
                        let _ = event_tx.send(ServerEvent::Disconnected(addr));
                    }
                }
            }
        });
        // Inbound loop — owns the socket recv.
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            loop {
                match sock.recv_from(&mut buf).await {
                    Ok((n, src)) => {
                        let data = buf[..n].to_vec();
                        Self::handle_inbound(
                            Arc::clone(&inner),
                            Arc::clone(&sock),
                            src,
                            data,
                            event_tx.clone(),
                        )
                        .await;
                    }
                    Err(e) => {
                        warn!("UDP recv error: {e}");
                        break;
                    }
                }
            }
        });

        (event_rx, out_tx)
    }

    // ── inbound dispatch ────────────────────────────────────────────

    async fn handle_inbound(
        inner: Arc<Mutex<Inner>>,
        sock: Arc<UdpSocket>,
        src: SocketAddr,
        data: Vec<u8>,
        events: mpsc::UnboundedSender<ServerEvent>,
    ) {
        let packet = match parse_packet(&data) {
            Ok(p) => p,
            Err(e) => {
                trace!("parse fail from {src}: {e}");
                return;
            }
        };
        match packet {
            RaknetPacket::Offline(offline) => {
                Self::handle_offline(inner, sock, src, offline, events).await;
            }
            RaknetPacket::Ack(ack) => {
                Self::handle_ack(&inner, src, &ack).await;
            }
            RaknetPacket::Nack(nack) => {
                Self::handle_nack(&inner, &sock, src, &nack).await;
            }
            RaknetPacket::FrameSet {
                sequence_number,
                frames,
            } => {
                Self::handle_frames(&inner, &sock, src, sequence_number, frames, events).await;
            }
        }
    }

    async fn handle_offline(
        inner: Arc<Mutex<Inner>>,
        sock: Arc<UdpSocket>,
        src: SocketAddr,
        packet: OfflinePacket,
        events: mpsc::UnboundedSender<ServerEvent>,
    ) {
        let mut g = inner.lock().await;
        match packet {
            OfflinePacket::UnconnectedPing { time, client_guid } => {
                debug!("ping from {src}");
                let motd = format!(
                    "MCPE;{};{};{};RustMine;0;20;{};RustMine;Survival;{};",
                    g.motd,
                    crate::protocol_version(),
                    crate::game_version(),
                    g.guid,
                    if g.online_mode { 1 } else { 0 },
                );
                drop(g);
                let raw = encode_offline(&OfflinePacket::UnconnectedPong {
                    time,
                    server_guid: inner.lock().await.guid,
                    motd,
                });
                let _ = sock.send_to(&raw, src).await;
                let mut g = inner.lock().await;
                use std::collections::hash_map::Entry;
                if let Entry::Vacant(e) = g.connections.entry(src) {
                    e.insert(Connection::new(src, client_guid, crate::MAX_MTU));
                }
            }
            OfflinePacket::OpenConnectionRequest1 {
                protocol_version,
                mtu,
            } => {
                debug!("open-conn-1 from {src} mtu={mtu}");
                if g.connections.len() >= MAX_CONNECTIONS {
                    return;
                }
                if protocol_version != crate::RAKNET_PROTOCOL_VERSION {
                    let reply = OfflinePacket::IncompatibleProtocol {
                        server_protocol: crate::RAKNET_PROTOCOL_VERSION,
                        server_guid: g.guid,
                    };
                    drop(g);
                    let _ = sock.send_to(&encode_offline(&reply), src).await;
                    return;
                }
                let reply = OfflinePacket::OpenConnectionReply1 {
                    server_guid: g.guid,
                    use_encryption: g.online_mode,
                    mtu: mtu.min(crate::MAX_MTU),
                };
                let raw = encode_offline(&reply);
                drop(g);
                if sock.send_to(&raw, src).await.is_err() {
                    return;
                }
                let mut g = inner.lock().await;
                if let Some(c) = g.connections.get_mut(&src) {
                    c.mtu = mtu.min(crate::MAX_MTU);
                    c.mtu_size = c.mtu;
                    c.state = ConnectionState::Handshaking;
                    c.last_recv = Instant::now();
                }
            }
            OfflinePacket::OpenConnectionRequest2 {
                server_address: _,
                mtu,
                client_guid,
            } => {
                debug!("open-conn-2 from {src}");
                let was_connected = matches!(
                    g.connections.get(&src).map(|c| c.state),
                    Some(ConnectionState::Connected)
                );
                if let Some(c) = g.connections.get_mut(&src) {
                    c.mtu = mtu.min(crate::MAX_MTU);
                    c.mtu_size = c.mtu;
                    c.client_guid = client_guid;
                    c.state = ConnectionState::Connected;
                    c.last_recv = Instant::now();
                } else {
                    let mut c = Connection::new(src, client_guid, mtu.min(crate::MAX_MTU));
                    c.state = ConnectionState::Connected;
                    g.connections.insert(src, c);
                }
                let reply = OfflinePacket::OpenConnectionReply2 {
                    server_guid: g.guid,
                    client_address: src,
                    mtu: mtu.min(crate::MAX_MTU),
                    encryption_enabled: g.online_mode,
                };
                drop(g);
                let _ = sock.send_to(&encode_offline(&reply), src).await;
                if !was_connected {
                    let _ = events.send(ServerEvent::Connected(src));
                }
            }
            _ => {}
        }
    }

    async fn handle_ack(inner: &Arc<Mutex<Inner>>, src: SocketAddr, ack: &Ack) {
        let mut g = inner.lock().await;
        let Some(c) = g.connections.get_mut(&src) else { return };
        for r in &ack.sequences {
            for s in r.start..=r.end {
                c.pending_frames.retain(|f| f.sequence_number != s);
            }
        }
    }

    async fn handle_nack(
        inner: &Arc<Mutex<Inner>>,
        sock: &Arc<UdpSocket>,
        src: SocketAddr,
        nack: &super::packet::Nack,
    ) {
        let resend: Vec<Vec<u8>> = {
            let mut g = inner.lock().await;
            let Some(c) = g.connections.get_mut(&src) else { return };
            let mut out = Vec::new();
            for r in &nack.sequences {
                for s in r.start..=r.end {
                    if let Some(p) = c.pending_frames.iter_mut().find(|p| p.sequence_number == s) {
                        if p.retries < MAX_RETRIES {
                            p.retries += 1;
                            p.send_time = Instant::now();
                            out.push(p.raw_data.clone());
                        }
                    }
                }
            }
            out
        };
        for raw in resend {
            let _ = sock.send_to(&raw, src).await;
        }
    }

    async fn handle_frames(
        inner: &Arc<Mutex<Inner>>,
        sock: &Arc<UdpSocket>,
        src: SocketAddr,
        seq: u32,
        frames: Vec<Frame>,
        events: mpsc::UnboundedSender<ServerEvent>,
    ) {
        let mut ack_raw = None;
        let mut completed: Vec<Vec<u8>> = Vec::new();
        {
            let mut g = inner.lock().await;
            let Some(c) = g.connections.get_mut(&src) else { return };
            if c.state != ConnectionState::Connected {
                return;
            }
            c.last_recv = Instant::now();
            if c.recv_queue.contains(seq) {
                return;
            }
            c.recv_queue.push(seq);
            let ranges: Vec<_> = c
                .recv_queue
                .ranges()
                .iter()
                .map(|&(s, e)| crate::SequenceRange { start: s, end: e })
                .collect();
            ack_raw = Some(encode_ack(&Ack { sequences: ranges }, false));

            for f in frames {
                if f.is_split {
                    let (Some(sid), Some(sidx), Some(scount)) = (f.split_id, f.split_index, f.split_count) else { continue };
                    if let Some(reassembled) = c.feed_split(sid, sidx, scount, f.body) {
                        completed.push(reassembled);
                    }
                } else {
                    completed.push(f.body);
                }
            }
            c.expire_split_packets();
        }
        if let Some(raw) = ack_raw {
            let _ = sock.send_to(&raw, src).await;
        }
        for payload in completed {
            let _ = events.send(ServerEvent::GamePacket(src, payload));
        }
    }

    // ── outbound ────────────────────────────────────────────────────

    async fn outbound_loop(
        inner: Arc<Mutex<Inner>>,
        sock: Arc<UdpSocket>,
        mut rx: OutboundReceiver,
    ) {
        while let Some((dest, payload)) = rx.recv().await {
            let datagrams = Self::build_datagrams(&inner, dest, payload).await;
            for raw in datagrams {
                if let Err(e) = sock.send_to(&raw, dest).await {
                    trace!("send to {dest} failed: {e}");
                }
            }
        }
    }

    async fn build_datagrams(
        inner: &Arc<Mutex<Inner>>,
        dest: SocketAddr,
        payload: Vec<u8>,
    ) -> Vec<Vec<u8>> {
        let mut g = inner.lock().await;
        let Some(c) = g.connections.get_mut(&dest) else { return Vec::new() };
        if c.state != ConnectionState::Connected {
            return Vec::new();
        }
        let mtu = c.mtu_size.max(576);
        // All outbound game traffic is reliable-ordered on channel 0.
        let reliability = Reliability::ReliableOrdered;
        let ch: u8 = 0;
        let rel_idx = {
            let v = c.reliable_index;
            c.reliable_index = c.reliable_index.wrapping_add(1);
            v
        };
        let ord_idx = {
            let v = c.order_indices[ch as usize];
            c.order_indices[ch as usize] = c.order_indices[ch as usize].wrapping_add(1);
            v
        };

        let single_overhead = 4 /* datagram */ + 1 + 2 + 3 /* reliable idx */ + 4 /* ordered */;
        let max_single = (mtu as usize).saturating_sub(single_overhead);

        let mut frames: Vec<Frame> = if payload.len() <= max_single {
            vec![Frame {
                reliability,
                is_split: false,
                reliable_index: Some(rel_idx),
                sequence_index: None,
                order_index: Some(ord_idx),
                order_channel: ch,
                split_count: None,
                split_id: None,
                split_index: None,
                body: payload,
            }]
        } else {
            let split_id = c.next_split_id;
            c.next_split_id = c.next_split_id.wrapping_add(1);
            Self::split_payload(&payload, mtu, split_id, reliability, ch, rel_idx, ord_idx)
        };

        let mut datagrams = Vec::new();
        while !frames.is_empty() {
            let seq = c.send_seq;
            c.send_seq = c.send_seq.wrapping_add(1);
            let mut entry = FrameSetEntry {
                sequence_number: seq,
                frames: Vec::new(),
            };
            let mut size = 4usize;
            while let Some(next) = frames.first() {
                let enc = encode_frame(next);
                if size + enc.len() > mtu as usize && !entry.frames.is_empty() {
                    break;
                }
                size += enc.len();
                entry.frames.push(frames.remove(0));
            }
            let raw = encode_datagram(&entry);
            c.pending_frames.push_back(PendingFrame {
                sequence_number: seq,
                reliable_index: rel_idx,
                raw_data: raw.clone(),
                send_time: Instant::now(),
                retries: 0,
            });
            datagrams.push(raw);
        }
        datagrams
    }

    fn split_payload(
        body: &[u8],
        mtu: u16,
        split_id: u16,
        reliability: Reliability,
        ch: u8,
        rel_idx: u32,
        ord_idx: u32,
    ) -> Vec<Frame> {
        let overhead = 4 + 1 + 2 + 3 + 4 + 4 + 2 + 4; // datagram + max frame header + split fields
        let chunk = (mtu as usize).saturating_sub(overhead).max(16);
        let count = ((body.len() + chunk - 1) / chunk).max(1) as u32;
        body.chunks(chunk)
            .enumerate()
            .map(|(i, piece)| Frame {
                reliability,
                is_split: true,
                reliable_index: Some(rel_idx),
                sequence_index: None,
                order_index: Some(ord_idx),
                order_channel: ch,
                split_count: Some(count),
                split_id: Some(split_id),
                split_index: Some(i as u32),
                body: piece.to_vec(),
            })
            .collect()
    }

    async fn resend_pending(inner: &Arc<Mutex<Inner>>, sock: &Arc<UdpSocket>) {
        let to_send = {
            let mut g = inner.lock().await;
            let mut out = Vec::new();
            let now = Instant::now();
            for c in g.connections.values_mut() {
                for p in c.pending_frames.iter_mut() {
                    if p.retries >= MAX_RETRIES {
                        continue;
                    }
                    if now.duration_since(p.send_time) >= RESEND_INTERVAL {
                        p.retries += 1;
                        p.send_time = now;
                        out.push((c.addr, p.raw_data.clone()));
                    }
                }
            }
            out
        };
        for (addr, raw) in to_send {
            let _ = sock.send_to(&raw, addr).await;
        }
    }

    async fn timeout_stale(inner: &Arc<Mutex<Inner>>) -> Vec<SocketAddr> {
        let mut g = inner.lock().await;
        let now = Instant::now();
        let mut dropped = Vec::new();
        g.connections.retain(|addr, c| {
            if c.state == ConnectionState::Connected && now.duration_since(c.last_recv) > CONNECTION_TIMEOUT {
                info!("Connection {addr} timed out");
                dropped.push(*addr);
                false
            } else {
                true
            }
        });
        for c in g.connections.values_mut() {
            c.pending_frames.retain(|p| p.retries < MAX_RETRIES);
            c.expire_split_packets();
        }
        dropped
    }
}

