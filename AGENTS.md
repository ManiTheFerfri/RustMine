# AGENTS.md

## Build & Test

- **Build:** `cargo build --workspace`
- **Test:** `cargo test --workspace`
- **Lint:** `cargo clippy --workspace --all-targets -- -D warnings`
- **Format:** `cargo fmt --all`
- **Run server:** `cargo run --release` (from workspace root; config in `server.toml`)
- **Single crate:** `cargo build -p rustmine-server` etc.

## Architecture

- Workspace with 10 crates under `crates/`. See `docs/architecture.md`.
- Server binary is `crates/server` → binary named `rustmine`.
- Game loop is sync, single-threaded, 20 TPS. Networking is async (tokio).
- Protocol constants live in `crates/protocol/src/lib.rs`. Target: v1001 / 1.26.30.
- ECS is `hecs`, re-exported from `crates/ecs`.

## Conventions

- Never `unwrap()`/`expect()` on network-derived input — return errors.
- `thiserror` for lib crates, `anyhow` only at binary boundary.
- CI runs fmt + clippy + test on push. `-D warnings` in CI.
- Protocol version must be cited from CloudburstMC/Protocol VERSIONS.md.

## Key Types

### World (`rustmine-world`)
- `Chunk`: 16x256x16 chunk column with subchunks
- `SubChunk`: 16x16x16 block data with runtime palette
- `BlockPos`: World block coordinates
- `ChunkPos`: Chunk coordinates
- `BlockState`: Block types (Air, Stone, Grass, etc.)
- `WorldGenerator`: Trait for terrain generation (FlatGenerator, NoiseGenerator)

### Game (`rustmine-game`)
- `GameState`: Server game state with players and world state
- `PlayerData`: Player entity data (position, velocity, rotation, etc.)
- `GameEvent`: Events processed each tick
- `GameOutput`: Outputs from game logic to network layer
- `GameManager`: Shared game state coordinator

### Commands (`rustmine-commands`)
- `CommandManager`: Command registry and execution
- `CommandRegistry`: Built-in command implementations
- `CommandContext`: Command execution context
- `CommandSender`: Who executed the command (Player, Console, CommandBlock)

### Protocol (`rustmine-protocol`)
- `encode_chunk_column()`: Encode full chunk for network
- `encode_subchunk()`: Encode single subchunk
- Session state machine in `crates/server/src/session.rs`

## World Generation

Two generators available:
1. **FlatGenerator**: Creates a flat world at configurable height
2. **NoiseGenerator**: Procedural terrain using hash-based noise

Configure in `server.toml`:
```toml
[game]
flat_world = true   # true = flat, false = noise-based
seed = 12345        # World seed
```

## Game Loop

- 20 TPS tick rate (50ms per tick)
- Advances time by 1 tick per second in-game
- Processes game events and broadcasts updates
- Logs TPS info every 5 seconds

## Resources

- [CloudburstMC/Protocol](https://github.com/CloudburstMC/Protocol) — primary protocol reference
- [wiki.vg RakNet Protocol](https://wiki.vg/Raknet_Protocol) — RakNet spec
- [PocketMine-MP BedrockProtocol](https://github.com/pmmp/BedrockProtocol) — PHP protocol reference
