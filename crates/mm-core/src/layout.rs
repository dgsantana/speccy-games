//! Where Manic Miner keeps its playing-area buffers.
//!
//! The buffers themselves are in [`speccy::layout`], because Jet Set Willy uses
//! the same addresses. What is left here is the lookup table the original used
//! to find a pixel row inside them, which lives in Manic Miner's data.

pub use speccy::layout::{
    ATTR_BACK, ATTR_BUF, PLAY_ATTRS, PLAY_PIXELS, SCREEN_BACK, SCREEN_BUF,
};

/// Address in the playing-area pixel buffer of the start of pixel row `row`.
///
/// The original kept this as a 128-entry lookup table because working it out on
/// a Z80 cost more than the table did.
#[inline]
pub fn screen_row_addr(row: u8) -> u16 {
    mm_data::caverns::SCREEN_BUFFER_ADDRS[(row & 127) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_agrees_with_the_arithmetic() {
        // speccy works the address out; the original looked it up. They must
        // describe the same buffer.
        for row in 0..128usize {
            let computed = SCREEN_BUF as usize
                + speccy::layout::cell_offset(row / 8, row % 8, 0);
            assert_eq!(
                screen_row_addr(row as u8) as usize,
                computed,
                "pixel row {row}"
            );
        }
    }
}
