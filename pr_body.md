## Phase 3 - World Sync (Major Update)

This PR implements a massive update to the RustMine Minecraft Bedrock server, advancing from Phase 2 (Login) to Phase 3 (World Sync).

### 🚀 Major Changes

#### World System (rustmine-world)
- Complete chunk data structures: Chunk (16x256x16), SubChunk (16x16x16)
- BlockState enum with runtime IDs (Air, Stone, Grass, Dirt, Water, Bedrock, Ores, etc.)
- BlockPos and ChunkPos coordinate types with utility methods
- WorldGenerator trait for extensible terrain generation
- **FlatGenerator**: Creates flat worlds at configurable height
- **NoiseGenerator**: Procedural terrain with hash-based noise function

#### Chunk Encoding (rustmine-protocol)
- Subchunk encoding (version 8 format) with runtime palettes
- Dynamic bits-per-block calculation for palette compression
- Chunk column encoding for LevelChunk packet
- Chunk decoding module for future client simulation

#### 20 TPS Game Loop (rustmine-game)
- Fixed 50ms tick duration (20 TPS)
- GameState with player management and world state
- Time of day advancement (1 tick per second in-game)
- GameEvent/GameOutput for tick communication
- PlayerData with full position, velocity, rotation tracking

#### Commands System (rustmine-commands)
- Command parser with quoted string support
- CommandRegistry with 20+ built-in commands
- CommandManager with async execution support
- Permission levels (Everyone, GameMaster, Admin, Console)

#### Server Improvements (rustmine-server)
- Full 20 TPS game loop with tick counting
- Chunk streaming based on player position and view distance
- Session position tracking for movement sync
- Console command interface with live input
- Server info reporting (TPS, uptime, player count)
- Configurable flat world mode in server.toml
- MovePlayer packet parsing for position updates

#### ECS Components (rustmine-ecs)
- Player components: Position, Rotation, Velocity, PlayerInfo
- Block components: BlockPosition, BlockType
- Item components: ItemComponent
- Spawn helpers for player and item entities

#### Protocol Updates
- Added 15+ missing packet IDs for gameplay
- Added inventory packet IDs

### Files Changed
- 18 files changed, +3672 lines

### Testing
Build and test with:
```bash
cargo build --workspace
cargo test --workspace
```

### TODO (Future PRs)
- Full inventory system
- Block interactions (break/place)
- World persistence (LevelDB)
- Plugin API
- Multi-version protocol support
