//! Per-player session state machine driving the Bedrock login handshake.
//!
//! After RakNet establishes a connection, batches of Bedrock packets flow
//! through this session. We step through the states required to drop the
//! client in-world (offline mode): RequestNetworkSettings → NetworkSettings
//! → Login → PlayStatus → ResourcePacks → StartGame → spawn packets.

use tokio::sync::mpsc;
use tracing::{debug, info, trace};

use rustmine_protocol::id;
use rustmine_protocol::login;
use rustmine_protocol::SUPPORTED_PROTOCOL_VERSION;

/// States a session moves through while logging in.
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

    /// Feed a decoded (unwrapped) game packet to the session state machine.
    pub fn on_packet(&mut self, pid: u8, _body: &[u8]) {
        trace!(pid, state = ?self.state, "session packet");
        match self.state {
            SessionState::WaitSettings if pid == id::REQUEST_NETWORK_SETTINGS => {
                // Send NetworkSettings and transition.
                self.send_packet(login::network_settings(0));
                self.state = SessionState::WaitLogin;
                debug!("Sent NetworkSettings, waiting for Login");
            }
            SessionState::WaitLogin if pid == id::LOGIN => {
                // Minimal parsing: we don't care about JWT in offline mode;
                // we trust any username. Parse protocol version + username
                // well enough to log and reject mismatched versions.
                let mut pos: usize = 0;
                // protocol version: u32 BE at start of body
                if _body.len() >= 4 {
                    let proto = u32::from_be_bytes([_body[0], _body[1], _body[2], _body[3]]);
                    if proto != SUPPORTED_PROTOCOL_VERSION {
                        info!(proto, "client protocol mismatch, disconnecting");
                        self.send_packet(login::disconnect(&format!(
                            "Unsupported protocol {proto}, expected {SUPPORTED_PROTOCOL_VERSION}"
                        )));
                        self.state = SessionState::Closed;
                        return;
                    }
                }
                // Skip chain data; we don't validate it in offline mode. For
                // logging we attempt to extract a username — best effort.
                self.username = extract_offline_username(_body).unwrap_or_else(|| "Steve".into());

                info!(user = self.username.as_str(), "Player login (offline mode)");

                // PlayStatus(LoginSuccess)
                self.send_packet(login::play_status(login::PLAY_STATUS_LOGIN_SUCCESS));
                // ResourcePacksInfo (none)
                self.send_packet(login::resource_packs_info());
                self.state = SessionState::WaitResourcePacks;
            }
            SessionState::WaitResourcePacks if pid == id::RESOURCE_PACK_CLIENT_RESPONSE => {
                self.send_packet(login::play_status(login::PLAY_STATUS_SPAWN));
                self.send_packet(login::resource_pack_stack());
                self.send_packet(login::biome_definition_list());

                let spawn = (0.0f32, 66.0f32, 0.0f32);
                // StartGame
                self.send_packet(login::start_game(
                    self.entity_id,
                    self.runtime_id,
                    1, // creative, keeps things simple for phase 2
                    spawn,
                    0,
                    "world",
                ));
                self.send_packet(login::set_spawn_position());
                self.send_packet(login::set_time(6000));
                self.send_packet(login::set_difficulty(1));
                self.send_packet(login::set_player_game_type(1));
                // Send a minimal empty chunk at (0, 0) so the client has terrain.
                self.send_packet(login::empty_level_chunk(0, 0, 1));
                self.send_packet(login::network_chunk_publisher_update(
                    (spawn.0 as i32, spawn.1 as i32, spawn.2 as i32),
                    self.view_distance * 16,
                ));
                self.send_packet(login::chunk_radius_updated(self.view_distance as i32));
                self.send_packet(login::play_status_player_spawn());
                self.state = SessionState::Spawned;
                info!(user = self.username.as_str(), "Client reached in-world state");
            }
            SessionState::Spawned => {
                // Phase 2: just log; movement/world sync handled in Phase 3.
                if pid == id::MOVE_PLAYER {
                    trace!("move player from {}", self.username);
                } else if pid == id::REQUEST_CHUNK_RADIUS {
                    self.send_packet(login::chunk_radius_updated(self.view_distance as i32));
                }
            }
            _ => {
                trace!(pid, state = ?self.state, "ignoring packet in state");
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
