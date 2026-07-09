#![allow(dead_code)] // fields/methods used as protocol implementation grows

//! RakNet transport protocol for Minecraft Bedrock Edition.
//!
//! Implements UDP socket handling, offline/online message flow,
//! reliability layers (unreliable, reliable, reliable-ordered),
//! datagram framing, ACK/NACK, connection lifecycle, and MTU negotiation.
//!
//! ## References
//! - <https://wiki.vg/Raknet_Protocol>
//! - <https://github.com/pmmp/RakLib> (PHP reference)
//! - <https://github.com/CloudburstMC/Protocol> (Java reference)

mod connection;
mod packet;
mod server;

pub use connection::{Connection, ConnectionState};
pub use packet::id;
pub use packet::{
    decode_ack, decode_datagram, decode_frame, decode_nack, decode_offline, encode_ack,
    encode_datagram, encode_frame, encode_offline, parse_packet, Ack, Frame, FrameSetEntry,
    FrameType, Nack, OfflinePacket, RaknetPacket, Reliability, SequenceRange, MAGIC,
};
pub use server::RaknetServer;

/// RakNet protocol version used by Bedrock.
pub const RAKNET_PROTOCOL_VERSION: u8 = 11;

/// Maximum transmission unit (typical for Bedrock).
pub const MAX_MTU: u16 = 1492;
