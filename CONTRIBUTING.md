# Contributing to RustMine

## Getting Started

```bash
git clone https://github.com/ManiTheFerfri/RustMine.git
cd RustMine
cargo build --workspace
```

## Development Workflow

1. **Pick or create an issue.** Phase 1 tasks are tracked in `PROGRESS.md`.
2. **Branch from `main`.** Use a descriptive name: `feat/raknet-handshake`, `fix/mtu-negotiation`.
3. **Write code.** Follow existing conventions in `AGENTS.md`.
4. **Verify before committing:**
   ```bash
   cargo fmt --all
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```
5. **Commit with a short, descriptive message.** See commit history for style.
6. **Open a pull request.**

## Code Style

- `thiserror` for library error types, `anyhow` only at the binary boundary.
- Never `unwrap()`/`expect()` on network-derived input.
- Use `# Safety` doc comments when writing unsafe code.
- Protocol constants cite their source.

## Testing

- Every packet type needs a round-trip test (serialize → deserialize → assert equality).
- Integration tests spin up a real server instance with a mock RakNet client.
- Run the full suite before submitting: `cargo test --workspace`.

## Protocol Research

Before implementing any protocol layer, consult:
- [CloudburstMC/Protocol](https://github.com/CloudburstMC/Protocol) — primary Bedrock protocol reference
- [wiki.vg RakNet Protocol](https://wiki.vg/Raknet_Protocol) — RakNet specification
- [PocketMine-MP BedrockProtocol](https://github.com/pmmp/BedrockProtocol) — PHP reference

Cite what you find in code comments and `docs/protocol-notes.md`.
