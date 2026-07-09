# Protocol Notes

## Target Version

- **Protocol version:** 1001
- **Minecraft version:** Bedrock 1.26.30
- **Source:** [CloudburstMC/Protocol VERSIONS.md](https://github.com/CloudburstMC/Protocol/blob/3.0/VERSIONS.md)
- **Reference implementations:**
  - [CloudburstMC/Protocol](https://github.com/CloudburstMC/Protocol) (Java, primary reference)
  - [PocketMine-MP BedrockProtocol](https://github.com/pmmp/BedrockProtocol) (PHP)
  - [rak-rs](https://crates.io/crates/rak-rs) (Rust RakNet crate, may be referenced but not used as dependency)

## RakNet

The Bedrock protocol uses RakNet as its transport layer over UDP.

Key resources:
- [wiki.vg RakNet Protocol](https://wiki.vg/Raknet_Protocol) (community-maintained spec)
- [raklib](https://github.com/pmmp/RakLib) (PHP reference for RakNet)
- [rak-rs](https://github.com/caelunshun/rak-rs) (Rust RakNet implementation)

RakNet reliability layers used by Bedrock:
- 0: Unreliable
- 3: Reliable
- 5: ReliableOrdered
- Not used: UnreliableSequenced (1), ReliableSequenced (4), etc.

## NBT

Bedrock uses **little-endian** NBT (unlike Java Edition's big-endian).
Bedrock also uses network VarInt (unsigned) variants in some packets instead
of the standard NBT VarInt.

## World Format

Bedrock uses LevelDB for world storage, not Anvil region files.
The database layout uses specific key prefixes for chunks, entities, map data, etc.

## Known Gaps vs. Vanilla

*To be filled in as protocol implementation progresses.*
