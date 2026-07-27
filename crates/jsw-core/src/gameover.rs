//! The game over sequence: a foot comes down on the barrel Willy is standing
//! on, and then "Game Over" glistens for a second and a half.
//!
//! The routine at 35521 does the whole thing in one blocking loop. Here it is a
//! step per frame, because a blocking loop would freeze the window.

use speccy::memory::{ATTR_FILE, DISPLAY, DrawMode, Memory, display_row_offset};
use speccy::sound::SoundQueue;

/// How far the foot has come down. The original counts this in the same units
/// its screen address table uses, so it steps by four for two pixel rows.
pub const STEP: u8 = 4;

/// The distance at which the foot meets the barrel.
pub const LANDS_AT: u8 = 196;

/// Frames the message glistens for: the original's delay works out at about a
/// second and a half.
pub const GLISTEN_FRAMES: u8 = 26;

/// The column everything in the sequence is drawn at.
const COLUMN: u16 = 15;

/// Set the screen up: a blank sheet with Willy standing on a barrel.
pub fn open(mem: &mut Memory) {
    // The top two thirds only; the status area below is left as it was.
    mem.fill(DISPLAY, 4096, 0);

    // The one Willy frame drawn from the front, at (12,15).
    let willy: [u8; 32] = jsw_data::sprites::WILLY[64..96]
        .try_into()
        .expect("a Willy frame is 32 bytes");
    mem.draw_16x16(&willy, at(12), DrawMode::Overwrite);
    mem.draw_16x16(&jsw_data::sprites::BARREL, at(14), DrawMode::Overwrite);
}

/// One step of the foot's descent, reporting whether it has landed.
pub fn descend(distance: u8, mem: &mut Memory, sounds: &mut SoundQueue) -> bool {
    // The foot is drawn without the last one being rubbed out, so what is left
    // behind reads as a leg stretching down from the top of the screen.
    let row = usize::from(distance) / 16;
    let pixel_row = (usize::from(distance) / 2) % 8;
    let where_it_is = DISPLAY + display_row_offset(row * 8 + pixel_row) as u16 + COLUMN;
    mem.draw_16x16(&jsw_data::sprites::FOOT, where_it_is, DrawMode::Overwrite);

    // A note per step, rising as the foot falls.
    sounds.note(255 - distance, 8);

    // The whole screen takes one of four papers, bright white on top of it, and
    // the barrel keeps its own red.
    let paper = ((distance & 12) << 1) | 71;
    for cell in 0..512u16 {
        mem.write(ATTR_FILE + cell, paper);
    }
    let barrel = (paper & 250) | 2;
    for cell in [463, 464, 495, 496] {
        mem.write(ATTR_FILE + cell, barrel);
    }

    distance.wrapping_add(STEP) >= LANDS_AT
}

/// Put the message up.
pub fn message(mem: &mut Memory) {
    mem.print_str("Game", DISPLAY + display_row_offset(6 * 8) as u16 + 10);
    mem.print_str("Over", DISPLAY + display_row_offset(6 * 8) as u16 + 18);
}

/// Run the colours of the message's eight letters on by one.
pub fn glisten(step: u8, mem: &mut Memory) {
    // Four letters of "Game" at (6,10) and four of "Over" at (6,18), each one
    // colour further round than the last.
    let cells = [10u16, 11, 12, 13, 18, 19, 20, 21];
    for (letter, &column) in cells.iter().enumerate() {
        let ink = (step.wrapping_add(letter as u8)) & 7;
        mem.write(ATTR_FILE + 6 * 32 + column, ink | 64);
    }
}

/// Display-file address of the top-left cell of a sprite drawn at `row`.
fn at(row: usize) -> u16 {
    DISPLAY + display_row_offset(row * 8) as u16 + COLUMN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sequence_opens_on_willy_and_the_barrel() {
        let mut mem = Memory::new();
        mem.fill(DISPLAY, 4096, 255);
        open(&mut mem);

        // The screen is cleared apart from what is drawn on it.
        let lit: u32 = (0..4096).map(|i| mem.read(DISPLAY + i).count_ones()).sum();
        assert!(lit > 0, "nothing was drawn");
        // Two sprites' worth, against the 32768 lit pixels it started with.
        assert!(lit < 1000, "the screen was not cleared: {lit} pixels");

        // Both of them are in the middle of the screen, two rows apart.
        let willy: u32 = (0..8)
            .map(|line| mem.read(at(12) + line * 256).count_ones())
            .sum();
        let barrel: u32 = (0..8)
            .map(|line| mem.read(at(14) + line * 256).count_ones())
            .sum();
        assert!(willy > 0 && barrel > 0, "Willy {willy}, barrel {barrel}");
    }

    #[test]
    fn the_foot_comes_down_two_pixel_rows_at_a_time() {
        let mut mem = Memory::new();
        let mut sounds = SoundQueue::default();
        open(&mut mem);

        // Where the top of the foot is, frame by frame.
        let mut tops = Vec::new();
        let mut distance = 0u8;
        loop {
            let landed = descend(distance, &mut mem, &mut sounds);
            let top = (0..128usize).find(|&y| {
                let at = DISPLAY + display_row_offset(y) as u16 + COLUMN;
                mem.read(at) != 0
            });
            tops.push(top);
            if landed {
                break;
            }
            distance += STEP;
        }

        assert_eq!(
            distance + STEP,
            LANDS_AT,
            "the foot stopped in the wrong place"
        );
        assert_eq!(tops[0], Some(0), "the foot starts at the top of the screen");
        // It never rubs out what it has already drawn, so the leg only grows.
        assert!(
            tops.iter().all(|&top| top == Some(0)),
            "the leg was rubbed out behind the foot"
        );
        assert_eq!(sounds.drain().count(), 49, "a note per step of the descent");
    }

    #[test]
    fn the_barrel_stays_red_whatever_the_screen_does() {
        let mut mem = Memory::new();
        let mut sounds = SoundQueue::default();
        let mut papers = std::collections::BTreeSet::new();

        let mut distance = 0u8;
        while distance < LANDS_AT {
            descend(distance, &mut mem, &mut sounds);
            papers.insert(mem.read(ATTR_FILE) & 56);
            for cell in [463, 464, 495, 496] {
                assert_eq!(mem.read(ATTR_FILE + cell) & 7, 2, "the barrel lost its red");
            }
            distance += STEP;
        }
        assert_eq!(papers.len(), 4, "the screen should run through four papers");
    }

    #[test]
    fn the_message_glistens_in_a_different_colour_per_letter() {
        let mut mem = Memory::new();
        message(&mut mem);
        let lit: u32 = (0..4096).map(|i| mem.read(DISPLAY + i).count_ones()).sum();
        assert!(lit > 0, "the message was not printed");

        glisten(0, &mut mem);
        let inks: Vec<u8> = [10u16, 11, 12, 13, 18, 19, 20, 21]
            .iter()
            .map(|&column| mem.read(ATTR_FILE + 6 * 32 + column) & 7)
            .collect();
        assert_eq!(inks, vec![0, 1, 2, 3, 4, 5, 6, 7]);

        // A step on moves every letter one colour round.
        glisten(1, &mut mem);
        let inks: Vec<u8> = [10u16, 11, 12, 13, 18, 19, 20, 21]
            .iter()
            .map(|&column| mem.read(ATTR_FILE + 6 * 32 + column) & 7)
            .collect();
        assert_eq!(inks, vec![1, 2, 3, 4, 5, 6, 7, 0]);
    }
}
