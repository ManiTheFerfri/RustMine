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

## Phase 2 — Login (in progress)

- [x] VarInt/ZigZag helpers (rustmine-nbt)
- [x] Bedrock 0xfe batch framing (encode/decode, with round-trip tests)
- [x] Packet ID table for v1001 / 1.26.30
- [x] Login handshake encoders: NetworkSettings, PlayStatus, ResourcePacksInfo, ResourcePackStack, Disconnect, StartGame (minimal), SetSpawnPosition, SetTime, SetDifficulty, SetPlayerGameType, ChunkRadiusUpdated, BiomeDefinitionList, NetworkChunkPublisherUpdate, empty LevelChunk, PlayerList
- [x] Session state machine driving the client from RequestNetworkSettings → in-world (offline mode)
- [x] Server wires RakNet events into Session dispatch and flushes responses through the outbound queue
- [ ] JWT chain parsing / online-mode auth (gated by `auth.online_mode`)
- [ ] Full skin payload parsing in PlayerList
- [ ] Client reaches in-world state verified against a real Bedrock 1.26.30 client (needs manual test)

## Phase 3 — World Sync

- [ ] Chunk generation (flat + simple noise)
- [ ] Correct chunk network encoding (subchunks v8, biomes, block palette)
- [ ] Player spawn position + spawn chunk radius
- [ ] Movement sync (MovePlayer broadcast)

## Phase 4 — Interaction

- [ ] Block break/place
- [ ] Inventory
- [ ] Item pickup
- [ ] Basic mob AI
- [ ] Chat
- [ ] Core commands

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
