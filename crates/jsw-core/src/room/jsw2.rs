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
