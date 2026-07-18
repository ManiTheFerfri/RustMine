//! Variable-length integer helpers used by Bedrock's network protocol.
//!
//! Bedrock uses protobuf-style unsigned VarInts, and ZigZag-encoded signed
//! VarInts (VarSInt) for positions, entity IDs, etc. These helpers operate
//! on `&[u8]` slices for decoding, and append to `Vec<u8>` for encoding.

use crate::NbtError;

/// Read an unsigned VarInt (up to 5 bytes for u32).
pub fn read_var_u32(buf: &[u8], pos: &mut usize) -> Result<u32, NbtError> {
    let mut result: u32 = 0;
    let mut shift: u32 = 0;
    loop {
        if *pos >= buf.len() {
            return Err(NbtError::UnexpectedEof);
        }
        if shift >= 35 {
            return Err(NbtError::VarIntTooLong);
        }
        let b = buf[*pos];
        *pos += 1;
        result |= ((b & 0x7f) as u32) << shift;
        if b & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
    }
}

/// Read an unsigned VarLong (up to 10 bytes for u64).
pub fn read_var_u64(buf: &[u8], pos: &mut usize) -> Result<u64, NbtError> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    loop {
        if *pos >= buf.len() {
            return Err(NbtError::UnexpectedEof);
        }
        if shift >= 70 {
            return Err(NbtError::VarIntTooLong);
        }
        let b = buf[*pos];
        *pos += 1;
        result |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
    }
}

/// Read a ZigZag-encoded signed VarInt.
pub fn read_var_i32(buf: &[u8], pos: &mut usize) -> Result<i32, NbtError> {
    let raw = read_var_u32(buf, pos)?;
    Ok(zigzag_decode_i32(raw))
}

/// Read a ZigZag-encoded signed VarLong.
pub fn read_var_i64(buf: &[u8], pos: &mut usize) -> Result<i64, NbtError> {
    let raw = read_var_u64(buf, pos)?;
    Ok(zigzag_decode_i64(raw))
}

/// Write an unsigned VarInt.
pub fn write_var_u32(buf: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut b = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            b |= 0x80;
        }
        buf.push(b);
        if value == 0 {
            break;
        }
    }
}

/// Write an unsigned VarLong.
pub fn write_var_u64(buf: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut b = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            b |= 0x80;
        }
        buf.push(b);
        if value == 0 {
            break;
        }
    }
}

/// Write a ZigZag-encoded signed VarInt.
pub fn write_var_i32(buf: &mut Vec<u8>, value: i32) {
    write_var_u32(buf, zigzag_encode_i32(value));
}

/// Write a ZigZag-encoded signed VarLong.
pub fn write_var_i64(buf: &mut Vec<u8>, value: i64) {
    write_var_u64(buf, zigzag_encode_i64(value));
}

fn zigzag_encode_i32(v: i32) -> u32 {
    ((v << 1) ^ (v >> 31)) as u32
}

fn zigzag_decode_i32(v: u32) -> i32 {
    ((v >> 1) as i32) ^ -((v & 1) as i32)
}

fn zigzag_encode_i64(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

fn zigzag_decode_i64(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn var_u32_roundtrip() {
        for v in [0u32, 1, 127, 128, 255, 16384, 1_000_000, u32::MAX] {
            let mut buf = Vec::new();
            write_var_u32(&mut buf, v);
            let mut pos = 0;
            assert_eq!(read_var_u32(&buf, &mut pos).unwrap(), v);
            assert_eq!(pos, buf.len());
        }
    }

    #[test]
    fn var_i32_roundtrip() {
        for v in [0i32, -1, 1, -64, 64, -1_000_000, 1_000_000, i32::MIN, i32::MAX] {
            let mut buf = Vec::new();
            write_var_i32(&mut buf, v);
            let mut pos = 0;
            assert_eq!(read_var_i32(&buf, &mut pos).unwrap(), v);
            assert_eq!(pos, buf.len());
        }
    }

    #[test]
    fn var_i64_roundtrip() {
        for v in [0i64, -1, 1, i64::MIN, i64::MAX] {
            let mut buf = Vec::new();
            write_var_i64(&mut buf, v);
            let mut pos = 0;
            assert_eq!(read_var_i64(&buf, &mut pos).unwrap(), v);
        }
    }
}
