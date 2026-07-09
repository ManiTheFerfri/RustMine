//! Minecraft Bedrock protocol definitions and (de)serialization.
//!
//! Defines packet types, version-aware codecs, batching, and framing
//! for the Bedrock application-layer protocol.

/// Target Bedrock protocol version.
/// Pinned to Minecraft Bedrock 1.26.30 (protocol v1001) per
/// CloudburstMC/Protocol VERSIONS.md.
///
/// Source: <https://github.com/CloudburstMC/Protocol/blob/3.0/VERSIONS.md>
pub const SUPPORTED_PROTOCOL_VERSION: u32 = 1001;

/// Minecraft version string matching the target protocol version.
pub const SUPPORTED_GAME_VERSION: &str = "1.26.30";
