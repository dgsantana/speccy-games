//! The swinging ropes, and Willy hanging from one.
//!
//! A rope is an entity like a guardian, but it is the only one that writes back
//! into Willy: the drawing routine at 37540 finds him by noticing that a segment
//! of rope has been drawn over pixels that are already on the screen, and from
//! then on it decides where he is. The swing itself is the arm of the mover at
//! 37081 that the guardians never reach.
//!
//! Five rooms have one: We must perform a Quirkafleeg, On the Roof, Cold Store,
//! Swimming Pool and The Beach. All five use the same definition - 32 segments
//! hanging from the top of the room, turning back at animation frame 54.

use jsw_data::entities::{ROPE_TABLE, SCREEN_TABLE};
use speccy::memory::{Memory, addr_of};

use crate::entity::BUFFER;
use crate::willy::{LET_GO, Willy, facing};

/// The segment Willy is stopped at when there is no room above to climb into.
const CEILING: u8 = 12;

/// Swing a rope one frame: the rope arm of the mover at 37081.
///
/// The second byte is the animation frame index. Bit 7 of it says which side of
/// the centre the rope is hanging, and the low bits how far from the centre; bit
/// 7 of the first byte says which way it is travelling. The frame index moves in
/// twos, and in fours near the centre where the rope is moving fastest, which is
/// what the two-step arithmetic below is doing.
pub fn step(buffer: &mut [u8; BUFFER]) {
    let mut frame = buffer[1];

    if buffer[0] & 128 == 0 {
        // Swinging right to left.
        if frame & 128 == 0 {
            // Towards the centre, 4 to 54.
            frame = frame.wrapping_sub(2);
            if frame < 20 {
                frame = frame.wrapping_sub(2);
                if frame == 0 {
                    // Through the centre and out the other side.
                    frame = 128;
                }
            }
        } else {
            // Away from the centre, 128 to 180.
            frame = frame.wrapping_add(2);
            if frame < 146 {
                frame = frame.wrapping_add(2);
            }
        }
    } else {
        // Swinging left to right.
        if frame & 128 == 0 {
            // Away from the centre, 0 to 52.
            frame = frame.wrapping_add(2);
            if frame < 18 {
                frame = frame.wrapping_add(2);
            }
        } else {
            // Towards the centre, 132 to 182.
            frame = frame.wrapping_sub(2);
            if frame < 148 {
                frame = frame.wrapping_sub(2);
                if frame == 128 {
                    frame = 0;
                }
            }
        }
    }

    buffer[1] = frame;
    // The eighth byte holds the frame the rope turns back at, which is 54 for
    // every rope in the game.
    if frame & 127 == buffer[7] {
        buffer[0] ^= 128;
    }
}

/// Draw a rope, and move Willy along it if he is holding on: the routine at
/// 37540.
///
/// The rope is drawn from the top down, one pixel per segment, as a single set
/// bit rotated through a byte; each rotation past the end of the byte moves the
/// drawing on to the next cell along. How far to rotate for each segment, and
/// how many pixel rows to drop, both come from [`ROPE_TABLE`], read at an offset
/// that slides with the animation frame.
///
/// The original keeps four working values in the buffers: the x-coordinate and
/// the drawing byte in the rope's own fourth and sixth bytes, and - because it
/// has run out of room - the segment counter and the "Willy is on me" flag in
/// the *second and fourth bytes of the next slot's buffer*. Three of the four
/// are rewritten from scratch on every pass and are locals here; `holding` is
/// the fourth, which has to last from the frame he takes hold to the frame he
/// lets go, so it is kept per slot by [`crate::entity::Entities`].
///
/// The spill is harmless in the game as it stands: in each of the five rope
/// rooms the slot after the rope holds either an arrow, whose second and fourth
/// bytes are unused, or the terminator, whose first byte the rope never touches.
pub fn draw(
    buffer: &[u8; BUFFER],
    holding: &mut bool,
    willy: &mut Willy,
    has_room_above: bool,
    mem: &mut Memory,
) {
    let frame = buffer[1];
    let length = buffer[4];

    let mut segment: u8 = 0;
    let mut column = buffer[2];
    let mut bit: u8 = 128;
    // An index into the screen address table, which is also Willy's
    // y-coordinate in half pixels. A rope hangs from the very top of the room,
    // so it starts at zero.
    let mut y: u8 = 0;

    loop {
        let at = addr_of(
            SCREEN_TABLE[y.wrapping_add(1) as usize],
            SCREEN_TABLE[y as usize].wrapping_add(column),
        );

        // While the status indicator is clear, anything already drawn where a
        // segment of rope is about to go is Willy, and the rope takes hold of
        // him. Nothing else is drawn before the rope: guardians come after it in
        // the same pass, and items after them.
        if willy.rope == 0 && bit & mem.read(at) != 0 {
            willy.rope = segment;
            *holding = true;
        }

        if *holding && willy.rope == segment {
            // He hangs from the segment he caught, so the rope decides his
            // animation frame and where he is drawn. Which frame depends on
            // where in the byte the segment's pixel sits: the outer positions
            // put him a cell further left.
            let mut at_column = column;
            let sprite = if bit < 4 {
                1
            } else if bit < 16 {
                0
            } else if bit < 64 {
                at_column = at_column.wrapping_sub(1);
                3
            } else {
                at_column = at_column.wrapping_sub(1);
                2
            };
            willy.frame = sprite;
            willy.set_column((at_column & 31) as usize);
            // Sixteen half pixels - a whole cell - above the segment, so the
            // rope runs through the middle of his sprite.
            willy.y = y.wrapping_sub(16);
            willy.sync_cell();
        }

        mem.write(at, mem.read(at) | bit);

        // The table is read at the segment's distance from the frame index, so
        // the batch of entries used slides up and down it as the rope swings.
        // The top half holds rotations, the bottom half how far to drop.
        let index = segment.wrapping_add(frame);
        y = y.wrapping_add(ROPE_TABLE[(index | 128) as usize]);
        let rotations = ROPE_TABLE[(index & 127) as usize];

        if rotations != 0 {
            if frame & 128 == 0 {
                for _ in 0..rotations {
                    bit = bit.rotate_right(1);
                    if bit & 128 != 0 {
                        column = column.wrapping_add(1);
                    }
                }
            } else {
                for _ in 0..rotations {
                    bit = bit.rotate_left(1);
                    if bit & 1 != 0 {
                        column = column.wrapping_sub(1);
                    }
                }
            }
        }

        if segment == length {
            break;
        }
        segment += 1;
    }

    ride(buffer, willy, holding, has_room_above, length);
}

/// Willy's own movement along a rope, from 37726: the tail of the drawing
/// routine, which runs once the whole rope has been drawn.
fn ride(
    buffer: &[u8; BUFFER],
    willy: &mut Willy,
    holding: &mut bool,
    has_room_above: bool,
    length: u8,
) {
    if willy.rope & 128 != 0 {
        // He has just let go. The indicator counts up from 240 and wraps to
        // zero sixteen frames later, until when no rope will catch him.
        willy.rope = willy.rope.wrapping_add(1);
        *holding = false;
        return;
    }
    if !*holding || willy.flags & facing::MOVING == 0 {
        return;
    }

    // Facing the way the rope is swinging carries him down it, facing the other
    // way climbs it. The original works that out by lining Willy's direction bit
    // up with the rope's and reading the result as 1 or -1.
    let sense = willy.flags.rotate_right(1) ^ buffer[0];
    let step = (sense.rotate_left(2) & 2).wrapping_sub(1);
    willy.rope = willy.rope.wrapping_add(step);

    // A room whose neighbour above is itself has nowhere to climb to, so he is
    // held a dozen segments down.
    if !has_room_above && willy.rope < CEILING {
        willy.rope = CEILING;
    }

    if willy.rope > length {
        // Off the bottom of the rope. He is squared up to a whole pixel row -
        // which may lift him slightly - before gravity takes him.
        willy.rope = LET_GO;
        willy.y &= 248;
        willy.airborne = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use speccy::layout::{COLUMNS, SCREEN_BUF, cell_offset};

    use crate::entity::{Entities, Kind};
    use crate::room::Room;
    use crate::willy::facing;

    /// On the Roof's rope, which is the definition all five of them share.
    fn a_rope() -> [u8; BUFFER] {
        let entities = Entities::load(&Room::load(18));
        let slot = entities
            .kinds()
            .iter()
            .position(|&kind| kind == Kind::Rope)
            .expect("On the Roof has a rope");
        entities.buffers[slot]
    }

    /// Draw a rope on its own, and report where each of its segments landed, in
    /// order from the top down.
    fn segments(rope: &[u8; BUFFER]) -> Vec<(u16, u8)> {
        let mut mem = Memory::new();
        draw(rope, &mut false, &mut Willy::default(), true, &mut mem);

        let mut found = Vec::new();
        for y in 0..128usize {
            for column in 0..COLUMNS {
                let at = SCREEN_BUF + cell_offset(y / 8, y % 8, column) as u16;
                let byte = mem.read(at);
                if byte != 0 {
                    found.push((at, byte));
                }
            }
        }
        found
    }

    #[test]
    fn a_rope_swings_out_and_back_in_a_closed_cycle() {
        let start = a_rope();
        let mut rope = start;
        let mut turns = 0;
        let mut frames = 0;

        for _ in 0..1000 {
            let before = rope[0];
            step(&mut rope);
            frames += 1;
            if rope[0] != before {
                turns += 1;
            }
            if rope == start {
                break;
            }
        }

        assert_eq!(rope, start, "the swing never came back round");
        assert_eq!(turns, 2, "a full swing turns back exactly twice");
        assert_eq!(frames, 90, "the period of the swing");
    }

    #[test]
    fn a_rope_swings_through_the_centre_rather_than_stopping_at_it() {
        // The frame index runs 0 to 54 on one side and 128 to 182 on the other,
        // and it steps in fours near the centre, where the rope moves fastest.
        let mut rope = a_rope();
        let mut sides = (false, false);
        let mut steps = std::collections::BTreeSet::new();

        for _ in 0..68 {
            let before = rope[1] & 127;
            step(&mut rope);
            sides.0 |= rope[1] & 128 == 0;
            sides.1 |= rope[1] & 128 != 0;
            assert!(rope[1] & 127 <= 54, "past the end of its travel");
            let after = rope[1] & 127;
            if (rope[1] & 128 == 0) == (before <= 54) {
                steps.insert(after.abs_diff(before));
            }
        }

        assert!(sides.0 && sides.1, "it stayed on one side of the centre");
        assert!(steps.contains(&4), "it never sped up through the centre");
    }

    #[test]
    fn a_rope_hangs_from_the_top_of_the_room_to_its_full_length() {
        let rope = a_rope();
        let drawn = segments(&rope);

        // Thirty-two segments below the topmost one, and one pixel each.
        assert_eq!(drawn.len(), usize::from(rope[4]) + 1);
        for (_, byte) in &drawn {
            assert_eq!(byte.count_ones(), 1, "a segment is a single pixel");
        }

        // The first one hangs at the very top, in the column the room gives it.
        let (top, bit) = drawn[0];
        assert_eq!(top, SCREEN_BUF + u16::from(rope[2]));
        assert_eq!(bit, 128, "the rope starts at the left of its cell");
    }

    #[test]
    fn a_swinging_rope_leans_one_way_and_then_the_other() {
        // The lowest segment is the one that travels furthest, so its column is
        // the easiest measure of the swing.
        let mut rope = a_rope();
        let mut columns = std::collections::BTreeSet::new();
        for _ in 0..68 {
            step(&mut rope);
            let drawn = segments(&rope);
            let (at, _) = *drawn.last().expect("the bottom of the rope");
            columns.insert(at % 32);
        }
        let leftmost = *columns.iter().next().expect("it moved");
        let rightmost = *columns.iter().next_back().expect("it moved");
        assert!(
            rightmost - leftmost >= 8,
            "the rope barely swung: columns {leftmost} to {rightmost}"
        );
    }

    /// Put Willy in the way of one segment of the rope and draw it, so that it
    /// takes hold of him exactly there.
    fn grab(rope: &[u8; BUFFER], segment: usize) -> (Willy, bool, Memory) {
        let (at, bit) = segments(rope)[segment];
        let mut mem = Memory::new();
        mem.write(at, bit);

        let mut willy = Willy::default();
        let mut holding = false;
        draw(rope, &mut holding, &mut willy, true, &mut mem);
        (willy, holding, mem)
    }

    #[test]
    fn a_rope_takes_hold_of_willy_where_it_is_drawn_over_him() {
        let rope = a_rope();
        let (willy, holding, _) = grab(&rope, 20);

        assert!(holding, "the rope did not notice him");
        assert_eq!(willy.rope, 20, "it caught him at the wrong segment");
        assert!(willy.on_rope());
    }

    #[test]
    fn a_rope_hangs_him_a_cell_above_the_segment_he_holds() {
        let rope = a_rope();
        let (willy, _, _) = grab(&rope, 20);

        // The segment's own pixel row, in half pixels, less a whole cell.
        let (at, _) = segments(&rope)[20];
        let offset = (at - SCREEN_BUF) as usize;
        let y = (offset / 2048) * 64 + (offset % 256) / 32 * 8 + (offset % 2048) / 256;
        assert_eq!(willy.y, (y as u8) * 2 - 16);
    }

    #[test]
    fn nothing_is_caught_when_the_screen_is_empty() {
        let rope = a_rope();
        let mut mem = Memory::new();
        let mut willy = Willy::default();
        let mut holding = false;
        draw(&rope, &mut holding, &mut willy, true, &mut mem);

        assert!(!holding);
        assert_eq!(willy.rope, 0);
    }

    /// One more frame of the same rope, with Willy already holding on.
    fn ride_on(rope: &[u8; BUFFER], willy: &mut Willy, holding: &mut bool, above: bool) {
        let mut mem = Memory::new();
        draw(rope, holding, willy, above, &mut mem);
    }

    #[test]
    fn facing_the_way_the_rope_swings_carries_him_down_it() {
        let rope = a_rope();
        let (mut willy, mut holding, _) = grab(&rope, 20);

        // Bit 7 of the first byte is reset, so the rope swings right to left;
        // facing left is facing the way it goes.
        assert_eq!(rope[0] & 128, 0);
        willy.flags = facing::LEFT | facing::MOVING;
        ride_on(&rope, &mut willy, &mut holding, true);
        assert_eq!(willy.rope, 21, "he should have slid down a segment");

        willy.flags = facing::MOVING;
        ride_on(&rope, &mut willy, &mut holding, true);
        assert_eq!(willy.rope, 20, "he should have climbed a segment");
    }

    #[test]
    fn standing_still_on_a_rope_stays_put() {
        let rope = a_rope();
        let (mut willy, mut holding, _) = grab(&rope, 20);
        willy.flags = facing::LEFT;
        ride_on(&rope, &mut willy, &mut holding, true);
        assert_eq!(willy.rope, 20);
    }

    #[test]
    fn there_is_no_climbing_past_the_ceiling_with_no_room_above() {
        let rope = a_rope();
        let (mut willy, mut holding, _) = grab(&rope, 14);
        willy.flags = facing::MOVING;

        for _ in 0..8 {
            ride_on(&rope, &mut willy, &mut holding, false);
        }
        assert_eq!(willy.rope, CEILING, "he climbed out of the room");

        // With somewhere to go, the same climb carries on past it.
        let (mut willy, mut holding, _) = grab(&rope, 14);
        willy.flags = facing::MOVING;
        for _ in 0..8 {
            ride_on(&rope, &mut willy, &mut holding, true);
        }
        assert!(willy.rope < CEILING, "he stopped at the ceiling anyway");
    }

    #[test]
    fn sliding_off_the_bottom_drops_him() {
        let rope = a_rope();
        let (mut willy, mut holding, _) = grab(&rope, 30);
        willy.flags = facing::LEFT | facing::MOVING;
        willy.airborne = 3;

        // Segments 31 and 32, and then the one past the end of the rope.
        for _ in 0..3 {
            ride_on(&rope, &mut willy, &mut holding, true);
        }

        assert_eq!(willy.rope, LET_GO, "he stayed on past the end of the rope");
        assert!(!willy.on_rope());
        assert_eq!(willy.airborne, 0, "his fall should start afresh");
        assert_eq!(willy.y & 7, 0, "he should be squared up before falling");
    }

    #[test]
    fn a_rope_will_not_catch_him_again_for_sixteen_frames() {
        let rope = a_rope();
        let mut willy = Willy {
            rope: LET_GO,
            ..Willy::default()
        };
        let mut holding = true;

        // Sixteen passes with him right in the rope's way, and it ignores him.
        for frame in 0..16 {
            let (at, bit) = segments(&rope)[20];
            let mut mem = Memory::new();
            mem.write(at, bit);
            draw(&rope, &mut holding, &mut willy, true, &mut mem);
            assert!(!holding, "the rope caught him on frame {frame}");
            assert!(!willy.on_rope());
        }

        assert_eq!(willy.rope, 0, "the count should have wrapped to zero");

        // And now it can.
        let (willy, holding, _) = grab(&rope, 20);
        assert!(holding && willy.rope == 20);
    }
}
