//! Block interaction packet parsing and validation helpers.
//!
//! Provides minimal decoders for BREAK_BLOCK (0x14) and PLACE_BLOCK (0x16)
//! packets used during Phase 4 interaction implementation.

use rustmine_nbt::{read_var_i32, read_var_u32};
use crate::id;
use crate::write_packet;

/// Parsed result from a BREAK_BLOCK packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreakBlockInfo {
    /// Block position in world coordinates.
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Which face of the block was broken (optional).
    pub face: Option<i32>,
}

/// Parsed result from a PLACE_BLOCK packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceBlockInfo {
    /// Block position in world coordinates.
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Which face of the adjacent block the new block was placed against.
    pub face: Option<i32>,
    /// Runtime block ID of the block being placed.
    pub runtime_id: Option<u32>,
}

/// Try to parse a BREAK_BLOCK (0x14) payload.
///
/// Assumes the format: varint x, varint y, varint z, [optional varint face].
/// Returns None if the payload is too short or malformed.
pub fn parse_break_block(body: &[u8]) -> Option<BreakBlockInfo> {
    let mut pos = 0usize;

    let x = read_var_i32(body, &mut pos).ok()?;
    let y = read_var_i32(body, &mut pos).ok()?;
    let z = read_var_i32(body, &mut pos).ok()?;

    // Some Bedrock versions include a face/direction varint after the position.
    let face = if pos < body.len() {
        // Try reading an additional varint as face; if it consumes too much, ignore.
        let mut face_pos = pos;
        read_var_i32(body, &mut face_pos).ok()
    } else {
        None
    };

    Some(BreakBlockInfo { x, y, z, face })
}

/// Try to parse a PLACE_BLOCK (0x16) payload.
///
/// Assumes: varint x, varint y, varint z, [optional varint face],
/// [optional runtime_id]. Returns None on too-short payload.
pub fn parse_place_block(body: &[u8]) -> Option<PlaceBlockInfo> {
    let mut pos = 0usize;

    let x = read_var_i32(body, &mut pos).ok()?;
    let y = read_var_i32(body, &mut pos).ok()?;
    let z = read_var_i32(body, &mut pos).ok()?;

    // Face is usually present.
    let face = if let Ok(v) = read_var_i32(body, &mut pos) {
        Some(v)
    } else {
        None
    };

    // Runtime ID (or block state ID) is sometimes included.
    let runtime_id = if let Ok(v) = read_var_u32(body, &mut pos) {
        Some(v)
    } else {
        None
    };

    Some(PlaceBlockInfo {
        x,
        y,
        z,
        face,
        runtime_id,
    })
}

/// Basic validation rules for a block break action.
///
/// Checks that the block is within the world bounds (Y must be 0..255 for
/// Bedrock) and that it isn't air (nothing to break).
/// For a real server, additional checks (reach distance, tool requirements,
/// server-authoritative breaking) would be added.
pub fn validate_break(
    x: i32,
    y: i32,
    z: i32,
    is_air: bool,
) -> Result<(), String> {
    if y < 0 || y >= 256 {
        return Err(format!("Block Y {y} out of world bounds (0-255)"));
    }
    if is_air {
        return Err("Cannot break air".to_string());
    }
    Ok(())
}

/// Encode an UpdateBlocks (0x15) packet for a single block change.
/// Minimal format: position (varint x, y, z) + runtime_id (varint) + flags (varint 0).
pub fn encode_update_block(x: i32, y: i32, z: i32, runtime_id: u32) -> Vec<u8> {
    let mut p = Vec::new();
    rustmine_nbt::write_var_i32(&mut p, x);
    rustmine_nbt::write_var_i32(&mut p, y);
    rustmine_nbt::write_var_i32(&mut p, z);
    rustmine_nbt::write_var_u32(&mut p, runtime_id);
    rustmine_nbt::write_var_u32(&mut p, 0); // flags: 0
    write_packet(id::UPDATE_BLOCKS, &p)
}

/// Check if a block position is within reach distance of a player position.
/// Reach distance is measured in Euclidean distance (approximate for simplicity).
pub fn within_reach(
    player_x: f32, player_y: f32, player_z: f32,
    block_x: i32, block_y: i32, block_z: i32,
    max_reach: f32,
) -> bool {
    let dx = (player_x - block_x as f32).abs();
    let dy = (player_y - block_y as f32).abs();
    let dz = (player_z - block_z as f32).abs();
    (dx * dx + dy * dy + dz * dz).sqrt() <= max_reach
}

/// Basic validation rules for a block place action.
///
/// Checks world bounds and that the target position isn't already occupied
/// by a non-air block.
pub fn validate_place(
    x: i32,
    y: i32,
    z: i32,
    is_air: bool,
    runtime_id: Option<u32>,
) -> Result<(), String> {
    if y < 0 || y >= 256 {
        return Err(format!("Block Y {y} out of world bounds (0-255)"));
    }
    if !is_air {
        return Err("Target position already occupied by a block".to_string());
    }
    if runtime_id.is_none() {
        return Err("No block runtime ID provided".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_break_simple() {
        // Build a minimal payload: x=10, y=64, z=-5 (varint encoded)
        let mut body = Vec::new();
        rustmine_nbt::write_var_i32(&mut body, 10);
        rustmine_nbt::write_var_i32(&mut body, 64);
        rustmine_nbt::write_var_i32(&mut body, -5);

        let info = parse_break_block(&body).expect("should parse");
        assert_eq!(info.x, 10);
        assert_eq!(info.y, 64);
        assert_eq!(info.z, -5);
        assert!(info.face.is_none());
    }

    #[test]
    fn test_parse_break_with_face() {
        let mut body = Vec::new();
        rustmine_nbt::write_var_i32(&mut body, 0);
        rustmine_nbt::write_var_i32(&mut body, 63);
        rustmine_nbt::write_var_i32(&mut body, 0);
        rustmine_nbt::write_var_i32(&mut body, 1); // face = 1

        let info = parse_break_block(&body).expect("should parse");
        assert_eq!(info.x, 0);
        assert_eq!(info.face, Some(1));
    }

    #[test]
    fn test_parse_place_simple() {
        let mut body = Vec::new();
        rustmine_nbt::write_var_i32(&mut body, 1);
        rustmine_nbt::write_var_i32(&mut body, 65);
        rustmine_nbt::write_var_i32(&mut body, 2);
        rustmine_nbt::write_var_i32(&mut body, 0); // face
        rustmine_nbt::write_var_u32(&mut body, 2); // runtime_id (grass)

        let info = parse_place_block(&body).expect("should parse");
        assert_eq!(info.x, 1);
        assert_eq!(info.y, 65);
        assert_eq!(info.z, 2);
        assert_eq!(info.face, Some(0));
        assert_eq!(info.runtime_id, Some(2));
    }

    #[test]
    fn test_validate_break_ok() {
        assert!(validate_break(10, 64, 10, false).is_ok());
    }

    #[test]
    fn test_validate_break_air() {
        assert!(validate_break(0, 64, 0, true).is_err());
    }

    #[test]
    fn test_validate_break_out_of_bounds() {
        assert!(validate_break(0, 300, 0, false).is_err());
    }

    #[test]
    fn test_validate_place_ok() {
        assert!(validate_place(5, 64, 5, true, Some(1)).is_ok());
    }

    #[test]
    fn test_validate_place_occupied() {
        assert!(validate_place(5, 64, 5, false, Some(1)).is_err());
    }

    #[test]
    fn test_validate_place_no_id() {
        assert!(validate_place(5, 64, 5, true, None).is_err());
    }
}
