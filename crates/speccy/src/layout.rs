//! The working buffers a game draws its playing area in.
//!
//! Manic Miner and Jet Set Willy both build a frame in their own copy of the
//! playing area and blit it to the display file, and both put those copies at
//! the same addresses — Jet Set Willy grew out of Manic Miner's code, so this
//! is inheritance rather than coincidence. Two games agreeing is enough to call
//! it part of the machine as this project models it.
//!
//! The playing area is the top sixteen character rows: 4096 pixel bytes and 512
//! attributes.

/// Working attribute buffer for the playing area, 32 columns by 16 rows.
pub const ATTR_BUF: u16 = 23552;
/// Attributes of the empty room, copied into [`ATTR_BUF`] at the start of each frame.
pub const ATTR_BACK: u16 = 24064;
/// Working pixel buffer for the playing area, the top two thirds of a display file.
pub const SCREEN_BUF: u16 = 24576;
/// Pixels of the empty room, copied into [`SCREEN_BUF`] at the start of each frame.
pub const SCREEN_BACK: u16 = 28672;

/// Bytes in a playing-area pixel buffer (16 character rows).
pub const PLAY_PIXELS: usize = 4096;
/// Bytes in a playing-area attribute buffer (16 character rows).
pub const PLAY_ATTRS: usize = 512;

/// Character rows in the playing area.
pub const ROWS: usize = 16;
/// Character columns in the playing area.
pub const COLUMNS: usize = 32;

/// Offset into a playing-area pixel buffer of one pixel row of one cell.
///
/// The buffer is laid out like the display file it is copied to: two thirds,
/// each of eight pixel rows by eight character rows. Manic Miner reads this out
/// of a 128-entry table because a Z80 could not afford the arithmetic; the
/// arithmetic is here, and `mm-core` checks the two against each other.
#[inline]
pub const fn cell_offset(row: usize, pixel_row: usize, column: usize) -> usize {
    (row / 8) * 2048 + pixel_row * 256 + (row % 8) * 32 + column
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_cell_starts_at_the_top_of_the_buffer() {
        assert_eq!(cell_offset(0, 0, 0), 0);
    }

    #[test]
    fn a_pixel_row_is_a_page_below_the_one_above_it() {
        assert_eq!(cell_offset(0, 1, 0), 256);
    }

    #[test]
    fn the_next_character_row_is_thirty_two_bytes_along() {
        assert_eq!(cell_offset(1, 0, 0), 32);
    }

    #[test]
    fn the_second_third_starts_two_thousand_and_forty_eight_bytes_in() {
        assert_eq!(cell_offset(8, 0, 0), 2048);
    }
}
