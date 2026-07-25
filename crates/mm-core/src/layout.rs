//! Where Manic Miner keeps its working buffers.
//!
//! The game draws a frame into its own copy of the playing area and blits that
//! to the display file, so it needs four buffers alongside the Spectrum's real
//! ones. The addresses are the original's; another game of the era would pick
//! its own, which is why these live here rather than in `speccy`.

/// Working attribute buffer for the playing area, 32 columns by 16 rows.
pub const ATTR_BUF: u16 = 23552;
/// Attributes of the empty cavern, copied into [`ATTR_BUF`] at the start of each frame.
pub const ATTR_BACK: u16 = 24064;
/// Working pixel buffer for the playing area, the top two thirds of a display file.
pub const SCREEN_BUF: u16 = 24576;
/// Pixels of the empty cavern, copied into [`SCREEN_BUF`] at the start of each frame.
pub const SCREEN_BACK: u16 = 28672;

/// Bytes in a playing-area pixel buffer (16 character rows).
pub const PLAY_PIXELS: usize = 4096;
/// Bytes in a playing-area attribute buffer (16 character rows).
pub const PLAY_ATTRS: usize = 512;

/// Address in the playing-area pixel buffer of the start of pixel row `row`.
///
/// The original kept this as a 128-entry lookup table because working it out on
/// a Z80 cost more than the table did.
#[inline]
pub fn screen_row_addr(row: u8) -> u16 {
    mm_data::caverns::SCREEN_BUFFER_ADDRS[(row & 127) as usize]
}
