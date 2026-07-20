//! Entity-component-system for game entities.
//!
//! Uses `hecs` as the underlying ECS library. Components cover players,
//! mobs, items, projectiles, and other game objects.

pub use hecs;

/// Player components
pub mod player {
    use super::hecs::{Entity, World};
    use crate::Vec3;
    
    /// Player position component
    #[derive(Debug, Clone, Copy)]
    pub struct Position {
        pub x: f32,
        pub y: f32,
        pub z: f32,
    }
    
    impl Position {
        pub fn new(x: f32, y: f32, z: f32) -> Self {
            Self { x, y, z }
        }
        
        pub fn from_vec3(v: Vec3) -> Self {
            Self { x: v.x, y: v.y, z: v.z }
        }
        
        pub fn to_vec3(&self) -> Vec3 {
            Vec3::new(self.x, self.y, self.z)
        }
        
        pub fn chunk_coords(&self) -> (i32, i32) {
            ((self.x as i32) >> 4, (self.z as i32) >> 4)
        }
    }
    
    /// Player rotation component
    #[derive(Debug, Clone, Copy)]
    pub struct Rotation {
        pub yaw: f32,
        pub pitch: f32,
    }
    
    impl Default for Rotation {
        fn default() -> Self {
            Self { yaw: 0.0, pitch: 0.0 }
        }
    }
    
    /// Player velocity component
    #[derive(Debug, Clone, Copy)]
    pub struct Velocity {
        pub x: f32,
        pub y: f32,
        pub z: f32,
    }
    
    impl Default for Velocity {
        fn default() -> Self {
            Self { x: 0.0, y: 0.0, z: 0.0 }
        }
    }
    
    /// Player metadata component
    #[derive(Debug, Clone)]
    pub struct PlayerInfo {
        pub username: String,
        pub entity_id: i64,
        pub runtime_id: u64,
        pub gamemode: u32,
        pub health: f32,
        pub hunger: u32,
    }
    
    /// Player network state
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum NetworkState {
        Connecting,
        LoggingIn,
        Playing,
        Disconnected,
    }
    
    impl Default for NetworkState {
        fn default() -> Self {
            Self::Connecting
        }
    }
    
    /// Helper to spawn a player entity
    pub fn spawn_player(
        world: &mut World,
        username: String,
        entity_id: i64,
        runtime_id: u64,
        x: f32,
        y: f32,
        z: f32,
    ) -> Entity {
        world.spawn((
            Position::new(x, y, z),
            Rotation::default(),
            Velocity::default(),
            PlayerInfo {
                username,
                entity_id,
                runtime_id,
                gamemode: 1, // Creative by default
                health: 20.0,
                hunger: 20,
            },
            NetworkState::Playing,
        ))
    }
}

/// Block components
pub mod block {
    use super::hecs::World;
    
    /// Block position component
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct BlockPosition {
        pub x: i32,
        pub y: i32,
        pub z: i32,
    }
    
    impl BlockPosition {
        pub fn new(x: i32, y: i32, z: i32) -> Self {
            Self { x, y, z }
        }
        
        pub fn chunk_coords(&self) -> (i32, i32) {
            (self.x >> 4, self.z >> 4)
        }
        
        pub fn local_coords(&self) -> (u8, u8, u8) {
            (
                (self.x & 0xF) as u8,
                (self.y & 0xFF) as u8,
                (self.z & 0xF) as u8,
            )
        }
    }
    
    /// Block type component
    #[derive(Debug, Clone, Copy)]
    pub struct BlockType {
        pub runtime_id: u32,
        pub name: &'static str,
    }
    
    impl BlockType {
        pub fn air() -> Self {
            Self { runtime_id: 0, name: "air" }
        }
        
        pub fn stone() -> Self {
            Self { runtime_id: 1, name: "stone" }
        }
        
        pub fn grass() -> Self {
            Self { runtime_id: 2, name: "grass" }
        }
        
        pub fn dirt() -> Self {
            Self { runtime_id: 3, name: "dirt" }
        }
    }
    
    /// Place a block in the world (as a component for simulation)
    pub fn place_block(
        _world: &mut World,
        _pos: BlockPosition,
        _block_type: BlockType,
    ) {
        // In a real implementation, this would update chunk data
        // For now, it's a placeholder for the ECS-based approach
    }
}

/// Entity spawner helper
pub mod spawn {
    use super::hecs::{Entity, World};
    
    /// Spawn an item entity
    pub fn spawn_item(
        world: &mut World,
        item_id: u32,
        count: u8,
        x: f32,
        y: f32,
        z: f32,
    ) -> Entity {
        world.spawn((
            super::player::Position::new(x, y, z),
            super::player::Velocity::default(),
            ItemComponent {
                item_id,
                count,
                pickup_delay: 0,
            },
        ))
    }
    
    /// Item entity component
    #[derive(Debug, Clone, Copy)]
    pub struct ItemComponent {
        pub item_id: u32,
        pub count: u8,
        pub pickup_delay: u16,
    }
}

/// Utility re-exports
pub use player::{spawn_player, NetworkState, PlayerInfo, Position, Rotation, Velocity};
pub use block::{BlockPosition, BlockType, place_block};
pub use spawn::{spawn_item, ItemComponent};

// Re-export Vec3 for convenience
#[derive(Debug, Clone, Copy)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    
    pub fn distance(&self, other: &Vec3) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

impl Default for Vec3 {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, z: 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hecs::World;
    
    #[test]
    fn test_spawn_player() {
        let mut world = World::new();
        let entity = spawn_player(
            &mut world,
            "TestPlayer".to_string(),
            1,
            1,
            0.0,
            70.0,
            0.0,
        );
        
        // Query for the player
        let mut query = world.query::<&PlayerInfo>();
        let players: Vec<_> = query.iter().collect();
        
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].1.username, "TestPlayer");
    }
    
    #[test]
    fn test_position_chunk_coords() {
        let pos = Position::new(20.0, 70.0, -5.0);
        let (cx, cz) = pos.chunk_coords();
        assert_eq!(cx, 1);
        assert_eq!(cz, -1);
    }
    
    #[test]
    fn test_vec3_distance() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(3.0, 4.0, 0.0);
        assert!((a.distance(&b) - 5.0).abs() < 0.001);
    }
}
