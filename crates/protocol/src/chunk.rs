//! Bedrock chunk encoding for network transmission.
//!
//! Handles encoding of 16x256x16 chunks into the Bedrock network format
//! used by the LevelChunk packet (0x3a).

use crate::id;
use crate::write_packet;
use crate::codec::write_var_i32;
use crate::nbt::{write_var_i32 as nbt_write_i32, write_var_u32 as nbt_write_u32};

/// A 16x16x16 subchunk for network encoding
#[derive(Debug, Clone)]
pub struct SubChunk {
    /// Block palette mapping indices to runtime IDs
    pub palette: Vec<u32>,
    /// Block data stored as indices into palette (4096 entries for 16^3)
    pub blocks: Vec<u16>,
}

impl SubChunk {
    /// Create a new empty (air) subchunk
    pub fn new() -> Self {
        Self {
            palette: vec![0], // Start with air (ID 0)
            blocks: vec![0; 4096],
        }
    }

    /// Create a subchunk filled with a single block type
    pub fn filled(runtime_id: u32) -> Self {
        Self {
            palette: vec![runtime_id],
            blocks: vec![0; 4096],
        }
    }

    /// Create a terrain subchunk with simple height-based block types
    pub fn with_terrain(surface_world_y: i32) -> Self {
        let mut subchunk = Self::new();
        let base_y = surface_world_y / 16;
        let local_surface = surface_world_y % 16;
        
        for lx in 0..16u8 {
            for lz in 0..16u8 {
                for ly in 0..16u8 {
                    let world_y = base_y * 16 + ly as i32;
                    let runtime_id = if world_y == 0 {
                        10 // Bedrock
                    } else if world_y < local_surface - 5 {
                        1 // Stone
                    } else if world_y < local_surface {
                        3 // Dirt
                    } else if world_y == local_surface {
                        2 // Grass
                    } else {
                        0 // Air
                    };
                    subchunk.set_block(lx, ly, lz, runtime_id);
                }
            }
        }
        subchunk
    }

    /// Set block at local coordinates (0-15)
    pub fn set_block(&mut self, lx: u8, ly: u8, lz: u8, runtime_id: u32) {
        let idx = Self::local_to_index(lx, ly, lz);
        
        // Find or add to palette
        let palette_idx = if let Some(pos) = self.palette.iter().position(|&id| id == runtime_id) {
            pos
        } else {
            let pos = self.palette.len();
            self.palette.push(runtime_id);
            pos
        };

        self.blocks[idx] = palette_idx as u16;
    }

    fn local_to_index(lx: u8, ly: u8, lz: u8) -> usize {
        (ly as usize) * 16 * 16 + (lz as usize) * 16 + (lx as usize)
    }
}

impl Default for SubChunk {
    fn default() -> Self {
        Self::new()
    }
}

/// Encode a full chunk column into Bedrock network format.
/// 
/// The format consists of:
/// - Chunk X/Z coordinates
/// - Subchunk count
/// - Subchunk data for each height level
/// - Border/legacy data (kept minimal)
pub fn encode_chunk_column(
    chunk_x: i32,
    chunk_z: i32,
    subchunks: &[SubChunk],
    cache_enabled: bool,
) -> Vec<u8> {
    let mut payload = Vec::new();
    
    // Coordinates
    write_var_i32(&mut payload, chunk_x);
    write_var_i32(&mut payload, chunk_z);
    
    // Subchunk count (1-16)
    let needed_height = calculate_needed_height(subchunks).max(1);
    payload.extend_from_slice(&(needed_height as u32).to_le_bytes());
    
    // Cache enabled flag
    payload.push(if cache_enabled { 1 } else { 0 });
    
    // Encode each subchunk
    for i in 0..needed_height {
        if i < subchunks.len() {
            let sub_data = encode_subchunk(&subchunks[i]);
            payload.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
            payload.extend_from_slice(&sub_data);
        } else {
            // Empty subchunk
            let empty = encode_empty_subchunk();
            payload.extend_from_slice(&(empty.len() as u32).to_le_bytes());
            payload.extend_from_slice(&empty);
        }
    }
    
    // Blob hash (used for caching) - 0 for no caching
    payload.extend_from_slice(&0u64.to_le_bytes());
    
    // Legacy border (deprecated but still sent)
    payload.extend_from_slice(&0i32.to_le_bytes());
    
    // Cache status
    payload.push(0); // 0 = no blob
    
    write_packet(id::LEVEL_CHUNK, &payload)
}

/// Calculate how many subchunks we need to send (find highest non-empty)
fn calculate_needed_height(subchunks: &[SubChunk]) -> usize {
    for i in (0..subchunks.len()).rev() {
        if !is_subchunk_empty(&subchunks[i]) {
            return i + 1;
        }
    }
    1 // Always send at least one
}

/// Check if a subchunk is completely empty (all air)
fn is_subchunk_empty(subchunk: &SubChunk) -> bool {
    subchunk.blocks.iter().all(|&idx| idx == 0)
}

/// Encode a single subchunk into Bedrock subchunk format (version 8)
///
/// Format:
/// - version: u8 (8 = standard runtime ID format)
/// - num_storages: u8
/// - Block data: bits per block, packed words
/// - Palette count and entries
fn encode_subchunk(subchunk: &SubChunk) -> Vec<u8> {
    let mut data = Vec::new();
    
    // Version 8 (standard runtime ID format)
    data.push(8);
    
    // Number of storages (1 = single palette)
    data.push(1);
    
    // Calculate bits per block needed for palette size
    let palette_size = subchunk.palette.len();
    let bits_per_block = calculate_bits_per_block(palette_size);
    
    // Write bits per block as varint
    nbt_write_u32(&mut data, bits_per_block as u32);
    
    // Calculate words needed
    let blocks_per_word = 32 / bits_per_block;
    let words_needed = (4096 + blocks_per_word - 1) / blocks_per_word;
    
    // Pack block indices into words
    let mut current_word = 0u32;
    let mut bits_in_word = 0u32;
    
    for &block_idx in &subchunk.blocks {
        current_word |= (block_idx as u32) << bits_in_word;
        bits_in_word += bits_per_block;
        
        if bits_in_word >= 32 {
            data.extend_from_slice(&current_word.to_le_bytes());
            current_word = 0;
            bits_in_word = 0;
        }
    }
    
    // Write remaining bits
    if bits_in_word > 0 {
        data.extend_from_slice(&current_word.to_le_bytes());
    }
    
    // Palette entries count
    nbt_write_u32(&mut data, palette_size as u32);
    
    // Palette entries (runtime IDs)
    for &runtime_id in &subchunk.palette {
        nbt_write_u32(&mut data, runtime_id);
    }
    
    data
}

/// Encode an empty subchunk (all air)
fn encode_empty_subchunk() -> Vec<u8> {
    let mut data = Vec::new();
    
    // Version 8
    data.push(8);
    
    // Single storage
    data.push(1);
    
    // 1 bit per block (minimum)
    data.push(1);
    
    // Word array: 128 words of zeros for 4096 blocks at 1 bit each
    for _ in 0..128u32 {
        data.extend_from_slice(&0u32.to_le_bytes());
    }
    
    // Palette with just air
    nbt_write_u32(&mut data, 1);
    nbt_write_u32(&mut data, 0);
    
    data
}

/// Calculate bits per block needed for given palette size
fn calculate_bits_per_block(palette_size: usize) -> usize {
    if palette_size <= 1 {
        1
    } else if palette_size <= 2 {
        1
    } else if palette_size <= 4 {
        2
    } else if palette_size <= 8 {
        3
    } else if palette_size <= 16 {
        4
    } else if palette_size <= 32 {
        5
    } else if palette_size <= 64 {
        6
    } else if palette_size <= 128 {
        7
    } else if palette_size <= 256 {
        8
    } else {
        16 // Fallback to 16 bits
    }
}

/// Network chunk publisher update packet (0x79)
pub fn network_chunk_publisher_update(
    coords: (i32, i32, i32),
    radius: u32,
) -> Vec<u8> {
    let mut payload = Vec::new();
    nbt_write_i32(&mut payload, coords.0);
    nbt_write_i32(&mut payload, coords.1);
    nbt_write_i32(&mut payload, coords.2);
    payload.extend_from_slice(&radius.to_le_bytes());
    // Saved chunks count (0 = none)
    payload.extend_from_slice(&0u32.to_le_bytes());
    
    write_packet(id::NETWORK_CHUNK_PUBLISHER_UPDATE, &payload)
}

/// Chunk radius updated packet (0x46)
pub fn chunk_radius_updated(radius: i32) -> Vec<u8> {
    let mut payload = Vec::new();
    nbt_write_i32(&mut payload, radius);
    write_packet(id::CHUNK_RADIUS_UPDATED, &payload)
}

/// Build subchunks from raw world data
pub fn build_chunk_subchunks(blocks: &[[u32; 4096]; 16]) -> Vec<SubChunk> {
    let mut subchunks = Vec::with_capacity(16);
    
    for y in 0..16 {
        let mut subchunk = SubChunk::new();
        
        for lx in 0..16u8 {
            for lz in 0..16u8 {
                for ly in 0..16u8 {
                    let idx = (ly as usize) * 16 * 16 + (lz as usize) * 16 + (lx as usize);
                    let runtime_id = blocks[y][idx];
                    subchunk.set_block(lx, ly, lz, runtime_id);
                }
            }
        }
        
        subchunks.push(subchunk);
    }
    
    subchunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subchunk_new() {
        let subchunk = SubChunk::new();
        assert_eq!(subchunk.palette.len(), 1);
        assert_eq!(subchunk.blocks.len(), 4096);
    }

    #[test]
    fn test_subchunk_filled() {
        let subchunk = SubChunk::filled(5); // Oak log
        assert_eq!(subchunk.palette.len(), 1);
        assert_eq!(subchunk.palette[0], 5);
    }

    #[test]
    fn test_subchunk_set_block() {
        let mut subchunk = SubChunk::new();
        subchunk.set_block(0, 0, 0, 1);
        assert!(subchunk.palette.contains(&1));
    }

    #[test]
    fn test_encode_empty_subchunk() {
        let subchunk = SubChunk::new();
        let encoded = encode_subchunk(&subchunk);
        
        // Should start with version byte
        assert_eq!(encoded[0], 8);
        // Should have reasonable size
        assert!(encoded.len() > 500);
    }

    #[test]
    fn test_encode_filled_subchunk() {
        let subchunk = SubChunk::filled(1);
        let encoded = encode_subchunk(&subchunk);
        
        assert!(!encoded.is_empty());
        assert_eq!(encoded[0], 8);
    }

    #[test]
    fn test_encode_terrain_subchunk() {
        let subchunk = SubChunk::with_terrain(64);
        let encoded = encode_subchunk(&subchunk);
        
        // Should encode terrain
        assert!(!encoded.is_empty());
    }

    #[test]
    fn test_encode_chunk_column() {
        let subchunks = vec![SubChunk::new(); 16];
        let encoded = encode_chunk_column(0, 0, &subchunks, false);
        
        // Should produce valid packet
        assert!(encoded.len() > 10);
    }

    #[test]
    fn test_calculate_bits_per_block() {
        assert_eq!(calculate_bits_per_block(1), 1);
        assert_eq!(calculate_bits_per_block(2), 1);
        assert_eq!(calculate_bits_per_block(3), 2);
        assert_eq!(calculate_bits_per_block(16), 4);
        assert_eq!(calculate_bits_per_block(256), 8);
    }
}
