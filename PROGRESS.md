# Progress

## Phase 0 — Scaffolding ✅

- [x] Cargo workspace with 10 crates
- [x] Server binary with clap + toml config loading + tracing logging
- [x] GitHub Actions CI (fmt, clippy, test)
- [x] Architecture docs, protocol notes, AGENTS.md

## Phase 1 — Transport (RakNet) ✅

- [x] UDP socket + unconnected ping/pong (server discovery)
- [x] Connection handshake (open connection request/reply)
- [x] Reliability layers (unreliable, reliable, reliable-ordered)
- [x] Packet splitting/reassembly (MTU): encoder splits oversize payloads; receiver reassembles split frames keyed by split_id and expires stale partials
- [x] ACK/NACK handling with periodic resend of un-ACKed reliable datagrams
- [x] Connection lifecycle (timeouts, disconnects, ping tracking)
- [x] Channel-based hooks (`ServerEvent` / outbound sender) for the game loop to receive batches and queue sends without blocking I/O
- [x] Round-trip tests for offline packets, frames, datagrams, ACK/NACK, and split-frame reassembly

## Phase 2 — Login (complete) ✅

- [x] VarInt/ZigZag helpers (rustmine-nbt)
- [x] Bedrock 0xfe batch framing (encode/decode, with round-trip tests)
- [x] Packet ID table for v1001 / 1.26.30
- [x] Login handshake encoders: NetworkSettings, PlayStatus, ResourcePacksInfo, ResourcePackStack, Disconnect, StartGame (minimal), SetSpawnPosition, SetTime, SetDifficulty, SetPlayerGameType, ChunkRadiusUpdated, BiomeDefinitionList, NetworkChunkPublisherUpdate, empty LevelChunk, PlayerList
- [x] Session state machine driving the client from RequestNetworkSettings → in-world (offline mode)
- [x] Server wires RakNet events into Session dispatch and flushes responses through the outbound queue
- [x] JWT chain parsing / online-mode auth (gated by `auth.online_mode`)
- [x] Full skin payload parsing in PlayerList
- [x] Client reaches in-world state verified against a real Bedrock 1.26.30 client

## Phase 3 — World Sync (major update) 🚀

- [x] **World crate complete rewrite**
  - Full chunk model with 16x16x16 subchunks
  - Block state management with runtime IDs
  - BlockPos and ChunkPos coordinate types
- [x] **Terrain generation**
  - FlatGenerator for flat worlds
  - NoiseGenerator for procedural terrain with hash-based noise
  - WorldGenerator trait for extensibility
- [x] **Proper chunk network encoding**
  - SubChunk encoding with runtime palettes
  - Version 8 subchunk format (Bedrock standard)
  - Bits-per-block calculation for palette compression
  - Chunk column encoding for LevelChunk packet
- [x] **20 TPS game loop**
  - Fixed 50ms tick duration
  - GameState with player management
  - Time of day advancement (1 tick per second in-game)
  - Console commands integration
- [x] **Player movement sync**
  - MovePlayer packet parsing
  - Position tracking in session
  - Chunk radius requests handled
- [x] **Server improvements**
  - Chunk generation and streaming to players
  - Configurable flat world mode
  - Server info logging (TPS, uptime, player count)
  - Graceful shutdown handling

## Phase 4 — Interaction (in progress)

- [ ] Block break/place with validation
- [ ] Inventory system
- [ ] Item pickup
- [ ] Basic mob AI
- [ ] Chat system with formatting
- [ ] Core commands implementation

## Phase 5 — Persistence

- [ ] LevelDB world save/load
- [ ] Player data save/load across sessions

## Phase 6 — Plugin API + Polish

- [ ] Plugin trait system
- [ ] Example plugin
- [ ] Performance pass
- [ ] Documentation pass

## Phase 7 (Stretch)

- [ ] Multi-version protocol support
- [ ] WASM plugin sandbox
- [ ] Redstone-equivalent circuitry
- [ ] More mobs/items/recipes

## Recent Updates (Big Update)

### World System
- Complete world crate with proper chunk data structures
- BlockState enum with runtime ID mapping (Air, Stone, Grass, Dirt, Water, etc.)
- SubChunk (16x16x16) and Chunk (16x256x16) column structures
- Flat and noise-based terrain generators

### Chunk Encoding
- Proper Bedrock subchunk format v8 encoding
- Dynamic bits-per-block calculation for palette efficiency
- LevelChunk packet encoding with subchunk streaming

### Game Loop
- 20 TPS tick system (50ms per tick)
- GameState with player tracking and world state
- Time advancement (in-game day/night cycle)
- Console commands (list, tps, stop)

### Commands
- Full command parser with quoted string support
- CommandRegistry with built-in commands:
  - `/stop` - Shutdown server
  - `/list` - List players
  - `/say` - Broadcast message
  - `/kick` - Kick player
  - `/gamemode` - Change gamemode
  - `/give` - Give items
  - `/time` - Set time
  - `/difficulty` - Set difficulty
  - `/weather` - Set weather
  - `/tp` - Teleport
  - `/summon` - Spawn entities
  - `/kill` - Kill entities
  - `/heal` / `/feed` - Healing
  - `/effect` - Potion effects
  - And more...

### Server Features
- ServerState with shared world and game state
- Chunk streaming based on player position and view distance
- Session position tracking
- Command console with live input
- Server info reporting (TPS, uptime, players)
