//! The things in a room that move: guardians, and later ropes and arrows.
//!
//! A room names up to eight of them, each as a definition number and a column.
//! Entering a room copies the eight-byte definition into a buffer and drops the
//! column into it, which is the routine at 35120. From then on the buffer is the
//! entity: [`Entities::step`] is the mover at 37056 and [`Entities::draw`] the
//! drawer at 37310.
//!
//! Ropes and arrows are recognised and left alone for now.

use speccy::layout::{ATTR_BUF, COLUMNS, ROWS};
use speccy::memory::{DrawMode, Memory, addr_of};

use crate::room::Room;

/// Entity slots in a room.
pub const SLOTS: usize = 8;

/// Bytes in an entity buffer.
pub const BUFFER: usize = 8;

/// Where the guardian graphics start, so a sprite page byte can be turned into
/// an index into the table.
const GRAPHICS_BASE: usize = 0xAB00;

/// What an entity is, from bits 0 to 2 of its first byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The slot is not in use.
    Unused,
    Horizontal,
    Vertical,
    Rope,
    Arrow,
    /// A value the original would treat as a guardian without being either.
    Other,
}

impl Kind {
    /// The kind of an entity from its first byte.
    pub fn of_public(first: u8) -> Self {
        Self::of(first)
    }

    fn of(first: u8) -> Self {
        match first & 7 {
            0 => Self::Unused,
            1 => Self::Horizontal,
            2 => Self::Vertical,
            3 => Self::Rope,
            4 => Self::Arrow,
            _ => Self::Other,
        }
    }
}

/// The eight entity buffers of the current room.
#[derive(Debug, Clone)]
pub struct Entities {
    /// Eight buffers of eight bytes. A first byte of 255 ends the list.
    pub buffers: [[u8; BUFFER]; SLOTS],
    /// Debug switch: leave every guardian still and harmless.
    #[cfg(feature = "debug")]
    pub disabled: bool,
}

impl Default for Entities {
    fn default() -> Self {
        Self {
            buffers: [[0; BUFFER]; SLOTS],
            #[cfg(feature = "debug")]
            disabled: false,
        }
    }
}

impl Entities {
    /// Fill the buffers from a room's entity list, as the original does on
    /// entering a room.
    pub fn load(room: &Room) -> Self {
        let mut entities = Self::default();
        for (slot, spec) in room.entities.iter().enumerate() {
            if spec.definition == 255 {
                // The list ends here; the original marks the buffer so the
                // movement and drawing loops stop too.
                entities.buffers[slot][0] = 255;
                break;
            }
            let definition = jsw_data::entities::ENTITY_DEFS[spec.definition as usize];
            entities.buffers[slot] = definition;
            // The room's column replaces the definition's, which is why one
            // definition can be used by several rooms.
            entities.buffers[slot][2] = spec.x;
        }
        entities
    }

    /// Whether the guardians are running. Always true unless the `debug`
    /// feature is on and they have been switched off.
    #[inline]
    pub fn active(&self) -> bool {
        #[cfg(feature = "debug")]
        {
            !self.disabled
        }
        #[cfg(not(feature = "debug"))]
        {
            true
        }
    }

    /// The buffers that are in use, stopping at the terminator.
    fn live(&self) -> impl Iterator<Item = (usize, &[u8; BUFFER])> {
        self.buffers
            .iter()
            .enumerate()
            .take_while(|(_, buffer)| buffer[0] != 255)
    }

    /// Move every guardian one frame: the routine at 37056.
    pub fn step(&mut self) {
        if !self.active() {
            return;
        }
        for slot in 0..SLOTS {
            if self.buffers[slot][0] == 255 {
                break;
            }
            // The mover keeps only bits 0 and 1, so an arrow or an unused slot
            // reads as zero here and is skipped.
            match self.buffers[slot][0] & 3 {
                1 => step_horizontal(&mut self.buffers[slot]),
                2 => step_vertical(&mut self.buffers[slot]),
                // Ropes swing; not yet ported.
                _ => {}
            }
        }
    }

    /// Draw every guardian, reporting whether one of them touched Willy: the
    /// routine at 37310.
    pub fn draw(&self, mem: &mut Memory) -> bool {
        if !self.active() {
            return false;
        }
        let mut hit = false;
        for (_, buffer) in self.live() {
            match Kind::of(buffer[0]) {
                Kind::Horizontal | Kind::Vertical | Kind::Other => {
                    hit |= draw_guardian(buffer, mem);
                }
                // Ropes and arrows are drawn by their own routines, which are
                // not written yet.
                Kind::Rope | Kind::Arrow | Kind::Unused => {}
            }
        }
        hit
    }

    /// What each slot holds, for tests and for the room dumper.
    pub fn kinds(&self) -> Vec<Kind> {
        self.live().map(|(_, b)| Kind::of(b[0])).collect()
    }
}

/// A horizontal guardian walks its column range, four animation frames a cell.
fn step_horizontal(buffer: &mut [u8; BUFFER]) {
    if buffer[0] & 128 == 0 {
        // Moving right to left.
        buffer[0] = buffer[0].wrapping_sub(32) & 127;
        if buffer[0] < 96 {
            return;
        }
        if buffer[2] & 31 == buffer[6] {
            buffer[0] = 129;
        } else {
            buffer[2] = buffer[2].wrapping_sub(1);
        }
    } else {
        // Moving left to right.
        buffer[0] = buffer[0].wrapping_add(32) | 128;
        if buffer[0] >= 160 {
            return;
        }
        if buffer[2] & 31 == buffer[7] {
            buffer[0] = 97;
        } else {
            buffer[2] = buffer[2].wrapping_add(1);
        }
    }
}

/// A vertical guardian bobs between two heights, reversing at each end.
fn step_vertical(buffer: &mut [u8; BUFFER]) {
    // Bit 3 flips every pass; with bit 4 set the frame advances every pass,
    // otherwise every second one.
    buffer[0] ^= 8;
    if buffer[0] & 24 != 0 {
        buffer[0] = buffer[0].wrapping_add(32);
    }

    buffer[3] = buffer[3].wrapping_add(buffer[4]);
    if buffer[3] >= buffer[7] {
        buffer[4] = buffer[4].wrapping_neg();
        return;
    }
    if buffer[3] == buffer[6] {
        buffer[3] = buffer[6];
        buffer[4] = buffer[4].wrapping_neg();
        return;
    }
    if buffer[3] > buffer[6] {
        return;
    }
    buffer[3] = buffer[6];
    buffer[4] = buffer[4].wrapping_neg();
}

/// Colour a guardian's cells and draw its sprite, reporting a collision.
fn draw_guardian(buffer: &[u8; BUFFER], mem: &mut Memory) -> bool {
    let y = buffer[3];
    let x = buffer[2] & 31;

    // The attribute address comes from the same table the screen address does.
    let low = jsw_data::entities::SCREEN_TABLE[y as usize].wrapping_add(x);
    let high = 92 | (y >> 7);
    let attr = addr_of(high, low);

    // Ink and bright from the buffer's colour nibble; paper is left alone.
    let colour = ((buffer[1] & 15).wrapping_add(56)) & 71;
    let rows = if y & 14 == 0 { 2 } else { 3 };
    for row in 0..rows {
        let at = attr.wrapping_add(row * 32);
        if !in_play(at) {
            continue;
        }
        mem.write(at, colour);
        mem.write(at.wrapping_add(1), colour);
    }

    // The animation frame is masked out of the first byte and merged with the
    // base sprite index from the third.
    let frame = ((buffer[1] & buffer[0]) | buffer[2]) & 224;
    let start = usize::from(buffer[5]) * 256 + usize::from(frame);
    let Some(offset) = start.checked_sub(GRAPHICS_BASE) else {
        return false;
    };
    if offset + 32 > jsw_data::sprites::GUARDIANS.len() {
        return false;
    }
    let sprite: [u8; 32] = jsw_data::sprites::GUARDIANS[offset..offset + 32]
        .try_into()
        .expect("32 bytes");

    let screen = addr_of(
        jsw_data::entities::SCREEN_TABLE[y.wrapping_add(1) as usize],
        jsw_data::entities::SCREEN_TABLE[y as usize] | x,
    );
    mem.draw_16x16(&sprite, screen, DrawMode::Blend)
}

/// Whether an attribute address is inside the playing area, so a guardian
/// hanging off the bottom does not scribble on the status area.
fn in_play(attr: u16) -> bool {
    let Some(offset) = attr.checked_sub(ATTR_BUF) else {
        return false;
    };
    (offset as usize) < ROWS * COLUMNS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_off_licence_has_three_guardians() {
        // Its entity list is 10, 12, 44 and then the terminator.
        let entities = Entities::load(&Room::load(0));
        assert_eq!(
            entities.kinds(),
            vec![Kind::Vertical, Kind::Horizontal, Kind::Vertical]
        );
    }

    #[test]
    fn a_horizontal_guardian_turns_around_at_both_ends() {
        let room = Room::load(0);
        let mut entities = Entities::load(&room);
        let slot = 1; // The horizontal one.

        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..400 {
            entities.step();
            seen.insert(entities.buffers[slot][2] & 31);
        }
        let min = *seen.iter().next().expect("it moved");
        let max = *seen.iter().next_back().expect("it moved");
        assert!(max > min, "the guardian never moved");

        // It must stay inside the limits its definition gives.
        let buffer = entities.buffers[slot];
        assert!(min >= buffer[6], "it walked past its left limit");
        assert!(max <= buffer[7], "it walked past its right limit");
    }

    #[test]
    fn a_vertical_guardian_reverses_between_its_limits() {
        let room = Room::load(0);
        let mut entities = Entities::load(&room);
        let slot = 0; // The first vertical one.
        let (min, max) = (entities.buffers[slot][6], entities.buffers[slot][7]);

        let mut low = 255u8;
        let mut high = 0u8;
        for _ in 0..400 {
            entities.step();
            let y = entities.buffers[slot][3];
            low = low.min(y);
            high = high.max(y);
        }
        assert!(high > low, "the guardian never moved");
        assert!(low >= min, "it rose above its limit: {low} < {min}");
        assert!(high <= max, "it sank below its limit: {high} > {max}");
    }

    #[test]
    fn drawing_guardians_puts_ink_on_the_screen() {
        let room = Room::load(0);
        let entities = Entities::load(&room);
        let mut mem = Memory::new();
        room.draw(&mut mem);

        entities.draw(&mut mem);
        let coloured = (0..(ROWS * COLUMNS))
            .filter(|&cell| mem.read(ATTR_BUF + cell as u16) != 0)
            .count();
        assert!(coloured > 0, "no guardian was coloured in");
    }

    #[test]
    fn a_guardian_on_a_high_sprite_page_is_actually_drawn() {
        // First Landing's guardian keeps its graphics on page 180, well past the
        // 2048 bytes the data used to carry, so it was silently skipped: the
        // cells were coloured as it passed but nothing was ever drawn in them.
        let room = Room::load(28);
        let entities = Entities::load(&room);
        assert_eq!(entities.buffers[0][5], 180, "the page this test is about");

        let mut mem = Memory::new();
        room.draw(&mut mem);
        let before: u32 = (0..4096)
            .map(|i| mem.read(speccy::layout::SCREEN_BUF + i).count_ones())
            .sum();
        entities.draw(&mut mem);
        let after: u32 = (0..4096)
            .map(|i| mem.read(speccy::layout::SCREEN_BUF + i).count_ones())
            .sum();
        assert!(
            after > before,
            "the guardian coloured its cells but drew no pixels"
        );
    }

    #[test]
    fn every_room_moves_and_draws_without_panicking() {
        for number in 0..jsw_data::ROOM_COUNT {
            let room = Room::load(number);
            let mut entities = Entities::load(&room);
            let mut mem = Memory::new();
            room.draw(&mut mem);
            for _ in 0..40 {
                entities.step();
                entities.draw(&mut mem);
            }
        }
    }
}
