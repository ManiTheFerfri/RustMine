//! Phase 2 login handshake packet encoders and minimal decoders.
//!
//! Covers just enough of the session-initiation packets to move a client
//! past the "Loading resources..." screen into the spawned world, in
//! offline mode:
//!
//! 1. Client → Server: RequestNetworkSettings (0xc1)
//! 2. Server → Client: NetworkSettings (0x8f)
//! 3. Client → Server: Login (0x01)
//! 4. Server → Client: PlayStatus(LoginSuccess) (0x02)
//! 5. Server → Client: ResourcePacksInfo (0x06)
//! 6. Client → Server: ResourcePackClientResponse (0x08)
//! 7. Server → Client: PlayStatus(InitialSpawn) + StartGame (0x0b) + ...
//!
//! Online-mode (JWT-chain validation, ECDH) is stubbed out: we advertise
//! offline mode via NetworkSettings.compression_threshold=0 / NetworkSettings
//! flags and accept any username. The goal of Phase 2 is reaching in-game.

use crate::id;
use crate::{write_packet, SUPPORTED_GAME_VERSION, SUPPORTED_PROTOCOL_VERSION};

/// Encode a NetworkSettings (0x8f) packet that disables compression and
/// tells the client we are doing offline auth.
pub fn network_settings(compression_threshold: u16) -> Vec<u8> {
    let mut payload = Vec::new();
    // compression_threshold: u16 LE. 0 = no compression.
    payload.extend_from_slice(&compression_threshold.to_le_bytes());
    // compression_algorithm: u16 LE (0 = zlib, 1 = snappy). We don't compress,
    // but we still need to write a valid value.
    payload.extend_from_slice(&0u16.to_le_bytes());
    // enable_client_throttling: bool
    payload.push(0);
    // throttle_threshold: u8
    payload.push(0);
    // throttle_scalar: f32 LE
    payload.extend_from_slice(&0.0f32.to_le_bytes());
    write_packet(id::NETWORK_SETTINGS, &payload)
}

/// Encode a PlayStatus (0x02) packet with the given status code.
pub fn play_status(status: i32) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&status.to_be_bytes()); // status is big-endian i32
    write_packet(id::PLAY_STATUS, &payload)
}

// Play status codes used in Phase 2.
pub const PLAY_STATUS_LOGIN_SUCCESS: i32 = 0;
pub const PLAY_STATUS_SPAWN: i32 = 3;
pub const PLAY_STATUS_PLAYER_SPAWN: i32 = 4;

/// Encode a Disconnect (0x05) packet with a message.
pub fn disconnect(message: &str) -> Vec<u8> {
    let mut payload = Vec::new();
    // hide_disconnect_screen: bool
    payload.push(0);
    // message: string
    let msg_bytes = message.as_bytes();
    // VarInt length-prefixed string.
    rustmine_nbt::write_var_u32(&mut payload, msg_bytes.len() as u32);
    payload.extend_from_slice(msg_bytes);
    write_packet(id::DISCONNECT, &payload)
}

/// Encode a ResourcePacksInfo (0x06) packet with no resource/behavior packs.
/// That's all we need for offline mode.
pub fn resource_packs_info() -> Vec<u8> {
    let mut payload = Vec::new();
    // must_accept: bool
    payload.push(0);
    // has_addons: bool
    payload.push(0);
    // has_scripts: bool
    payload.push(0);
    // force_server_packs: bool (pre 1.19.30 pad; some versions)
    payload.push(0);
    // behaviour pack count: u16 LE unsigned short (LE) — actually it's unsigned
    // short LE = 0
    payload.extend_from_slice(&0u16.to_le_bytes());
    // resource pack count: unsigned short LE
    payload.extend_from_slice(&0u16.to_le_bytes());
    // Experimental: Game version string present? In pre-1.20.30 this was not
    // here; newer builds expect no more fields here.
    write_packet(id::RESOURCE_PACKS_INFO, &payload)
}

/// Encode a ResourcePackStack (0x07) with no packs.
pub fn resource_pack_stack() -> Vec<u8> {
    let mut payload = Vec::new();
    // must_accept: bool
    payload.push(0);
    // behavior pack count: unsigned varint
    rustmine_nbt::write_var_u32(&mut payload, 0);
    // resource pack count: unsigned varint
    rustmine_nbt::write_var_u32(&mut payload, 0);
    // game_version: string
    let mut ver = Vec::new();
    rustmine_nbt::write_var_u32(&mut ver, SUPPORTED_GAME_VERSION.len() as u32);
    ver.extend_from_slice(SUPPORTED_GAME_VERSION.as_bytes());
    payload.extend_from_slice(&ver);
    // experiments count (varint) + experiments[] + were_experiments_previously_toggled: bool
    rustmine_nbt::write_var_u32(&mut payload, 0);
    payload.push(0);
    // base_game_version string (post 1.16.100)
    payload.extend_from_slice(&ver);
    write_packet(id::RESOURCE_PACK_STACK, &payload)
}

/// Encode a StartGame (0x0b) packet sufficient to drop the client into the
/// world. This is intentionally minimal: many fields are zeroed out but we
/// write the correct field count/sizes to keep the decoder on-rail.
#[allow(clippy::too_many_arguments)]
pub fn start_game(
    entity_id: i64,
    runtime_id: u64,
    player_gamemode: u32,
    spawn: (f32, f32, f32),
    seed: i64,
    world_name: &str,
) -> Vec<u8> {
    let mut p = Vec::new();
    // entity_id: varlong
    rustmine_nbt::write_var_i64(&mut p, entity_id);
    // runtime_entity_id: varulong (write as varu64 of signed zigzag-decoded, same bit pattern)
    rustmine_nbt::write_var_u64(&mut p, runtime_id);
    // player_gamemode: varint
    rustmine_nbt::write_var_i32(&mut p, player_gamemode as i32);
    // position: 3 x lfloat (vec3)
    p.extend_from_slice(&spawn.0.to_le_bytes());
    p.extend_from_slice(&spawn.1.to_le_bytes());
    p.extend_from_slice(&spawn.2.to_le_bytes());
    // yaw, pitch: 2 x lfloat
    p.extend_from_slice(&0.0f32.to_le_bytes());
    p.extend_from_slice(&0.0f32.to_le_bytes());

    // --- world settings (beginning of "settings" blob) ---
    // seed (unused in vanilla; but sent as varlong + int)
    rustmine_nbt::write_var_i64(&mut p, seed);
    rustmine_nbt::write_var_i32(&mut p, seed as i32);
    // spawn settings:
    // bieme_type: u8 (0 = DEFAULT)
    p.push(0);
    // custom_biome_name: string
    write_str(&mut p, "");
    // dimension: varint
    rustmine_nbt::write_var_i32(&mut p, 0);
    // generator: varint (1 = flat)
    rustmine_nbt::write_var_i32(&mut p, 1);
    // world_gamemode: varint
    rustmine_nbt::write_var_i32(&mut p, player_gamemode as i32);
    // difficulty: varint
    rustmine_nbt::write_var_i32(&mut p, 1);
    // spawn_block_position (BlockCoordinates) = 3xvarint
    rustmine_nbt::write_var_i32(&mut p, 0);
    rustmine_nbt::write_var_i32(&mut p, 64);
    rustmine_nbt::write_var_i32(&mut p, 0);
    // achievements_disabled: bool
    p.push(1);
    // editor_world_type: varint
    rustmine_nbt::write_var_i32(&mut p, 0);
    // created_in_editor: bool
    p.push(0);
    // exported_from_editor: bool
    p.push(0);
    // day_cycle_stop_time: varint
    rustmine_nbt::write_var_i32(&mut p, -1);
    // edu_offer: varint
    rustmine_nbt::write_var_i32(&mut p, 0);
    // edu_features_enabled: bool
    p.push(0);
    // edu_product_uuid: string
    write_str(&mut p, "");
    // rain_level: lfloat
    p.extend_from_slice(&0.0f32.to_le_bytes());
    // lightning_level: lfloat
    p.extend_from_slice(&0.0f32.to_le_bytes());
    // confirmed_platform_locked_content: bool
    p.push(0);
    // multiplayer_game: bool
    p.push(1);
    // broadcast_to_lan: bool
    p.push(1);
    // xbl_broadcast_mode: varint
    rustmine_nbt::write_var_i32(&mut p, 0);
    // platform_broadcast_mode: varint
    rustmine_nbt::write_var_i32(&mut p, 0);
    // commands_enabled: bool
    p.push(1);
    // texture_pack_required: bool
    p.push(0);
    // gamerules: array(u32 varint length) — empty
    rustmine_nbt::write_var_u32(&mut p, 0);
    // experiments: experiments object — 0 count + bool
    rustmine_nbt::write_var_u32(&mut p, 0);
    p.push(0);
    // bonus_chest_enabled: bool
    p.push(0);
    // starting_map_enabled: bool
    p.push(0);
    // permission_level: varint (1 = member)
    rustmine_nbt::write_var_i32(&mut p, 1);
    // server_chunk_tick_range: i32 LE
    p.extend_from_slice(&4i32.to_le_bytes());
    // locked_behavior_packs: bool
    p.push(0);
    // locked_resource_packs: bool
    p.push(0);
    // from_locked_world_template: bool
    p.push(0);
    // msa_gamertags_only: bool
    p.push(0);
    // from_world_template: bool
    p.push(0);
    // world_template_id_locked: bool
    p.push(0);
    // only_spawn_v1_villagers: bool
    p.push(0);
    // persona_disabled: bool
    p.push(0);
    // custom_skins_disabled: bool
    p.push(0);
    // emote_chat_muted: bool
    p.push(0);
    // game_version: string
    write_str(&mut p, SUPPORTED_GAME_VERSION);
    // limited_world_width/i32 LE + limited_world_length/i32 LE + limited_world_result (bool)
    p.extend_from_slice(&0i32.to_le_bytes());
    p.extend_from_slice(&0i32.to_le_bytes());
    p.push(0);
    // new_nether: bool
    p.push(1);
    // edu_shared_uri: string
    write_str(&mut p, "");
    // force_experimental: bool
    p.push(0);
    // chat_restriction_level: u8
    p.push(0);
    // disabling_player_interactions: bool
    p.push(0);
    // server_id: string
    write_str(&mut p, "");
    // world_id: string
    write_str(&mut p, "");
    // scenario_id: string
    write_str(&mut p, "");

    // --- end world settings ---
    // level_id: string
    write_str(&mut p, world_name);
    // world_name: string
    write_str(&mut p, world_name);
    // premium_world_template_id: string
    write_str(&mut p, "00000000-0000-0000-0000-000000000000");
    // is_trial: bool
    p.push(0);
    // player_movement_settings: movement_type (varint) + various (0)
    rustmine_nbt::write_var_i32(&mut p, 0); // server authoritative movement v1
    // rewind_history_size: i32 LE
    p.extend_from_slice(&40i32.to_le_bytes());
    // server_authoritative_block_breaking: bool
    p.push(0);
    // current_tick: i64 LE
    p.extend_from_slice(&0i64.to_le_bytes());
    // enchantment_seed: varint
    rustmine_nbt::write_var_i32(&mut p, 0);
    // block_properties: NBT list (empty): single 0x00 (End tag) is standard for empty list
    p.push(0x00);
    // item_table: NBT list (empty)
    p.push(0x00);
    // multiplayer_correlation_id: string
    write_str(&mut p, "00000000-0000-0000-0000-000000000000");
    // server_authoritative_inventory: bool
    p.push(1);
    // engine: u8 (0 = Pocket)
    p.push(0);
    // block_palette_checksum: u32 LE (0 = no check)
    p.extend_from_slice(&0u32.to_le_bytes());
    // world_template_id: UUID (16 bytes of zero)
    p.extend_from_slice(&[0u8; 16]);
    // client_side_generation: bool
    p.push(0);
    // block_network_ids_are_hashes: bool
    p.push(0);
    // network_permission_level: varint
    rustmine_nbt::write_var_i32(&mut p, 0);
    // restricted_spawn_radius: varint (0 = no restriction)
    rustmine_nbt::write_var_i32(&mut p, 0);

    write_packet(id::START_GAME, &p)
}

/// Encode a minimal SetSpawnPosition (0x2b).
pub fn set_spawn_position() -> Vec<u8> {
    let mut p = Vec::new();
    // spawn_type: varint (0 = player_spawn, 2 = world_spawn)
    rustmine_nbt::write_var_i32(&mut p, 2);
    // block coordinates: varint x, y, z
    rustmine_nbt::write_var_i32(&mut p, 0);
    rustmine_nbt::write_var_i32(&mut p, 64);
    rustmine_nbt::write_var_i32(&mut p, 0);
    // dimension: varint
    rustmine_nbt::write_var_i32(&mut p, 0);
    // spawn_forced (bool) + spawn angle (lfloat)
    p.push(0);
    p.extend_from_slice(&0.0f32.to_le_bytes());
    write_packet(id::SET_SPAWN_POSITION, &p)
}

/// Encode a SetTime (0x0a) packet (time = 0, daytime).
pub fn set_time(time: i32) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&time.to_be_bytes());
    write_packet(id::SET_TIME, &p)
}

/// Encode SetDifficulty (0x3c).
pub fn set_difficulty(difficulty: u32) -> Vec<u8> {
    let mut p = Vec::new();
    rustmine_nbt::write_var_u32(&mut p, difficulty);
    write_packet(id::SET_DIFFICULTY, &p)
}

/// Encode SetPlayerGameType (0x3e).
pub fn set_player_game_type(gamemode: u32) -> Vec<u8> {
    let mut p = Vec::new();
    rustmine_nbt::write_var_i32(&mut p, gamemode as i32);
    write_packet(id::SET_PLAYER_GAME_TYPE, &p)
}

/// Encode an empty ChunkRadiusUpdated (0x46) replying with view distance.
pub fn chunk_radius_updated(radius: i32) -> Vec<u8> {
    let mut p = Vec::new();
    rustmine_nbt::write_var_i32(&mut p, radius);
    write_packet(id::CHUNK_RADIUS_UPDATED, &p)
}

/// Encode a PlayStatus(PlayerSpawn) packet which lets the client spawn.
pub fn play_status_player_spawn() -> Vec<u8> {
    play_status(PLAY_STATUS_PLAYER_SPAWN)
}

/// Encode BiomeDefinitionList (0x7a) as a single NBT End tag — enough to
/// satisfy the client in minimal-join mode.
pub fn biome_definition_list() -> Vec<u8> {
    write_packet(id::BIOME_DEFINITION_LIST, &[0x00])
}

/// Encode NetworkChunkPublisherUpdate (0x79) with a radius.
pub fn network_chunk_publisher_update(coords: (i32, i32, i32), radius: u32) -> Vec<u8> {
    let mut p = Vec::new();
    rustmine_nbt::write_var_i32(&mut p, coords.0);
    rustmine_nbt::write_var_i32(&mut p, coords.1);
    rustmine_nbt::write_var_i32(&mut p, coords.2);
    p.extend_from_slice(&radius.to_le_bytes());
    // saved_chunks: u32 LE (0)
    p.extend_from_slice(&0u32.to_le_bytes());
    write_packet(id::NETWORK_CHUNK_PUBLISHER_UPDATE, &p)
}

/// Encode the LevelChunk (0x3a) packet for a single empty (air) 16×16 chunk.
/// The client requires at least one chunk around spawn before it shows the
/// player. This writes a minimally-valid subchunk with height=0.
pub fn empty_level_chunk(chunk_x: i32, chunk_z: i32, subchunk_count: u8) -> Vec<u8> {
    let mut p = Vec::new();
    rustmine_nbt::write_var_i32(&mut p, chunk_x);
    rustmine_nbt::write_var_i32(&mut p, chunk_z);
    // sub_chunk_count: u32 LE
    p.extend_from_slice(&(subchunk_count as u32).to_le_bytes());
    // cache_enabled: bool
    p.push(0);

    // Build a minimal chunk payload. We send one "empty" subchunk followed by
    // zeroed border/biome data. This is far from a correct chunk but will
    // reliably let the client load the world without crashing.
    let mut chunk_data = Vec::new();
    // subchunk version: 8 (post-1.2.13). Storing all-air palettes uses version 8.
    chunk_data.push(8);
    // num_storages = 1 (lowest bit set), 0x1 (single storage, runtime palette)
    chunk_data.push(0x01);
    // network_persistent: 0
    // Y-index for this subchunk: 0 (byte)
    chunk_data.push(0);
    // Palette: single-entry (runtime id 0 = air). Blocks are 4096 entries but
    // since version 8 uses word-persistent we send a "0" palette entry and
    // word array filled with 0.
    // Block bit-width for indices: 1 (minimum for palette size 2)
    chunk_data.push(1);
    // Word count for 4096 entries at 1 bit each = ceil(4096/32) = 128
    for _ in 0..128u32 {
        chunk_data.extend_from_slice(&0u32.to_le_bytes());
    }
    // Palette entries (varint count, then varint runtime ids)
    rustmine_nbt::write_var_u32(&mut chunk_data, 1);
    rustmine_nbt::write_var_u32(&mut chunk_data, 0); // air

    // Append payload: length as u32 LE, then chunk_data
    p.extend_from_slice(&(chunk_data.len() as u32).to_le_bytes());
    p.extend_from_slice(&chunk_data);

    write_packet(id::LEVEL_CHUNK, &p)
}

/// Encode an empty PlayerList (0x3f) add/remove. The client needs at least
/// the local player in the list before the spawn screen shows correctly.
pub fn player_list_add(xuid: &str, username: &str, entity_id: i64) -> Vec<u8> {
    let mut p = Vec::new();
    // type: 0 = add, 1 = remove
    p.push(0);
    // count: varint
    rustmine_nbt::write_var_u32(&mut p, 1);
    // entry: uuid (16 zero bytes for offline)
    p.extend_from_slice(&[0u8; 16]);
    // entity_id: varlong
    rustmine_nbt::write_var_i64(&mut p, entity_id);
    // username: string
    write_str(&mut p, username);
    // xuid: string
    write_str(&mut p, xuid);
    // platform_chat_id: string
    write_str(&mut p, "");
    // device_os: i32 LE
    p.extend_from_slice(&0i32.to_le_bytes());
    // skin data (we send empty to get past the join — a bare minimum skin).
    // skin_id: string, skin_resource_patch: string, etc.  An empty skin ID
    // and a 0-byte skin image is the smallest that parses.
    write_str(&mut p, "");
    write_str(&mut p, ""); // playfab_id
    write_str(&mut p, ""); // skin_id
    write_str(&mut p, ""); // play_fab_id (duped for protocol quirks)
    write_str(&mut p, ""); // skin_resource_patch
    // skin_image width/height/data (0 bytes)
    write_str(&mut p, ""); // skin_id alt
    // skin_data width/height (u32 LE x 2) + data length varuint of 0
    p.extend_from_slice(&0u32.to_le_bytes());
    p.extend_from_slice(&0u32.to_le_bytes());
    rustmine_nbt::write_var_u32(&mut p, 0);
    // animations count: varint (0)
    rustmine_nbt::write_var_u32(&mut p, 0);
    // cape_image width/height + data length 0
    p.extend_from_slice(&0u32.to_le_bytes());
    p.extend_from_slice(&0u32.to_le_bytes());
    rustmine_nbt::write_var_u32(&mut p, 0);
    // premium skin / persona / cape_on_classic / primary_user: 4 bools
    p.extend_from_slice(&[0, 0, 0, 1]);
    // cape_id: string
    write_str(&mut p, "");
    // skin colour (string) + arm size (string)
    write_str(&mut p, "");
    write_str(&mut p, "wide");
    // persona skin toggle pieces count: varint 0
    rustmine_nbt::write_var_u32(&mut p, 0);
    // tint colors count: varint 0
    rustmine_nbt::write_var_u32(&mut p, 0);
    // verification/flags for skin (u32 LE 0)
    p.extend_from_slice(&0u32.to_le_bytes());
    write_packet(id::PLAYER_LIST, &p)
}

fn write_str(buf: &mut Vec<u8>, s: &str) {
    rustmine_nbt::write_var_u32(buf, s.len() as u32);
    buf.extend_from_slice(s.as_bytes());
}

pub const PROTOCOL_VERSION_NETWORK: u32 = SUPPORTED_PROTOCOL_VERSION;
