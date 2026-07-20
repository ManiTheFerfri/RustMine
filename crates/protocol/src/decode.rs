//! Chunk decoding for parsing received Bedrock chunk data.
//!
//! This module handles decoding of LevelChunk packets and subchunk data
//! from the network. Useful for client simulation or world loading.

use crate::nbt::{read_var_u32, NbtError};

/// Decoded subchunk data
#[derive(Debug, Clone)]
pub struct DecodedSubChunk {
    /// Version byte
    pub version: u8,
    /// Block palette (runtime IDs)
    pub palette: Vec<u32>,
    /// Decoded block indices (4096 entries)
    pub blocks: Vec<u16>,
}

impl DecodedSubChunk {
    /// Get block runtime ID at local coordinates
    pub fn get_block(&self, lx: u8, ly: u8, lz: u8) -> u32 {
        let idx = Self::local_to_index(lx, ly, lz);
        let palette_idx = self.blocks.get(idx).copied().unwrap_or(0) as usize;
        self.palette.get(palette_idx).copied().unwrap_or(0)
    }

    fn local_to_index(lx: u8, ly: u8, lz: u8) -> usize {
        (ly as usize) * 16 * 16 + (lz as usize) * 16 + (lx as usize)
    }
}

/// Decode a subchunk from network format
pub fn decode_subchunk(data: &[u8]) -> Result<DecodedSubChunk, ChunkDecodeError> {
    if data.is_empty() {
        return Err(ChunkDecodeError::UnexpectedEnd);
    }

    let mut pos = 0usize;
    
    // Version
    let version = data[pos];
    pos += 1;
    
    // Number of storages
    let num_storages = data[pos];
    pos += 1;
    
    // For now, handle single storage only
    if num_storages != 1 {
        return Err(ChunkDecodeError::UnsupportedStorageCount(num_storages));
    }
    
    // Bits per block
    let bits_per_block = data[pos] as usize;
    pos += 1;
    
    if bits_per_block == 0 {
        // Empty chunk - all air
        return Ok(DecodedSubChunk {
            version,
            palette: vec![0], // Air
            blocks: vec![0; 4096],
        });
    }
    
    // Calculate words per chunk
    let blocks_per_word = 32 / bits_per_block;
    let words_needed = (4096 + blocks_per_word - 1) / blocks_per_word;
    
    // Decode block indices from words
    let mut blocks = Vec::with_capacity(4096);
    
    for _ in 0..words_needed {
        if pos + 4 > data.len() {
            return Err(ChunkDecodeError::UnexpectedEnd);
        }
        
        let word = u32::from_le_bytes([
            data[pos], data[pos + 1], data[pos + 2], data[pos + 3],
        ]);
        pos += 4;
        
        let mut bits_read = 0;
        while bits_read < 32 && blocks.len() < 4096 {
            let mask = (1u32 << bits_per_block) - 1;
            let idx = (word >> bits_read) & mask;
            blocks.push(idx as u16);
            bits_read += bits_per_block;
        }
    }
    
    // Pad to 4096 if needed
    while blocks.len() < 4096 {
        blocks.push(0);
    }
    
    // Read palette
    let palette_len = read_var_u32(data, &mut pos)
        .map_err(|_| ChunkDecodeError::PaletteReadError)? as usize;
    
    let mut palette = Vec::with_capacity(palette_len);
    for _ in 0..palette_len {
        let id = read_var_u32(data, &mut pos)
            .map_err(|_| ChunkDecodeError::PaletteReadError)?;
        palette.push(id);
    }
    
    Ok(DecodedSubChunk {
        version,
        palette,
        blocks,
    })
}

/// Chunk decoding errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum ChunkDecodeError {
    #[error("unexpected end of data")]
    UnexpectedEnd,
    
    #[error("unsupported storage count: {0}")]
    UnsupportedStorageCount(u8),
    
    #[error("failed to read palette")]
    PaletteReadError,
    
    #[error("invalid subchunk version: {0}")]
    InvalidVersion(u8),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{encode_subchunk, SubChunk};

    #[test]
    fn test_roundtrip_empty_subchunk() {
        let original = SubChunk::new();
        let encoded = encode_subchunk(&original);
        let decoded = decode_subchunk(&encoded).unwrap();
        
        assert_eq!(decoded.version, 8);
        assert_eq!(decoded.palette, vec![0]);
        assert_eq!(decoded.blocks.len(), 4096);
    }

    #[test]
    fn test_roundtrip_filled_subchunk() {
        let original = SubChunk::filled(5); // Oak log
        let encoded = encode_subchunk(&original);
        let decoded = decode_subchunk(&encoded).unwrap();
        
        assert_eq!(decoded.palette, vec![5]);
        assert_eq!(decoded.get_block(0, 0, 0), 5);
    }

    #[test]
    fn test_roundtrip_terrain_subchunk() {
        let original = SubChunk::with_terrain(64);
        let encoded = encode_subchunk(&original);
        let decoded = decode_subchunk(&encoded).unwrap();
        
        // Should preserve terrain structure
        assert!(!decoded.palette.is_empty());
        assert_eq!(decoded.blocks.len(), 4096);
    }
}
