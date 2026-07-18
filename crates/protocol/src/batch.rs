//! Bedrock "batched packet" framing.
//!
//! After the RakNet handshake completes, game packets are wrapped in a
//! single 0xfe frame that contains multiple length-prefixed packets.
//! Early in the session (pre-start-game) these packets are uncompressed;
//! later they may be zlib-compressed. Phase 2 implements the uncompressed
//! path only (compression is negotiated later via NetworkSettings).
//!
//! Wire format (inside a RakNet frame body):
//!
//! ```text
//!   [0xfe] [u16 BE len] [packet bytes, repeated]
//! ```

use thiserror::Error;

/// Default payload size (post-compression) beyond which a batch *should*
/// be compressed. We don't enable compression yet, but keep the constant
/// for documentation parity.
pub const COMPRESSION_THRESHOLD: usize = 256;

#[derive(Debug, Error)]
pub enum BatchError {
    #[error("empty batch")]
    Empty,
    #[error("bad batch magic byte (expected 0xfe, got {0:#04x})")]
    BadMagic(u8),
    #[error("truncated batch payload")]
    Truncated,
}

/// Encode a batch of pre-encoded Bedrock packets into a 0xfe wrapper
/// (uncompressed). Each inner packet is prefixed with its length as a
//! unsigned VarInt.
pub fn encode_batch(packets: &[Vec<u8>]) -> Vec<u8> {
    let mut body = Vec::new();
    for pkt in packets {
        rustmine_nbt::write_var_u32(&mut body, pkt.len() as u32);
        body.extend_from_slice(pkt);
    }
    let mut out = Vec::with_capacity(1 + body.len());
    out.push(0xfe);
    out.extend_from_slice(&body);
    out
}

/// Decode an uncompressed 0xfe batch into individual packet bodies.
/// The returned buffers include the packet ID byte but NOT the length
/// prefix.
pub fn decode_batch(raw: &[u8]) -> Result<Vec<&[u8]>, BatchError> {
    if raw.is_empty() {
        return Err(BatchError::Empty);
    }
    if raw[0] != 0xfe {
        return Err(BatchError::BadMagic(raw[0]));
    }
    let mut pos = 1;
    let mut out = Vec::new();
    while pos < raw.len() {
        let mut cur = pos;
        let len = rustmine_nbt::read_var_u32(raw, &mut cur)
            .map_err(|_| BatchError::Truncated)? as usize;
        if cur + len > raw.len() {
            return Err(BatchError::Truncated);
        }
        out.push(&raw[cur..cur + len]);
        pos = cur + len;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_batch_roundtrip_is_empty_list() {
        let raw = encode_batch(&[]);
        assert_eq!(raw, vec![0xfe]);
        assert!(decode_batch(&raw).unwrap().is_empty());
    }

    #[test]
    fn batch_roundtrip() {
        let packets = vec![vec![1u8, 2, 3], vec![10, 20], vec![255]];
        let raw = encode_batch(&packets);
        let decoded = decode_batch(&raw).unwrap();
        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded[0], &[1, 2, 3]);
        assert_eq!(decoded[1], &[10, 20]);
        assert_eq!(decoded[2], &[255]);
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(decode_batch(&[0xff]).is_err());
    }
}
