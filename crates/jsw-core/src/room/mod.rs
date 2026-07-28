//! A room of the mansion, decoded from its 256 bytes.
//!
//! Every room carries its own tile graphics, which is the big difference from
//! Manic Miner: there the cavern's layout named tiles from a fixed set, here the
//! six bitmaps travel with the room. That is why the mansion's rooms look so
//! unlike each other.

pub mod jsw2;

use speccy::layout::{ATTR_BACK, ATTR_BUF, COLUMNS, ROWS, SCREEN_BACK, cell_offset};
use speccy::memory::{Memory, add_lsb, lsb, msb};

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
    /// The same name, trimmed and with anything unprintable as `?`, for
    /// anything outside the game that wants to say which room this is.
    pub title: String,
    /// Two bits a cell, sixteen rows of thirty-two.
    pub layout: [u8; CELLS],
    pub tiles: [Tile; 6],
    pub conveyor: Run,
    pub ramp: Run,
    pub border: u8,
    pub item: [u8; 8],
    pub exits: Exits,
    pub entities: [EntitySlot; 8],
    /// Jet Set Willy II's own cell types, one per cell, when this is one of its
    /// rooms: 0 air, 1 water, 2 earth, 3 fire, 4 forward ramp, 5 left conveyor,
    /// 6 item, 7 back ramp, 8 right conveyor.
    ///
    /// Jet Set Willy has nothing like it, and does not need one: there, what a
    /// cell is can be read off its colour, because a room's six tiles have six
    /// distinct attributes. Jet Set Willy II draws its rooms from one shared
    /// table of 512 cell graphics, so colours repeat - in 47 of its rooms the
    /// deadly tile is the same colour as a floor or a wall - and the only
    /// reliable answer is the type the room was stored with.
    pub types: Option<[u8; CELLS]>,
    /// The graphic for each of those nine types, when [`Room::types`] is set.
    /// Air has none: it is left black.
    type_tiles: Option<[Tile; 8]>,
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
            title: title_of(&name),
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
            types: None,
            type_tiles: None,
        }
    }

    /// Decode Jet Set Willy II's room `number`.
    ///
    /// The room comes out as the same `Room` Jet Set Willy's does, so
    /// everything downstream - drawing, walking, the conveyor, the map - is
    /// shared. Only the reading differs, and that is in [`jsw2`].
    ///
    /// The nine cell types map onto the six kinds the engine knows: air is
    /// background, water is floor, earth is wall, fire is nasty, both ramps are
    /// ramp and both conveyors are conveyor. An item cell is background with an
    /// item standing in it.
    #[must_use]
    pub fn load_jsw2(number: usize) -> Self {
        let read = jsw2::entry(number);

        let mut layout = [0u8; CELLS];
        for (cell, &kind) in layout.iter_mut().zip(read.cells.iter()) {
            *cell = match kind {
                1 => 1,
                2 => 2,
                3 => 3,
                _ => 0,
            };
        }

        // The eight slots start at water, not at air: air has no graphic at all
        // and is left black, which is why a room's background is the paper the
        // Spectrum starts with. So slot N draws cell type N + 1.
        let type_tiles: [Tile; 8] =
            std::array::from_fn(|slot| jsw2::cell_graphic(read.patterns[slot]));

        // The six the engine names, for anything that still asks that way. The
        // ramp and the conveyor take the forward and left-hand graphics; their
        // opposite numbers are drawn from [`Room::type_tiles`].
        let tiles = [
            Tile::default(),
            type_tiles[0],
            type_tiles[1],
            type_tiles[2],
            type_tiles[3],
            type_tiles[4],
        ];

        let mut name = [32u8; 32];
        for (slot, byte) in name.iter_mut().zip(read.name.bytes()) {
            *slot = byte;
        }

        Self {
            number,
            title: read.name.clone(),
            name,
            layout,
            tiles,
            conveyor: conveyor_of(&read.cells),
            ramp: ramp_of(&read.cells),
            border: read.border,
            item: type_tiles[5].pixels,
            exits: read.exits,
            entities: [EntitySlot::default(); 8],
            types: Some(read.cells),
            type_tiles: Some(type_tiles),
        }
    }

    /// Whether this is a room at all.
    ///
    /// Three of the sixty-four 256-byte blocks hold code and leftovers rather
    /// than rooms, so their "names" are not text. The game never takes Willy
    /// there; only a debug jump can reach them.
    pub fn is_real(&self) -> bool {
        self.name.iter().all(|&b| b == 32 || (33..127).contains(&b))
    }

    /// What is at a cell, taking the ramp and conveyor into account.
    ///
    /// A Jet Set Willy II room answers from the types it was stored with, which
    /// is the only way to get a room with two ramps in it right: Jet Set
    /// Willy's model has room for one of each, and the second would be lost.
    pub fn kind_at(&self, row: usize, column: usize) -> Kind {
        if let Some(types) = &self.types {
            return Self::kind_of_type(types[row * COLUMNS + column]);
        }
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

    /// Which of the engine's six kinds one of Jet Set Willy II's nine cell
    /// types is. A cell with an item in it is walked through like any other
    /// background cell.
    fn kind_of_type(cell: u8) -> Kind {
        match cell {
            1 => Kind::Floor,
            2 => Kind::Wall,
            3 => Kind::Nasty,
            4 | 7 => Kind::Ramp,
            5 | 8 => Kind::Conveyor,
            _ => Kind::Background,
        }
    }

    /// Whether the cell at (`row`, `column`) is `kind`.
    ///
    /// This is the question the engine actually asks, and the two games answer
    /// it differently. Jet Set Willy compares the colour in the working buffer
    /// against the room's tile, which is what the original does at 36406 and
    /// everywhere else it tests the ground. Jet Set Willy II cannot: its rooms
    /// come from a shared table of cell graphics and their colours repeat, so
    /// in 47 rooms the deadly tile is the colour of a floor. It answers from
    /// the cell types instead.
    #[must_use]
    pub fn is_at(&self, mem: &speccy::Memory, row: usize, column: usize, kind: Kind) -> bool {
        if row >= ROWS || column >= COLUMNS {
            return false;
        }
        if self.types.is_some() {
            return self.kind_at(row, column) == kind;
        }
        let at = ATTR_BUF + (row * COLUMNS + column) as u16;
        mem.read(at) == self.tile(kind).attr
    }

    /// Which way the conveyor under a cell runs, if there is one there: 0 left,
    /// 1 right. A Jet Set Willy II room can have several, going different ways.
    #[must_use]
    pub fn conveyor_direction_at(&self, row: usize, column: usize) -> Option<u8> {
        match &self.types {
            Some(types) => match types[row * COLUMNS + column] {
                5 => Some(0),
                8 => Some(1),
                _ => None,
            },
            None => Some(self.conveyor.direction),
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

    /// Wind the conveyor on one frame: the routine at 39707.
    ///
    /// Only two of the eight pixel rows of the conveyor tile move - the first
    /// and the third - and they move in opposite directions, two pixels each, so
    /// the belt appears to run one way along the top and back along the bottom.
    /// The two rows are read from the leftmost tile and then written to every
    /// tile of the run, which is why a conveyor is always in step with itself.
    ///
    /// It works on the empty-room buffer rather than the working one, so the
    /// belt keeps turning without being redrawn every frame.
    pub fn move_conveyor(&self, mem: &mut speccy::Memory) {
        if self.conveyor.length == 0 {
            return;
        }
        let Some((row, column)) = self.conveyor.start() else {
            return;
        };

        let top = SCREEN_BACK + cell_offset(row, 0, column) as u16;
        let bottom = SCREEN_BACK + cell_offset(row, 2, column) as u16;
        let rightwards = self.conveyor.direction != 0;
        let (top_row, bottom_row) = if rightwards {
            (
                mem.read(top).rotate_right(2),
                mem.read(bottom).rotate_left(2),
            )
        } else {
            (
                mem.read(top).rotate_left(2),
                mem.read(bottom).rotate_right(2),
            )
        };

        // The original walks the run with INC L, so a conveyor that reaches the
        // right-hand edge carries on at the left of the same pixel row rather
        // than moving down a row.
        for step in 0..self.conveyor.length {
            mem.write(add_lsb(top, step), top_row);
            mem.write(add_lsb(bottom, step), bottom_row);
        }
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

    /// The graphic a cell is drawn with.
    ///
    /// Jet Set Willy II has eight of them to the engine's six, because its two
    /// ramps and its two conveyors are drawn differently from each other even
    /// though they behave the same way.
    #[must_use]
    pub fn tile_at(&self, row: usize, column: usize) -> Tile {
        match (&self.types, &self.type_tiles) {
            (Some(types), Some(tiles)) => match types[row * COLUMNS + column] {
                0 => Tile::default(),
                cell => tiles[cell as usize - 1],
            },
            _ => self.tile(self.kind_at(row, column)),
        }
    }

    /// Draw the room into the empty-room buffers, which every frame starts from.
    pub fn draw(&self, mem: &mut Memory) {
        for row in 0..ROWS {
            for column in 0..COLUMNS {
                let tile = self.tile_at(row, column);
                mem.write(ATTR_BACK + (row * COLUMNS + column) as u16, tile.attr);
                for (pixel_row, &byte) in tile.pixels.iter().enumerate() {
                    let at = SCREEN_BACK + cell_offset(row, pixel_row, column) as u16;
                    mem.write(at, byte);
                }
            }
        }
    }
}

/// A room name with the padding taken off. Non-text bytes come back as `?`,
/// because three of the 64 rooms hold code rather than a room.
/// The conveyor a Jet Set Willy II room draws as cell types, as a [`Run`].
///
/// Jet Set Willy names its conveyor in the room definition; Jet Set Willy II
/// paints it into the shape instead, so the run has to be found. A conveyor
/// lies along a row, and its cell type says which way it moves.
fn conveyor_of(cells: &[u8; CELLS]) -> Run {
    let Some(start) = cells.iter().position(|&cell| cell == 5 || cell == 8) else {
        return Run::default();
    };
    let kind = cells[start];
    let length = cells[start..]
        .iter()
        .take_while(|&&cell| cell == kind)
        .count()
        .min(COLUMNS - start % COLUMNS);

    Run {
        direction: u8::from(kind == 8),
        addr: ATTR_BACK + start as u16,
        length: length as u8,
    }
}

/// The ramp, the same way - but a ramp climbs a cell at a time, so its cells
/// are a diagonal rather than a row and counting along the shape would always
/// find a run of one.
///
/// [`Run`] names a ramp by its foot, which is its lowest cell, and the engine
/// walks upwards from there. Which way it leans is decided by looking at the
/// cell one row up: the game's two ramp types are drawn the same way round in
/// every room, but reading the shape costs nothing and cannot be wrong.
fn ramp_of(cells: &[u8; CELLS]) -> Run {
    let is_ramp = |row: usize, column: usize| {
        row < ROWS && column < COLUMNS && matches!(cells[row * COLUMNS + column], 4 | 7)
    };
    let Some(foot) = (0..CELLS).rev().find(|&cell| matches!(cells[cell], 4 | 7)) else {
        return Run::default();
    };
    let (row, column) = (foot / COLUMNS, foot % COLUMNS);

    // One cell up and one along, whichever way the next cell of the ramp is.
    let rightwards = column + 1 < COLUMNS && is_ramp(row.wrapping_sub(1), column + 1);
    let mut length = 1u8;
    let mut at = (row, column);
    while length < ROWS as u8 {
        let Some(up) = at.0.checked_sub(1) else { break };
        let along = if rightwards {
            at.1 + 1
        } else {
            match at.1.checked_sub(1) {
                Some(column) => column,
                None => break,
            }
        };
        if !is_ramp(up, along) {
            break;
        }
        at = (up, along);
        length += 1;
    }

    Run {
        direction: u8::from(rightwards),
        addr: ATTR_BACK + foot as u16,
        length,
    }
}

fn title_of(name: &[u8; 32]) -> String {
    name.iter()
        .map(|&b| {
            if (32..127).contains(&b) {
                b as char
            } else {
                '?'
            }
        })
        .collect::<String>()
        .trim()
        .to_owned()
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
        assert_eq!(room.title, "The Off Licence");
    }

    #[test]
    fn the_last_three_blocks_are_not_rooms() {
        // 61 to 63 hold code, which is why their names read as rubbish.
        for number in 0..jsw_data::ROOM_COUNT {
            let room = Room::load(number);
            assert_eq!(
                room.is_real(),
                number < 61,
                "room {number} named {:?}",
                room.title
            );
        }
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
        assert_eq!(Room::load(1).title, "The Bridge");
        // It has no exit the other three ways, which the original spells as
        // "back to room 0".
        assert_eq!(room.exits.right, 0);
        assert_eq!(room.exits.up, 0);
        assert_eq!(room.exits.down, 0);
    }

    #[test]
    fn a_conveyor_belt_turns_two_pixels_a_frame_each_way() {
        // The Off Licence's belt moves left, so the top pixel row of the tile
        // shifts left and the third row shifts right.
        let room = Room::load(0);
        assert_eq!(room.conveyor.direction, 0);
        let (row, column) = room.conveyor.start().expect("it has a conveyor");

        let mut mem = Memory::new();
        room.draw(&mut mem);
        let top = SCREEN_BACK + cell_offset(row, 0, column) as u16;
        let bottom = SCREEN_BACK + cell_offset(row, 2, column) as u16;
        let (was_top, was_bottom) = (mem.read(top), mem.read(bottom));

        room.move_conveyor(&mut mem);
        assert_eq!(mem.read(top), was_top.rotate_left(2));
        assert_eq!(mem.read(bottom), was_bottom.rotate_right(2));

        // Every tile of the run is left holding the same two rows.
        for step in 0..room.conveyor.length {
            assert_eq!(mem.read(add_lsb(top, step)), was_top.rotate_left(2));
            assert_eq!(mem.read(add_lsb(bottom, step)), was_bottom.rotate_right(2));
        }

        // Four frames of two pixels bring it back where it started.
        for _ in 0..3 {
            room.move_conveyor(&mut mem);
        }
        assert_eq!(mem.read(top), was_top, "the belt did not come full circle");
    }

    #[test]
    fn the_other_pixel_rows_of_a_conveyor_stay_where_they_are() {
        let room = Room::load(0);
        let (row, column) = room.conveyor.start().expect("it has a conveyor");
        let mut mem = Memory::new();
        room.draw(&mut mem);

        let still: Vec<u8> = [1, 3, 4, 5, 6, 7]
            .iter()
            .map(|&pixel_row| mem.read(SCREEN_BACK + cell_offset(row, pixel_row, column) as u16))
            .collect();
        room.move_conveyor(&mut mem);
        for (n, &pixel_row) in [1, 3, 4, 5, 6, 7].iter().enumerate() {
            assert_eq!(
                mem.read(SCREEN_BACK + cell_offset(row, pixel_row, column) as u16),
                still[n],
                "pixel row {pixel_row} moved"
            );
        }
    }

    #[test]
    fn a_room_without_a_conveyor_is_left_alone() {
        // Under the MegaTree has no belt.
        let room = Room::load(2);
        assert_eq!(room.conveyor.length, 0);
        let mut mem = Memory::new();
        room.draw(&mut mem);
        let before: Vec<u8> = (0..4096).map(|i| mem.read(SCREEN_BACK + i)).collect();
        room.move_conveyor(&mut mem);
        let after: Vec<u8> = (0..4096).map(|i| mem.read(SCREEN_BACK + i)).collect();
        assert_eq!(before, after);
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

    #[test]
    fn every_jsw2_room_loads_and_draws_inside_the_buffers() {
        for number in 0..jsw2_data::ROOM_COUNT {
            let room = Room::load_jsw2(number);
            assert_eq!(room.number, number);

            let mut mem = Memory::new();
            room.draw(&mut mem);
            assert_eq!(mem.read(SCREEN_BACK - 1), 0, "room {number} wrote too low");
            assert_eq!(
                mem.read(SCREEN_BACK + 4096),
                0,
                "room {number} wrote past its buffer"
            );
        }
    }

    #[test]
    fn the_first_jsw2_room_is_the_off_licence_with_a_floor() {
        let room = Room::load_jsw2(0);
        assert_eq!(room.title, "The Off Licence");
        // Willy has to be able to stand on the bottom row.
        assert!(
            (0..COLUMNS).any(|column| room.kind_at(15, column) == Kind::Floor),
            "nothing to stand on in room 0"
        );
    }

    #[test]
    fn a_jsw2_room_keeps_both_its_ramps_and_invents_no_conveyor() {
        // Macaroni Ted has two ramps - one climbing each way - and no conveyor
        // at all. Jet Set Willy's model holds one ramp and one conveyor, so the
        // second ramp used to vanish, and the room's floor was shoved sideways
        // because its colour matched the conveyor graphic the room never uses.
        let room = Room::load_jsw2(62);
        assert_eq!(room.title, "Macaroni Ted");

        let ramps = (0..ROWS)
            .flat_map(|row| (0..COLUMNS).map(move |column| (row, column)))
            .filter(|&(row, column)| room.kind_at(row, column) == Kind::Ramp)
            .count();
        assert_eq!(ramps, 10, "both ramps should be there, five cells each");

        let mem = Memory::new();
        for column in 0..COLUMNS {
            assert!(
                !room.is_at(&mem, 15, column, Kind::Conveyor),
                "the floor at column {column} is not a conveyor"
            );
        }
    }

    #[test]
    fn a_jsw2_floor_is_not_deadly_just_because_it_shares_a_colour() {
        // First Landing's fire tile is the same colour as its water tile, so
        // reading the colour made its floor kill Willy on contact.
        let room = Room::load_jsw2(26);
        assert_eq!(room.title, "First Landing");
        assert_eq!(
            room.tile(Kind::Nasty).attr,
            room.tile(Kind::Floor).attr,
            "this test is pointless unless the two really do share a colour"
        );

        let mem = Memory::new();
        let floors = (0..ROWS)
            .flat_map(|row| (0..COLUMNS).map(move |column| (row, column)))
            .filter(|&(row, column)| room.kind_at(row, column) == Kind::Floor);
        for (row, column) in floors {
            assert!(
                !room.is_at(&mem, row, column, Kind::Nasty),
                "the floor at ({row},{column}) kills him"
            );
        }
    }
}
