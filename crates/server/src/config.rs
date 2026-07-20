//! Server configuration types.
//!
//! Reads `server.toml` (or a user-specified path) to configure the server.
//! Mirrors `server.properties` from vanilla Bedrock with RustMine-specific additions.

#![allow(dead_code)] // fields used by future phases

use std::path::Path;

use serde::Deserialize;

/// Top-level server configuration.
#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    /// Server identity settings.
    pub server: ServerSection,
    /// Game world settings.
    pub game: GameSection,
    /// Authentication settings.
    pub auth: AuthSection,
    /// Logging settings.
    #[serde(default)]
    pub logging: LoggingSection,
}

/// Server identity and network settings.
#[derive(Debug, Deserialize)]
pub struct ServerSection {
    /// Display name of the server.
    #[serde(default = "default_name")]
    pub name: String,
    /// Message of the day shown in the server list.
    #[serde(default = "default_motd")]
    pub motd: String,
    /// UDP port to listen on.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Maximum concurrent players.
    #[serde(default = "default_max_players")]
    pub max_players: u32,
    /// IP address to bind to (0.0.0.0 for all interfaces).
    #[serde(default = "default_bind_address")]
    pub bind_address: String,
}

/// Game world settings.
#[derive(Debug, Deserialize)]
pub struct GameSection {
    /// View distance in chunks (radius from player).
    #[serde(default = "default_view_distance")]
    pub view_distance: u32,
    /// Default game mode for new players (survival, creative, adventure).
    #[serde(default = "default_gamemode")]
    pub gamemode: String,
    /// Default difficulty (peaceful, easy, normal, hard).
    #[serde(default = "default_difficulty")]
    pub difficulty: String,
    /// World name used for save folder.
    #[serde(default = "default_world_name")]
    pub world_name: String,
    /// World seed for terrain generation.
    #[serde(default)]
    pub seed: u64,
    /// Generate a flat world instead of terrain.
    #[serde(default)]
    pub flat_world: bool,
}

/// Authentication settings.
#[derive(Debug, Deserialize)]
pub struct AuthSection {
    /// Whether Xbox Live authentication is required.
    /// When false, clients can join without authentication (offline mode).
    /// Online mode requires implementing JWT chain validation + ECDH key exchange.
    #[serde(default = "default_online_mode")]
    pub online_mode: bool,
}

/// Logging settings.
#[derive(Debug, Default, Deserialize)]
pub struct LoggingSection {
    /// Tracing filter directive (e.g. "info", "debug", "rustmine=debug").
    #[serde(default = "default_log_level")]
    pub level: String,
}

impl ServerConfig {
    /// Load a `ServerConfig` from a TOML file path.
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let content = std::fs::read_to_string(path).map_err(LoadError::Io)?;
        toml::from_str(&content).map_err(LoadError::Parse)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("I/O error reading config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),
}

// ── defaults ───────────────────────────────────────────────────────────

fn default_name() -> String {
    "RustMine Server".into()
}
fn default_motd() -> String {
    "A RustMine Bedrock Server".into()
}
const fn default_port() -> u16 {
    19132
}
const fn default_max_players() -> u32 {
    20
}
fn default_bind_address() -> String {
    "0.0.0.0".into()
}
const fn default_view_distance() -> u32 {
    10
}
fn default_gamemode() -> String {
    "survival".into()
}
fn default_difficulty() -> String {
    "normal".into()
}
fn default_world_name() -> String {
    "world".into()
}
const fn default_online_mode() -> bool {
    false
}
fn default_log_level() -> String {
    "info".into()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            server: ServerSection {
                name: default_name(),
                motd: default_motd(),
                port: default_port(),
                max_players: default_max_players(),
                bind_address: default_bind_address(),
            },
            game: GameSection {
                view_distance: default_view_distance(),
                gamemode: default_gamemode(),
                difficulty: default_difficulty(),
                world_name: default_world_name(),
                seed: 0,
                flat_world: false,
            },
            auth: AuthSection {
                online_mode: default_online_mode(),
            },
            logging: LoggingSection {
                level: default_log_level(),
            },
        }
    }
}
