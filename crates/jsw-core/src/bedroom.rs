//! The two rooms the game treats specially: Master Bedroom and The Bathroom.
//!
//! Maria stands in the doorway of Master Bedroom until every item is collected,
//! and kills Willy if he touches her. Once they are all in she is gone, and
//! reaching the bed sends him running for the toilet in The Bathroom - which is
//! the whole of the game's ending. The routines are at 39799, 39850 and 39870.

use speccy::layout::{ATTR_BUF, COLUMNS, SCREEN_BUF, cell_offset};
use speccy::memory::{DrawMode, Memory};

use crate::willy::Willy;

/// Master Bedroom, where Maria is.
pub const BEDROOM: usize = 35;

/// The Bathroom, where the toilet is.
pub const BATHROOM: usize = 33;

/// Where Maria stands.
const MARIA_ROW: usize = 11;
const MARIA_COLUMN: usize = 14;

/// Where the toilet is.
const TOILET_ROW: usize = 13;
const TOILET_COLUMN: usize = 28;

/// How far Willy is through the game: the original's game mode indicator at
/// 34271.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Quest {
    /// There are still items to collect, and Maria is on guard.
    #[default]
    Collecting,
    /// Every item is in. Maria has gone, and the bed is worth reaching.
    AllCollected,
    /// He has reached the bed and is running for the toilet, whatever the
    /// player does.
    ToTheToilet,
    /// His head is down it. Nothing moves him now.
    HeadDownTheToilet,
}

impl Quest {
    /// Whether he is on his way to the toilet or already there. The original
    /// keeps this as bit 1 of the mode byte and reads it in three places, all of
    /// which mean "the player is no longer driving".
    pub fn on_the_errand(self) -> bool {
        matches!(self, Self::ToTheToilet | Self::HeadDownTheToilet)
    }
}

/// Draw Maria if she is on guard, reporting that she has caught Willy, and send
/// him off to the toilet if he has reached the bed: the routine at 39799.
pub fn bed(quest: &mut Quest, willy: &Willy, minute: u8, mem: &mut Memory) -> bool {
    if *quest != Quest::Collecting {
        // She is gone. Reaching the bed - the left-hand end of the room - sets
        // him running.
        let (_, column) = willy.position();
        if *quest == Quest::AllCollected && column < 6 {
            *quest = Quest::ToTheToilet;
        }
        return false;
    }

    // Which of her four frames to draw: her foot taps while Willy is on the
    // floor, and she raises her arm as he climbs towards her.
    let frame = if willy.y == 208 {
        // On the floor below the ramp: foot down or foot raised, alternating
        // with the minute counter.
        usize::from(minute & 2 != 0)
    } else if willy.y >= 192 {
        // Eight or fewer pixels above the floor: she starts to raise her arm.
        2
    } else {
        3
    };

    let sprite: [u8; 32] = jsw_data::sprites::MARIA[frame * 32..(frame + 1) * 32]
        .try_into()
        .expect("a Maria frame is 32 bytes");
    let at = SCREEN_BUF + cell_offset(MARIA_ROW, 0, MARIA_COLUMN) as u16;
    let caught = mem.draw_16x16(&sprite, at, DrawMode::Blend);

    // Bright magenta above, white below, in both cells of each row.
    for (row, colour) in [(MARIA_ROW, 69), (MARIA_ROW + 1, 7)] {
        let attr = ATTR_BUF + (row * COLUMNS + MARIA_COLUMN) as u16;
        mem.write(attr, colour);
        mem.write(attr + 1, colour);
    }

    caught
}

/// Animate the toilet in The Bathroom: the routine at 39870.
///
/// Two frames while it waits for him and two more with his head down it, chosen
/// by the bottom bit of the minute counter.
pub fn toilet(quest: Quest, minute: u8, mem: &mut Memory) {
    let mut frame = usize::from(minute & 1);
    if quest == Quest::HeadDownTheToilet {
        frame += 2;
    }

    let sprite = &jsw_data::sprites::TOILET[frame * 32..(frame + 1) * 32];
    for (line, pair) in sprite.chunks_exact(2).enumerate() {
        let y = TOILET_ROW * 8 + line;
        let at = SCREEN_BUF + cell_offset(y / 8, y % 8, TOILET_COLUMN) as u16;
        mem.write(at, mem.read(at) | pair[0]);
        mem.write(at + 1, mem.read(at + 1) | pair[1]);
    }

    for row in [TOILET_ROW, TOILET_ROW + 1] {
        let attr = ATTR_BUF + (row * COLUMNS + TOILET_COLUMN) as u16;
        mem.write(attr, 7);
        mem.write(attr + 1, 7);
    }
}

/// Whether Willy has reached the toilet: the routine at 39850.
///
/// The original compares the low byte of his attribute buffer address with 188,
/// which is column 28 of whichever row he is on - so he arrives at the toilet by
/// being in the right column, wherever he is standing.
pub fn reached_toilet(willy: &Willy) -> bool {
    (willy.cell & 255) == 188
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::room::Room;
    use speccy::layout::ATTR_BACK;

    fn staged(number: usize) -> (Room, Memory) {
        let room = Room::load(number);
        let mut mem = Memory::new();
        room.draw(&mut mem);
        mem.copy(ATTR_BACK, ATTR_BUF, speccy::layout::PLAY_ATTRS);
        mem.copy(
            speccy::layout::SCREEN_BACK,
            SCREEN_BUF,
            speccy::layout::PLAY_PIXELS,
        );
        (room, mem)
    }

    #[test]
    fn maria_stands_in_the_bedroom_until_the_last_item_is_in() {
        let (_, mut mem) = staged(BEDROOM);
        let willy = Willy::default();

        let before: u32 = (0..4096)
            .map(|i| mem.read(SCREEN_BUF + i).count_ones())
            .sum();
        let mut quest = Quest::Collecting;
        bed(&mut quest, &willy, 0, &mut mem);
        let after: u32 = (0..4096)
            .map(|i| mem.read(SCREEN_BUF + i).count_ones())
            .sum();
        assert!(after > before, "Maria was not drawn");

        // With every item collected she is gone.
        let (_, mut mem) = staged(BEDROOM);
        let mut quest = Quest::AllCollected;
        bed(&mut quest, &willy, 0, &mut mem);
        let gone: u32 = (0..4096)
            .map(|i| mem.read(SCREEN_BUF + i).count_ones())
            .sum();
        assert_eq!(gone, before, "Maria is still on guard");
    }

    #[test]
    fn maria_taps_her_foot_and_raises_her_arm() {
        // Four different frames, chosen by the minute counter while Willy is on
        // the floor and by his height as he climbs.
        let mut drawn = std::collections::BTreeSet::new();
        for (y, minute) in [(208, 0), (208, 2), (200, 0), (100, 0)] {
            let (_, mut mem) = staged(BEDROOM);
            let willy = Willy {
                y,
                ..Willy::default()
            };
            bed(&mut Quest::Collecting, &willy, minute, &mut mem);
            let pixels: Vec<u8> = (0..4096).map(|i| mem.read(SCREEN_BUF + i)).collect();
            drawn.insert(pixels);
        }
        assert_eq!(drawn.len(), 4, "Maria has four frames and used fewer");
    }

    #[test]
    fn maria_catches_willy_when_he_walks_into_her() {
        let (_, mut mem) = staged(BEDROOM);
        let willy = Willy::default();

        // Draw Willy's own pixels where Maria stands, which is what she checks
        // against.
        let at = SCREEN_BUF + cell_offset(MARIA_ROW + 1, 0, MARIA_COLUMN) as u16;
        for row in 0..8 {
            mem.write(at + row * 256, 255);
        }
        assert!(bed(&mut Quest::Collecting, &willy, 0, &mut mem));
    }

    #[test]
    fn reaching_the_bed_sends_him_to_the_toilet() {
        let (_, mut mem) = staged(BEDROOM);
        let mut quest = Quest::AllCollected;

        // At the right of the room he keeps looking.
        let willy = Willy {
            cell: ATTR_BUF + 13 * COLUMNS as u16 + 20,
            ..Willy::default()
        };
        bed(&mut quest, &willy, 0, &mut mem);
        assert_eq!(quest, Quest::AllCollected);

        // The bed is at column 5.
        let willy = Willy {
            cell: ATTR_BUF + 13 * COLUMNS as u16 + 5,
            ..Willy::default()
        };
        bed(&mut quest, &willy, 0, &mut mem);
        assert_eq!(quest, Quest::ToTheToilet);
    }

    #[test]
    fn the_toilet_has_two_frames_and_two_more_with_his_head_in_it() {
        let mut drawn = std::collections::BTreeSet::new();
        for (quest, minute) in [
            (Quest::Collecting, 0),
            (Quest::Collecting, 1),
            (Quest::HeadDownTheToilet, 0),
            (Quest::HeadDownTheToilet, 1),
        ] {
            let (_, mut mem) = staged(BATHROOM);
            toilet(quest, minute, &mut mem);
            drawn.insert(
                (0..4096)
                    .map(|i| mem.read(SCREEN_BUF + i))
                    .collect::<Vec<u8>>(),
            );
        }
        assert_eq!(drawn.len(), 4, "the toilet should have four frames");
    }

    #[test]
    fn the_toilet_is_reached_by_column_alone() {
        let willy = Willy {
            cell: ATTR_BUF + 13 * COLUMNS as u16 + 28,
            ..Willy::default()
        };
        assert!(reached_toilet(&willy));

        let willy = Willy {
            cell: ATTR_BUF + 13 * COLUMNS as u16 + 27,
            ..Willy::default()
        };
        assert!(!reached_toilet(&willy));
    }
}
