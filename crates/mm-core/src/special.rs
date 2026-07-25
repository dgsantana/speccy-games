//! The guardians that only appear in one or two caverns.
//!
//! Eugene patrols Eugene's Lair, the Skylabs crash into Skylab Landing Bay, the
//! Kong Beast waits above two caverns, and the Solar Power Generator fires a
//! light beam that burns air when it touches Willy.

use mm_data::TileKind;

use crate::cavern::Cavern;
use crate::guardian::{Guardians, attribute_address, set_guardian_attributes, sprite_address};
use crate::score::Score;
use crate::sound::SoundQueue;
use crate::speccy::{
    ATTR_BUF, DrawMode, Memory, SCREEN_BUF, addr_of, lsb, msb, rot_l, rot_r, screen_row_addr,
};
use crate::willy::{Willy, frame_at};

/// State shared by Eugene and the Kong Beast, which reuse the same two variables.
#[derive(Debug, Clone, Copy, Default)]
pub struct Specials {
    /// Eugene: 0 moving down, 1 moving up. Kong Beast: 0 on the ledge, 1 falling, 2 dead.
    pub state: u8,
    /// Pixel y-coordinate of Eugene or the Kong Beast.
    pub height: u8,
}

/// Run whichever special guardian this cavern has. Returns true if Willy died.
#[allow(clippy::too_many_arguments)]
pub fn update(
    specials: &mut Specials,
    guardians: &mut Guardians,
    cavern: &mut Cavern,
    willy: &mut Willy,
    mem: &mut Memory,
    score: &mut Score,
    sounds: &mut SoundQueue,
    items_remaining: u8,
) -> bool {
    match cavern.sheet {
        4 => {
            move_eugene(specials, items_remaining);
            if draw_eugene(specials, cavern, mem, items_remaining) {
                return true;
            }
        }
        13 if update_skylabs(guardians, cavern, mem) => return true,
        _ => {}
    }

    if cavern.sheet >= 8 && cavern.sheet != 13 && guardians.update_vertical(cavern, mem) {
        return true;
    }

    if matches!(cavern.sheet, 7 | 11)
        && update_kong(specials, guardians, cavern, willy, mem, score, sounds)
    {
        return true;
    }

    if cavern.sheet == 18 {
        light_beam(cavern, mem);
    }

    false
}

/// Eugene drifts down until the items are collected, then hunts back upward.
fn move_eugene(specials: &mut Specials, items_remaining: u8) {
    let descending = items_remaining > 0 || specials.state == 0;
    if descending {
        if specials.height + 1 == 88 {
            specials.state ^= 1;
        } else {
            specials.height += 1;
        }
    } else if specials.height.wrapping_sub(1) == 0 {
        specials.state ^= 1;
    } else {
        specials.height -= 1;
    }
}

fn draw_eugene(
    specials: &Specials,
    cavern: &Cavern,
    mem: &mut Memory,
    items_remaining: u8,
) -> bool {
    let row = rot_l(specials.height & 127, 1);
    let low = lsb(screen_row_addr(row / 2)) | 15;
    let high = msb(screen_row_addr(row.wrapping_add(1) / 2));
    let target = addr_of(high, low);

    if mem.draw_16x16(&mm_data::tiles::EUGENE, target, DrawMode::Blend) {
        return true;
    }

    let mut low = rot_l(specials.height & 120, 1) | 7;
    let high = if low & 0x80 != 0 { 93 } else { 92 };
    low = rot_l(low, 1) | 1;

    // Once the items are gone Eugene cycles colours as he closes in.
    let ink = if items_remaining == 0 {
        rot_r(cavern.clock, 2) & 7
    } else {
        7
    };
    set_guardian_attributes(cavern, mem, addr_of(high, low), ink);
    false
}

/// Skylabs fall to a crash site, disintegrate, and reappear eight columns along.
fn update_skylabs(guardians: &mut Guardians, cavern: &Cavern, mem: &mut Memory) -> bool {
    for slot in 0..guardians.vertical.len() {
        if guardians.vertical[slot].is_empty() {
            return false;
        }

        {
            let skylab = &mut guardians.vertical[slot];
            if skylab.pixel_y < skylab.max_y {
                skylab.pixel_y = skylab.pixel_y.wrapping_add(skylab.step);
            } else {
                skylab.frame += 1;
                if skylab.frame == 8 {
                    skylab.pixel_y = skylab.min_y;
                    skylab.column = (skylab.column + 8) & 31;
                    skylab.frame = 0;
                }
            }
        }

        let skylab = guardians.vertical[slot];
        let target = sprite_address(skylab.pixel_y, skylab.column);
        let sprite = frame_at(&guardians.sprites, rot_r(skylab.frame, 3));
        if mem.draw_16x16(&sprite, target, DrawMode::Blend) {
            return true;
        }
        let attr_addr = attribute_address(skylab.pixel_y, skylab.column);
        set_guardian_attributes(cavern, mem, attr_addr, skylab.attr);
    }
    false
}

/// The light beam travels down from the roof and reflects off guardians.
fn light_beam(cavern: &mut Cavern, mem: &mut Memory) {
    // The beam starts at (0,23) in the attribute buffer.
    let mut addr = ATTR_BUF + 23;
    let mut step: i32 = 32;

    let floor = cavern.tile_attr(TileKind::Floor);
    let wall = cavern.tile_attr(TileKind::Wall);
    let background = cavern.tile_attr(TileKind::Background);

    loop {
        let here = mem.read(addr);
        if here == floor || here == wall {
            return;
        }
        if here == 39 {
            // 39 is white ink on green paper, which is Willy. Burn four units of air.
            for _ in 0..4 {
                cavern.decrease_air(mem);
            }
        } else if here != background {
            // The beam bounced off a guardian.
            step = if step == 32 { -1 } else { 32 };
        }
        mem.write(addr, 119);

        let next = addr as i32 + step;
        if !(ATTR_BUF as i32..ATTR_BUF as i32 + 512).contains(&next) {
            return;
        }
        addr = next as u16;
    }
}

/// Move and draw the Kong Beast. Returns true if Willy died.
fn update_kong(
    specials: &mut Specials,
    guardians: &mut Guardians,
    cavern: &Cavern,
    willy: &Willy,
    mem: &mut Memory,
    score: &mut Score,
    sounds: &mut SoundQueue,
) -> bool {
    check_switch(cavern, willy, mem, ATTR_BUF + 6);

    if specials.state == 2 {
        return false;
    }

    // The sixth pixel row of the left switch reads 16 until it has been flipped.
    if mem.read(SCREEN_BACK_SWITCH) == 16 {
        return animate_kong(guardians, cavern, mem);
    }

    open_wall(guardians, cavern, mem);

    if check_switch(cavern, willy, mem, ATTR_BUF + 18) {
        remove_beast_floor(specials, cavern, mem);
    }

    if specials.state == 0 {
        return animate_kong(guardians, cavern, mem);
    }

    if specials.height < 100 {
        kong_falls(specials, guardians, cavern, mem, score, sounds);
        return false;
    }

    specials.state = 2;
    false
}

/// Sixth pixel row of the left-hand switch in the empty-cavern buffer.
const SCREEN_BACK_SWITCH: u16 = 29958;

fn animate_kong(guardians: &Guardians, cavern: &Cavern, mem: &mut Memory) -> bool {
    let sprite = frame_at(&guardians.sprites, cavern.clock & 32);
    if mem.draw_16x16(&sprite, SCREEN_BUF + 15, DrawMode::Blend) {
        return true;
    }
    for addr in [ATTR_BUF + 47, ATTR_BUF + 48, ATTR_BUF + 15, ATTR_BUF + 16] {
        mem.write(addr, 68);
    }
    false
}

/// Erode the wall next to the Kong Beast one pixel row per frame.
fn open_wall(guardians: &mut Guardians, cavern: &Cavern, mem: &mut Memory) {
    // The attribute of the wall cell at (11,17) in the empty-cavern buffer.
    const WALL_CELL: u16 = 24433;
    if mem.read(WALL_CELL) == 0 {
        return;
    }

    let mut addr = 32625u16;
    loop {
        let (mut high, mut low) = (msb(addr), lsb(addr));
        if mem.read(addr) != 0 {
            mem.write(addr, 0);
            // The matching row of the cell below, one third away.
            low = 145;
            high ^= 7;
            mem.write(addr_of(high, low), 0);
            return;
        }
        high -= 1;
        if high == 119 {
            let background = cavern.tile_attr(TileKind::Background);
            mem.write(WALL_CELL, background);
            mem.write(WALL_CELL + 32, background);
            // Let the guardian walk through the new opening.
            guardians.horizontal[1].right_limit = 114;
            return;
        }
        addr = addr_of(high, low);
    }
}

/// Drop the floor out from under the Kong Beast.
fn remove_beast_floor(specials: &mut Specials, cavern: &Cavern, mem: &mut Memory) {
    specials.height = 0;
    specials.state = 1;

    let background = cavern.tile_attr(TileKind::Background);
    mem.write(24143, background);
    mem.write(24144, background);

    let mut addr = 28751u16;
    for _ in 0..8 {
        mem.write(addr, 0);
        mem.write(addr + 1, 0);
        addr = addr_of(msb(addr).wrapping_add(1), lsb(addr));
    }
}

fn kong_falls(
    specials: &mut Specials,
    guardians: &Guardians,
    cavern: &Cavern,
    mem: &mut Memory,
    score: &mut Score,
    sounds: &mut SoundQueue,
) {
    specials.height += 4;
    sounds.note(specials.height, 16);

    let row = rot_l(specials.height, 1);
    let low = lsb(screen_row_addr(row / 2)) | 15;
    let high = msb(screen_row_addr(row.wrapping_add(1) / 2));
    let sprite = frame_at(&guardians.sprites, (cavern.clock & 32) | 64);
    mem.draw_16x16(&sprite, addr_of(high, low), DrawMode::Overwrite);

    score.add(100);

    // Multiplying an address built from page 23 by four lands in the attribute buffer.
    let addr = addr_of(23, specials.height & 120).wrapping_mul(4);
    let addr = addr_of(msb(addr), lsb(addr) | 15);
    set_guardian_attributes(cavern, mem, addr, 6);
}

/// Flip a switch if Willy is touching it. Returns true if it flipped this frame.
fn check_switch(cavern: &Cavern, willy: &Willy, mem: &mut Memory, switch: u16) -> bool {
    // Willy triggers the switch from either of the two cells it spans.
    if lsb(willy.location).wrapping_add(1) & 254 != lsb(switch) {
        return false;
    }
    if msb(willy.location) != msb(switch) {
        return false;
    }

    // The switch tile's sixth pixel row lives 25 pages further on.
    let pixels = addr_of(msb(switch).wrapping_add(25), lsb(switch));
    if mem.read(pixels) != cavern.tile(TileKind::Extra).sprite[5] {
        return false;
    }

    mem.write(pixels, 8);
    mem.write(addr_of(msb(pixels).wrapping_add(1), lsb(pixels)), 6);
    mem.write(addr_of(msb(pixels).wrapping_add(2), lsb(pixels)), 6);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eugene_descends_until_the_items_are_collected() {
        let mut specials = Specials::default();
        for _ in 0..10 {
            move_eugene(&mut specials, 3);
        }
        assert_eq!(specials.height, 10);
        assert_eq!(specials.state, 0);
    }

    #[test]
    fn eugene_turns_around_at_the_portal() {
        let mut specials = Specials {
            state: 0,
            height: 87,
        };
        move_eugene(&mut specials, 3);
        assert_eq!(specials.height, 87, "Eugene passed the portal");
        assert_eq!(specials.state, 1);
    }

    #[test]
    fn the_light_beam_stops_at_the_floor() {
        let mut cavern = Cavern::load(18);
        let mut mem = Memory::new();
        mem.load(ATTR_BUF, &cavern.layout);
        light_beam(&mut cavern, &mut mem);
        // The beam paints at least the cell it starts from.
        assert_eq!(mem.read(ATTR_BUF + 23), 119);
    }
}
