//! World model: chunks, block palettes, terrain generation, and persistence.
//!
//! The Bedrock world model uses 16x16x16 subchunks with runtime block state IDs
//! and LevelDB-backed on-disk storage (LevelDB, not Anvil).
