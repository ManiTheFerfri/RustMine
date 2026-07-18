//! Generic Bedrock packet reader/writer helpers.
//!
//! Bedrock's game packets (inside a batch) start with a single header byte
//! (the packet ID) followed by payload. Most fields are little-endian or
//! VarInt-encoded. This module exposes small helpers that the per-packet
//! encoders/decoders use.

use rustmine_nbt::{read_var_u32, write_var_u32};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("packet buffer too short")]
    TooShort,
    #[error("varint decode error")]
    VarInt,
    #[error("string too long")]
    StringTooLong,
    #[error("{0}")]
    Other(String),
}

/// Read a packet header from a raw packet body: returns the packet ID
/// and advances `pos` past the id byte.
pub fn read_packet_header(buf: &[u8], pos: &mut usize) -> Result<u8, CodecError> {
    if *pos >= buf.len() {
        return Err(CodecError::TooShort);
    }
    let id = buf[*pos];
    *pos += 1;
    Ok(id)
}

/// Write a packet with the given id and payload bytes (just concatenates).
pub fn write_packet(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + payload.len());
    out.push(id);
    out.extend_from_slice(payload);
    out
}

/// Read a Bedrock-style VarInt-length-prefixed UTF-8 string.
pub fn read_string(buf: &[u8], pos: &mut usize) -> Result<String, CodecError> {
    let len = read_var_u32(buf, pos).map_err(|_| CodecError::VarInt)? as usize;
    if *pos + len > buf.len() {
        return Err(CodecError::TooShort);
    }
    let bytes = buf[*pos..*pos + len].to_vec();
    *pos += len;
    String::from_utf8(bytes).map_err(|e| CodecError::Other(e.to_string()))
}

/// Write a Bedrock-style length-prefixed UTF-8 string.
pub fn write_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    write_var_u32(buf, bytes.len() as u32);
    buf.extend_from_slice(bytes);
}

/// Trait implemented by Bedrock packets that can encode/decode themselves.
///
/// Kept deliberately minimal for Phase 2 — most packets are written directly
/// with the helpers above.
pub trait BedrockPacket {
    const ID: u8;
    fn encode(&self) -> Vec<u8>;
}
