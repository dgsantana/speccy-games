//! Jet Set Willy's byte tables, generated from the published disassembly.
//!
//! Nothing here is hand written; see `tools/gen_jsw_data.py`. The tables are the
//! game's own bytes, read out of the image the disassembly assembles to, so
//! they can be checked against it line by line.
//!
//! Jet Set Willy is Copyright (c) 1984 Software Projects Ltd.

pub mod entities;
pub mod items;
pub mod rooms;
pub mod sprites;

/// Rooms in the mansion.
pub const ROOM_COUNT: usize = 64;

/// Bytes in a room definition.
pub const ROOM_SIZE: usize = 256;

/// Items Willy has to collect.
pub const ITEM_COUNT: usize = 83;
