//! RustMine — A high-performance Minecraft Bedrock server written in Rust.
//!
//! Entrypoint for the server binary. Handles CLI parsing, configuration
//! loading, logging setup, starts the RakNet transport, and wires game-layer
//! sessions for Phase 2-3 world sync.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::Parser;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{info, warn, error};

mod config;
mod session;

use config::ServerConfig;
use rustmine_protocol::batch;
use rustmine_protocol::chunk::{self as chunk_module, SubChunk};
use rustmine_raknet::{RaknetServer, Reliability};
use rustmine_world::{World, WorldGenerator, FlatGenerator, NoiseGenerator, Chunk, BlockPos, BlockState};
use rustmine_game::{GameState, GameEvent, GameOutput, Vec3, Rotation, GameManager};
use rustmine_commands::{CommandManager, CommandSender};

/// Command-line arguments for the RustMine server.
#[derive(Parser, Debug)]
#[command(
    name = "rustmine",
    version,
    about = "A high-performance Minecraft Bedrock server"
)]
struct Cli {
    /// Path to the server configuration file (TOML).
    #[arg(short, long, default_value = "server.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let cfg = ServerConfig::load(&cli.config)
        .with_context(|| format!("Failed to load config from {}", cli.config.display()))?;

    let bind_addr: SocketAddr = format!("{}:{}", cfg.server.bind_address, cfg.server.port)
        .parse()
        .context("Invalid bind address")?;

    info!(
        name = cfg.server.name.as_str(),
        port = cfg.server.port,
        motd = cfg.server.motd.as_str(),
        view_distance = cfg.game.view_distance,
        "RustMine server starting"
    );

    let socket = UdpSocket::bind(bind_addr)
        .await
        .context("Failed to bind UDP socket")?;
    info!("UDP socket bound to {bind_addr}");

    run_server(socket, cfg).await;
    Ok(())
}

/// Shared server state accessible across tasks
struct ServerState {
    config: ServerConfig,
    sessions: RwLock<HashMap<SocketAddr, session::Session>>,
    world: Mutex<World>,
    game: Mutex<GameState>,
    commands: CommandManager,
    tick_count: u64,
    start_time: Instant,
}

impl ServerState {
    fn new(config: ServerConfig) -> Self {
        // Create world generator based on config
        let seed = config.game.seed;
        let generator: Arc<dyn WorldGenerator> = if config.game.flat_world {
            Arc::new(FlatGenerator::new(64, 62))
        } else {
            Arc::new(NoiseGenerator::new(seed, 64, 20, 1.0))
        };

        let world = World::new(
            config.game.world_name.clone(),
            seed,
            generator,
        );

        Self {
            config,
            sessions: RwLock::new(HashMap::new()),
            world: Mutex::new(world),
            game: Mutex::new(GameState::new()),
            commands: CommandManager::new(),
            tick_count: 0,
            start_time: Instant::now(),
        }
    }

    /// Add a new session for a player
    async fn add_session(&self, addr: SocketAddr, entity_id: i64) {
        let session = session::Session::new(entity_id);
        self.sessions.write().await.insert(addr, session);
    }

    /// Remove a session
    async fn remove_session(&self, addr: SocketAddr) {
        self.sessions.write().await.remove(&addr);
    }

    /// Get session by address
    async fn get_session(&self, addr: SocketAddr) -> Option<tokio::sync::RwLockReadGuard<'_, session::Session>> {
        Some(self.sessions.read().await.get(&addr)?.read().await)
    }

    /// Process game outputs (chunk data, block updates, etc.)
    async fn process_game_output(&self, output: GameOutput, send_handle: &mpsc::UnboundedSender<(SocketAddr, Vec<u8>, Reliability, u8)>) {
        match output {
            GameOutput::ChunkData { chunk_x, chunk_z, data } => {
                // Broadcast chunk to nearby players
                let sessions = self.sessions.read().await;
                for (_addr, session) in sessions.iter() {
                    let session_guard = session.read().await;
                    if session_guard.state == session::SessionState::Spawned {
                        let _ = send_handle.send((
                            _addr.clone(),
                            data.clone(),
                            Reliability::ReliableOrdered,
                            0,
                        ));
                    }
                }
            }
            GameOutput::BlockUpdate { x, y, z, block_id } => {
                // Send block update to all players
                info!("Block update at ({}, {}, {}): {}", x, y, z, block_id);
            }
            GameOutput::TimeUpdate { time } => {
                // Broadcast time update
                let sessions = self.sessions.read().await;
                for (addr, session) in sessions.iter() {
                    let session_guard = session.read().await;
                    if session_guard.state == session::SessionState::Spawned {
                        let packet = rustmine_protocol::login::set_time(time as i32);
                        let _ = send_handle.send((
                            addr.clone(),
                            packet,
                            Reliability::ReliableOrdered,
                            0,
                        ));
                    }
                }
            }
            GameOutput::ChatMessage { sender, message } => {
                info!("[{}] {}", sender, message);
            }
            GameOutput::PlayerSpawned { entity_id, username } => {
                info!("Player spawned: {} (entity {})", username, entity_id);
            }
            GameOutput::PlayerDespawned { entity_id } => {
                info!("Player despawned: entity {}", entity_id);
            }
        }
    }

    /// Generate and send chunks for a player based on their position
    async fn send_chunks_for_player(
        &self,
        addr: SocketAddr,
        player_x: f32,
        player_z: f32,
        view_distance: u32,
        send_handle: &mpsc::UnboundedSender<(SocketAddr, Vec<u8>, Reliability, u8)>,
    ) {
        let chunk_x = (player_x as i32) >> 4;
        let chunk_z = (player_z as i32) >> 4;
        let radius = view_distance as i32;

        let mut world = self.world.lock().await;

        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let cx = chunk_x + dx;
                let cz = chunk_z + dz;

                // Get or generate chunk
                let chunk = world.get_chunk(cx, cz);

                // Convert to subchunks for network encoding
                let subchunks: Vec<SubChunk> = chunk.subchunks.iter().map(|s| {
                    SubChunk {
                        palette: s.palette.clone(),
                        blocks: s.blocks.clone(),
                    }
                }).collect();

                // Encode chunk
                let packet = chunk_module::encode_chunk_column(cx, cz, &subchunks, false);

                let _ = send_handle.send((
                    addr,
                    packet,
                    Reliability::ReliableOrdered,
                    0,
                ));
            }
        }
    }
}

/// Main server loop
async fn run_server(socket: UdpSocket, cfg: ServerConfig) {
    let state = Arc::new(ServerState::new(cfg.clone()));

    // Channels between RakNet I/O and the game task.
    let (in_tx, in_rx) = mpsc::unbounded_channel::<(SocketAddr, Vec<u8>)>();
    let (out_tx, out_rx) = mpsc::unbounded_channel::<(SocketAddr, Vec<u8>, Reliability, u8)>();
    let (conn_tx, conn_rx) = mpsc::unbounded_channel::<SocketAddr>();

    let motd = cfg.server.motd.clone();
    let online = cfg.auth.online_mode;

    let server = RaknetServer::new_with_hooks(motd, online, in_tx, out_tx, conn_tx);
    let send_handle = server.outgoing_sender().expect("outgoing sender");

    let socket = Arc::new(socket);
    let raknet_task = {
        let socket = Arc::clone(&socket);
        tokio::spawn(async move { server.run_arc(socket).await })
    };

    // New connections handler
    let conn_state = Arc::clone(&state);
    tokio::spawn(async move {
        let mut rx = conn_rx;
        while let Some(addr) = rx.recv().await {
            let entity_id = {
                let mut sessions = conn_state.sessions.write().await;
                let id = sessions.len() as i64 + 1;
                sessions.entry(addr).or_insert_with(|| session::Session::new(id));
                id
            };
            
            // Add player to game state
            {
                let mut game = conn_state.game.lock().await;
                game.add_player(entity_id, format!("Player{}", entity_id));
                game.tick(Some(GameEvent::PlayerJoin(entity_id)));
            }
            
            info!("New connection from {addr} (entity {})", entity_id);
        }
    });

    // Game task: decode batches, dispatch to session, flush responses
    let game_state = Arc::clone(&state);
    let out_rx_handle = tokio::spawn(async move {
        while let Some(_) = out_rx.recv().await {
            // Outbound queue handler - responses are sent directly via send_handle
        }
    });

    // Network packet handler
    let packet_state = Arc::clone(&state);
    let packet_task = tokio::spawn(async move {
        let mut rx: mpsc::UnboundedReceiver<(SocketAddr, Vec<u8>)> = in_rx;
        
        while let Some((src, payload)) = rx.recv().await {
            let packets = match batch::decode_batch(&payload) {
                Ok(v) => v,
                Err(e) => {
                    warn!("bad batch from {src}: {e}");
                    continue;
                }
            };

            let responses: Vec<Vec<u8>>;
            let should_send_chunks;
            
            {
                let mut sessions = packet_state.sessions.write().await;
                let session = sessions.get_mut(&src);
                
                if let Some(session) = session {
                    let mut session_guard = session.write().await;
                    
                    for raw in &packets {
                        if raw.is_empty() {
                            continue;
                        }
                        should_send_chunks = session_guard.on_packet(raw[0], &raw[1..]);
                    }
                    
                    // Collect responses and interactions
                    let interactions = session_guard.take_interactions();
                    let entity_id = session_guard.entity_id;
                    let player_pos = session_guard.position; // capture before drop
                    responses = session_guard.collect_responses();
                    drop(session_guard); // Explicit drop for clarity
                    
                    // Apply interaction events to world and game state
                    for event in interactions {
                        match event {
                            session::BlockInteractionEvent::Break { x, y, z } => {
                                // Reach validation
                                if !rustmine_protocol::interaction::within_reach(
                                    player_pos.x, player_pos.y, player_pos.z,
                                    x, y, z, 5.0,
                                ) {
                                    info!(
                                        "Block break at ({}, {}, {}) rejected: out of reach (max 5.0)",
                                        x, y, z
                                    );
                                    continue;
                                }
                                let block_pos = BlockPos::new(x, y, z);
                                {
                                    let mut world = packet_state.world.lock().await;
                                    let current_block = world.get_block(block_pos);
                                    if current_block != BlockState::Air {
                                        let _ = world.set_block(block_pos, BlockState::Air);
                                        info!("Block broken at ({}, {}, {}): set to air", x, y, z);
                                        let update_packet = rustmine_protocol::interaction::encode_update_block(
                                            x, y, z, 0,
                                        );
                                        let sessions = packet_state.sessions.read().await;
                                        for (addr, session) in sessions.iter() {
                                            let guard = session.read().await;
                                            if guard.state == session::SessionState::Spawned {
                                                let _ = send_handle.send((
                                                    addr.clone(),
                                                    update_packet.clone(),
                                                    rustmine_raknet::Reliability::ReliableOrdered,
                                                    0,
                                                ));
                                            }
                                        }
                                    } else {
                                        info!("Block break at ({}, {}, {}): already air", x, y, z);
                                    }
                                }
                                {
                                    let mut game = packet_state.game.lock().await;
                                    game.tick(Some(GameEvent::BlockBreak(
                                        entity_id,
                                        (x, y, z),
                                    )));
                                }
                            }
                            session::BlockInteractionEvent::Place { x, y, z, runtime_id } => {
                                if !rustmine_protocol::interaction::within_reach(
                                    player_pos.x, player_pos.y, player_pos.z,
                                    x, y, z, 5.0,
                                ) {
                                    info!(
                                        "Block place at ({}, {}, {}) rejected: out of reach (max 5.0)",
                                        x, y, z
                                    );
                                    continue;
                                }
                                let block_pos = BlockPos::new(x, y, z);
                                if let Some(rid) = runtime_id {
                                    info!("Block place at ({}, {}, {}): runtime_id {}", x, y, z, rid);
                                    {
                                        let mut world = packet_state.world.lock().await;
                                        let state = rustmine_world::BlockState::from_runtime_id(rid);
                                        let _ = world.set_block(block_pos, state);
                                    }
                                    let update_packet = rustmine_protocol::interaction::encode_update_block(
                                        x, y, z, rid,
                                    );
                                    let sessions = packet_state.sessions.read().await;
                                    for (addr, session) in sessions.iter() {
                                        let guard = session.read().await;
                                        if guard.state == session::SessionState::Spawned {
                                            let _ = send_handle.send((
                                                addr.clone(),
                                                update_packet.clone(),
                                                rustmine_raknet::Reliability::ReliableOrdered,
                                                0,
                                            ));
                                        }
                                    }
                                    {
                                        let mut game = packet_state.game.lock().await;
                                        game.tick(Some(GameEvent::BlockPlace(
                                            entity_id,
                                            (x, y, z),
                                            rid,
                                        )));
                                    }
                                } else {
                                    info!("Block place at ({}, {}, {}): no runtime ID provided", x, y, z);
                                }
                            }
                        }
                    }
                } else {
                    continue;
                }
            }
            
            // Send responses
            if !responses.is_empty() {
                let batched = batch::encode_batch(&responses);
                if let Err(e) = send_handle.send((src.clone(), batched, Reliability::ReliableOrdered, 0)) {
                    warn!("failed to enqueue send to {src}: {e}");
                }
            }
            
            // Send chunks after login complete
            if should_send_chunks {
                let (x, z) = {
                    let sessions = packet_state.sessions.read().await;
                    if let Some(session) = sessions.get(&src) {
                        let guard = session.read().await;
                        (guard.position.x, guard.position.z, guard.view_distance)
                    } else {
                        (0.0, 0.0, 4)
                    }
                };
                
                packet_state.send_chunks_for_player(src, x, z, 2, &send_handle).await;
            }
        }
    });

    // 20 TPS Game loop
    let game_loop_state = Arc::clone(&state);
    let tick_handle = send_handle.clone();
    let game_loop = tokio::spawn(async move {
        const TICK_DURATION: Duration = Duration::from_millis(50); // 20 TPS
        
        loop {
            let tick_start = Instant::now();
            
            // Process game tick
            {
                let mut game = game_loop_state.game.lock().await;
                let outputs = game.tick(None);
                
                // Process game outputs
                for output in outputs {
                    game_loop_state.process_game_output(output, &tick_handle).await;
                }
            }
            
            game_loop_state.tick_count += 1;
            
            // Broadcast tick info every 100 ticks (5 seconds)
            if game_loop_state.tick_count % 100 == 0 {
                let uptime = game_loop_state.start_time.elapsed().as_secs();
                let players = game_loop_state.game.lock().await.players.len();
                info!(
                    ticks = game_loop_state.tick_count,
                    uptime_s = uptime,
                    players = players,
                    "Server tick"
                );
            }
            
            // Sleep to maintain 20 TPS
            let elapsed = tick_start.elapsed();
            if elapsed < TICK_DURATION {
                tokio::time::sleep(TICK_DURATION - elapsed).await;
            }
        }
    });

    // Command console task
    let console_state = Arc::clone(&state);
    tokio::spawn(async move {
        use tokio::io::{self, AsyncBufReadExt};
        
        let mut stdin = io::BufReader::new(io::stdin).lines();
        
        loop {
            if let Ok(Some(line)) = stdin.next_line().await {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                
                if line.eq_ignore_ascii_case("stop") || line.eq_ignore_ascii_case("exit") {
                    info!("Server shutdown requested");
                    break;
                }
                
                if line.eq_ignore_ascii_case("list") || line.eq_ignore_ascii_case("players") {
                    let players = console_state.game.lock().await.players.values()
                        .map(|p| p.username.clone())
                        .collect::<Vec<_>>();
                    
                    if players.is_empty() {
                        println!("No players online");
                    } else {
                        println!("{} player(s) online: {}", players.len(), players.join(", "));
                    }
                    continue;
                }
                
                if line.eq_ignore_ascii_case("tps") {
                    let uptime = console_state.start_time.elapsed().as_secs();
                    let ticks = console_state.tick_count;
                    let tps = if uptime > 0 { ticks as f64 / uptime as f64 } else { 0.0 };
                    println!("TPS: {:.2} ({} ticks, {}s uptime)", tps, ticks, uptime);
                    continue;
                }
                
                // Execute command
                match console_state.commands.execute_raw(line, CommandSender::Console).await {
                    Ok(output) => {
                        if !output.message.is_empty() {
                            println!("{}", output.message);
                        }
                        if output.message.contains("Stopping") {
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("Command error: {}", e);
                    }
                }
            }
        }
    });

    // Wait for tasks
    tokio::select! {
        _ = raknet_task => {
            warn!("RakNet task exited");
        }
        _ = packet_task => {
            warn!("Packet handler task exited");
        }
        _ = game_loop => {
            warn!("Game loop exited");
        }
        _ = out_rx_handle => {
            warn!("Output handler exited");
        }
    }
    
    info!("Server shutting down");
}
