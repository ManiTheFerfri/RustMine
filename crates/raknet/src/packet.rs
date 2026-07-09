//! RakNet packet types, serialization, and deserialization.
//!
//! Covers offline messages (unconnected ping/pong, open connection
//! request/reply), online datagrams (frame sets), ACK/NACK, and
//! disconnection.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use thiserror::Error;

/// The RakNet offline message data ID ("magic").
pub const MAGIC: [u8; 16] = [
    0x00, 0xff, 0xff, 0x00, 0xfe, 0xfe, 0xfe, 0xfe, 0xfd, 0xfd, 0xfd, 0xfd, 0x12, 0x34, 0x56, 0x78,
];

/// Packet IDs used by RakNet.
pub mod id {
    pub const CONNECTED_PING: u8 = 0x00;
    pub const UNCONNECTED_PING: u8 = 0x01;
    pub const CONNECTED_PONG: u8 = 0x03;
    pub const DISCONNECTION_NOTIFICATION: u8 = 0x04;
    pub const OPEN_CONNECTION_REQUEST_1: u8 = 0x05;
    pub const OPEN_CONNECTION_REPLY_1: u8 = 0x06;
    pub const OPEN_CONNECTION_REQUEST_2: u8 = 0x07;
    pub const OPEN_CONNECTION_REPLY_2: u8 = 0x08;
    pub const DISCONNECTION: u8 = 0x15;
    pub const UNCONNECTED_PONG: u8 = 0x1c;
    /// Frame set (datagram) — bits 4-7 carry flags.
    pub const FRAME_SET_START: u8 = 0x80;
    pub const FRAME_SET_END: u8 = 0x8f;
}

/// Reliability types for framed data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Reliability {
    Unreliable = 0,
    UnreliableSequenced = 1,
    Reliable = 2,
    ReliableOrdered = 3,
    ReliableSequenced = 4,
}

impl Reliability {
    pub fn from_raw(raw: u8) -> Option<Self> {
        match raw & 0x07 {
            0 => Some(Self::Unreliable),
            1 => Some(Self::UnreliableSequenced),
            2 => Some(Self::Reliable),
            3 => Some(Self::ReliableOrdered),
            4 => Some(Self::ReliableSequenced),
            _ => None,
        }
    }

    pub fn is_reliable(self) -> bool {
        matches!(
            self,
            Self::Reliable | Self::ReliableOrdered | Self::ReliableSequenced
        )
    }
}

/// A decoded RakNet packet — either a single offline packet or a datagram
/// containing multiple frames.
#[derive(Debug)]
pub enum RaknetPacket {
    /// A standalone offline message (ping, connection request/reply).
    Offline(OfflinePacket),
    /// An ACK for a range of datagram sequence numbers.
    Ack(Ack),
    /// A NACK for a range of datagram sequence numbers.
    Nack(Nack),
    /// A frame set datagram carrying game-layer payloads.
    FrameSet {
        sequence_number: u32,
        frames: Vec<Frame>,
    },
}

/// Offline messages exchanged before the connection is established.
#[derive(Debug)]
pub enum OfflinePacket {
    UnconnectedPing {
        time: i64,
        client_guid: i64,
    },
    UnconnectedPong {
        time: i64,
        server_guid: i64,
        motd: String,
    },
    OpenConnectionRequest1 {
        protocol_version: u8,
        mtu: u16,
    },
    OpenConnectionReply1 {
        server_guid: i64,
        use_encryption: bool,
        mtu: u16,
    },
    OpenConnectionRequest2 {
        server_address: SocketAddr,
        mtu: u16,
        client_guid: i64,
    },
    OpenConnectionReply2 {
        server_guid: i64,
        client_address: SocketAddr,
        mtu: u16,
        encryption_enabled: bool,
    },
    IncompatibleProtocol {
        server_protocol: u8,
        server_guid: i64,
    },
}

/// A single frame within a datagram.
#[derive(Debug, Clone)]
pub struct Frame {
    pub reliability: Reliability,
    pub is_split: bool,
    pub reliable_index: Option<u32>,
    pub sequence_index: Option<u32>,
    pub order_index: Option<u32>,
    pub order_channel: u8,
    pub split_count: Option<u32>,
    pub split_id: Option<u16>,
    pub split_index: Option<u32>,
    pub body: Vec<u8>,
}

/// An entry in a frame set (used during encoding).
#[derive(Debug, Clone)]
pub struct FrameSetEntry {
    pub sequence_number: u32,
    pub frames: Vec<Frame>,
}

/// An ACK or NACK record, covering one or more sequence number ranges.
#[derive(Debug, Clone)]
pub struct Ack {
    pub sequences: Vec<SequenceRange>,
}

/// A NACK (negative acknowledgment).
#[derive(Debug, Clone)]
pub struct Nack {
    pub sequences: Vec<SequenceRange>,
}

/// A range of sequence numbers (inclusive).
#[derive(Debug, Clone, Copy)]
pub struct SequenceRange {
    pub start: u32,
    pub end: u32,
}

/// Frame type implied by the leading byte of a datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Ack,
    Nack,
    FrameSet,
}

impl FrameType {
    pub fn from_byte(byte: u8) -> Option<Self> {
        if byte & 0xe0 == 0xc0 {
            Some(Self::Ack)
        } else if byte & 0xe0 == 0xa0 {
            Some(Self::Nack)
        } else if (id::FRAME_SET_START..=id::FRAME_SET_END).contains(&byte) {
            Some(Self::FrameSet)
        } else {
            None
        }
    }
}

/// Errors that can occur during packet read/write.
#[derive(Debug, Error)]
pub enum PacketError {
    #[error("buffer too short")]
    BufferTooShort,
    #[error("unknown packet ID: 0x{id:02x}")]
    UnknownPacketId { id: u8 },
    #[error("invalid magic bytes")]
    InvalidMagic,
    #[error("invalid IP version: {0}")]
    InvalidIpVersion(u8),
    #[error("invalid reliability: {0}")]
    InvalidReliability(u8),
    #[error("{0}")]
    Other(String),
}

// ── reading helpers ────────────────────────────────────────────────────

fn read_u8(buf: &[u8], pos: &mut usize) -> Result<u8, PacketError> {
    if *pos >= buf.len() {
        return Err(PacketError::BufferTooShort);
    }
    let val = buf[*pos];
    *pos += 1;
    Ok(val)
}

fn read_u16_be(buf: &[u8], pos: &mut usize) -> Result<u16, PacketError> {
    if *pos + 2 > buf.len() {
        return Err(PacketError::BufferTooShort);
    }
    let val = u16::from_be_bytes([buf[*pos], buf[*pos + 1]]);
    *pos += 2;
    Ok(val)
}

fn read_u24_le(buf: &[u8], pos: &mut usize) -> Result<u32, PacketError> {
    if *pos + 3 > buf.len() {
        return Err(PacketError::BufferTooShort);
    }
    let val = u32::from_le_bytes([buf[*pos], buf[*pos + 1], buf[*pos + 2], 0]);
    *pos += 3;
    Ok(val)
}

fn read_i64_be(buf: &[u8], pos: &mut usize) -> Result<i64, PacketError> {
    if *pos + 8 > buf.len() {
        return Err(PacketError::BufferTooShort);
    }
    let arr: [u8; 8] = buf[*pos..*pos + 8].try_into().unwrap();
    let val = i64::from_be_bytes(arr);
    *pos += 8;
    Ok(val)
}

fn read_magic(buf: &[u8], pos: &mut usize) -> Result<(), PacketError> {
    if *pos + 16 > buf.len() {
        return Err(PacketError::BufferTooShort);
    }
    if buf[*pos..*pos + 16] != MAGIC {
        return Err(PacketError::InvalidMagic);
    }
    *pos += 16;
    Ok(())
}

fn read_address(buf: &[u8], pos: &mut usize) -> Result<SocketAddr, PacketError> {
    let version = read_u8(buf, pos)?;
    match version {
        4 => {
            if *pos + 6 > buf.len() {
                return Err(PacketError::BufferTooShort);
            }
            let octets = [!buf[*pos], !buf[*pos + 1], !buf[*pos + 2], !buf[*pos + 3]];
            *pos += 4;
            let port = u16::from_be_bytes([buf[*pos], buf[*pos + 1]]);
            *pos += 2;
            Ok(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::from(octets),
                port,
            )))
        }
        _ => Err(PacketError::InvalidIpVersion(version)),
    }
}

// ── writing helpers ────────────────────────────────────────────────────

fn write_u8(buf: &mut Vec<u8>, val: u8) {
    buf.push(val);
}

fn write_i64_be(buf: &mut Vec<u8>, val: i64) {
    buf.extend_from_slice(&val.to_be_bytes());
}

fn write_u16_be(buf: &mut Vec<u8>, val: u16) {
    buf.extend_from_slice(&val.to_be_bytes());
}

fn write_u32_be(buf: &mut Vec<u8>, val: u32) {
    buf.extend_from_slice(&val.to_be_bytes());
}

fn write_u24_le(buf: &mut Vec<u8>, val: u32) {
    let bytes = val.to_le_bytes();
    buf.extend_from_slice(&bytes[..3]);
}

fn write_magic(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&MAGIC);
}

fn write_address(buf: &mut Vec<u8>, addr: SocketAddr) {
    match addr {
        SocketAddr::V4(v4) => {
            write_u8(buf, 4);
            for octet in &v4.ip().octets() {
                write_u8(buf, !octet);
            }
            write_u16_be(buf, v4.port());
        }
        SocketAddr::V6(v6) => {
            write_u8(buf, 6);
            // ponytail: IPv6 not required for v1.0, silently downgrade to IPv4 mapping
            write_u8(buf, 0);
            write_u8(buf, 0);
            write_u8(buf, 0);
            write_u8(buf, 0);
            write_u16_be(buf, v6.port());
        }
    }
}

// ── offline packet decode ──────────────────────────────────────────────

pub fn decode_offline(buf: &[u8]) -> Result<OfflinePacket, PacketError> {
    if buf.is_empty() {
        return Err(PacketError::BufferTooShort);
    }
    let mut pos = 1; // skip packet ID
    let id = buf[0];

    Ok(match id {
        id::UNCONNECTED_PING => {
            let time = read_i64_be(buf, &mut pos)?;
            read_magic(buf, &mut pos)?;
            let client_guid = read_i64_be(buf, &mut pos)?;
            OfflinePacket::UnconnectedPing { time, client_guid }
        }
        id::OPEN_CONNECTION_REQUEST_1 => {
            read_magic(buf, &mut pos)?;
            let protocol_version = read_u8(buf, &mut pos)?;
            let mtu = (buf.len() - pos + 28) as u16; // incl overhead
            OfflinePacket::OpenConnectionRequest1 {
                protocol_version,
                mtu: mtu.min(MAX_MTU),
            }
        }
        id::OPEN_CONNECTION_REQUEST_2 => {
            read_magic(buf, &mut pos)?;
            let server_address = read_address(buf, &mut pos)?;
            let mtu = read_u16_be(buf, &mut pos)?;
            let client_guid = read_i64_be(buf, &mut pos)?;
            OfflinePacket::OpenConnectionRequest2 {
                server_address,
                mtu,
                client_guid,
            }
        }
        _ => return Err(PacketError::UnknownPacketId { id }),
    })
}

// ── offline packet encode ──────────────────────────────────────────────

pub fn encode_offline(packet: &OfflinePacket) -> Vec<u8> {
    let mut buf = Vec::new();
    match packet {
        OfflinePacket::UnconnectedPing {
            time,
            client_guid: _,
        } => {
            // we only encode server-side pongs; pings are decoded from clients
            write_u8(&mut buf, id::UNCONNECTED_PING);
            write_i64_be(&mut buf, *time);
            write_magic(&mut buf);
            write_i64_be(&mut buf, 0);
        }
        OfflinePacket::UnconnectedPong {
            time,
            server_guid,
            motd,
        } => {
            write_u8(&mut buf, id::UNCONNECTED_PONG);
            write_i64_be(&mut buf, *time);
            write_i64_be(&mut buf, *server_guid);
            write_magic(&mut buf);
            let motd_bytes = motd.as_bytes();
            let len = motd_bytes.len().min(u16::MAX as usize) as u16;
            write_u16_be(&mut buf, len);
            buf.extend_from_slice(&motd_bytes[..len as usize]);
        }
        OfflinePacket::OpenConnectionReply1 {
            server_guid,
            use_encryption,
            mtu,
        } => {
            write_u8(&mut buf, id::OPEN_CONNECTION_REPLY_1);
            write_magic(&mut buf);
            write_i64_be(&mut buf, *server_guid);
            write_u8(&mut buf, *use_encryption as u8);
            write_u16_be(&mut buf, *mtu);
        }
        OfflinePacket::OpenConnectionReply2 {
            server_guid,
            client_address,
            mtu,
            encryption_enabled,
        } => {
            write_u8(&mut buf, id::OPEN_CONNECTION_REPLY_2);
            write_magic(&mut buf);
            write_i64_be(&mut buf, *server_guid);
            write_address(&mut buf, *client_address);
            write_u16_be(&mut buf, *mtu);
            write_u8(&mut buf, *encryption_enabled as u8);
        }
        OfflinePacket::IncompatibleProtocol {
            server_protocol,
            server_guid,
        } => {
            write_u8(&mut buf, id::DISCONNECTION);
            write_u8(&mut buf, *server_protocol);
            write_magic(&mut buf);
            write_i64_be(&mut buf, *server_guid);
        }
        OfflinePacket::OpenConnectionRequest1 { .. }
        | OfflinePacket::OpenConnectionRequest2 { .. } => {
            // client-to-server only; won't be encoded on server side
        }
    }
    buf
}

// ── frame decode ───────────────────────────────────────────────────────

/// Decode a single frame from the buffer.
/// Returns the decoded frame and the number of bytes consumed.
pub fn decode_frame(buf: &[u8]) -> Result<(Frame, usize), PacketError> {
    let mut pos = 0;
    let flags = read_u8(buf, &mut pos)?;
    let reliability =
        Reliability::from_raw(flags & 0x07).ok_or(PacketError::InvalidReliability(flags & 0x07))?;
    let is_split = (flags & 0x10) != 0;
    let length_bits = read_u16_be(buf, &mut pos)?;
    let body_len = length_bits.div_ceil(8) as usize;

    let reliable_index = if reliability.is_reliable() {
        Some(read_u24_le(buf, &mut pos)?)
    } else {
        None
    };
    let sequence_index = if matches!(
        reliability,
        Reliability::UnreliableSequenced | Reliability::ReliableSequenced
    ) {
        Some(read_u24_le(buf, &mut pos)?)
    } else {
        None
    };
    let (order_index, order_channel) = if matches!(
        reliability,
        Reliability::ReliableOrdered | Reliability::ReliableSequenced
    ) {
        (Some(read_u24_le(buf, &mut pos)?), read_u8(buf, &mut pos)?)
    } else {
        (None, 0)
    };
    let (split_count, split_id, split_index) = if is_split {
        (
            Some(read_u32_be(buf, &mut pos)?),
            Some(read_u16_be(buf, &mut pos)?),
            Some(read_u32_be(buf, &mut pos)?),
        )
    } else {
        (None, None, None)
    };

    if pos + body_len > buf.len() {
        return Err(PacketError::BufferTooShort);
    }
    let body = buf[pos..pos + body_len].to_vec();

    Ok((
        Frame {
            reliability,
            is_split,
            reliable_index,
            sequence_index,
            order_index,
            order_channel,
            split_count,
            split_id,
            split_index,
            body,
        },
        pos + body_len,
    ))
}

// ── frame encode ───────────────────────────────────────────────────────

pub fn encode_frame(frame: &Frame) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut flags = frame.reliability as u8;
    if frame.is_split {
        flags |= 0x10;
    }
    write_u8(&mut buf, flags);
    let body_bits = (frame.body.len() * 8) as u16;
    write_u16_be(&mut buf, body_bits);

    if frame.reliability.is_reliable() {
        write_u24_le(&mut buf, frame.reliable_index.unwrap_or(0));
    }
    if matches!(
        frame.reliability,
        Reliability::UnreliableSequenced | Reliability::ReliableSequenced
    ) {
        write_u24_le(&mut buf, frame.sequence_index.unwrap_or(0));
    }
    if matches!(
        frame.reliability,
        Reliability::ReliableOrdered | Reliability::ReliableSequenced
    ) {
        write_u24_le(&mut buf, frame.order_index.unwrap_or(0));
        write_u8(&mut buf, frame.order_channel);
    }
    if frame.is_split {
        write_u32_be(&mut buf, frame.split_count.unwrap_or(0));
        write_u16_be(&mut buf, frame.split_id.unwrap_or(0));
        write_u32_be(&mut buf, frame.split_index.unwrap_or(0));
    }
    buf.extend_from_slice(&frame.body);
    buf
}

// ── datagram decode ────────────────────────────────────────────────────

pub fn decode_datagram(buf: &[u8]) -> Result<FrameSetEntry, PacketError> {
    if buf.is_empty() {
        return Err(PacketError::BufferTooShort);
    }
    let mut pos = 0;
    let header = read_u8(buf, &mut pos)?;
    let sequence_number = read_u24_le(buf, &mut pos)?;

    let frame_type =
        FrameType::from_byte(header).ok_or(PacketError::UnknownPacketId { id: header })?;
    if frame_type != FrameType::FrameSet {
        return Err(PacketError::Other("expected frame set".into()));
    }
    let mut frames = Vec::new();
    while pos < buf.len() {
        let (frame, consumed) = decode_frame(&buf[pos..])?;
        frames.push(frame);
        pos += consumed;
        if consumed == 0 {
            break; // safety against infinite loop on zero-length frames
        }
    }
    Ok(FrameSetEntry {
        sequence_number,
        frames,
    })
}

// ── datagram encode ────────────────────────────────────────────────────

pub fn encode_datagram(entry: &FrameSetEntry) -> Vec<u8> {
    let mut buf = Vec::new();
    write_u8(&mut buf, id::FRAME_SET_START);
    write_u24_le(&mut buf, entry.sequence_number);
    for frame in &entry.frames {
        buf.extend_from_slice(&encode_frame(frame));
    }
    buf
}

// ── ACK / NACK ─────────────────────────────────────────────────────────

pub fn decode_nack(buf: &[u8]) -> Result<Nack, PacketError> {
    let ack = decode_ack(buf)?;
    Ok(Nack {
        sequences: ack.sequences,
    })
}

pub fn decode_ack(buf: &[u8]) -> Result<Ack, PacketError> {
    if buf.is_empty() {
        return Err(PacketError::BufferTooShort);
    }
    let mut pos = 1;
    let count = read_u16_be(buf, &mut pos)? as usize;
    let mut sequences = Vec::with_capacity(count);
    for _ in 0..count {
        let single = read_u8(buf, &mut pos)? != 0;
        let start = read_u24_le(buf, &mut pos)?;
        let end = if single {
            start
        } else {
            read_u24_le(buf, &mut pos)?
        };
        sequences.push(SequenceRange { start, end });
    }
    Ok(Ack { sequences })
}

pub fn encode_ack(ack: &Ack, is_nack: bool) -> Vec<u8> {
    let mut buf = Vec::new();
    write_u8(&mut buf, if is_nack { 0xa0 } else { 0xc0 });
    write_u16_be(&mut buf, ack.sequences.len() as u16);
    for range in &ack.sequences {
        let single = range.start == range.end;
        write_u8(&mut buf, single as u8);
        write_u24_le(&mut buf, range.start);
        if !single {
            write_u24_le(&mut buf, range.end);
        }
    }
    buf
}

// ── top-level dispatch ─────────────────────────────────────────────────

/// Parse any RakNet packet from raw bytes.
pub fn parse_packet(buf: &[u8]) -> Result<RaknetPacket, PacketError> {
    if buf.is_empty() {
        return Err(PacketError::BufferTooShort);
    }
    let id = buf[0];
    let frame_type = FrameType::from_byte(id);
    match frame_type {
        None => match id {
            id::UNCONNECTED_PING
            | id::OPEN_CONNECTION_REQUEST_1
            | id::OPEN_CONNECTION_REQUEST_2 => Ok(RaknetPacket::Offline(decode_offline(buf)?)),
            _ => Err(PacketError::UnknownPacketId { id }),
        },
        Some(FrameType::Ack) => Ok(RaknetPacket::Ack(decode_ack(buf)?)),
        Some(FrameType::Nack) => Ok(RaknetPacket::Nack(decode_nack(buf)?)),
        Some(FrameType::FrameSet) => {
            let entry = decode_datagram(buf)?;
            Ok(RaknetPacket::FrameSet {
                sequence_number: entry.sequence_number,
                frames: entry.frames,
            })
        }
    }
}

// ── misc helpers ───────────────────────────────────────────────────────

fn read_u32_be(buf: &[u8], pos: &mut usize) -> Result<u32, PacketError> {
    if *pos + 4 > buf.len() {
        return Err(PacketError::BufferTooShort);
    }
    let val = u32::from_be_bytes([buf[*pos], buf[*pos + 1], buf[*pos + 2], buf[*pos + 3]]);
    *pos += 4;
    Ok(val)
}

/// Maximum MTU we accept. Packets claiming larger are clamped.
const MAX_MTU: u16 = 1492;
