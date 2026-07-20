//! Well-known Bedrock packet IDs (v1001).
//!
//! A tiny curated list needed for Phase 2 login + in-world. Extend as needed.

pub const LOGIN: u8 = 0x01;
pub const PLAY_STATUS: u8 = 0x02;
pub const SERVER_TO_CLIENT_HANDSHAKE: u8 = 0x03;
pub const CLIENT_TO_SERVER_HANDSHAKE: u8 = 0x04;
pub const DISCONNECT: u8 = 0x05;
pub const RESOURCE_PACKS_INFO: u8 = 0x06;
pub const RESOURCE_PACK_STACK: u8 = 0x07;
pub const RESOURCE_PACK_CLIENT_RESPONSE: u8 = 0x08;
pub const TEXT: u8 = 0x09;
pub const SET_TIME: u8 = 0x0a;
pub const START_GAME: u8 = 0x0b;
pub const ADD_PLAYER: u8 = 0x0d;
pub const MOVE_PLAYER: u8 = 0x13;
pub const UPDATE_BLOCKS: u8 = 0x15;
pub const SET_SPAWN_POSITION: u8 = 0x2b;
pub const RESPAWN_POSITION: u8 = 0x2d;
pub const CONTAINER_OPEN: u8 = 0x2e;
pub const LEVEL_CHUNK: u8 = 0x3a;
pub const SET_DIFFICULTY: u8 = 0x3c;
pub const SET_PLAYER_GAME_TYPE: u8 = 0x3e;
pub const PLAYER_LIST: u8 = 0x3f;
pub const REQUEST_CHUNK_RADIUS: u8 = 0x45;
pub const CHUNK_RADIUS_UPDATED: u8 = 0x46;
pub const NETWORK_CHUNK_PUBLISHER_UPDATE: u8 = 0x79;
pub const BIOME_DEFINITION_LIST: u8 = 0x7a;
pub const NETWORK_SETTINGS: u8 = 0x8f;
pub const REQUEST_NETWORK_SETTINGS: u8 = 0xc1;
pub const PACKET_VIOLATION_WARNING: u8 = 0x9c;

// Additional client-bound packets for gameplay
pub const INTERACT: u8 = 0x10;
pub const USE_ITEM: u8 = 0x11;
pub const PLAYER_INPUT: u8 = 0x12;
pub const BREAK_BLOCK: u8 = 0x14;
pub const PLACE_BLOCK: u8 = 0x16;
pub const RESPAWN: u8 = 0x30;
pub const CHAT_MESSAGE: u8 = 0x31;
pub const COMMAND_REQUEST: u8 = 0x4e;
pub const SET_LOCAL_PLAYER_AS_INITIALIZED: u8 = 0x5d;

// Inventory packets
pub const INVENTORY_TRANSACTION: u8 = 0x17;
pub const INVENTORY_SLOT: u8 = 0x19;
pub const INVENTERY_CONTENT: u8 = 0x1a;
pub const CONTAINER_SET_DATA: u8 = 0x22;
