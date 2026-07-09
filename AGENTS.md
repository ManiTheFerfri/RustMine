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

## Resources

- [CloudburstMC/Protocol](https://github.com/CloudburstMC/Protocol) — primary protocol reference
- [wiki.vg RakNet Protocol](https://wiki.vg/Raknet_Protocol) — RakNet spec
- [PocketMine-MP BedrockProtocol](https://github.com/pmmp/BedrockProtocol) — PHP protocol reference
