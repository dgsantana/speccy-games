//! The eighty-three things Willy has to tidy away.
//!
//! Ported from the routine at 37841. The item table is two halves: the entry for
//! item N is a byte at 41984 + N and another at 42240 + N. The first holds the
//! room number in bits 0 to 5, a collection flag in bit 6 that is *set* while
//! the item is still there, and the top bit of its position in bit 7. The second
//! is the low byte of its address in the buffers.
//!
//! Collection needs no geometry at all. Willy is drawn first, and his drawing
//! forces white ink into every background cell he covers, so an item whose cell
//! has white ink is an item he is standing in. That is how the original does it,
//! and it is why a room whose background is already white ink - the Swimming
//! Pool - hands over its items the moment you walk in.

use speccy::layout::ATTR_BUF;
use speccy::memory::{Memory, addr_of, next_pixel_row};

/// The lowest numbered item. Anything below this is not in the game.
pub const FIRST: usize = jsw_data::items::FIRST_ITEM;

/// Items in the mansion.
pub const COUNT: usize = 256 - FIRST;

/// The item table, as the game keeps it: the flags change as items are taken.
#[derive(Debug, Clone)]
pub struct Items {
    /// The first half of the table, whose bit 6 is the collection flag.
    flags: [u8; 256],
    /// The second half: the low byte of each item's address.
    places: [u8; 256],
    /// How many Willy has collected.
    pub collected: usize,
}

impl Default for Items {
    fn default() -> Self {
        Self::new()
    }
}

impl Items {
    /// A fresh table with every item still to be found, which is what the
    /// routine at 34825 makes by setting bit 6 of every entry.
    pub fn new() -> Self {
        let mut flags = jsw_data::items::ITEM_TABLE[0];
        for entry in flags.iter_mut().skip(FIRST) {
            *entry |= 64;
        }
        Self {
            flags,
            places: jsw_data::items::ITEM_TABLE[1],
            collected: 0,
        }
    }

    /// How many are still out there.
    pub fn remaining(&self) -> usize {
        (FIRST..256).filter(|&n| self.flags[n] & 64 != 0).count()
    }

    /// Whether a particular item is still uncollected, for tests.
    pub fn present(&self, item: usize) -> bool {
        self.flags[item] & 64 != 0
    }

    /// Draw the items in this room and collect the one Willy is touching.
    ///
    /// `minute` is the game's minute counter, which the original mixes with the
    /// item's index to make the colours cycle. Returns how many were collected,
    /// so the caller can make the noise.
    pub fn draw(&mut self, room: usize, minute: u8, graphic: &[u8; 8], mem: &mut Memory) -> usize {
        let mut taken = 0;
        for item in FIRST..256 {
            // The room number and the collection flag are compared in one go.
            if self.flags[item] & 127 != (room as u8 | 64) {
                continue;
            }

            let attr = addr_of(92 + ((self.flags[item] >> 7) & 1), self.places[item]);
            if mem.read(attr) & 7 == 7 {
                // White ink under the item: Willy is standing in it.
                self.flags[item] &= !64;
                self.collected += 1;
                taken += 1;
                continue;
            }

            // The ink cycles with the clock so the items twinkle.
            let ink = (minute.wrapping_add(item as u8) & 3) + 3;
            let was = mem.read(attr);
            mem.write(attr, (was & 248) | ink);

            // Bit 3 of the screen address's high byte comes from bit 7 of the
            // same flags byte the attribute page came from.
            let page = 96 + ((self.flags[item] >> 3) & 8);
            let mut at = addr_of(page, self.places[item]);
            for &byte in graphic {
                mem.write(at, byte);
                at = next_pixel_row(at);
            }
        }
        taken
    }

    /// Where an item is, as a cell in the playing area, for tests.
    pub fn cell_of(&self, item: usize) -> u16 {
        addr_of(92 + ((self.flags[item] >> 7) & 1), self.places[item])
    }

    /// The room an item is in.
    pub fn room_of(&self, item: usize) -> usize {
        usize::from(self.flags[item] & 63)
    }
}

/// Whether an address is a cell of the playing area's attribute buffer.
pub fn in_play(attr: u16) -> bool {
    (ATTR_BUF..ATTR_BUF + 512).contains(&attr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::room::Room;

    #[test]
    fn a_new_game_has_eighty_three_items_to_find() {
        let items = Items::new();
        assert_eq!(COUNT, 83);
        assert_eq!(items.remaining(), 83);
        assert_eq!(items.collected, 0);
    }

    #[test]
    fn the_first_item_is_in_the_watch_tower() {
        // The disassembly says item 173 is at (8,25) in room 50, Watch Tower.
        let items = Items::new();
        assert_eq!(items.room_of(FIRST), 50);
        assert_eq!(Room::load(50).title, "Watch Tower");
    }

    #[test]
    fn every_item_lands_inside_the_playing_area() {
        let items = Items::new();
        for item in FIRST..256 {
            assert!(
                in_play(items.cell_of(item)),
                "item {item} sits outside the playing area"
            );
        }
    }

    #[test]
    fn an_item_is_drawn_into_the_room_it_belongs_to() {
        let mut items = Items::new();
        let room = Room::load(50);
        let mut mem = Memory::new();
        room.draw(&mut mem);
        mem.copy(speccy::layout::ATTR_BACK, ATTR_BUF, speccy::layout::PLAY_ATTRS);

        let before: u32 = (0..4096)
            .map(|i| mem.read(speccy::layout::SCREEN_BUF + i).count_ones())
            .sum();
        let taken = items.draw(50, 0, &room.item, &mut mem);
        let after: u32 = (0..4096)
            .map(|i| mem.read(speccy::layout::SCREEN_BUF + i).count_ones())
            .sum();

        assert_eq!(taken, 0, "nothing should be collected without Willy there");
        assert!(after > before, "no item was drawn in the Watch Tower");
    }

    #[test]
    fn white_ink_under_an_item_collects_it() {
        let mut items = Items::new();
        let room = Room::load(50);
        let mut mem = Memory::new();
        room.draw(&mut mem);
        mem.copy(speccy::layout::ATTR_BACK, ATTR_BUF, speccy::layout::PLAY_ATTRS);

        // Willy's drawing would have forced white ink here.
        let item = (FIRST..256).find(|&n| items.room_of(n) == 50).expect("one");
        let cell = items.cell_of(item);
        let was = mem.read(cell);
        mem.write(cell, (was & 248) | 7);

        let taken = items.draw(50, 0, &room.item, &mut mem);
        assert_eq!(taken, 1);
        assert!(!items.present(item), "the item is still there");
        assert_eq!(items.collected, 1);
        assert_eq!(items.remaining(), 82);
    }

    #[test]
    fn an_item_in_another_room_is_left_alone() {
        let mut items = Items::new();
        let room = Room::load(0);
        let mut mem = Memory::new();
        room.draw(&mut mem);
        items.draw(0, 0, &room.item, &mut mem);
        // The Watch Tower's items are untouched.
        assert!(items.present(FIRST));
    }
}
