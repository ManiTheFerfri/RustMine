//! Plugin/extension API for RustMine.
//!
//! A trait-based API providing event hooks (player join/leave, block
//! break/place, chat, command execution, tick) for both first-party
//! and third-party extensions.
//!
//! Native Rust plugins link against this crate. WASM sandbox support
//! (via `wasmtime`) is a planned stretch goal.
