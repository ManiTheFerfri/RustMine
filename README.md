# RustMine

A high-performance, protocol-compatible server implementation for Minecraft: Bedrock Edition, written entirely in Rust.

**Status:** Phase 0 (scaffolding) — no networking yet.

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

Copy `server.toml` and edit as needed. All keys have defaults (offline mode, port 19132, survival mode, view distance 10).

## Development

```bash
cargo fmt --all          # format
cargo clippy --workspace # lint
cargo test --workspace   # test
cargo build --workspace  # build all crates
```

## Architecture

See [`docs/architecture.md`](docs/architecture.md) for crate layout and data flow.

## License

GPL-3.0-or-later — see [LICENSE](LICENSE).
