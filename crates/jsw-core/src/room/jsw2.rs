//! Jet Set Willy II's rooms, which are not stored the way Jet Set Willy's are.
//!
//! There is no disassembly of this game; the format is documented at
//! <https://www.seasip.info/Jsw/jsw2room.html> and everything here was checked
//! against the snapshot `jsw2-data` was lifted from.
//!
//! A room is an entry in a table whose address is the word at 32361: a pointer
//! to a compressed shape, a byte of high bits, eight cell pattern bytes, a
//! name-and-border byte, and a name. The shape unpacks to 512 cells, and the
//! exits, the guardians and the arrows follow it.

use super::{Exits, Tile};

/// Cells in a room: thirty-two across, sixteen down.
pub const CELLS: usize = 512;

/// A byte below this is a type and a repeat count; at or above it, air only.
const AIR_RUN: u8 = 144;

/// Where room `number`'s entry is named in the table.
#[must_use]
pub fn entry_of(number: usize) -> u16 {
    jsw2_data::ROOM_TABLE.wrapping_add(2 * number as u16)
}

/// The address of room `number`'s entry, followed through the table.
fn room_at(number: usize) -> u16 {
    jsw2_data::word(entry_of(number))
}

/// Unpack room `number`'s shape.
///
/// A byte below 144 gives a cell type in bits 4 to 7 and a repeat count less
/// one in bits 0 to 3; anything higher is that many air cells less 127. The
/// count stops at 512 whatever the last run claims: nine rooms end with a run
/// that would carry them past the end of the room, and the game simply stops.
#[must_use]
pub fn cells(number: usize) -> [u8; CELLS] {
    let mut cells = [0u8; CELLS];
    let mut at = jsw2_data::word(room_at(number));
    let mut filled = 0;

    while filled < CELLS {
        let byte = jsw2_data::read(at);
        at = at.wrapping_add(1);
        let (kind, run) = if byte < AIR_RUN {
            (byte >> 4, usize::from(byte & 15) + 1)
        } else {
            (0, usize::from(byte) - 127)
        };
        let run = run.min(CELLS - filled);
        cells[filled..filled + run].fill(kind);
        filled += run;
    }
    cells
}

/// Where a room's shape ends, which is where its exits begin.
#[must_use]
pub fn shape_end(number: usize) -> u16 {
    let mut at = jsw2_data::word(room_at(number));
    let mut filled = 0;
    while filled < CELLS {
        let byte = jsw2_data::read(at);
        at = at.wrapping_add(1);
        filled += if byte < AIR_RUN {
            usize::from(byte & 15) + 1
        } else {
            usize::from(byte) - 127
        };
    }
    at
}

/// Where a room's name starts: past the shape pointer, the high bits, the eight
/// pattern bytes and the name-and-border byte.
const NAME_OFFSET: u16 = 12;

/// Room `number`'s name, with its tokens expanded.
///
/// The name is stored as bytes whose last one has bit 7 set. A byte below 32 is
/// not a character but an index into the table of words the game builds names
/// from - so The Off Licence is stored as the token for "The " and then the
/// eleven characters of "Off Licence".
#[must_use]
pub fn name(number: usize) -> String {
    let mut text = String::new();
    let mut at = room_at(number).wrapping_add(NAME_OFFSET);
    loop {
        let byte = jsw2_data::read(at);
        at = at.wrapping_add(1);
        let ch = byte & 127;
        if ch < 32 {
            text.push_str(&token(ch));
        } else {
            text.push(ch as char);
        }
        if byte & 128 != 0 {
            break;
        }
    }
    // A token carries a space after it, so a name ending in one has a space it
    // does not want.
    text.trim_end().to_owned()
}

/// Where a room's name ends, which is where its exits are.
#[must_use]
pub fn name_end(number: usize) -> u16 {
    let mut at = room_at(number).wrapping_add(NAME_OFFSET);
    while jsw2_data::read(at) & 128 == 0 {
        at = at.wrapping_add(1);
    }
    at.wrapping_add(1)
}

/// The `index`th word of the token table, with the space that follows it in a
/// name. Each word ends at the byte with bit 7 set.
fn token(index: u8) -> String {
    let mut at = jsw2_data::TOKENS;
    for _ in 1..index {
        while jsw2_data::read(at) & 128 == 0 {
            at = at.wrapping_add(1);
        }
        at = at.wrapping_add(1);
    }

    let mut word = String::new();
    loop {
        let byte = jsw2_data::read(at);
        at = at.wrapping_add(1);
        let ch = byte & 127;
        // A token can start with a token: "Megatree" is stored as the word for
        // "The" and then its own letters, which is how "Under The Megatree"
        // costs two bytes in the room.
        if ch < 32 {
            word.push_str(&token(ch));
        } else {
            word.push(ch as char);
        }
        if byte & 128 != 0 {
            break;
        }
    }
    word.push(' ');
    word
}

/// Everything a room entry holds, before it is turned into a [`super::Room`].
#[derive(Debug, Clone)]
pub struct Entry {
    /// Cell type per cell, row-major: 0 air, 1 water, 2 earth, 3 fire,
    /// 4 forward ramp, 5 left conveyor, 6 item, 7 back ramp, 8 right conveyor.
    pub cells: [u8; CELLS],
    /// The eight cell graphics the room draws itself with, as indices into the
    /// cell table.
    pub patterns: [u16; 8],
    pub exits: Exits,
    /// Whether a rope hangs in this room. Where it hangs is decided by the
    /// room's special-case code, which nothing reads yet.
    pub rope: bool,
    pub conveyor_animates: bool,
    /// The room's special-case code, from T5. Recorded, not acted on.
    pub special: u8,
    /// Seven bytes each, at most eight between these and the arrows.
    pub guardians: Vec<[u8; 7]>,
    pub arrows: Vec<[u8; 2]>,
    pub name: String,
    pub border: u8,
}

/// Read room `number`'s entry.
#[must_use]
pub fn entry(number: usize) -> Entry {
    let at = room_at(number);

    // Eight cell pattern bytes, each with its ninth bit in the high-bits byte:
    // bit 7 of that byte is bit 8 of the first pattern, bit 6 of the second,
    // and so on down.
    let high = jsw2_data::read(at.wrapping_add(2));
    let mut patterns = [0u16; 8];
    for (slot, pattern) in patterns.iter_mut().enumerate() {
        let low = jsw2_data::read(at.wrapping_add(3 + slot as u16));
        let ninth = (high >> (7 - slot)) & 1;
        *pattern = u16::from(low) | (u16::from(ninth) << 8);
    }

    // The exits follow the name, in the order left, up, right, down - not the
    // order Jet Set Willy stores them in. They count from one rather than from
    // zero, and a room names itself in a direction it has no exit in, which is
    // what the engine already takes "no room that way" to mean.
    let mut after = name_end(number);
    let door = |offset: u16| jsw2_data::read(after.wrapping_add(offset)).saturating_sub(1);
    let exits = Exits {
        left: door(0),
        up: door(1),
        right: door(2),
        down: door(3),
    };
    after = after.wrapping_add(4);

    let t4 = jsw2_data::read(after);
    after = after.wrapping_add(1);
    let t5 = if t4 & 16 != 0 {
        let byte = jsw2_data::read(after);
        after = after.wrapping_add(1);
        byte
    } else {
        0
    };

    let mut guardians = Vec::new();
    for _ in 0..(t4 & 15) {
        let mut record = [0u8; 7];
        for (index, byte) in record.iter_mut().enumerate() {
            *byte = jsw2_data::read(after.wrapping_add(index as u16));
        }
        after = after.wrapping_add(7);
        guardians.push(record);
    }

    // Arrows only exist if T5 says so, and take what is left of the eight slots
    // the guardians did not use.
    let mut arrows = Vec::new();
    if t5 & 128 != 0 {
        for _ in guardians.len()..8 {
            let record = [
                jsw2_data::read(after),
                jsw2_data::read(after.wrapping_add(1)),
            ];
            // Two zero bytes end the list.
            if record == [0, 0] {
                break;
            }
            after = after.wrapping_add(2);
            arrows.push(record);
        }
    }

    Entry {
        cells: cells(number),
        patterns,
        exits,
        rope: t4 & 128 != 0,
        conveyor_animates: t4 & 64 != 0,
        special: t5 & 63,
        guardians,
        arrows,
        name: name(number),
        border: jsw2_data::read(at.wrapping_add(11)) & 7,
    }
}

/// The cell graphic a pattern names: nine bytes, an attribute and eight rows.
///
/// Bit 7 of the attribute means inverse rather than flash. The game inverts
/// those cells as it starts up and clears the bit, so that is done here.
#[must_use]
pub fn cell_graphic(pattern: u16) -> Tile {
    let at = jsw2_data::CELL_TABLE.wrapping_add(pattern.wrapping_mul(9));
    let attr = jsw2_data::read(at);
    let inverse = attr & 128 != 0;

    let mut pixels = [0u8; 8];
    for (row, byte) in pixels.iter_mut().enumerate() {
        let read = jsw2_data::read(at.wrapping_add(1 + row as u16));
        *byte = if inverse { !read } else { read };
    }
    Tile {
        attr: attr & 127,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_room_unpacks_to_exactly_five_hundred_and_twelve_cells() {
        for number in 0..jsw2_data::ROOM_COUNT {
            let cells = cells(number);
            assert_eq!(cells.len(), CELLS, "room {number}");
            // Nothing may claim to be a cell type the format does not have.
            assert!(
                cells.iter().all(|&cell| cell <= 8),
                "room {number} has a cell type past the eight the game knows"
            );
        }
    }

    #[test]
    fn the_last_run_of_a_shape_is_truncated_rather_than_trusted() {
        // Nine of the rooms end with a run that would carry them past 512
        // cells. Room 34 is one, and overshoots by twelve.
        let cells = cells(34);
        assert_eq!(cells.len(), CELLS);
    }

    #[test]
    fn room_names_expand_through_the_token_table() {
        // Room 0's name is stored as the token for "The " and then "Off Licence".
        assert_eq!(name(0), "The Off Licence");

        for number in 0..jsw2_data::ROOM_COUNT {
            let name = name(number);
            assert!(!name.is_empty(), "room {number} has no name");
            assert!(
                name.len() <= 32,
                "room {number} is called {name:?}, which will not fit on the screen"
            );
            assert!(
                name.bytes().all(|b| (32..127).contains(&b)),
                "room {number} is called {name:?}, which is not printable"
            );
        }
    }

    #[test]
    fn a_room_entry_reads_out_whole() {
        for number in 0..jsw2_data::ROOM_COUNT {
            let room = entry(number);
            assert!(
                (room.exits.left as usize) < jsw2_data::ROOM_COUNT
                    && (room.exits.right as usize) < jsw2_data::ROOM_COUNT
                    && (room.exits.up as usize) < jsw2_data::ROOM_COUNT
                    && (room.exits.down as usize) < jsw2_data::ROOM_COUNT,
                "room {number} ({}) leads somewhere that is not a room: {:?}",
                room.name,
                room.exits
            );
            assert!(
                room.guardians.len() + room.arrows.len() <= 8,
                "room {number} has {} things moving in it",
                room.guardians.len() + room.arrows.len()
            );
            assert!(
                room.patterns.iter().all(|&p| p < 512),
                "room {number} names a cell graphic past the table"
            );
        }
    }

    #[test]
    fn the_cell_graphics_of_the_first_room_are_the_ones_in_memory() {
        let room = entry(0);
        // Room 0's first slot is cell graphic 0, which is nine bytes at 35960.
        let tile = cell_graphic(room.patterns[0]);
        assert_eq!(tile.attr, jsw2_data::read(jsw2_data::CELL_TABLE) & 127);
        assert_eq!(tile.pixels[0], jsw2_data::read(jsw2_data::CELL_TABLE + 1));
    }

    #[test]
    fn the_first_room_is_mostly_air_with_a_floor_along_the_bottom() {
        let cells = cells(0);
        // The bottom row of any room Willy can stand in is solid.
        assert!(
            cells[15 * 32..].iter().all(|&cell| cell != 0),
            "the floor of room 0 has a hole in it"
        );
        // And the top row of The Off Licence is open sky.
        assert!(cells[..32].iter().all(|&cell| cell == 0));
    }
}
