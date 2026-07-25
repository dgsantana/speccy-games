//! The Spectrum's address space.
//!
//! Games of this era move data around by absolute address, and draw a sprite by
//! walking down a column incrementing the *high* byte, which is how the display
//! file is laid out. Modelling the address space directly is what keeps that
//! arithmetic honest, so [`Memory`] is a flat 64K array and the display and
//! attribute files are named windows into it. Games put their own working
//! buffers wherever the original put them.

/// Display file: 6144 bytes of pixels, in Spectrum thirds-and-rows order.
pub const DISPLAY: u16 = 16384;
/// Attribute file: one byte per 8x8 cell, 32 columns by 24 rows.
pub const ATTR_FILE: u16 = 22528;
/// Bytes in the display file.
pub const DISPLAY_LEN: usize = 6144;
/// Bytes in the attribute file.
pub const ATTR_LEN: usize = 768;

/// Screen width in pixels.
pub const WIDTH: usize = 256;
/// Screen height in pixels.
pub const HEIGHT: usize = 192;

/// The Spectrum's addressable memory.
///
/// Reads and writes are plain byte accesses; the interesting part is the address
/// arithmetic in [`next_pixel_row`] and friends.
pub struct Memory {
    bytes: Box<[u8; 65536]>,
}

impl core::fmt::Debug for Memory {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Memory").finish_non_exhaustive()
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

impl Memory {
    pub fn new() -> Self {
        Self {
            bytes: vec![0u8; 65536].into_boxed_slice().try_into().unwrap(),
        }
    }

    #[inline]
    pub fn read(&self, addr: u16) -> u8 {
        self.bytes[addr as usize]
    }

    #[inline]
    pub fn write(&mut self, addr: u16, byte: u8) {
        self.bytes[addr as usize] = byte;
    }

    /// Read `len` bytes starting at `addr`.
    pub fn slice(&self, addr: u16, len: usize) -> &[u8] {
        let start = addr as usize;
        &self.bytes[start..start + len]
    }

    /// Overwrite `len` bytes starting at `addr` with `byte`.
    pub fn fill(&mut self, addr: u16, len: usize, byte: u8) {
        let start = addr as usize;
        self.bytes[start..start + len].fill(byte);
    }

    /// Copy `len` bytes from one region to another. The regions must not overlap.
    pub fn copy(&mut self, from: u16, to: u16, len: usize) {
        self.bytes
            .copy_within(from as usize..from as usize + len, to as usize);
    }

    /// Copy a source slice into memory at `addr`.
    pub fn load(&mut self, addr: u16, src: &[u8]) {
        let start = addr as usize;
        self.bytes[start..start + src.len()].copy_from_slice(src);
    }
}

/// Move an address down one pixel row by incrementing its high byte.
///
/// This wraps within the high byte, exactly as `INC H` does, so it walks the
/// eight rows of a character cell and then leaves the cell.
#[inline]
pub const fn next_pixel_row(addr: u16) -> u16 {
    (addr & 0x00ff) | (addr.wrapping_add(0x0100) & 0xff00)
}

/// Move an address one cell to the right by incrementing its low byte.
#[inline]
pub const fn next_cell(addr: u16) -> u16 {
    (addr & 0xff00) | ((addr as u8).wrapping_add(1) as u16)
}

/// Add `n` to an address's low byte, wrapping within that byte.
#[inline]
pub const fn add_lsb(addr: u16, n: u8) -> u16 {
    (addr & 0xff00) | ((addr as u8).wrapping_add(n) as u16)
}

/// The high byte of an address.
#[inline]
pub const fn msb(addr: u16) -> u8 {
    (addr >> 8) as u8
}

/// The low byte of an address.
#[inline]
pub const fn lsb(addr: u16) -> u8 {
    addr as u8
}

/// Join a high and low byte into an address.
#[inline]
pub const fn addr_of(msb: u8, lsb: u8) -> u16 {
    ((msb as u16) << 8) | lsb as u16
}

/// Rotate a byte left, as `RLC` does.
#[inline]
pub const fn rot_l(byte: u8, n: u32) -> u8 {
    byte.rotate_left(n)
}

/// Rotate a byte right, as `RRC` does.
#[inline]
pub const fn rot_r(byte: u8, n: u32) -> u8 {
    byte.rotate_right(n)
}

/// Offset into a display file of the leftmost pixel byte of row `y`.
///
/// The display file is ordered by third, then by pixel row within a character,
/// then by character row, which is why this is bit shuffling rather than `y * 32`.
#[inline]
pub const fn display_row_offset(y: usize) -> usize {
    ((y & 0xc0) << 5) | ((y & 0x07) << 8) | ((y & 0x38) << 2)
}

impl Memory {
    /// Draw `sprite`'s bytes down a column, one pixel row per byte.
    pub fn draw_sprite(&mut self, sprite: &[u8], mut addr: u16) {
        for &byte in sprite {
            self.write(addr, byte);
            addr = next_pixel_row(addr);
        }
    }

    /// Draw one character of the Spectrum ROM font at `addr`.
    pub fn print_char(&mut self, ch: u8, addr: u16) {
        let index = ch.saturating_sub(32) as usize;
        let glyph = crate::font::FONT[index.min(95)];
        self.draw_sprite(&glyph, addr);
    }

    /// Print a string one character per cell, starting at `addr`.
    pub fn print_str(&mut self, text: &str, addr: u16) {
        for (i, ch) in text.bytes().enumerate() {
            self.print_char(ch, addr.wrapping_add(i as u16));
        }
    }

    /// Draw a 16x16 sprite, the original's `DRWFIX`.
    ///
    /// In [`DrawMode::Blend`] the sprite is OR-ed onto what is already there and
    /// the call reports whether any set bit landed on a set bit, which is how the
    /// game detects a guardian touching Willy.
    pub fn draw_16x16(&mut self, sprite: &[u8; 32], mut addr: u16, mode: DrawMode) -> bool {
        for pair in sprite.chunks_exact(2) {
            let (mut left, mut right) = (pair[0], pair[1]);
            if mode == DrawMode::Blend {
                let (bg_left, bg_right) = (self.read(addr), self.read(next_cell(addr)));
                if left & bg_left != 0 || right & bg_right != 0 {
                    return true;
                }
                left |= bg_left;
                right |= bg_right;
            }
            self.write(addr, left);
            self.write(next_cell(addr), right);

            addr = next_pixel_row(addr);
            if msb(addr) & 7 != 0 {
                continue;
            }
            // Bottom row of this cell reached: step to the top row of the cell below,
            // undoing the third-boundary jump unless we really crossed one.
            let mut hi = msb(addr).wrapping_sub(8);
            let lo = lsb(addr).wrapping_add(32);
            if lo & 224 == 0 {
                hi = hi.wrapping_add(8);
            }
            addr = addr_of(hi, lo);
        }
        false
    }
}

/// How [`Memory::draw_16x16`] combines a sprite with what is already on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawMode {
    /// Replace the background.
    Overwrite,
    /// OR onto the background and report collisions.
    Blend,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_rows_step_down_a_page_at_a_time() {
        // Each step is one pixel row, which is 256 bytes further into the file.
        // After eight steps we are one character row down but in the next third,
        // which is exactly why the sprite code has to correct for the jump.
        let mut addr = DISPLAY;
        for step in 1..=8u16 {
            addr = next_pixel_row(addr);
            assert_eq!(addr, DISPLAY + step * 256);
        }
        // The high byte wraps, and only the high byte.
        assert_eq!(next_pixel_row(0xff40), 0x0040);
    }

    #[test]
    fn low_byte_arithmetic_stays_in_the_low_byte() {
        assert_eq!(next_cell(0x40ff), 0x4000);
        assert_eq!(add_lsb(0x40f0, 32), 0x4010);
    }

    #[test]
    fn display_rows_follow_the_spectrum_layout() {
        assert_eq!(display_row_offset(0), 0);
        assert_eq!(display_row_offset(1), 256);
        assert_eq!(display_row_offset(8), 32);
        assert_eq!(display_row_offset(64), 2048);
        assert_eq!(display_row_offset(191), 6144 - 32);
    }
}
