# RustMine

A high-performance, protocol-compatible server implementation for Minecraft: Bedrock Edition, written entirely in Rust.

**Status:** Phase 3 (World Sync) — Full terrain generation and 20 TPS game loop!

## Features

### Implemented
- **Transport Layer**: RakNet protocol with reliable/ordered packet delivery
- **Login System**: Full Bedrock 1.26.30 login handshake (offline mode)
- **World Generation**: Flat and procedural terrain with noise-based generation
- **Chunk System**: Proper Bedrock chunk encoding with subchunk streaming
- **Game Loop**: 20 TPS tick system with time advancement
- **Commands**: Built-in server commands (gamemode, tp, give, kick, etc.)
- **Player Movement**: Position tracking and chunk loading on player move

### Planned
- Full inventory system
- Block interactions (break/place)
- World persistence (LevelDB)
- Plugin API
- Multi-version protocol support

## Target

- **Minecraft version:** Bedrock 1.26.30
- **Protocol version:** 1001
- **Source:** [CloudburstMC/Protocol](https://github.com/CloudburstMC/Protocol)

## Quick Start

```bash
# Build
cargo build --release

# Run (uses server.toml in working directory)
cargo run --release

# Run with custom config
cargo run --release -- -c path/to/server.toml
```

## Configuration

Copy `server.toml` and edit as needed:

```toml
[server]
name = "RustMine Server"
motd = "A RustMine Bedrock Server"
port = 19132
max_players = 20
bind_address = "0.0.0.0"

[game]
view_distance = 10          # Chunk render distance
gamemode = "survival"       # Default gamemode
difficulty = "normal"       # Difficulty level
world_name = "world"        # World folder name
seed = 0                    # World seed
flat_world = true            # Use flat terrain generator

[auth]
online_mode = false         # Xbox Live authentication

[logging]
level = "info"             # Log level
```

## Server Commands

Type commands directly in the server console:

| Command | Description |
|---------|-------------|
| `/stop` | Stop the server |
| `/list` | List online players |
| `/say <msg>` | Broadcast a message |
| `/kick <player> [reason]` | Kick a player |
| `/gamemode <mode> [player]` | Change gamemode (0=survival, 1=creative, 2=adventure, 3=spectator) |
| `/tp [player] <x> <y> <z>` | Teleport player |
| `/give <player> <item> [amount]` | Give items |
| `/time set <value>` | Set world time |
| `/difficulty <level>` | Set difficulty (peaceful/easy/normal/hard) |
| `/weather <type>` | Set weather (clear/rain/thunder) |
| `/summon <entity>` | Spawn an entity |
| `/kill [player]` | Kill a player |
| `/heal [player] [amount]` | Heal a player |
| `/feed [player] [amount]` | Feed a player |
| `/effect <player> <effect> [duration]` | Give potion effect |
| `tps` | Show server TPS |
| `players` | List players |

## Development

```bash
cargo fmt --all          # Format
cargo clippy --workspace # Lint
cargo test --workspace   # Test
cargo build --workspace  # Build all crates
```

## Architecture

See [`docs/architecture.md`](docs/architecture.md) for crate layout and data flow.

### Crate Structure

```
crates/
├── raknet/          UDP transport (RakNet protocol)
├── protocol/        Bedrock packet definitions and encoding
├── nbt/             Little-endian NBT + VarInt helpers
├── world/           Chunk model, terrain generation
├── ecs/             Entity-component-system (hecs-backed)
├── game/            Game simulation and state management
├── commands/        Command parser and built-in commands
├── plugin-api/      Plugin trait system (planned)
├── server/          Server binary entrypoint
└── net-cli-tools/   Packet sniffer/replay tools (optional)
```

## License

GPL-3.0-or-later — see [LICENSE](LICENSE).
