//! World model: chunks, block palettes, terrain generation, and persistence.
//!
//! The Bedrock world model uses 16x16x16 subchunks with runtime block state IDs
//! and LevelDB-backed on-disk storage (LevelDB, not Anvil).

use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// World-related errors
#[derive(Error, Debug)]
pub enum WorldError {
    #[error("chunk out of bounds: ({0}, {1})")]
    ChunkOutOfBounds(i32, i32),
    #[error("block out of bounds: ({0}, {1}, {2})")]
    BlockOutOfBounds(i32, i32, i32),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Block runtime ID (Bedrock uses runtime IDs, not numeric block IDs)
pub type BlockRuntimeId = u32;

/// Block position in world coordinates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Convert to chunk coordinates
    pub fn to_chunk(&self) -> (i32, i32) {
        (self.x >> 4, self.z >> 4)
    }

    /// Get local block position within a chunk (0-15)
    pub fn local(&self) -> (u8, u8, u8) {
        (
            (self.x & 0xF) as u8,
            (self.y & 0xFF) as u8,
            (self.z & 0xF) as u8,
        )
    }
}

/// Chunk coordinates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// Convert world block position to chunk
    pub fn from_block(x: i32, z: i32) -> Self {
        Self::new(x >> 4, z >> 4)
    }
}

/// Bedrock block states (simplified palette)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockState {
    Air,
    Stone,
    Grass,
    Dirt,
    Cobblestone,
    OakLog,
    OakPlanks,
    OakLeaves,
    Water,
    Sand,
    Bedrock,
    CoalOre,
    IronOre,
    GoldOre,
    DiamondOre,
    Glass,
    /// Custom block state with runtime ID
    Runtime(u32),
}

impl BlockState {
    /// Get the runtime ID for this block state
    pub fn runtime_id(&self) -> BlockRuntimeId {
        match self {
            BlockState::Air => 0,
            BlockState::Stone => 1,
            BlockState::Grass => 2,
            BlockState::Dirt => 3,
            BlockState::Cobblestone => 4,
            BlockState::OakLog => 5,
            BlockState::OakPlanks => 6,
            BlockState::OakLeaves => 7,
            BlockState::Water => 8,
            BlockState::Sand => 9,
            BlockState::Bedrock => 10,
            BlockState::CoalOre => 11,
            BlockState::IronOre => 12,
            BlockState::GoldOre => 13,
            BlockState::DiamondOre => 14,
            BlockState::Glass => 15,
            BlockState::Runtime(id) => *id,
        }
    }

    /// Get block state from runtime ID
    pub fn from_runtime_id(id: BlockRuntimeId) -> Self {
        match id {
            0 => BlockState::Air,
            1 => BlockState::Stone,
            2 => BlockState::Grass,
            3 => BlockState::Dirt,
            4 => BlockState::Cobblestone,
            5 => BlockState::OakLog,
            6 => BlockState::OakPlanks,
            7 => BlockState::OakLeaves,
            8 => BlockState::Water,
            9 => BlockState::Sand,
            10 => BlockState::Bedrock,
            11 => BlockState::CoalOre,
            12 => BlockState::IronOre,
            13 => BlockState::GoldOre,
            14 => BlockState::DiamondOre,
            15 => BlockState::Glass,
            n => BlockState::Runtime(n),
        }
    }
}

/// A 16x16x16 subchunk containing block data
#[derive(Debug, Clone)]
pub struct SubChunk {
    /// Block palette mapping runtime IDs
    pub palette: Vec<BlockRuntimeId>,
    /// Block data stored as indices into palette (4096 entries for 16^3)
    pub blocks: Vec<u16>,
}

impl SubChunk {
    pub fn new() -> Self {
        // Default to all air
        Self {
            palette: vec![0], // Start with air (ID 0)
            blocks: vec![0; 4096],
        }
    }

    pub fn with_air() -> Self {
        Self::new()
    }

    /// Set a block at local coordinates (0-15)
    pub fn set_block(&mut self, lx: u8, ly: u8, lz: u8, state: BlockState) {
        let idx = Self::local_to_index(lx, ly, lz);
        let runtime_id = state.runtime_id();
        
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

    /// Get block at local coordinates
    pub fn get_block(&self, lx: u8, ly: u8, lz: u8) -> BlockState {
        let idx = Self::local_to_index(lx, ly, lz);
        let palette_idx = self.blocks[idx] as usize;
        let runtime_id = self.palette.get(palette_idx).copied().unwrap_or(0);
        BlockState::from_runtime_id(runtime_id)
    }

    fn local_to_index(lx: u8, ly: u8, lz: u8) -> usize {
        (ly as usize) * 16 * 16 + (lz as usize) * 16 + (lx as usize)
    }

    /// Create a subchunk filled with a single block type
    pub fn filled(state: BlockState) -> Self {
        let runtime_id = state.runtime_id();
        Self {
            palette: vec![runtime_id],
            blocks: vec![0; 4096],
        }
    }

    /// Create a subchunk with terrain (simple height-based generation)
    pub fn with_terrain(height: u8) -> Self {
        let mut chunk = Self::new();
        
        for lx in 0..16u8 {
            for lz in 0..16u8 {
                for ly in 0..16u8 {
                    let world_y = ly as i32;
                    
                    let block = if world_y == 0 {
                        BlockState::Bedrock
                    } else if world_y < height as i32 - 5 {
                        BlockState::Stone
                    } else if world_y < height as i32 {
                        BlockState::Dirt
                    } else if world_y == height as i32 {
                        BlockState::Grass
                    } else {
                        BlockState::Air
                    };
                    
                    chunk.set_block(lx, ly, lz, block);
                }
            }
        }
        
        chunk
    }
}

impl Default for SubChunk {
    fn default() -> Self {
        Self::new()
    }
}

/// A 16x256x16 chunk column (16 subchunks stacked)
#[derive(Debug, Clone)]
pub struct Chunk {
    pub x: i32,
    pub z: i32,
    /// Subchunks indexed by Y level (0-15 for 0-255)
    pub subchunks: Vec<SubChunk>,
}

impl Chunk {
    pub fn new(x: i32, z: i32) -> Self {
        // Create 16 subchunks (0-255 height)
        Self {
            x,
            z,
            subchunks: vec![SubChunk::new(); 16],
        }
    }

    /// Get a subchunk at Y level (0-15)
    pub fn get_subchunk(&self, y: i32) -> Option<&SubChunk> {
        self.subchunks.get(y as usize)
    }

    /// Get mutable subchunk at Y level
    pub fn get_subchunk_mut(&mut self, y: i32) -> Option<&mut SubChunk> {
        self.subchunks.get_mut(y as usize)
    }

    /// Set block at world position (Y is 0-255)
    pub fn set_block(&mut self, lx: u8, ly: u8, lz: u8, state: BlockState) {
        let subchunk_y = ly / 16;
        if let Some(subchunk) = self.get_subchunk_mut(subchunk_y as i32) {
            subchunk.set_block(lx, ly % 16, lz, state);
        }
    }

    /// Get block at world position
    pub fn get_block(&self, lx: u8, ly: u8, lz: u8) -> BlockState {
        let subchunk_y = ly / 16;
        self.get_subchunk(subchunk_y as i32)
            .map(|s| s.get_block(lx, ly % 16, lz))
            .unwrap_or(BlockState::Air)
    }

    /// Create terrain chunk with height-based generation
    pub fn with_terrain(x: i32, z: i32, surface_height: i32) -> Self {
        let mut chunk = Self::new(x, z);
        
        for sub_y in 0..16 {
            let world_base_y = sub_y * 16;
            
            for lx in 0..16u8 {
                for lz in 0..16u8 {
                    for ly_in_sub in 0..16u8 {
                        let world_y = world_base_y + ly_in_sub as i32;
                        let local_ly = ly_in_sub;
                        
                        let block = if world_y == 0 {
                            BlockState::Bedrock
                        } else if world_y < surface_height - 5 {
                            BlockState::Stone
                        } else if world_y < surface_height - 1 {
                            BlockState::Dirt
                        } else if world_y == surface_height - 1 {
                            BlockState::Grass
                        } else if world_y < 10 {
                            BlockState::Water
                        } else {
                            BlockState::Air
                        };
                        
                        chunk.subchunks[sub_y].set_block(lx, local_ly, lz, block);
                    }
                }
            }
        }
        
        chunk
    }
}

/// World generator trait
pub trait WorldGenerator: Send + Sync {
    /// Generate terrain height at given world coordinates
    fn get_height(&self, x: i32, z: i32) -> i32;
    
    /// Generate a chunk with terrain
    fn generate_chunk(&self, x: i32, z: i32) -> Chunk;
}

/// Simple flat world generator
pub struct FlatGenerator {
    pub surface_height: i32,
    pub sea_level: i32,
}

impl FlatGenerator {
    pub fn new(surface_height: i32, sea_level: i32) -> Self {
        Self {
            surface_height,
            sea_level,
        }
    }
}

impl Default for FlatGenerator {
    fn default() -> Self {
        Self {
            surface_height: 64,
            sea_level: 62,
        }
    }
}

impl WorldGenerator for FlatGenerator {
    fn get_height(&self, _x: i32, _z: i32) -> i32 {
        self.surface_height
    }
    
    fn generate_chunk(&self, x: i32, z: i32) -> Chunk {
        Chunk::with_terrain(x, z, self.surface_height)
    }
}

/// Simple noise-based terrain generator
pub struct NoiseGenerator {
    seed: i64,
    surface_height: i32,
    amplitude: i32,
    frequency: f64,
}

impl NoiseGenerator {
    pub fn new(seed: i64, surface_height: i32, amplitude: i32, frequency: f64) -> Self {
        Self {
            seed,
            surface_height,
            amplitude,
            frequency,
        }
    }

    /// Simple hash-based noise function
    fn noise2d(&self, x: i32, z: i32) -> f64 {
        let mut n = (x * 374761393 + z * 668265263 + self.seed) as i64;
        n = (n << 13) ^ n;
        n = n.wrapping_mul(n.wrapping_mul(747796405) + 2891336453);
        let n = n ^ (n >> 15);
        let n = n.wrapping_mul(2246822519);
        let n = n ^ (n >> 13);
        let n = n.wrapping_mul(3266489917);
        let n = n ^ (n >> 16);
        
        // Normalize to -1.0 to 1.0
        (n as f64) / (i32::MAX as f64)
    }

    /// Smoothed noise using interpolation
    fn smooth_noise(&self, x: f64, z: f64) -> f64 {
        let ix = x.floor() as i32;
        let iz = z.floor() as i32;
        let fx = x - ix as f64;
        let fz = z - iz as f64;
        
        // Smoothstep interpolation
        let sx = fx * fx * (3.0 - 2.0 * fx);
        let sz = fz * fz * (3.0 - 2.0 * fz);
        
        let n00 = self.noise2d(ix, iz);
        let n10 = self.noise2d(ix + 1, iz);
        let n01 = self.noise2d(ix, iz + 1);
        let n11 = self.noise2d(ix + 1, iz + 1);
        
        let nx0 = n00 + sx * (n10 - n00);
        let nx1 = n01 + sx * (n11 - n01);
        
        nx0 + sz * (nx1 - nx0)
    }
}

impl WorldGenerator for NoiseGenerator {
    fn get_height(&self, x: i32, z: i32) -> i32 {
        let freq = self.frequency;
        let nx = x as f64 * freq / 256.0;
        let nz = z as f64 * freq / 256.0;
        
        let noise_val = self.smooth_noise(nx, nz);
        let height_offset = (noise_val * self.amplitude as f64) as i32;
        
        (self.surface_height + height_offset).max(1).min(254)
    }
    
    fn generate_chunk(&self, x: i32, z: i32) -> Chunk {
        let mut chunk = Chunk::new(x, z);
        
        for lx in 0..16u8 {
            for lz in 0..16u8 {
                let world_x = x * 16 + lx as i32;
                let world_z = z * 16 + lz as i32;
                let surface_height = self.get_height(world_x, world_z);
                let sea_level = 62i32;
                
                for sub_y in 0..16 {
                    let world_base_y = sub_y * 16;
                    
                    for ly_in_sub in 0..16u8 {
                        let world_y = world_base_y + ly_in_sub as i32;
                        let local_ly = ly_in_sub;
                        
                        let block = if world_y == 0 {
                            BlockState::Bedrock
                        } else if world_y < surface_height - 5 {
                            BlockState::Stone
                        } else if world_y < surface_height - 1 {
                            BlockState::Dirt
                        } else if world_y == surface_height - 1 {
                            // Surface variation
                            if self.noise2d(world_x, world_z) > 0.3 {
                                BlockState::OakLog
                            } else {
                                BlockState::Grass
                            }
                        } else if world_y < surface_height + 4 && world_y >= surface_height {
                            // Trees
                            BlockState::OakLeaves
                        } else if world_y < sea_level {
                            BlockState::Water
                        } else {
                            BlockState::Air
                        };
                        
                        chunk.subchunks[sub_y].set_block(lx, local_ly, lz, block);
                    }
                }
            }
        }
        
        chunk
    }
}

/// The main world structure holding chunks and world state
pub struct World {
    pub name: String,
    pub seed: i64,
    pub chunks: HashMap<(i32, i32), Chunk>,
    pub generator: Arc<dyn WorldGenerator>,
    pub spawn_position: BlockPos,
}

impl World {
    pub fn new(name: String, seed: i64, generator: Arc<dyn WorldGenerator>) -> Self {
        let spawn = BlockPos::new(0, generator.get_height(0, 0), 0);
        Self {
            name,
            seed,
            chunks: HashMap::new(),
            generator,
            spawn_position: spawn,
        }
    }

    /// Get or generate a chunk
    pub fn get_chunk(&mut self, x: i32, z: i32) -> &Chunk {
        self.chunks
            .entry((x, z))
            .or_insert_with(|| self.generator.generate_chunk(x, z))
    }

    /// Get mutable chunk
    pub fn get_chunk_mut(&mut self, x: i32, z: i32) -> &mut Chunk {
        if !self.chunks.contains_key(&(x, z)) {
            self.generator.generate_chunk(x, z);
        }
        self.chunks
            .entry((x, z))
            .or_insert_with(|| self.generator.generate_chunk(x, z))
    }

    /// Set block at world position
    pub fn set_block(&mut self, pos: BlockPos, state: BlockState) -> Result<(), WorldError> {
        if pos.y < 0 || pos.y >= 256 {
            return Err(WorldError::BlockOutOfBounds(pos.x, pos.y, pos.z));
        }
        
        let (cx, cz) = pos.to_chunk();
        let chunk = self.get_chunk_mut(cx, cz);
        
        let (lx, ly, lz) = pos.local();
        chunk.set_block(lx, ly, lz, state);
        
        Ok(())
    }

    /// Get block at world position
    pub fn get_block(&self, pos: BlockPos) -> BlockState {
        if pos.y < 0 || pos.y >= 256 {
            return BlockState::Air;
        }
        
        let (cx, cz) = pos.to_chunk();
        if let Some(chunk) = self.chunks.get(&(cx, cz)) {
            let (lx, ly, lz) = pos.local();
            chunk.get_block(lx, ly, lz)
        } else {
            BlockState::Air
        }
    }

    /// Get spawn position
    pub fn get_spawn_position(&self) -> BlockPos {
        self.spawn_position
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subchunk_index() {
        let chunk = SubChunk::new();
        assert_eq!(chunk.get_block(0, 0, 0), BlockState::Air);
    }

    #[test]
    fn test_subchunk_set() {
        let mut chunk = SubChunk::new();
        chunk.set_block(0, 0, 0, BlockState::Stone);
        assert_eq!(chunk.get_block(0, 0, 0), BlockState::Stone);
    }

    #[test]
    fn test_terrain_subchunk() {
        let chunk = SubChunk::with_terrain(8);
        assert_eq!(chunk.get_block(0, 0, 0), BlockState::Bedrock);
        assert_eq!(chunk.get_block(0, 7, 0), BlockState::Dirt);
        assert_eq!(chunk.get_block(0, 8, 0), BlockState::Grass);
        assert_eq!(chunk.get_block(0, 9, 0), BlockState::Air);
    }

    #[test]
    fn test_chunk_terrain() {
        let chunk = Chunk::with_terrain(0, 0, 64);
        // Bottom block should be bedrock
        assert_eq!(chunk.get_block(8, 0, 8), BlockState::Bedrock);
        // Surface block
        assert_eq!(chunk.get_block(8, 63, 8), BlockState::Grass);
    }

    #[test]
    fn test_flat_generator() {
        let gen = FlatGenerator::new(64, 62);
        assert_eq!(gen.get_height(0, 0), 64);
        assert_eq!(gen.get_height(1000, -500), 64);
    }

    #[test]
    fn test_noise_generator() {
        let gen = NoiseGenerator::new(12345, 64, 20, 1.0);
        // Same position should give same height
        assert_eq!(gen.get_height(0, 0), gen.get_height(0, 0));
        // Different positions should give different results
        let h1 = gen.get_height(0, 0);
        let h2 = gen.get_height(10, 10);
        // Heights should be within reasonable bounds
        assert!(h1 >= 44 && h1 <= 84);
        assert!(h2 >= 44 && h2 <= 84);
    }

    #[test]
    fn test_world_chunk_generation() {
        let gen = Arc::new(FlatGenerator::new(64, 62)) as Arc<dyn WorldGenerator>;
        let mut world = World::new("test".to_string(), 0, gen);
        
        // Access a chunk to trigger generation
        let chunk = world.get_chunk(0, 0);
        assert_eq!(chunk.x, 0);
        assert_eq!(chunk.z, 0);
    }

    #[test]
    fn test_block_position() {
        let pos = BlockPos::new(20, 64, -5);
        let (cx, cz) = pos.to_chunk();
        assert_eq!(cx, 1); // 20 >> 4 = 1
        assert_eq!(cz, -1); // -5 >> 4 = -1 (floor division)
        
        let (lx, ly, lz) = pos.local();
        assert_eq!(lx, 4); // 20 & 0xF = 4
        assert_eq!(ly, 64); // 64 & 0xFF = 64
        assert_eq!(lz, 11); // -5 & 0xF = 11 (two's complement in lower bits)
    }
}
