//! Miner Willy: movement, jumping, falling and the ways he dies.

use mm_data::TileKind;

use crate::cavern::Cavern;
use crate::layout::screen_row_addr;
use speccy::input::Input;
use speccy::memory::{DrawMode, Memory, addr_of, lsb, msb, rot_l, rot_r};
use speccy::sound::SoundQueue;

/// Airborne status values with meaning beyond "falling for this many frames".
const NOT_AIRBORNE: u8 = 0;
const JUMPING: u8 = 1;
const STARTED_FALLING: u8 = 2;
const FATAL_FALL: u8 = 12;
const KILLED: u8 = 255;

/// Maps Willy's direction and movement flags plus the direction he is being
/// pushed onto a new set of flags.
///
/// Index is `flags + 4` when moving left, `+ 8` when moving right, and `+ 12`
/// when a conveyor and the keyboard pull him both ways at once, in which case
/// the flags are left alone.
const LR_MOVEMENT: [u8; 16] = [
    0, 1, 0, 1, // no movement: stop, keeping the direction faced
    1, 3, 1, 3, // moving left
    2, 0, 2, 0, // moving right
    0, 1, 2, 3, // pulled both ways: no change
];

/// Willy's state. Positions are held the way the original held them: a pixel
/// y-coordinate at twice its real value, and an address in the attribute buffer.
#[derive(Debug, Clone, Copy)]
pub struct Willy {
    pub lives: u8,
    /// Bit 0: facing left. Bit 1: moving.
    pub flags: u8,
    /// Twice Willy's pixel y-coordinate.
    pub pixel_y: u8,
    /// Walk animation frame, 0 to 3.
    pub frame: u8,
    /// 0 standing, 1 jumping, 2 to 11 falling safely, 12 or more falling fatally,
    /// 255 killed.
    pub airborne: u8,
    /// Willy's top-left cell, as an address in the attribute buffer.
    pub location: u16,
    /// Jump animation counter, 0 to 17.
    pub jumping: u8,
}

impl Default for Willy {
    fn default() -> Self {
        Self {
            lives: 3,
            flags: 0,
            pixel_y: 208,
            frame: 0,
            airborne: 0,
            location: 23970,
            jumping: 0,
        }
    }
}

impl Willy {
    /// Place Willy at the start position for a cavern, keeping his lives.
    pub fn enter_cavern(&mut self, sheet: usize) {
        use mm_data::caverns::{
            WILLY_START_AIRBORNE, WILLY_START_DIRECTION, WILLY_START_FRAME, WILLY_START_JUMP,
            WILLY_START_LOCATION, WILLY_START_PIXEL_Y,
        };
        self.pixel_y = WILLY_START_PIXEL_Y[sheet];
        self.frame = WILLY_START_FRAME[sheet];
        self.flags = WILLY_START_DIRECTION[sheet];
        self.airborne = WILLY_START_AIRBORNE[sheet];
        self.location = WILLY_START_LOCATION[sheet];
        self.jumping = WILLY_START_JUMP[sheet];
    }

    pub fn is_dead(&self) -> bool {
        self.airborne == KILLED
    }

    pub fn kill(&mut self) {
        self.airborne = KILLED;
    }

    /// Clear the moving flag, leaving the direction faced alone.
    fn stop(&mut self) {
        self.flags &= !2;
    }

    /// Recompute [`Willy::location`] from a new pixel y-coordinate.
    fn sync_location(&mut self) -> u16 {
        let mut low = self.pixel_y & 240;
        let high = if low & 0x80 != 0 { 93 } else { 92 };
        low = rot_l(low, 1) & !1;
        let x = (self.location & 31) as u8;
        self.location = addr_of(high, low | x);
        self.location
    }

    /// Willy's screen column, 0 to 31.
    pub fn column(&self) -> u8 {
        (self.location & 31) as u8
    }
}

/// Advance a jump by one frame. Returns true once the jump is over.
fn update_jump(
    willy: &mut Willy,
    cavern: &Cavern,
    mem: &Memory,
    sounds: &mut SoundQueue,
) -> bool {
    // The counter runs 0 to 17; this turns it into an even offset from -8 to +8,
    // giving the jump its arc.
    let step = (willy.jumping & !1).wrapping_sub(8);
    willy.pixel_y = willy.pixel_y.wrapping_add(step);

    let location = willy.sync_location();
    let wall = cavern.tile_attr(TileKind::Wall);
    if mem.read(location) == wall || mem.read(location + 1) == wall {
        hit_wall(willy);
        return true;
    }

    willy.jumping += 1;

    // Pitch rises as Willy rises and falls as he falls.
    let height = willy.jumping;
    let offset = height.abs_diff(8);
    sounds.note(rot_l(offset + 1, 3), 32);

    match willy.jumping {
        18 => {
            // Willy keeps falling unless he has landed on something.
            willy.airborne = 6;
            true
        }
        16 | 13 => false,
        _ => {
            move_in_direction_faced(willy, cavern, mem);
            true
        }
    }
}

/// Willy's head hit a wall: drop him back below it and start him falling.
fn hit_wall(willy: &mut Willy) {
    willy.pixel_y = willy.pixel_y.wrapping_add(16) & 240;
    willy.sync_location();
    willy.airborne = STARTED_FALLING;
    willy.stop();
}

/// Read the controls and set Willy moving. Returns true if the landing killed him.
fn land_and_read_input(
    willy: &mut Willy,
    cavern: &Cavern,
    mem: &Memory,
    input: Input,
    below: u16,
) -> bool {
    if willy.airborne >= FATAL_FALL {
        willy.kill();
        return true;
    }
    willy.airborne = NOT_AIRBORNE;

    let conveyor = cavern.tile_attr(TileKind::Conveyor);
    let on_conveyor =
        mem.read(below) == conveyor || mem.read(below + 1) == conveyor;

    let mut pull = 0u8;
    if input.left || (on_conveyor && cavern.conveyor.direction == 0) {
        pull |= 4;
    }
    if input.right || (on_conveyor && cavern.conveyor.direction == 1) {
        pull |= 8;
    }
    willy.flags = LR_MOVEMENT[(willy.flags + pull) as usize];

    if input.jump {
        willy.jumping = 0;
        willy.airborne = JUMPING;
    }

    move_in_direction_faced(willy, cavern, mem);
    false
}

/// Step Willy one animation frame, crossing a cell boundary when the frame wraps.
fn move_in_direction_faced(willy: &mut Willy, cavern: &Cavern, mem: &Memory) {
    if willy.flags & 2 == 0 {
        return;
    }
    if willy.flags & 1 == 0 {
        move_right(willy, cavern, mem);
    } else if willy.frame == 0 {
        move_left(willy, cavern, mem);
    } else {
        willy.frame -= 1;
    }
}

/// Cross a cell boundary to the left, unless a wall blocks the way.
fn move_left(willy: &mut Willy, cavern: &Cavern, mem: &Memory) {
    let wall = cavern.tile_attr(TileKind::Wall);
    let mut addr = willy.location.wrapping_sub(1).wrapping_add(32);

    if mem.read(addr) == wall {
        return;
    }
    // When Willy straddles two rows of cells he needs a third cell to be clear.
    if willy.pixel_y & 15 != 0 && mem.read(addr + 32) == wall {
        return;
    }
    addr -= 32;
    if mem.read(addr) == wall {
        return;
    }

    willy.location = addr;
    willy.frame = 3;
}

/// Cross a cell boundary to the right, unless a wall blocks the way.
fn move_right(willy: &mut Willy, cavern: &Cavern, mem: &Memory) {
    if willy.frame != 3 {
        willy.frame += 1;
        return;
    }

    let wall = cavern.tile_attr(TileKind::Wall);
    let mut addr = willy.location.wrapping_add(2).wrapping_add(32);

    if mem.read(addr) == wall {
        return;
    }
    if willy.pixel_y & 15 != 0 && mem.read(addr + 32) == wall {
        return;
    }
    addr -= 32;
    if mem.read(addr) == wall {
        return;
    }

    willy.location = addr - 1;
    willy.frame = 0;
}

/// One frame of Willy's movement. Returns true if he died this frame.
pub fn update(
    willy: &mut Willy,
    cavern: &Cavern,
    mem: &mut Memory,
    input: Input,
    sounds: &mut SoundQueue,
) -> bool {
    if willy.airborne == JUMPING && update_jump(willy, cavern, mem, sounds) {
        return false;
    }

    // Willy only interacts with the ground when his sprite sits on a cell boundary.
    if willy.pixel_y & 15 == 0 {
        let background = cavern.tile_attr(TileKind::Background);
        let crumbling = cavern.tile_attr(TileKind::Crumbling);
        let nasty1 = cavern.tile_attr(TileKind::Nasty1);
        let nasty2 = cavern.tile_attr(TileKind::Nasty2);

        let left = willy.location.wrapping_add(64);
        let right = left + 1;

        if mem.read(left) == crumbling {
            crumble(cavern, mem, left);
        }
        let left_nasty = mem.read(left) == nasty1 || mem.read(left) == nasty2;
        if !left_nasty {
            if mem.read(right) == crumbling {
                crumble(cavern, mem, right);
            }
            let right_nasty = mem.read(right) == nasty1 || mem.read(right) == nasty2;
            if !right_nasty {
                if mem.read(right) != background {
                    return land_and_read_input(willy, cavern, mem, input, right);
                }
                if mem.read(left) != background {
                    return land_and_read_input(willy, cavern, mem, input, left);
                }
            }
        }
    }

    if willy.airborne == JUMPING {
        move_in_direction_faced(willy, cavern, mem);
        return false;
    }

    // Nothing underfoot: Willy falls.
    willy.stop();
    if willy.airborne == NOT_AIRBORNE {
        willy.airborne = STARTED_FALLING;
        return false;
    }

    willy.airborne += 1;
    sounds.note(rot_l(willy.airborne, 4), 32);
    willy.pixel_y = willy.pixel_y.wrapping_add(8);
    willy.sync_location();
    false
}

/// Wear away one pixel row of a crumbling floor tile, removing it when it is gone.
fn crumble(cavern: &Cavern, mem: &mut Memory, attr_addr: u16) {
    // The tile's pixel rows live in the empty-cavern buffer; this is the same
    // cell 27 pages further on, at its bottom pixel row.
    let low = lsb(attr_addr);
    let mut high = msb(attr_addr).wrapping_add(27) | 7;

    loop {
        high -= 1;
        let pixels = mem.read(addr_of(high, low));
        mem.write(addr_of(high + 1, low), pixels);
        if high & 7 == 0 {
            break;
        }
    }
    mem.write(addr_of(high, low), 0);

    high += 7;
    if mem.read(addr_of(high, low)) == 0 {
        mem.write(attr_addr, cavern.tile_attr(TileKind::Background));
    }
}

/// Colour in the six cells Willy's sprite covers and draw him.
///
/// Returns true if he touched a nasty.
pub fn draw(willy: &Willy, cavern: &Cavern, mem: &mut Memory) -> bool {
    let mut killed = false;

    // The top two rows of cells always take white ink. The bottom row only does
    // when Willy's sprite actually reaches into it, which is what passing his
    // real pixel y-coordinate rather than 15 tests for.
    let cells = [
        (willy.location, 15u8),
        (willy.location + 1, 15),
        (willy.location + 32, 15),
        (willy.location + 33, 15),
        (willy.location + 64, willy.pixel_y),
        (willy.location + 65, willy.pixel_y),
    ];
    for (addr, pixel_y) in cells {
        killed |= set_cell_attribute(cavern, mem, addr, pixel_y);
    }

    draw_sprite(willy, mem);
    killed
}

fn set_cell_attribute(cavern: &Cavern, mem: &mut Memory, addr: u16, pixel_y: u8) -> bool {
    let background = cavern.tile_attr(TileKind::Background);
    if mem.read(addr) == background && pixel_y & 15 != 0 {
        mem.write(addr, background | 7);
    }
    let here = mem.read(addr);
    here == cavern.tile_attr(TileKind::Nasty1) || here == cavern.tile_attr(TileKind::Nasty2)
}

/// Blend Willy's 16x16 sprite into the pixel buffer.
fn draw_sprite(willy: &Willy, mem: &mut Memory) {
    let mut row = willy.pixel_y / 2;
    // Bit 7 selects the left-facing frames; the walk frame contributes 32 each.
    let facing = rot_r(willy.flags & 1, 1);
    let mut index = rot_r(willy.frame & 3, 3) | facing;
    let column = willy.column();

    for _ in 0..16 {
        let base = screen_row_addr(row);
        let addr = addr_of(msb(base), lsb(base) | column);

        let left = mm_data::sprites::WILLY[index as usize] | mem.read(addr);
        mem.write(addr, left);
        index = index.wrapping_add(1);

        let right = mm_data::sprites::WILLY[index as usize] | mem.read(addr + 1);
        mem.write(addr + 1, right);
        index = index.wrapping_add(1);

        row = row.wrapping_add(1);
    }
}

/// Draw the remaining lives along the bottom of the screen.
pub fn draw_lives(willy: &Willy, mem: &mut Memory, note_index: u8, cheat: bool) {
    let mut addr = 20640u16;
    // The lives animate in step with the in-game tune.
    let frame = rot_l(note_index, 3) & 96;
    let sprite = frame_at(&mm_data::sprites::WILLY, frame);

    for _ in 0..willy.lives {
        mem.draw_16x16(&sprite, addr, DrawMode::Overwrite);
        addr += 2;
    }
    if cheat {
        mem.draw_16x16(&mm_data::tiles::BOOT, addr, DrawMode::Overwrite);
    }
}

/// The 32-byte sprite frame starting at `offset`, wrapping like the Z80 would.
pub fn frame_at(data: &[u8; 256], offset: u8) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = data[offset.wrapping_add(i as u8) as usize];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_table_turns_willy_around_before_moving_him() {
        // Facing right and standing still, pushed left: turn to face left, still still.
        assert_eq!(LR_MOVEMENT[4], 1);
        // Facing left and standing still, pushed left: start moving.
        assert_eq!(LR_MOVEMENT[5], 3);
        // Pulled both ways: unchanged.
        for flags in 0..4u8 {
            assert_eq!(LR_MOVEMENT[(flags + 12) as usize], flags);
        }
    }

    #[test]
    fn a_jump_rises_then_falls_and_ends_after_eighteen_frames() {
        let cavern = Cavern::load(0);
        let mut mem = Memory::new();
        let mut sounds = SoundQueue::default();
        let mut willy = Willy::default();
        willy.enter_cavern(0);
        let start_y = willy.pixel_y;

        willy.airborne = JUMPING;
        willy.jumping = 0;
        let mut lowest = start_y;
        for _ in 0..18 {
            if willy.airborne != JUMPING {
                break;
            }
            update_jump(&mut willy, &cavern, &mem, &mut sounds);
            lowest = lowest.min(willy.pixel_y);
        }
        assert!(lowest < start_y, "Willy never rose during the jump");
        assert_eq!(willy.jumping, 18);
        assert_eq!(willy.airborne, 6);
        let _ = &mut mem;
    }

    #[test]
    fn a_long_fall_is_fatal() {
        let cavern = Cavern::load(0);
        let mut mem = Memory::new();
        let mut willy = Willy::default();
        willy.enter_cavern(0);
        willy.airborne = FATAL_FALL;
        let below = willy.location.wrapping_add(64);
        assert!(land_and_read_input(
            &mut willy,
            &cavern,
            &mem,
            Input::default(),
            below
        ));
        assert!(willy.is_dead());
        let _ = &mut mem;
    }
}
