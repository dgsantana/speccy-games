//! Turning the display and attribute files into pixels.

use crate::memory::{ATTR_FILE, DISPLAY, HEIGHT, Memory, WIDTH, display_row_offset};

/// The sixteen ZX Spectrum colours: eight normal, then the same eight bright.
pub const PALETTE: [[u8; 3]; 16] = [
    [0x00, 0x00, 0x00], // black
    [0x00, 0x00, 0xd7], // blue
    [0xd7, 0x00, 0x00], // red
    [0xd7, 0x00, 0xd7], // magenta
    [0x00, 0xd7, 0x00], // green
    [0x00, 0xd7, 0xd7], // cyan
    [0xd7, 0xd7, 0x00], // yellow
    [0xd7, 0xd7, 0xd7], // white
    [0x00, 0x00, 0x00],
    [0x00, 0x00, 0xff],
    [0xff, 0x00, 0x00],
    [0xff, 0x00, 0xff],
    [0x00, 0xff, 0x00],
    [0x00, 0xff, 0xff],
    [0xff, 0xff, 0x00],
    [0xff, 0xff, 0xff],
];

/// The four fields packed into an attribute byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attribute {
    pub flash: bool,
    pub bright: bool,
    pub paper: u8,
    pub ink: u8,
}

impl From<u8> for Attribute {
    fn from(byte: u8) -> Self {
        Self {
            flash: byte & 0x80 != 0,
            bright: byte & 0x40 != 0,
            paper: (byte >> 3) & 7,
            ink: byte & 7,
        }
    }
}

impl Attribute {
    /// Palette indices for a set and an unset pixel, with FLASH already applied.
    pub fn colours(self, flash_on: bool) -> (usize, usize) {
        let (ink, paper) = if self.flash && flash_on {
            (self.paper, self.ink)
        } else {
            (self.ink, self.paper)
        };
        let bright = if self.bright { 8 } else { 0 };
        (ink as usize + bright, paper as usize + bright)
    }
}

/// A 256x192 RGBA frame, ready to upload as a texture.
pub struct Frame {
    pub pixels: Vec<u8>,
}

impl core::fmt::Debug for Frame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Frame")
            .field("bytes", &self.pixels.len())
            .finish()
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl Frame {
    pub fn new() -> Self {
        Self {
            pixels: vec![0; WIDTH * HEIGHT * 4],
        }
    }

    /// Expand the display and attribute files into RGBA pixels.
    ///
    /// `flash_on` is the current half of the flash cycle, which the original
    /// toggled roughly every 16 frames.
    pub fn render(&mut self, mem: &Memory, flash_on: bool) {
        for y in 0..HEIGHT {
            let row = DISPLAY as usize + display_row_offset(y);
            let attr_row = ATTR_FILE as usize + (y / 8) * 32;
            for col in 0..WIDTH / 8 {
                let bits = mem.read((row + col) as u16);
                let attr = Attribute::from(mem.read((attr_row + col) as u16));
                let (ink, paper) = attr.colours(flash_on);
                let mut out = (y * WIDTH + col * 8) * 4;
                for bit in (0..8).rev() {
                    let colour = PALETTE[if bits >> bit & 1 == 1 { ink } else { paper }];
                    self.pixels[out] = colour[0];
                    self.pixels[out + 1] = colour[1];
                    self.pixels[out + 2] = colour[2];
                    self.pixels[out + 3] = 0xff;
                    out += 4;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flash_swaps_ink_and_paper() {
        let attr = Attribute::from(0b1_0_010_101);
        assert!(attr.flash);
        assert_eq!(attr.paper, 2);
        assert_eq!(attr.ink, 5);
        assert_eq!(attr.colours(false), (5, 2));
        assert_eq!(attr.colours(true), (2, 5));
    }

    #[test]
    fn bright_selects_the_upper_palette() {
        let attr = Attribute::from(0b0_1_000_111);
        assert_eq!(attr.colours(false), (15, 8));
    }

    #[test]
    fn a_set_bit_paints_ink_at_the_left_of_the_cell() {
        let mut mem = Memory::new();
        mem.write(DISPLAY, 0b1000_0000);
        mem.write(ATTR_FILE, 0b0_0_010_110); // ink yellow, paper red
        let mut frame = Frame::new();
        frame.render(&mem, false);
        assert_eq!(&frame.pixels[0..3], &PALETTE[6]);
        assert_eq!(&frame.pixels[4..7], &PALETTE[2]);
    }
}
