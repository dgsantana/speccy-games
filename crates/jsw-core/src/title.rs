//! The title screen: the picture, Moonlight Sonata, and the scrolling message.
//!
//! Ported from the routine at 34762. The picture is not a bitmap. The game
//! holds 512 attribute bytes and four triangle characters, and builds the
//! drawing of the house by putting one triangle in every cell whose attribute
//! is not one of the flat values - which is why it takes so little room and why
//! it looks the way it does.
//!
//! The tune is the routine at 39136, which is not the one the game uses: it
//! plays each byte of the tune as a hundred short notes, dropping an octave
//! halfway through, and its loop is 40 T-states rather than the 56 of the
//! sound the rest of the game makes. Both of those have to be undone to turn a
//! byte of the table into a note our beeper plays; see [`note`].

use speccy::memory::{ATTR_FILE, DISPLAY, Memory, display_row_offset};
use speccy::sound::SoundQueue;

/// Character row the message scrolls along, which is the row the item count and
/// clock use once the game starts.
const MESSAGE_ROW: usize = 19;

/// Characters of the message shown at once, being the width of the screen.
const WINDOW: usize = 32;

/// Steps of the scroll before the original starts the whole title over.
pub const SCROLL_STEPS: usize = 224;

/// Halves of a note in the theme tune: the tune plays each byte at one pitch
/// and then an octave below it.
pub const THEME_HALVES: usize = jsw_data::music::THEME.len() * 2;

/// Duration units in half a note of the theme.
///
/// The original plays fifty short notes of 256 iterations each, and one of its
/// iterations is 40 T-states against the 56 our duration unit is measured in.
const HALF_NOTE: u8 = 36;

/// The attribute values that are flat colour rather than part of the picture.
const FLAT: [u8; 5] = [0, 211, 9, 45, 36];

/// The attribute the message row has while the tune plays, and while it scrolls.
const MESSAGE_ATTR: u8 = 70;
const SCROLLING_ATTR: u8 = 79;

/// Draw the title screen: the routine at 34799.
pub fn draw(mem: &mut Memory) {
    mem.fill(DISPLAY, 6144, 0);
    mem.load(ATTR_FILE, &jsw_data::title::ATTRS);
    mem.load(ATTR_FILE + 512, &jsw_data::hud::ATTRS);
    mem.fill(ATTR_FILE + (MESSAGE_ROW * 32) as u16, 32, MESSAGE_ATTR);
    print_window(mem, 0);

    // A triangle in every cell that is part of the picture. Which of the four
    // is decided by the attribute value and by whether the column is odd, so
    // the triangles alternate along a row and the house has diagonals.
    for cell in 0..512usize {
        let at = ATTR_FILE + cell as u16;
        let attr = mem.read(at);
        if FLAT.contains(&attr) {
            continue;
        }

        // 44 is drawn as a triangle and recoloured while it is at it.
        if attr == 44 {
            mem.write(at, 37);
        }
        let pair = usize::from(!matches!(attr, 8 | 41 | 5 | 44)) * 2;
        let triangle = jsw_data::title::TRIANGLES[pair + cell % 2];

        let (row, column) = (cell / 32, cell % 32);
        for (line, &byte) in triangle.iter().enumerate() {
            let to = DISPLAY + display_row_offset(row * 8 + line) as u16 + column as u16;
            mem.write(to, byte);
        }
    }
}

/// One note of the theme tune, or `None` once it has finished.
///
/// `half` counts halves of a note: the original plays fifty short notes at the
/// pitch in the table and fifty more at half the frequency, which is the octave
/// drop you can hear under the melody. Our beeper's pitch byte counts 56
/// T-state iterations where the title routine's counts 40, so the table's byte
/// is scaled by 5/7 to come out at the same frequency.
pub fn note(half: usize) -> Option<(u8, u8)> {
    let byte = *jsw_data::music::THEME.get(half / 2)?;
    if byte == 255 {
        return None;
    }
    let delay = if half.is_multiple_of(2) {
        u16::from(byte)
    } else {
        u16::from(byte) * 2
    };
    Some(((delay * 5 / 7) as u8, HALF_NOTE))
}

/// One step of the message scrolling across the bottom of the screen, with the
/// blip the original makes at 39178 for each.
pub fn scroll(mem: &mut Memory, step: usize, sounds: &mut SoundQueue) {
    mem.fill(ATTR_FILE + (MESSAGE_ROW * 32) as u16, 32, SCROLLING_ATTR);
    print_window(mem, step);

    // A value between 50 and 81, which the sound routine uses as both the pitch
    // and the length of its chirp: it counts down from there, delaying by the
    // counter each pass, so the note is roughly the square of it.
    let pitch = (step % 32) as u8 + 50;
    let iterations = u32::from(pitch) * u32::from(pitch) / 2;
    sounds.note(pitch, ((iterations / 256) as u8).max(1));
}

/// Print the thirty-two characters of the message that start at `from`.
fn print_window(mem: &mut Memory, from: usize) {
    let at = DISPLAY + display_row_offset(MESSAGE_ROW * 8) as u16;
    for (column, ch) in jsw_data::title::INTRO_MESSAGE
        .chars()
        .skip(from)
        .take(WINDOW)
        .enumerate()
    {
        // The copyright sign is 127 in the Spectrum's character set, which is
        // the last glyph of the font rather than anything Unicode agrees with.
        let byte = if ch == '©' { 127 } else { ch as u8 };
        mem.print_char(byte, at + column as u16);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_is_long_enough_to_scroll_all_the_way() {
        let length = jsw_data::title::INTRO_MESSAGE.chars().count();
        assert_eq!(length, 256, "the message is 256 characters in the original");
        assert!(
            SCROLL_STEPS + WINDOW <= length,
            "the scroll runs off the end"
        );
    }

    #[test]
    fn the_title_screen_draws_a_picture_and_a_message() {
        let mut mem = Memory::new();
        draw(&mut mem);

        let lit: u32 = (0..4096).map(|i| mem.read(DISPLAY + i).count_ones()).sum();
        assert!(lit > 0, "the picture is blank");

        // The message row says what it should, in the ROM font.
        let at = DISPLAY + display_row_offset(MESSAGE_ROW * 8) as u16;
        let plus = speccy::font::FONT[('+' as usize) - 32];
        assert_eq!(mem.read(at), plus[0], "the message is not there");

        // 44 is recoloured to 37 as the picture is drawn.
        assert!(
            (0..512).all(|cell| mem.read(ATTR_FILE + cell) != 44),
            "a cell kept the colour the original changes"
        );
    }

    #[test]
    fn the_theme_plays_and_then_stops() {
        assert!(note(0).is_some());
        // Each byte is played twice, an octave apart.
        let (first, _) = note(0).expect("a note");
        let (second, _) = note(1).expect("a note");
        // Within a byte of twice the pitch: the scaling is integer arithmetic.
        assert!(
            second.abs_diff(first * 2) <= 1,
            "the second half is an octave down"
        );

        // The table ends with 255, and the tune stops there rather than playing
        // it.
        let end = jsw_data::music::THEME.len() - 1;
        assert_eq!(jsw_data::music::THEME[end], 255);
        assert!(note(end * 2).is_none(), "the terminator was played");
    }
}
