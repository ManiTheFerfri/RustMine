//! Minecraft Bedrock protocol definitions and (de)serialization.
//!
//! Defines packet types, version-aware codecs, batching, and framing
//! for the Bedrock application-layer protocol (v1001 / 1.26.30).
//!
//! Phase 2 covers just enough of the protocol to let a client join an
//! empty world: batched packet framing, login handshake, play-status,
//! resource-pack response, start-game and basic movement.

#![allow(dead_code)]

pub mod batch;
pub mod chunk;
pub mod codec;
pub mod decode;
pub mod id;
pub mod login;

pub use batch::{decode_batch, encode_batch, COMPRESSION_THRESHOLD};
pub use codec::{read_packet_header, write_packet, BedrockPacket};

/// Target Bedrock protocol version.
/// Pinned to Minecraft Bedrock 1.26.30 (protocol v1001) per
/// CloudburstMC/Protocol VERSIONS.md.
///
/// Source: <https://github.com/CloudburstMC/Protocol/blob/3.0/VERSIONS.md>
pub const SUPPORTED_PROTOCOL_VERSION: u32 = 1001;

/// Minecraft version string matching the target protocol version.
pub const SUPPORTED_GAME_VERSION: &str = "1.26.30";
