//! A room of the mansion, decoded from its 256 bytes.
//!
//! Every room carries its own tile graphics, which is the big difference from
//! Manic Miner: there the cavern's layout named tiles from a fixed set, here the
//! six bitmaps travel with the room. That is why the mansion's rooms look so
//! unlike each other.

use speccy::layout::{ATTR_BACK, ATTR_BUF, COLUMNS, ROWS, SCREEN_BACK, cell_offset};
use speccy::memory::{Memory, lsb, msb};

/// Cells across the playing area.
pub const CELLS: usize = ROWS * COLUMNS;

/// Byte offsets inside a room definition. The original reads these by absolute
/// address; naming them is the only liberty taken.
mod offset {
    pub const LAYOUT: usize = 0x00;
    pub const NAME: usize = 0x80;
    pub const TILES: usize = 0xA0;
    pub const CONVEYOR: usize = 0xD6;
    pub const RAMP: usize = 0xDA;
    pub const BORDER: usize = 0xDE;
    pub const ITEM: usize = 0xE1;
    pub const EXITS: usize = 0xE9;
    pub const ENTITIES: usize = 0xF0;
}

/// One of the six bitmaps a room carries: an attribute and eight pixel rows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tile {
    pub attr: u8,
    pub pixels: [u8; 8],
}

/// What a cell of the layout is made of. The layout's two-bit codes only reach
/// the first four; ramp and conveyor cells are found through their own
/// definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Background,
    Floor,
    Wall,
    Nasty,
    Ramp,
    Conveyor,
}

/// A conveyor or a ramp: which way it goes, where it starts, how long it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Run {
    /// Conveyor: 0 moves left, 1 moves right. Ramp: 0 climbs to the left, 1 to
    /// the right.
    pub direction: u8,
    /// Where it starts, as an address in the empty room's attribute buffer.
    pub addr: u16,
    pub length: u8,
}

impl Run {
    /// Row and column of the start, or `None` if the address is outside the
    /// playing area — which is how a room says it has no conveyor.
    pub fn start(&self) -> Option<(usize, usize)> {
        let offset = self.addr.checked_sub(ATTR_BACK)? as usize;
        if offset >= CELLS || self.length == 0 {
            return None;
        }
        Some((offset / COLUMNS, offset % COLUMNS))
    }
}

/// Which room lies in each direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Exits {
    pub left: u8,
    pub right: u8,
    pub up: u8,
    pub down: u8,
}

/// An entity slot: a definition number and the column it starts in. A room has
/// eight, and the list stops at the first 255.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EntitySlot {
    pub definition: u8,
    pub x: u8,
}

/// A room, decoded.
#[derive(Debug, Clone)]
pub struct Room {
    pub number: usize,
    /// The 32-byte name, as bytes: three of the 64 "rooms" hold code rather than
    /// a room, and their names are not text.
    pub name: [u8; 32],
    /// Two bits a cell, sixteen rows of thirty-two.
    pub layout: [u8; CELLS],
    pub tiles: [Tile; 6],
    pub conveyor: Run,
    pub ramp: Run,
    pub border: u8,
    pub item: [u8; 8],
    pub exits: Exits,
    pub entities: [EntitySlot; 8],
}

impl Room {
    /// Decode room `number`.
    ///
    /// # Panics
    ///
    /// If `number` is not a room.
    pub fn load(number: usize) -> Self {
        let bytes = &jsw_data::rooms::ROOMS[number];

        let mut layout = [0u8; CELLS];
        for (index, cell) in layout.iter_mut().enumerate() {
            // Four cells to a byte, most significant pair leftmost.
            let byte = bytes[offset::LAYOUT + index / 4];
            *cell = (byte >> (6 - 2 * (index % 4))) & 3;
        }

        let mut tiles = [Tile::default(); 6];
        for (index, tile) in tiles.iter_mut().enumerate() {
            let at = offset::TILES + index * 9;
            tile.attr = bytes[at];
            tile.pixels.copy_from_slice(&bytes[at + 1..at + 9]);
        }

        let mut name = [0u8; 32];
        name.copy_from_slice(&bytes[offset::NAME..offset::NAME + 32]);

        let mut item = [0u8; 8];
        item.copy_from_slice(&bytes[offset::ITEM..offset::ITEM + 8]);

        let mut entities = [EntitySlot::default(); 8];
        for (index, slot) in entities.iter_mut().enumerate() {
            let at = offset::ENTITIES + index * 2;
            *slot = EntitySlot {
                definition: bytes[at],
                x: bytes[at + 1],
            };
        }

        Self {
            number,
            name,
            layout,
            tiles,
            conveyor: run(bytes, offset::CONVEYOR),
            ramp: run(bytes, offset::RAMP),
            border: bytes[offset::BORDER],
            item,
            exits: Exits {
                left: bytes[offset::EXITS],
                right: bytes[offset::EXITS + 1],
                up: bytes[offset::EXITS + 2],
                down: bytes[offset::EXITS + 3],
            },
            entities,
        }
    }

    /// The name with the padding taken off, for anything that wants to print it
    /// outside the game. Non-text bytes come back as `?`.
    pub fn name_text(&self) -> String {
        self.name
            .iter()
            .map(|&b| if (32..127).contains(&b) { b as char } else { '?' })
            .collect::<String>()
            .trim()
            .to_owned()
    }

    /// What is at a cell, taking the ramp and conveyor into account.
    pub fn kind_at(&self, row: usize, column: usize) -> Kind {
        if self.ramp_cells().any(|cell| cell == (row, column)) {
            return Kind::Ramp;
        }
        if self.conveyor_cells().any(|cell| cell == (row, column)) {
            return Kind::Conveyor;
        }
        match self.layout[row * COLUMNS + column] {
            0 => Kind::Background,
            1 => Kind::Floor,
            2 => Kind::Wall,
            _ => Kind::Nasty,
        }
    }

    /// The cells the conveyor runs through, left to right.
    pub fn conveyor_cells(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        let start = self.conveyor.start();
        (0..self.conveyor.length as usize).filter_map(move |step| {
            let (row, column) = start?;
            let column = column + step;
            (column < COLUMNS).then_some((row, column))
        })
    }

    /// The cells the ramp climbs through, from its foot.
    pub fn ramp_cells(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        let start = self.ramp.start();
        let rightwards = self.ramp.direction == 1;
        (0..self.ramp.length as usize).filter_map(move |step| {
            let (row, column) = start?;
            // Each step of a ramp is one cell up and one cell along.
            let row = row.checked_sub(step)?;
            let column = if rightwards {
                column.checked_add(step)?
            } else {
                column.checked_sub(step)?
            };
            (column < COLUMNS).then_some((row, column))
        })
    }

    /// The tile a kind is drawn with.
    pub fn tile(&self, kind: Kind) -> Tile {
        self.tiles[match kind {
            Kind::Background => 0,
            Kind::Floor => 1,
            Kind::Wall => 2,
            Kind::Nasty => 3,
            Kind::Ramp => 4,
            Kind::Conveyor => 5,
        }]
    }

    /// Draw the room into the empty-room buffers, which every frame starts from.
    pub fn draw(&self, mem: &mut Memory) {
        for row in 0..ROWS {
            for column in 0..COLUMNS {
                let tile = self.tile(self.kind_at(row, column));
                mem.write(ATTR_BACK + (row * COLUMNS + column) as u16, tile.attr);
                for (pixel_row, &byte) in tile.pixels.iter().enumerate() {
                    let at = SCREEN_BACK + cell_offset(row, pixel_row, column) as u16;
                    mem.write(at, byte);
                }
            }
        }
    }
}

/// Read a conveyor or ramp definition: direction, address, length.
fn run(bytes: &[u8; 256], at: usize) -> Run {
    Run {
        direction: bytes[at],
        addr: u16::from(bytes[at + 1]) | (u16::from(bytes[at + 2]) << 8),
        length: bytes[at + 3],
    }
}

/// Address in the working attribute buffer of a cell.
#[inline]
pub fn attr_addr(row: usize, column: usize) -> u16 {
    ATTR_BUF + (row * COLUMNS + column) as u16
}

/// Split an address the way the original does, for code that walks buffers by
/// byte rather than by cell.
#[inline]
pub fn split(addr: u16) -> (u8, u8) {
    (msb(addr), lsb(addr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_room_is_the_off_licence() {
        let room = Room::load(0);
        assert_eq!(room.name_text(), "The Off Licence");
    }

    #[test]
    fn every_room_decodes() {
        for number in 0..jsw_data::ROOM_COUNT {
            let room = Room::load(number);
            assert_eq!(room.number, number);
            // Every cell holds one of the four layout codes.
            assert!(room.layout.iter().all(|&code| code < 4));
        }
    }

    #[test]
    fn the_off_licence_leads_left_to_the_bridge() {
        let room = Room::load(0);
        assert_eq!(room.exits.left, 1);
        assert_eq!(Room::load(1).name_text(), "The Bridge");
        // It has no exit the other three ways, which the original spells as
        // "back to room 0".
        assert_eq!(room.exits.right, 0);
        assert_eq!(room.exits.up, 0);
        assert_eq!(room.exits.down, 0);
    }

    #[test]
    fn the_off_licence_has_a_conveyor_and_a_ramp() {
        let room = Room::load(0);
        // Conveyor at (9,19), twelve cells long, moving left.
        assert_eq!(room.conveyor.start(), Some((9, 19)));
        assert_eq!(room.conveyor.length, 12);
        assert_eq!(room.conveyor.direction, 0);
        assert_eq!(room.conveyor_cells().count(), 12);

        // Ramp at (14,23), four cells, climbing to the right.
        assert_eq!(room.ramp.start(), Some((14, 23)));
        assert_eq!(room.ramp.direction, 1);
        assert_eq!(
            room.ramp_cells().collect::<Vec<_>>(),
            vec![(14, 23), (13, 24), (12, 25), (11, 26)]
        );
    }

    #[test]
    fn a_ramp_cell_is_a_ramp_whatever_the_layout_says() {
        let room = Room::load(0);
        assert_eq!(room.kind_at(14, 23), Kind::Ramp);
        assert_eq!(room.kind_at(9, 19), Kind::Conveyor);
    }

    #[test]
    fn drawing_a_room_fills_both_empty_room_buffers() {
        let room = Room::load(0);
        let mut mem = Memory::new();
        room.draw(&mut mem);

        // Every cell carries the attribute of whatever it is made of. The Off
        // Licence's background attribute is zero, so counting non-zero cells
        // would say almost nothing.
        for row in 0..ROWS {
            for column in 0..COLUMNS {
                let tile = room.tile(room.kind_at(row, column));
                assert_eq!(
                    mem.read(ATTR_BACK + (row * COLUMNS + column) as u16),
                    tile.attr,
                    "attribute at ({row},{column})"
                );
                for (pixel_row, &byte) in tile.pixels.iter().enumerate() {
                    assert_eq!(
                        mem.read(SCREEN_BACK + cell_offset(row, pixel_row, column) as u16),
                        byte,
                        "pixels at ({row},{column}) row {pixel_row}"
                    );
                }
            }
        }

        // The bottom row of The Off Licence is floor, which is drawn.
        let floor = room.tile(Kind::Floor);
        assert_eq!(room.kind_at(15, 0), Kind::Floor);
        assert_ne!(floor.pixels, [0; 8]);
    }

    #[test]
    fn every_room_draws_without_leaving_the_buffers() {
        // Room::draw writes by computed address; this catches an arithmetic
        // slip in cell_offset before it turns into corrupted memory elsewhere.
        for number in 0..jsw_data::ROOM_COUNT {
            let mut mem = Memory::new();
            Room::load(number).draw(&mut mem);
            assert_eq!(mem.read(SCREEN_BACK - 1), 0, "room {number} wrote too low");
            assert_eq!(
                mem.read(SCREEN_BACK + 4096),
                0,
                "room {number} wrote past its buffer"
            );
        }
    }
}
