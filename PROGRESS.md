# Progress

## Phase 0 — Scaffolding ✅

- [x] Cargo workspace with 10 crates
- [x] Server binary with clap + toml config loading + tracing logging
- [x] GitHub Actions CI (fmt, clippy, test)
- [x] Architecture docs, protocol notes, AGENTS.md

## Phase 1 — Transport (RakNet)

- [ ] UDP socket + unconnected ping/pong (server discovery)
- [ ] Connection handshake (open connection request/reply)
- [ ] Reliability layers (unreliable, reliable, reliable-ordered)
- [ ] Packet splitting/reassembly (MTU)
- [ ] ACK/NACK handling
- [ ] Connection lifecycle (timeouts, disconnects, ping tracking)

## Phase 2 — Login

- [ ] Login packet handling
- [ ] Offline-mode auth
- [ ] Resource pack negotiation (no-packs path)
- [ ] Client reaches in-world state

## Phase 3 — World Sync

- [ ] Chunk generation (flat + simple noise)
- [ ] Chunk network encoding
- [ ] Player spawn
- [ ] Movement sync

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
