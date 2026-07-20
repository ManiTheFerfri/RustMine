//! Game logic: physics, crafting, block behaviors, mob AI, redstone-equivalent.
//!
//! This crate houses the authoritative game simulation rules.
//! The server tick loop drives this logic on a fixed 20 TPS cadence.

#![allow(unused_variables)]

use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;

/// Game-related errors
#[derive(Error, Debug)]
pub enum GameError {
    #[error("entity not found: {0}")]
    EntityNotFound(i64),
    #[error("player not found: {0}")]
    PlayerNotFound(String),
    #[error("invalid position: {0}")]
    InvalidPosition(String),
    #[error("block interaction failed: {0}")]
    BlockInteraction(String),
}

/// Player entity data
#[derive(Debug, Clone)]
pub struct PlayerData {
    pub entity_id: i64,
    pub username: String,
    pub position: Vec3,
    pub velocity: Vec3,
    pub rotation: Rotation,
    pub gamemode: Gamemode,
    pub health: f32,
    pub hunger: u32,
    pub on_ground: bool,
    pub sneaking: bool,
    pub sprinting: bool,
    pub view_distance: u32,
}

impl PlayerData {
    pub fn new(entity_id: i64, username: String) -> Self {
        Self {
            entity_id,
            username,
            position: Vec3::new(0.0, 70.0, 0.0),
            velocity: Vec3::new(0.0, 0.0, 0.0),
            rotation: Rotation::default(),
            gamemode: Gamemode::Survival,
            health: 20.0,
            hunger: 20,
            on_ground: false,
            sneaking: false,
            sprinting: false,
            view_distance: 10,
        }
    }
}

/// 3D vector type
#[derive(Debug, Clone, Copy, Default)]
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

    pub fn to_block_pos(&self) -> (i32, i32, i32) {
        (self.x as i32, self.y as i32, self.z as i32)
    }

    pub fn from_coords(x: f64, y: f64, z: f64) -> Self {
        Self {
            x: x as f32,
            y: y as f32,
            z: z as f32,
        }
    }
}

/// Player rotation (yaw, pitch)
#[derive(Debug, Clone, Copy, Default)]
pub struct Rotation {
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for Rotation {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

/// Game modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gamemode {
    Survival = 0,
    Creative = 1,
    Adventure = 2,
    Spectator = 3,
}

impl Gamemode {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::Survival,
            1 => Self::Creative,
            2 => Self::Adventure,
            3 => Self::Spectator,
            _ => Self::Survival,
        }
    }

    pub fn as_u32(&self) -> u32 {
        *self as u32
    }
}

/// Time of day in Minecraft ticks (0-24000)
#[derive(Debug, Clone, Copy)]
pub struct TimeOfDay {
    ticks: u32,
}

impl TimeOfDay {
    pub fn new(ticks: u32) -> Self {
        Self { ticks: ticks % 24000 }
    }

    pub fn from_hours(hours: u32) -> Self {
        Self::new(hours * 1000)
    }

    pub fn ticks(&self) -> u32 {
        self.ticks
    }

    pub fn is_day(&self) -> bool {
        self.ticks >= 0 && self.ticks < 12000
    }

    pub fn is_night(&self) -> bool {
        !self.is_day()
    }
}

impl Default for TimeOfDay {
    fn default() -> Self {
        Self::new(6000) // Noon
    }
}

/// Difficulty level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    Peaceful = 0,
    Easy = 1,
    Normal = 2,
    Hard = 3,
}

impl Difficulty {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::Peaceful,
            1 => Self::Easy,
            2 => Self::Normal,
            3 => Self::Hard,
            _ => Self::Normal,
        }
    }

    pub fn as_u32(&self) -> u32 {
        *self as u32
    }
}

/// World time and weather state
#[derive(Debug, Clone)]
pub struct WorldState {
    pub time: TimeOfDay,
    pub difficulty: Difficulty,
    pub rain_level: f32,
    pub lightning_level: f32,
    pub spawn_protection: u32,
}

impl Default for WorldState {
    fn default() -> Self {
        Self {
            time: TimeOfDay::default(),
            difficulty: Difficulty::Normal,
            rain_level: 0.0,
            lightning_level: 0.0,
            spawn_protection: 16,
        }
    }
}

/// Game event types for tick processing
#[derive(Debug, Clone)]
pub enum GameEvent {
    PlayerJoin(i64),
    PlayerLeave(i64),
    PlayerMove(i64, Vec3, Rotation),
    PlayerInteract(i64, (i32, i32, i32), InteractAction),
    PlayerChat(i64, String),
    PlayerCommand(i64, String),
    BlockBreak(i64, (i32, i32, i32)),
    BlockPlace(i64, (i32, i32, i32), u32),
}

/// Player interaction types
#[derive(Debug, Clone, Copy)]
pub enum InteractAction {
    LeftClick = 0,
    RightClick = 1,
    Leave = 2,
}

/// Event sent by the game to network layer
#[derive(Debug, Clone)]
pub enum GameOutput {
    PlayerSpawned { entity_id: i64, username: String },
    PlayerDespawned { entity_id: i64 },
    ChunkData { chunk_x: i32, chunk_z: i32, data: Vec<u8> },
    BlockUpdate { x: i32, y: i32, z: i32, block_id: u32 },
    TimeUpdate { time: u32 },
    ChatMessage { sender: String, message: String },
}

/// The main game state
pub struct GameState {
    pub players: HashMap<i64, PlayerData>,
    pub world_state: WorldState,
    pub tick: u64,
    pub stopped: bool,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            world_state: WorldState::default(),
            tick: 0,
            stopped: false,
        }
    }

    /// Add a player to the game
    pub fn add_player(&mut self, entity_id: i64, username: String) {
        let player = PlayerData::new(entity_id, username);
        self.players.insert(entity_id, player);
    }

    /// Remove a player from the game
    pub fn remove_player(&mut self, entity_id: i64) -> Option<PlayerData> {
        self.players.remove(&entity_id)
    }

    /// Get a player by entity ID
    pub fn get_player(&self, entity_id: i64) -> Option<&PlayerData> {
        self.players.get(&entity_id)
    }

    /// Get mutable player
    pub fn get_player_mut(&mut self, entity_id: i64) -> Option<&mut PlayerData> {
        self.players.get_mut(&entity_id)
    }

    /// Process a single game tick
    pub fn tick(&mut self, event: Option<GameEvent>) -> Vec<GameOutput> {
        let mut outputs = Vec::new();
        self.tick += 1;

        // Process event
        if let Some(evt) = event {
            self.process_event(evt, &mut outputs);
        }

        // Time update every 20 ticks (once per second)
        if self.tick % 20 == 0 {
            self.world_state.time = TimeOfDay::new(self.world_state.time.ticks() + 1);
            outputs.push(GameOutput::TimeUpdate {
                time: self.world_state.time.ticks(),
            });
        }

        outputs
    }

    fn process_event(&mut self, event: GameEvent, outputs: &mut Vec<GameOutput>) {
        match event {
            GameEvent::PlayerJoin(entity_id) => {
                if let Some(player) = self.players.get(&entity_id) {
                    outputs.push(GameOutput::PlayerSpawned {
                        entity_id,
                        username: player.username.clone(),
                    });
                }
            }
            GameEvent::PlayerLeave(entity_id) => {
                if let Some(player) = self.remove_player(entity_id) {
                    outputs.push(GameOutput::PlayerDespawned { entity_id });
                }
            }
            GameEvent::PlayerMove(entity_id, pos, rot) => {
                if let Some(player) = self.get_player_mut(entity_id) {
                    player.position = pos;
                    player.rotation = rot;
                }
            }
            GameEvent::BlockBreak(_entity_id, (x, y, z)) => {
                // Block break logic would go here
                outputs.push(GameOutput::BlockUpdate {
                    x,
                    y,
                    z,
                    block_id: 0, // Air
                });
            }
            GameEvent::BlockPlace(_entity_id, (x, y, z), block_id) => {
                outputs.push(GameOutput::BlockUpdate {
                    x,
                    y,
                    z,
                    block_id,
                });
            }
            GameEvent::PlayerChat(entity_id, message) => {
                if let Some(player) = self.get_player(entity_id) {
                    outputs.push(GameOutput::ChatMessage {
                        sender: player.username.clone(),
                        message,
                    });
                }
            }
            GameEvent::PlayerCommand(_entity_id, _command) => {
                // Command processing handled elsewhere
            }
            GameEvent::PlayerInteract(_entity_id, _pos, _action) => {
                // Interaction logic
            }
        }
    }

    /// Get all players in a given chunk
    pub fn players_in_chunk(&self, chunk_x: i32, chunk_z: i32) -> Vec<&PlayerData> {
        self.players
            .values()
            .filter(|p| {
                let (cx, cz) = ((p.position.x as i32) >> 4, (p.position.z as i32) >> 4);
                cx == chunk_x && cz == chunk_z
            })
            .collect()
    }

    /// Stop the game
    pub fn stop(&mut self) {
        self.stopped = true;
    }
}

/// Game manager for coordinating game state across the server
pub struct GameManager {
    state: Arc<tokio::sync::Mutex<GameState>>,
}

impl GameManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(tokio::sync::Mutex::new(GameState::new())),
        }
    }

    /// Get a clone of the shared state
    pub fn state(&self) -> Arc<tokio::sync::Mutex<GameState>> {
        Arc::clone(&self.state)
    }

    /// Add a player
    pub async fn add_player(&self, entity_id: i64, username: String) {
        let mut state = self.state.lock().await;
        state.add_player(entity_id, username);
    }

    /// Remove a player
    pub async fn remove_player(&self, entity_id: i64) -> Option<PlayerData> {
        let mut state = self.state.lock().await;
        state.remove_player(entity_id)
    }

    /// Process a tick
    pub async fn tick(&self, event: Option<GameEvent>) -> Vec<GameOutput> {
        let mut state = self.state.lock().await;
        state.tick(event)
    }
}

impl Default for GameManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

/// Channel for game events
pub type EventSender = mpsc::UnboundedSender<GameEvent>;
pub type EventReceiver = mpsc::UnboundedReceiver<GameEvent>;

/// Create a game event channel
pub fn create_event_channel() -> (EventSender, EventReceiver) {
    mpsc::unbounded_channel()
}

/// Game tick configuration
pub const TICKS_PER_SECOND: u32 = 20;
pub const TICK_DURATION_MS: u64 = 1000 / TICKS_PER_SECOND as u64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec3_distance() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(3.0, 4.0, 0.0);
        assert!((a.distance(&b) - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_player_data() {
        let player = PlayerData::new(1, "TestPlayer".to_string());
        assert_eq!(player.entity_id, 1);
        assert_eq!(player.username, "TestPlayer");
        assert_eq!(player.gamemode, Gamemode::Survival);
    }

    #[test]
    fn test_gamemode() {
        assert_eq!(Gamemode::from_u32(0), Gamemode::Survival);
        assert_eq!(Gamemode::from_u32(1), Gamemode::Creative);
        assert_eq!(Gamemode::as_u32(&Gamemode::Adventure), 2);
    }

    #[test]
    fn test_time_of_day() {
        let time = TimeOfDay::new(6000);
        assert!(time.is_day());
        
        let night = TimeOfDay::new(14000);
        assert!(night.is_night());
    }

    #[test]
    fn test_game_state() {
        let mut state = GameState::new();
        state.add_player(1, "Steve".to_string());
        
        assert!(state.get_player(1).is_some());
        assert_eq!(state.players.len(), 1);
        
        let removed = state.remove_player(1);
        assert!(removed.is_some());
        assert!(state.get_player(1).is_none());
    }

    #[test]
    fn test_tick() {
        let mut state = GameState::new();
        assert_eq!(state.tick, 0);
        
        state.tick(None);
        assert_eq!(state.tick, 1);
        
        // Time should update after 20 ticks
        state.tick(None);
        state.tick(None);
        // ... (continuing would eventually trigger time update)
    }
}
