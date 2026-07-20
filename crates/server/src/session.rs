//! Per-player session state machine driving the Bedrock login handshake and in-game state.
//!
//! After RakNet establishes a connection, batches of Bedrock packets flow
//! through this session. We step through the states required to drop the
//! client in-world (offline mode): RequestNetworkSettings → NetworkSettings
//! → Login → PlayStatus → ResourcePacks → StartGame → spawn packets.
//!
//! Phase 3 adds: chunk loading, movement sync, and block interactions.

use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, trace};

use rustmine_protocol::id;
use rustmine_protocol::login;
use rustmine_protocol::{write_packet, SUPPORTED_PROTOCOL_VERSION, decode_batch};
use rustmine_nbt::write_var_i32;

/// Player position and rotation
#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl Default for Position {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 70.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            on_ground: true,
        }
    }
}

/// States a session moves through while logging in and playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Waiting for the very first packet (RequestNetworkSettings).
    WaitSettings,
    /// After sending NetworkSettings; waiting for Login.
    WaitLogin,
    /// After login success + resource packs info; waiting for ResourcePackClientResponse.
    WaitResourcePacks,
    /// After sending StartGame — client is spawning; forward future packets to game loop.
    Spawned,
    /// Session closed.
    Closed,
}

/// Pending outbound datagram payloads (already wrapped in a RakNet frame body
/// — i.e. a 0xfe batch).
#[derive(Debug)]
pub struct Outbound {
    pub data: Vec<u8>,
    pub reliability: rustmine_raknet::Reliability,
    pub order_channel: u8,
}

/// Tracks a single connected player.
pub struct Session {
    pub username: String,
    pub entity_id: i64,
    pub runtime_id: u64,
    pub state: SessionState,
    pub view_distance: u32,
    pub position: Position,
    /// Outbound queue; the network task drains this and sends over RakNet.
    pub tx: mpsc::UnboundedSender<Outbound>,
    pub rx: mpsc::UnboundedReceiver<Outbound>,
}

impl Session {
    pub fn new(entity_id: i64) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            username: String::new(),
            entity_id,
            runtime_id: entity_id as u64,
            state: SessionState::WaitSettings,
            view_distance: 4,
            position: Position::default(),
            tx,
            rx,
        }
    }

    /// Queue a pre-encoded Bedrock packet for reliable ordered send. Packets
    /// are accumulated in the session's outbound queue and wrapped into a
    /// single 0xfe batch by the game task after each inbound burst.
    fn send_packet(&self, packet: Vec<u8>) {
        let _ = self.tx.send(Outbound {
            data: packet,
            reliability: rustmine_raknet::Reliability::ReliableOrdered,
            order_channel: 0,
        });
    }

    /// Convenience: queue many packets in one go.
    fn send_packets(&self, packets: impl IntoIterator<Item = Vec<u8>>) {
        for p in packets {
            self.send_packet(p);
        }
    }

    /// Collect all pending responses from the channel
    pub fn collect_responses(&mut self) -> Vec<Vec<u8>> {
        let mut responses = Vec::new();
        while let Ok(out) = self.rx.try_recv() {
            responses.push(out.data);
        }
        responses
    }

    /// Feed a decoded (unwrapped) game packet to the session state machine.
    /// Returns true if chunks should be sent after this packet.
    pub fn on_packet(&mut self, pid: u8, body: &[u8]) -> bool {
        trace!(pid, state = ?self.state, "session packet");
        match self.state {
            SessionState::WaitSettings if pid == id::REQUEST_NETWORK_SETTINGS => {
                // Send NetworkSettings and transition.
                self.send_packet(login::network_settings(0));
                self.state = SessionState::WaitLogin;
                debug!("Sent NetworkSettings, waiting for Login");
                false
            }
            SessionState::WaitLogin if pid == id::LOGIN => {
                // Minimal parsing: we don't care about JWT in offline mode;
                // we trust any username. Parse protocol version + username
                // well enough to log and reject mismatched versions.
                
                // protocol version: u32 BE at start of body
                if body.len() >= 4 {
                    let proto = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
                    if proto != SUPPORTED_PROTOCOL_VERSION {
                        info!(proto, "client protocol mismatch, disconnecting");
                        self.send_packet(login::disconnect(&format!(
                            "Unsupported protocol {proto}, expected {SUPPORTED_PROTOCOL_VERSION}"
                        )));
                        self.state = SessionState::Closed;
                        return false;
                    }
                }
                
                // Skip chain data; we don't validate it in offline mode. For
                // logging we attempt to extract a username — best effort.
                self.username = extract_offline_username(body).unwrap_or_else(|| format!("Player{}", self.entity_id));

                info!(user = self.username.as_str(), "Player login (offline mode)");

                // PlayStatus(LoginSuccess)
                self.send_packet(login::play_status(login::PLAY_STATUS_LOGIN_SUCCESS));
                // ResourcePacksInfo (none)
                self.send_packet(login::resource_packs_info());
                self.state = SessionState::WaitResourcePacks;
                false
            }
            SessionState::WaitResourcePacks if pid == id::RESOURCE_PACK_CLIENT_RESPONSE => {
                self.send_packet(login::play_status(login::PLAY_STATUS_SPAWN));
                self.send_packet(login::resource_pack_stack());
                self.send_packet(login::biome_definition_list());

                // Set spawn position (Y = 70 for flat world)
                let spawn = (0.0f32, 70.0f32, 0.0f32);
                self.position = Position {
                    x: spawn.0,
                    y: spawn.1,
                    z: spawn.2,
                    ..Default::default()
                };
                
                // StartGame
                self.send_packet(login::start_game(
                    self.entity_id,
                    self.runtime_id,
                    1, // creative for phase 2-3
                    spawn,
                    0,
                    "world",
                ));
                self.send_packet(login::set_spawn_position());
                self.send_packet(login::set_time(6000));
                self.send_packet(login::set_difficulty(1));
                self.send_packet(login::set_player_game_type(1));
                
                // Send initial chunks around spawn
                self.send_packet(login::network_chunk_publisher_update(
                    (spawn.0 as i32, spawn.1 as i32, spawn.2 as i32),
                    self.view_distance * 16,
                ));
                
                // Player list with local player
                self.send_packet(login::player_list_add("", &self.username, self.entity_id));
                
                self.send_packet(login::chunk_radius_updated(self.view_distance as i32));
                self.send_packet(login::play_status_player_spawn());
                self.state = SessionState::Spawned;
                info!(user = self.username.as_str(), "Client reached in-world state");
                true // Signal that chunks should be sent
            }
            SessionState::Spawned => {
                // Phase 3: handle movement, chunk requests, and interactions
                self.handle_spawned_packet(pid, body)
            }
            _ => {
                trace!(pid, state = ?self.state, "ignoring packet in state");
                false
            }
        }
    }

    /// Handle packets while in Spawned state
    fn handle_spawned_packet(&mut self, pid: u8, body: &[u8]) -> bool {
        match pid {
            id::MOVE_PLAYER => {
                // Parse MovePlayer packet to update position
                if let Some(pos) = parse_move_player(body) {
                    self.position = pos;
                    trace!(
                        x = pos.x,
                        y = pos.y,
                        z = pos.z,
                        yaw = pos.yaw,
                        pitch = pos.pitch,
                        "player moved"
                    );
                }
                false
            }
            id::REQUEST_CHUNK_RADIUS => {
                // Handle chunk radius request
                if let Ok(radius) = decode_varint(body) {
                    self.view_distance = radius.max(2).min(32) as u32;
                    trace!(radius = self.view_distance, "chunk radius request");
                }
                self.send_packet(login::chunk_radius_updated(self.view_distance as i32));
                false
            }
            id::PLAYER_INPUT => {
                // Player input (sneaking, sprinting, etc.)
                trace!("player input received");
                false
            }
            id::CHAT_MESSAGE => {
                // Handle chat message
                if let Some(msg) = extract_chat_message(body) {
                    info!(user = self.username.as_str(), message = %msg, "chat");
                }
                false
            }
            id::INTERACT => {
                // Player interaction
                trace!("player interaction");
                false
            }
            id::USE_ITEM => {
                // Use item on block
                trace!("use item");
                false
            }
            id::BREAK_BLOCK => {
                // Break block
                trace!("break block");
                false
            }
            id::PLACE_BLOCK => {
                // Place block
                trace!("place block");
                false
            }
            id::RESPAWN => {
                // Player respawn
                info!(user = self.username.as_str(), "player respawning");
                let spawn = (0.0f32, 70.0f32, 0.0f32);
                self.send_packet(login::start_game(
                    self.entity_id,
                    self.runtime_id,
                    1,
                    spawn,
                    0,
                    "world",
                ));
                self.send_packet(login::set_time(6000));
                self.position = Position::default();
                true
            }
            id::COMMAND_REQUEST => {
                // Handle command
                if let Some(cmd) = extract_command(body) {
                    info!(user = self.username.as_str(), command = %cmd, "command");
                    // TODO: Process command through command system
                }
                false
            }
            id::SET_LOCAL_PLAYER_AS_INITIALIZED => {
                // Player initialization complete
                trace!(user = self.username.as_str(), "player initialized");
                false
            }
            _ => {
                trace!(pid, "unhandled spawned packet");
                false
            }
        }
    }
}

/// Best-effort offline-mode username extractor from the Login packet body.
///
/// The body is:
///   [u32 BE protocol]
///   [u16 BE chain_jwt_length] + bytes + [u16 BE skin_jwt_length] + bytes
/// Inside the skin JWT there's a base64url JSON payload containing
/// "displayName". We just string-scan for "displayName" to avoid pulling in a
/// base64/JWT dependency in Phase 2.
fn extract_offline_username(body: &[u8]) -> Option<String> {
    let needle = b"displayName";
    let idx = find_subslice(body, needle)?;
    let rest = &body[idx + needle.len()..];
    // Expect `":"Steve"` or similar; find next quote, then take until the next quote.
    let start = rest.iter().position(|&b| b == b'"')?;
    let after = &rest[start + 1..];
    let end = after.iter().position(|&b| b == b'"')?;
    let name = &after[..end];
    if name.is_empty() {
        return None;
    }
    String::from_utf8(name.to_vec()).ok()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}

/// Parse MovePlayer packet body to extract position and rotation
fn parse_move_player(body: &[u8]) -> Option<Position> {
    if body.len() < 49 {
        return None;
    }
    
    let mut pos = 0usize;
    
    // runtime_entity_id: varint
    let _ = decode_varint_at(body, &mut pos)?;
    
    // position: 3 x lfloat (little-endian float)
    let x = read_lfloat(body, &mut pos)?;
    let y = read_lfloat(body, &mut pos)?;
    let z = read_lfloat(body, &mut pos)?;
    
    // pitch: lfloat
    let pitch = read_lfloat(body, &mut pos)?;
    
    // yaw: lfloat
    let yaw = read_lfloat(body, &mut pos)?;
    
    // Some versions have additional fields, but we stop here for basic parsing
    // head_yaw: lfloat
    // mode: u8
    // on_ground: bool
    // rid: i64 (sometimes)
    
    Some(Position {
        x,
        y,
        z,
        yaw,
        pitch,
        on_ground: false, // Default, would need more parsing
    })
}

/// Decode a varint from a slice at a position
fn decode_varint(body: &[u8]) -> Result<i32, ()> {
    decode_varint_at(body, &mut 0)
}

fn decode_varint_at(body: &[u8], pos: &mut usize) -> Result<i32, ()> {
    let mut result: i32 = 0;
    let mut shift: u32 = 0;
    
    loop {
        if *pos >= body.len() {
            return Err(());
        }
        if shift >= 35 {
            return Err(());
        }
        
        let b = body[*pos];
        *pos += 1;
        
        result |= ((b & 0x7f) as i32) << shift;
        
        if b & 0x80 == 0 {
            // Decode zigzag
            return Ok(((result as u32) >> 1) as i32 ^ -((result as u32) & 1) as i32);
        }
        
        shift += 7;
    }
}

/// Read a little-endian float at position
fn read_lfloat(body: &[u8], pos: &mut usize) -> Option<f32> {
    if *pos + 4 > body.len() {
        return None;
    }
    let bytes: [u8; 4] = body[*pos..*pos + 4].try_into().ok()?;
    *pos += 4;
    Some(f32::from_le_bytes(bytes))
}

/// Extract chat message from ChatMessage packet
fn extract_chat_message(body: &[u8]) -> Option<String> {
    if body.len() < 2 {
        return None;
    }
    
    let mut pos = 0usize;
    
    // message: string (varint length + bytes)
    let msg_len = decode_varint_at(body, &mut pos).ok()? as usize;
    if pos + msg_len > body.len() {
        return None;
    }
    
    String::from_utf8(body[pos..pos + msg_len].to_vec()).ok()
}

/// Extract command from CommandRequest packet
fn extract_command(body: &[u8]) -> Option<String> {
    if body.len() < 2 {
        return None;
    }
    
    let mut pos = 0usize;
    
    // command: string
    let cmd_len = decode_varint_at(body, &mut pos).ok()? as usize;
    if pos + cmd_len > body.len() {
        return None;
    }
    
    String::from_utf8(body[pos..pos + cmd_len].to_vec()).ok()
}
