# RustMine Architecture

## Crate Layout

```
crates/
├── raknet/          UDP transport (RakNet protocol: reliability, framing, ACK/NACK)
├── protocol/        Bedrock packet definitions, version-aware (de)serialization
├── nbt/             Little-endian NBT + VarInt helpers
├── world/           Chunk model, terrain generation, LevelDB persistence
├── ecs/             Entity-component-system (hecs-backed)
├── game/            Authoritative game simulation (physics, crafting, AI)
├── commands/        In-game command parser + built-in commands
├── plugin-api/      Public API for extensions (trait-based, WASM planned)
├── server/          Binary entrypoint: tick loop, session mgmt, config, CLI
└── net-cli-tools/   Optional packet sniffer/replay tools
```

## Data Flow

```
Client (UDP) → RakNet → Protocol → Server Tick Loop → ECS/Game/World
                                      │
                                      ▼
                                Plugin API hooks
                                      │
                                      ▼
                              Protocol → RakNet → Client
```

## Tick Loop

- **Rate:** 20 TPS (50ms per tick), matching vanilla Bedrock.
- **Model:** Single-writer game state. Network I/O runs on tokio async tasks.
  Incoming packets are fed into the tick loop via `tokio::sync::mpsc` channels.
  Outgoing packets are pushed from the tick loop into per-player send queues.
- **World access:** Single-threaded within the tick loop — no locks on world state.

## Key Design Decisions

| Decision                     | Rationale                                                      |
|------------------------------|----------------------------------------------------------------|
| `hecs` for ECS               | Lightweight, no proc macros, well-maintained.                  |
| `tokio` for networking only  | Game logic is sync and runs on a dedicated tick thread.        |
| `thiserror` for lib crates   | Explicit error types. `anyhow` only at the binary boundary.    |
| Offline-mode auth by default | Simplifies early development. Online-mode gated behind config. |
