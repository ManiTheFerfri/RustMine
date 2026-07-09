//! RakNet transport protocol implementation for Minecraft Bedrock.
//!
//! Implements UDP socket handling, reliability layers, connection handshake,
//! packet splitting/reassembly, and ACK/NACK handling per the RakNet specification.

/// RakNet protocol version. Updated per Bedrock protocol version.
pub const RAKNET_PROTOCOL_VERSION: u8 = 11;
