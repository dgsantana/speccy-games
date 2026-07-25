//! Guardians: the things patrolling a cavern that kill Willy on contact.
//!
//! Ordinary guardians come in two flavours. Horizontal ones walk a fixed stretch
//! of one row; vertical ones bob up and down a column. The specials — Eugene, the
//! Skylabs, the Kong Beast and the Solar Power Generator's light beam — live in
//! [`crate::special`].

use mm_data::guardians;

use crate::cavern::Cavern;
use crate::speccy::{DrawMode, Memory, addr_of, lsb, msb, rot_l, rot_r, screen_row_addr};
use crate::willy::{Willy, frame_at};

/// A guardian that patrols left and right along one row of cells.
#[derive(Debug, Clone, Copy, Default)]
pub struct Horizontal {
    /// Bit 7 is the animation speed; the rest is the colour attribute.
    pub speed_colour: u8,
    /// Position in the attribute buffer.
    pub attr_addr: u16,
    /// High byte of the same position in the pixel buffer.
    pub screen_msb: u8,
    /// Animation frame 0 to 7; 0 to 3 walk right, 7 down to 4 walk left.
    pub frame: u8,
    /// Low bytes of the leftmost and rightmost cells of the patrol.
    pub left_limit: u8,
    pub right_limit: u8,
}

impl Horizontal {
    /// An empty slot, which the loader leaves as zero or 255.
    pub fn is_empty(&self) -> bool {
        self.speed_colour == 0 || self.speed_colour == 255
    }
}

/// A guardian that moves up and down a column.
#[derive(Debug, Clone, Copy, Default)]
pub struct Vertical {
    pub attr: u8,
    pub frame: u8,
    pub pixel_y: u8,
    pub column: u8,
    /// Added to `pixel_y` each frame, negated when a limit is reached.
    pub step: u8,
    pub min_y: u8,
    pub max_y: u8,
}

impl Vertical {
    pub fn is_empty(&self) -> bool {
        self.attr == 255
    }
}

/// The guardians of one cavern, plus the sprite sheet they share.
#[derive(Debug)]
pub struct Guardians {
    pub horizontal: [Horizontal; 4],
    pub vertical: [Vertical; 4],
    pub sprites: [u8; 256],
}

impl Guardians {
    pub fn load(sheet: usize) -> Self {
        let mut horizontal = [Horizontal::default(); 4];
        for (slot, guardian) in horizontal.iter_mut().enumerate() {
            let raw = guardians::HORIZONTAL[sheet][slot];
            *guardian = Horizontal {
                speed_colour: raw[0] as u8,
                attr_addr: raw[1],
                screen_msb: raw[2] as u8,
                frame: raw[3] as u8,
                left_limit: raw[4] as u8,
                right_limit: raw[5] as u8,
            };
        }

        let mut vertical = [Vertical::default(); 4];
        for (slot, guardian) in vertical.iter_mut().enumerate() {
            let raw = guardians::VERTICAL[sheet][slot];
            *guardian = Vertical {
                attr: raw[0],
                frame: raw[1],
                pixel_y: raw[2],
                column: raw[3],
                step: raw[4],
                min_y: raw[5],
                max_y: raw[6],
            };
        }

        Self {
            horizontal,
            vertical,
            sprites: guardians::SPRITES[sheet],
        }
    }

    /// Step every horizontal guardian. The original stopped at the first empty
    /// slot, and so does this.
    pub fn move_horizontal(&mut self, cavern: &Cavern) {
        for guardian in &mut self.horizontal {
            if guardian.is_empty() {
                break;
            }
            move_one_horizontal(guardian, cavern.clock);
        }
    }

    /// Draw every horizontal guardian. Returns true if one of them touched Willy.
    pub fn draw_horizontal(&self, cavern: &Cavern, mem: &mut Memory) -> bool {
        for guardian in &self.horizontal {
            if guardian.is_empty() {
                continue;
            }
            if draw_one_horizontal(guardian, &self.sprites, cavern, mem) {
                return true;
            }
        }
        false
    }

    /// Step and draw every vertical guardian. Returns true if one touched Willy.
    pub fn update_vertical(&mut self, cavern: &Cavern, mem: &mut Memory) -> bool {
        for slot in 0..self.vertical.len() {
            if self.vertical[slot].is_empty() {
                continue;
            }
            let sprites = self.sprites;
            if update_one_vertical(&mut self.vertical[slot], &sprites, cavern, mem) {
                return true;
            }
        }
        false
    }
}

fn move_one_horizontal(guardian: &mut Horizontal, clock: u8) {
    // Bit 7 of the attribute byte halves the animation speed; the clock's bit 2
    // is what makes every other pass skip a slow guardian.
    if rot_r(clock & 4, 3) & guardian.speed_colour != 0 {
        return;
    }

    match guardian.frame {
        // Last frame walking right: step right, or turn around at the limit.
        3 => {
            if lsb(guardian.attr_addr) == guardian.right_limit {
                guardian.frame = 7;
            } else {
                guardian.frame = 0;
                guardian.attr_addr = addr_of(
                    msb(guardian.attr_addr),
                    lsb(guardian.attr_addr).wrapping_add(1),
                );
            }
        }
        // Last frame walking left: step left, or turn around at the limit.
        4 => {
            if lsb(guardian.attr_addr) == guardian.left_limit {
                guardian.frame = 0;
            } else {
                guardian.frame = 7;
                guardian.attr_addr = addr_of(
                    msb(guardian.attr_addr),
                    lsb(guardian.attr_addr).wrapping_sub(1),
                );
            }
        }
        5..=7 => guardian.frame -= 1,
        _ => guardian.frame += 1,
    }
}

fn draw_one_horizontal(
    guardian: &Horizontal,
    sprites: &[u8; 256],
    cavern: &Cavern,
    mem: &mut Memory,
) -> bool {
    // Bit 7 is the speed flag, not part of the colour, and must not become FLASH.
    let attr = guardian.speed_colour & 127;
    let addr = guardian.attr_addr;
    for offset in [0u16, 1, 32, 33] {
        mem.write(addr + offset, attr);
    }

    let mut index = rot_r(guardian.frame, 3);
    // From Miner Willy meets the Kong Beast onwards the guardians only have the
    // upper four frames, apart from two caverns that kept the full set.
    if cavern.sheet >= 7 && cavern.sheet != 9 && cavern.sheet != 15 {
        index |= 0x80;
    }

    let target = addr_of(guardian.screen_msb, lsb(guardian.attr_addr));
    mem.draw_16x16(&frame_at(sprites, index), target, DrawMode::Blend)
}

fn update_one_vertical(
    guardian: &mut Vertical,
    sprites: &[u8; 256],
    cavern: &Cavern,
    mem: &mut Memory,
) -> bool {
    guardian.frame = if guardian.frame < 3 {
        guardian.frame + 1
    } else {
        0
    };

    let next = guardian.pixel_y.wrapping_add(guardian.step);
    if next < guardian.min_y || next >= guardian.max_y {
        guardian.step = guardian.step.wrapping_neg();
    } else {
        guardian.pixel_y = next;
    }

    let target = sprite_address(guardian.pixel_y, guardian.column);
    let index = rot_r(guardian.frame, 3);
    if mem.draw_16x16(&frame_at(sprites, index), target, DrawMode::Blend) {
        return true;
    }

    let attr_addr = attribute_address(guardian.pixel_y, guardian.column);
    set_guardian_attributes(cavern, mem, attr_addr, guardian.attr);
    false
}

/// Address in the pixel buffer for a sprite at a pixel y-coordinate and column.
///
/// The high byte comes from the row below the sprite's top, because a 16-pixel
/// sprite starting mid-cell straddles a third boundary differently from its top row.
pub fn sprite_address(pixel_y: u8, column: u8) -> u16 {
    let row = rot_l(pixel_y & 127, 1);
    let low = lsb(screen_row_addr(row / 2)) | column;
    let high = msb(screen_row_addr(row.wrapping_add(1) / 2));
    addr_of(high, low)
}

/// Address in the attribute buffer for a sprite at a pixel y-coordinate and column.
pub fn attribute_address(pixel_y: u8, column: u8) -> u16 {
    let high = rot_l(pixel_y & 64, 2).wrapping_add(92);
    let low = (rot_l(pixel_y, 2) & 224) | column;
    addr_of(high, low)
}

/// Colour the six cells a 16x16 guardian sprite covers, keeping the cavern's paper.
pub fn set_guardian_attributes(cavern: &Cavern, mem: &mut Memory, addr: u16, ink: u8) {
    let background = cavern.tile_attr(mm_data::TileKind::Background);
    let attr = (background & 248) | ink;
    for offset in [0u16, 1, 32, 33, 64, 65] {
        mem.write(addr + offset, attr);
    }
}

/// Kill Willy if a guardian collided with him.
pub fn resolve_collision(willy: &mut Willy, collided: bool) -> bool {
    if collided {
        willy.kill();
    }
    collided
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_horizontal_guardian_turns_around_at_both_ends_of_its_path() {
        let mut guardian = Horizontal {
            speed_colour: 66,
            attr_addr: addr_of(92, 100),
            screen_msb: 96,
            frame: 3,
            left_limit: 98,
            right_limit: 100,
        };
        // At the right limit on the last right-walking frame: turn to face left.
        move_one_horizontal(&mut guardian, 0);
        assert_eq!(guardian.frame, 7);
        assert_eq!(lsb(guardian.attr_addr), 100);

        // Walk left to the limit.
        for _ in 0..3 {
            move_one_horizontal(&mut guardian, 0);
        }
        assert_eq!(guardian.frame, 4);
        move_one_horizontal(&mut guardian, 0);
        assert_eq!(lsb(guardian.attr_addr), 99);
    }

    #[test]
    fn central_cavern_has_horizontal_guardians_and_no_vertical_ones() {
        let guardians = Guardians::load(0);
        assert!(!guardians.horizontal[0].is_empty());
        assert!(guardians.vertical.iter().all(Vertical::is_empty));
    }

    #[test]
    fn a_vertical_guardian_reverses_at_its_limits() {
        let mut guardian = Vertical {
            attr: 68,
            frame: 0,
            pixel_y: 100,
            column: 8,
            step: 4,
            min_y: 96,
            max_y: 104,
        };
        let cavern = Cavern::load(8);
        let sprites = [0u8; 256];
        let mut mem = Memory::new();
        update_one_vertical(&mut guardian, &sprites, &cavern, &mut mem);
        // 104 is not below max_y, so the guardian reverses instead of moving.
        assert_eq!(guardian.pixel_y, 100);
        assert_eq!(guardian.step, 4u8.wrapping_neg());
    }
}
