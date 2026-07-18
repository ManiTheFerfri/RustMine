//! Bedrock little-endian NBT reader/writer.
//!
//! Implements the Bedrock Edition variant of Named Binary Tag format,
//! which uses little-endian encoding (distinct from Java Edition's
//! big-endian NBT). Also provides the ZigZag VarInt/VarLong helpers
//! used throughout the Bedrock network protocol.
//!
//! Only the subset of NBT required for login/start-game handshakes is
//! implemented here; full compound/array tag support will grow as
//! later phases demand it (network-permissions, item data, etc.).

#![allow(dead_code)]

use std::io;
use thiserror::Error;

pub mod varint;

pub use varint::{read_var_i32, read_var_i64, read_var_u32, write_var_i32, write_var_i64, write_var_u32};

/// Errors that can occur while (de)serializing NBT data.
#[derive(Debug, Error)]
pub enum NbtError {
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("invalid tag id: {0}")]
    InvalidTagId(u8),
    #[error("varint too long")]
    VarIntTooLong,
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Other(String),
}

impl From<NbtError> for io::Error {
    fn from(e: NbtError) -> Self {
        match e {
            NbtError::Io(ioe) => ioe,
            other => io::Error::new(io::ErrorKind::InvalidData, other.to_string()),
        }
    }
}

/// NBT tag identifiers (Bedrock uses the same IDs as Java NBT).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TagId {
    End = 0,
    Byte = 1,
    Short = 2,
    Int = 3,
    Long = 4,
    Float = 5,
    Double = 6,
    ByteArray = 7,
    String = 8,
    List = 9,
    Compound = 10,
    IntArray = 11,
    LongArray = 12,
}

impl TagId {
    pub fn from_u8(b: u8) -> Result<Self, NbtError> {
        match b {
            0 => Ok(Self::End),
            1 => Ok(Self::Byte),
            2 => Ok(Self::Short),
            3 => Ok(Self::Int),
            4 => Ok(Self::Long),
            5 => Ok(Self::Float),
            6 => Ok(Self::Double),
            7 => Ok(Self::ByteArray),
            8 => Ok(Self::String),
            9 => Ok(Self::List),
            10 => Ok(Self::Compound),
            11 => Ok(Self::IntArray),
            12 => Ok(Self::LongArray),
            other => Err(NbtError::InvalidTagId(other)),
        }
    }
}

/// Minimal Bedrock (little-endian) NBT writer for building login/start-game
/// payloads. Grows with later phases.
#[derive(Debug, Default)]
pub struct NbtWriter {
    buf: Vec<u8>,
}

impl NbtWriter {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.buf
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    pub fn write_byte(&mut self, b: u8) {
        self.buf.push(b);
    }

    pub fn write_i16_le(&mut self, v: i16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_i32_le(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_i64_le(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_f32_le(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_string(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let len = (bytes.len() as u16).min(u16::MAX);
        self.write_i16_le(len as i16);
        self.buf.extend_from_slice(&bytes[..len as usize]);
    }

    /// Write a root compound tag (id + empty name + fields), terminated by TAG_End.
    pub fn write_compound_header(&mut self, _name: &str) {
        self.write_byte(TagId::Compound as u8);
        self.write_string(""); // Bedrock network NBT typically uses empty root name
    }

    pub fn write_end(&mut self) {
        self.write_byte(TagId::End as u8);
    }

    /// Write a named byte field inside a compound.
    pub fn write_byte_field(&mut self, name: &str, v: u8) {
        self.write_byte(TagId::Byte as u8);
        self.write_string(name);
        self.write_byte(v);
    }

    /// Write a named int field inside a compound.
    pub fn write_int_field(&mut self, name: &str, v: i32) {
        self.write_byte(TagId::Int as u8);
        self.write_string(name);
        self.write_i32_le(v);
    }

    /// Write a named long field inside a compound.
    pub fn write_long_field(&mut self, name: &str, v: i64) {
        self.write_byte(TagId::Long as u8);
        self.write_string(name);
        self.write_i64_le(v);
    }

    /// Write a named string field inside a compound.
    pub fn write_string_field(&mut self, name: &str, v: &str) {
        self.write_byte(TagId::String as u8);
        self.write_string(name);
        self.write_string(v);
    }

    /// Write a named float field inside a compound.
    pub fn write_float_field(&mut self, name: &str, v: f32) {
        self.write_byte(TagId::Float as u8);
        self.write_string(name);
        self.write_f32_le(v);
    }
}
