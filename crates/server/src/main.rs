//! RustMine — A high-performance Minecraft Bedrock server written in Rust.
//!
//! Entrypoint for the server binary. Handles CLI parsing, configuration
//! loading, logging setup, starts the RakNet transport, and wires game-layer
//! sessions for Phase 2 offline-mode login.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

mod config;
mod session;

use config::ServerConfig;
use rustmine_protocol::batch;
use rustmine_raknet::{RaknetServer, Reliability};

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
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
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

struct GameState {
    cfg: ServerConfig,
    sessions: HashMap<SocketAddr, session::Session>,
    next_entity_id: i64,
}

impl GameState {
    fn new(cfg: ServerConfig) -> Self {
        Self {
            cfg,
            sessions: HashMap::new(),
            next_entity_id: 1,
        }
    }
    fn session_for(&mut self, addr: SocketAddr) -> &mut session::Session {
        let id = self.next_entity_id;
        self.next_entity_id += 1;
        self.sessions.entry(addr).or_insert_with(|| session::Session::new(id))
    }
}

async fn run_server(socket: UdpSocket, cfg: ServerConfig) {
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

    let state = Arc::new(Mutex::new(GameState::new(cfg)));

    // New connections: create a Session.
    let conn_state = Arc::clone(&state);
    tokio::spawn(async move {
        let mut rx = conn_rx;
        while let Some(addr) = rx.recv().await {
            let mut g = conn_state.lock().await;
            g.session_for(addr);
            info!("New connection from {addr}");
        }
    });

    // Game task: decode batches, dispatch to session, flush responses.
    let game_state = Arc::clone(&state);
    tokio::spawn(async move {
        let mut rx: mpsc::UnboundedReceiver<(SocketAddr, Vec<u8>)> = in_rx;
        let mut out_rx_ch: mpsc::UnboundedReceiver<(SocketAddr, Vec<u8>, Reliability, u8)> = out_rx;
        // Drive outgoing queue (we own the rx end) — forward to nowhere since
        // we send directly via send_handle below.
        tokio::spawn(async move { while let Some(_) = out_rx_ch.recv().await {} });
        while let Some((src, payload)) = rx.recv().await {
            let packets = match batch::decode_batch(&payload) {
                Ok(v) => v,
                Err(e) => {
                    warn!("bad batch from {src}: {e}");
                    continue;
                }
            };
            let mut responses: Vec<Vec<u8>> = Vec::new();
            {
                let mut g = game_state.lock().await;
                let session = g.session_for(src);
                for raw in packets {
                    if raw.is_empty() {
                        continue;
                    }
                    session.on_packet(raw[0], &raw[1..]);
                }
                while let Ok(out) = session.rx.try_recv() {
                    responses.push(out.data);
                }
            }
            if !responses.is_empty() {
                let batched = batch::encode_batch(&responses);
                if let Err(e) = send_handle.send((src, batched, Reliability::ReliableOrdered, 0)) {
                    warn!("failed to enqueue send to {src}: {e}");
                }
            }
        }
    });

    // The game loop will grow into a 20 TPS tick in Phase 3. For now we just
    // wait for the raknet task to exit (socket error).
    let _ = raknet_task.await;
}
