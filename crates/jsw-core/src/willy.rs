//! Willy: where he is, which way he is facing, and what gravity is doing to him.
//!
//! Ported from the routines at 36307, 36564 and 36796. The original keeps him in
//! two pieces that have to agree with each other: a y-coordinate counted in half
//! pixels, and an address in the attribute buffer for the top-left cell his
//! sprite covers. Both are kept here, and [`Willy::sync_cell`] is the original's
//! routine at 36508 that recomputes the second from the first.

use speccy::layout::{ATTR_BUF, COLUMNS, ROWS};

use crate::room::{Kind, Room};

/// Willy's y-coordinate counts half pixels, so a character row is sixteen.
pub const ROW_UNITS: u8 = 16;

/// The airborne counter at which landing kills him.
pub const FATAL_FALL: u8 = 12;

/// Frames in a jump.
pub const JUMP_FRAMES: u8 = 18;

/// Facing and moving, the two bits the original keeps in one byte at 34256.
///
/// The values matter: they index [`MOVEMENT`].
pub mod facing {
    /// Set when Willy faces left.
    pub const LEFT: u8 = 1;
    /// Set when he is moving rather than standing.
    pub const MOVING: u8 = 2;
}

/// The left-right movement table at 33825.
///
/// Indexed by the current facing-and-moving value plus 0 for no input, 4 for
/// left and 8 for right. Turning around costs a frame: pressing left while
/// facing right only turns him, it does not move him.
pub const MOVEMENT: [u8; 12] = [0, 1, 0, 1, 1, 3, 1, 3, 2, 0, 2, 0];

/// What happened to Willy this frame that the room cannot deal with itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing worth telling the game about.
    None,
    /// He walked off an edge and is now in another room.
    Left,
    Right,
    Above,
    Below,
    /// He landed from too great a height.
    Died,
}

/// Willy.
#[derive(Debug, Clone, Copy)]
pub struct Willy {
    /// Half-pixel y-coordinate. A character row is [`ROW_UNITS`].
    pub y: u8,
    /// Address in the working attribute buffer of the top-left cell he covers.
    pub cell: u16,
    /// Facing and moving flags; see [`facing`].
    pub flags: u8,
    /// Animation frame, 0 to 3. Four frames span one cell of walking.
    pub frame: u8,
    /// 0 standing, 1 jumping, 2 and up falling. Reaching [`FATAL_FALL`] before
    /// landing is fatal.
    pub airborne: u8,
    /// How far into a jump he is, 0 to [`JUMP_FRAMES`].
    pub jump_counter: u8,
}

/// Where a new game puts him: (13,20) in The Bathroom, with y=208, which is
/// what the original writes at 34789 and 34799.
pub const START_ROW: usize = 13;
pub const START_COLUMN: usize = 20;
pub const START_Y: u8 = 208;

impl Default for Willy {
    fn default() -> Self {
        Self {
            y: START_Y,
            cell: ATTR_BUF + (START_ROW * COLUMNS + START_COLUMN) as u16,
            flags: 0,
            frame: 0,
            airborne: 0,
            jump_counter: 0,
        }
    }
}

impl Willy {
    /// Character row and column of the top-left cell he covers.
    pub fn position(&self) -> (usize, usize) {
        let offset = self.cell.saturating_sub(ATTR_BUF) as usize;
        (offset / COLUMNS, offset % COLUMNS)
    }

    pub fn facing_left(&self) -> bool {
        self.flags & facing::LEFT != 0
    }

    /// How far below his y-coordinate he is actually drawn, in the same half
    /// pixel units, from the routine at 38344.
    ///
    /// Standing on a ramp, Willy is drawn 0, 2, 4 or 6 pixels lower than his
    /// y-coordinate says, chosen by his animation frame and reversed for a ramp
    /// that climbs to the right. His position still steps a whole cell at a
    /// time; this is what makes the climb look smooth rather than blocky, and
    /// it is why the disassembly notes he can be up to 6 pixels above the ramp
    /// he is standing on.
    pub fn draw_offset(&self, room: &Room, mem: &speccy::Memory) -> u8 {
        if self.airborne != 0 {
            return 0;
        }
        let (row, column) = self.position();
        let climbs_right = room.ramp.direction != 0;

        // The cell under the foot on the side the ramp climbs from.
        let under = if climbs_right { column + 1 } else { column };
        if row + 2 >= ROWS || cell_attr(mem, row + 2, under) != room.tile(Kind::Ramp).attr {
            return 0;
        }

        let step = (self.frame & 3) * 4;
        if climbs_right { 12 - step } else { step }
    }

    /// Which of the eight sprite frames to draw.
    pub fn sprite_frame(&self) -> usize {
        if self.facing_left() {
            4 + self.frame as usize
        } else {
            self.frame as usize
        }
    }

    /// Recompute the attribute-buffer cell from the y-coordinate, keeping the
    /// column. The original's routine at 36508.
    pub fn sync_cell(&mut self) {
        let column = (self.cell.saturating_sub(ATTR_BUF) as usize) % COLUMNS;
        let row = (self.y / ROW_UNITS) as usize;
        self.cell = ATTR_BUF + (row * COLUMNS + column) as u16;
    }

    /// Advance one frame: gravity first, then the keys.
    pub fn update(&mut self, room: &Room, mem: &mut speccy::Memory, input: Input) -> Outcome {
        // Counters 13 and 16 are the points on the way down where his sprite is
        // exactly two and one cell-heights above where the jump started, so
        // they are cell-aligned and worth testing for ground. Without them he
        // sails through every platform until the jump runs out.
        let mut check_ground = matches!(self.jump_counter, 13 | 16) || self.airborne != 1;
        if self.airborne == 1 {
            match self.rise_or_fall_through_jump(room, mem) {
                Outcome::None => {}
                other => return other,
            }
            check_ground = matches!(self.jump_counter, 13 | 16) || self.airborne != 1;
        }

        if check_ground {
            match self.settle(room, mem) {
                Outcome::None => {}
                other => return other,
            }
        }

        if self.airborne >= FATAL_FALL {
            return Outcome::Died;
        }
        self.read_keys(room, mem, input);
        self.walk(room, mem)
    }

    /// The jumping half of the routine at 36307.
    fn rise_or_fall_through_jump(&mut self, room: &Room, mem: &speccy::Memory) -> Outcome {
        // The counter, with bit 0 discarded, less 8: -8 rising, +8 falling.
        let step = (self.jump_counter & 254).wrapping_sub(8);
        self.y = self.y.wrapping_add(step);
        if self.y >= 240 {
            return Outcome::Above;
        }
        self.sync_cell();

        // Hitting a wall with the top of his head ends the jump early.
        let wall = room.tile(Kind::Wall).attr;
        let (row, column) = self.position();
        if cell_attr(mem, row, column) == wall || cell_attr(mem, row, column + 1) == wall {
            self.y = self.y.wrapping_add(16) & 240;
            self.sync_cell();
            self.airborne = 2;
            self.flags &= !facing::MOVING;
            return Outcome::None;
        }

        self.jump_counter += 1;
        if self.jump_counter == JUMP_FRAMES {
            // The jump is over; he keeps falling unless something catches him.
            self.airborne = 6;
        }
        Outcome::None
    }

    /// Standing, landing or falling: the routine from 36406.
    fn settle(&mut self, room: &Room, mem: &speccy::Memory) -> Outcome {
        // Only look for ground when his sprite is cell-aligned.
        if self.y & 14 == 0 {
            let (row, column) = self.position();
            if row + 2 >= ROWS {
                return Outcome::Below;
            }

            let below_left = cell_attr(mem, row + 2, column);
            let below_right = cell_attr(mem, row + 2, column + 1);
            let nasty = room.tile(Kind::Nasty).attr;
            let background = room.tile(Kind::Background).attr;

            let standing = below_left != nasty
                && below_right != nasty
                && (below_left != background || below_right != background);
            if standing {
                if self.airborne >= FATAL_FALL {
                    return Outcome::Died;
                }
                self.airborne = 0;
                return Outcome::None;
            }
        }

        if self.airborne == 1 {
            return Outcome::None;
        }

        // Falling, or about to.
        self.flags &= !facing::MOVING;
        if self.airborne == 0 {
            self.airborne = 2;
            return Outcome::None;
        }
        self.airborne += 1;
        if self.airborne == 16 {
            self.airborne = 12;
        }
        // Four pixels down.
        self.y = self.y.wrapping_add(8);
        self.sync_cell();
        Outcome::None
    }

    /// The keyboard half, from 36564: which way he is being asked to go, and
    /// whether the conveyor has a say.
    fn read_keys(&mut self, room: &Room, mem: &speccy::Memory, input: Input) {
        let mut left = input.left;
        let mut right = input.right;

        // A conveyor under his feet pushes him whether or not a key is held.
        if self.airborne == 0 && self.y & 14 == 0 {
            let (row, column) = self.position();
            if row + 2 < ROWS {
                let conveyor = room.tile(Kind::Conveyor).attr;
                let on_belt = cell_attr(mem, row + 2, column) == conveyor
                    || cell_attr(mem, row + 2, column + 1) == conveyor;
                if on_belt {
                    if room.conveyor.direction == 0 {
                        left = true;
                    } else {
                        right = true;
                    }
                }
            }
        }

        let input_index = if left {
            4
        } else if right {
            8
        } else {
            0
        };
        self.flags = MOVEMENT[input_index + self.flags as usize];

        if input.jump && self.airborne == 0 {
            self.jump_counter = 0;
            self.airborne = 1;
        }
    }

    /// The left-right half, from 36796. Walking a cell takes four frames; the
    /// fourth crosses the boundary.
    fn walk(&mut self, room: &Room, mem: &speccy::Memory) -> Outcome {
        if self.flags & facing::MOVING == 0 {
            return Outcome::None;
        }

        if self.facing_left() {
            if self.frame > 0 {
                self.frame -= 1;
                return Outcome::None;
            }
            self.step(room, mem, false)
        } else {
            if self.frame < 3 {
                self.frame += 1;
                return Outcome::None;
            }
            self.step(room, mem, true)
        }
    }

    /// Move one cell sideways, following a ramp up or down if there is one, and
    /// leaving the room if there is nothing else that way.
    fn step(&mut self, room: &Room, mem: &speccy::Memory, rightwards: bool) -> Outcome {
        let (row, column) = self.position();

        if rightwards {
            if column >= COLUMNS - 2 {
                return Outcome::Right;
            }
        } else if column == 0 {
            return Outcome::Left;
        }

        let target = if rightwards { column + 1 } else { column - 1 };
        let climb = self.ramp_step(room, mem, rightwards);
        let new_row = match climb {
            Climb::Up => row.checked_sub(1),
            Climb::Down => Some(row + 1),
            Climb::Level => Some(row),
        };
        let Some(new_row) = new_row else {
            return Outcome::None;
        };
        if new_row + 2 > ROWS {
            return Outcome::Below;
        }

        // A wall in the way stops him where he stands.
        let wall = room.tile(Kind::Wall).attr;
        let ahead = if rightwards { target + 1 } else { target };
        if cell_attr(mem, new_row, ahead) == wall
            || cell_attr(mem, new_row + 1, ahead) == wall
        {
            return Outcome::None;
        }

        self.y = match climb {
            Climb::Up => self.y.wrapping_sub(ROW_UNITS),
            Climb::Down => self.y.wrapping_add(ROW_UNITS),
            Climb::Level => self.y,
        };
        self.cell = ATTR_BUF + (new_row * COLUMNS + target) as u16;
        self.frame = if rightwards { 0 } else { 3 };
        Outcome::None
    }

    /// Whether the next cell along takes him up a ramp, down one, or neither.
    ///
    /// The original picks one cell to look at, and which one depends on both the
    /// way Willy is going and the way the ramp climbs. Moving left it is the
    /// cell 31 on from his own (one row down, one column left) for a ramp that
    /// climbs left, or 65 on (two rows down, one column right) for one that
    /// climbs right. Moving right it is 64 on (two rows down) or 34 on (one row
    /// down, two columns right). Finding the ramp tile there is what makes him
    /// step up or down instead of along.
    fn ramp_step(&self, room: &Room, mem: &speccy::Memory, rightwards: bool) -> Climb {
        // Mid-jump there is no ramp following; the original skips this entirely.
        if self.airborne != 0 {
            return Climb::Level;
        }
        let (row, column) = self.position();
        let ramp = room.tile(Kind::Ramp).attr;
        let climbs_right = room.ramp.direction != 0;

        let (probe, climb) = match (rightwards, climbs_right) {
            // Walking into the foot of the ramp: he goes up.
            (false, false) => ((row + 1, column.wrapping_sub(1)), Climb::Up),
            (true, true) => ((row + 1, column + 2), Climb::Up),
            // Walking off the top of it: he goes down.
            (false, true) => ((row + 2, column + 1), Climb::Down),
            (true, false) => ((row + 2, column), Climb::Down),
        };

        if probe.0 < ROWS && probe.1 < COLUMNS && cell_attr(mem, probe.0, probe.1) == ramp {
            climb
        } else {
            Climb::Level
        }
    }

    /// Put him where the original puts him when he arrives from another room.
    pub fn enter_from(&mut self, direction: Outcome) {
        match direction {
            Outcome::Left => self.set_column(COLUMNS - 2),
            Outcome::Right => self.set_column(0),
            Outcome::Above => {
                // The bottom floor of the room below.
                self.y = 208;
                self.sync_cell();
                self.airborne = 0;
            }
            Outcome::Below => {
                self.y = 0;
                self.sync_cell();
                if self.airborne < 11 {
                    self.airborne = 2;
                }
            }
            Outcome::None | Outcome::Died => {}
        }
    }

    fn set_column(&mut self, column: usize) {
        let (row, _) = self.position();
        self.cell = ATTR_BUF + (row * COLUMNS + column) as u16;
    }
}

/// Which way a step along a ramp goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Climb {
    Up,
    Down,
    Level,
}

/// What the game is asking Willy to do. A narrower thing than [`speccy::Input`],
/// so the engine cannot accidentally read a key it has no business reading.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub jump: bool,
}

/// The attribute of a cell in the working buffer.
fn cell_attr(mem: &speccy::Memory, row: usize, column: usize) -> u8 {
    if row >= ROWS || column >= COLUMNS {
        return 0;
    }
    mem.read(ATTR_BUF + (row * COLUMNS + column) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use speccy::Memory;
    use speccy::layout::{ATTR_BACK, PLAY_ATTRS};

    /// A room drawn into both buffers, which is what the game loop leaves.
    fn staged(number: usize) -> (Room, Memory) {
        let room = Room::load(number);
        let mut mem = Memory::new();
        room.draw(&mut mem);
        mem.copy(ATTR_BACK, ATTR_BUF, PLAY_ATTRS);
        (room, mem)
    }

    #[test]
    fn a_new_willy_stands_where_the_original_puts_him() {
        // The original writes y=208 at 34789 and the attribute-buffer address
        // 23988, which is (13,20), at 34799.
        let willy = Willy::default();
        assert_eq!(willy.y, 208);
        assert_eq!(willy.position(), (13, 20));
        assert_eq!(willy.cell, 23988);
    }

    #[test]
    fn a_ramp_is_climbed_a_whole_cell_at_a_time() {
        // The Bathroom's ramp climbs up-right from (12,9). Walking it must move
        // him one cell up per cell along, not a pixel at a time.
        let (room, mut mem) = staged(33);
        let mut willy = Willy {
            y: 10 * ROW_UNITS,
            cell: ATTR_BUF + 10 * COLUMNS as u16 + 8,
            flags: facing::MOVING,
            frame: 3,
            ..Willy::default()
        };
        let go_right = Input {
            right: true,
            ..Input::default()
        };

        let mut heights = vec![willy.y];
        for _ in 0..12 {
            willy.update(&room, &mut mem, go_right);
            if *heights.last().expect("seeded") != willy.y {
                heights.push(willy.y);
            }
        }
        // Every change of height is exactly one cell.
        for pair in heights.windows(2) {
            assert_eq!(
                pair[0] - pair[1],
                ROW_UNITS,
                "he changed height by {} units, not a whole cell",
                pair[0] - pair[1]
            );
        }
        assert!(heights.len() > 2, "he never climbed: {heights:?}");
    }

    #[test]
    fn standing_on_a_ramp_draws_him_between_cells() {
        // The Bathroom's ramp climbs right, so the offset counts down from 12
        // as the animation frame counts up: he creeps up a pixel at a time
        // between the whole-cell steps.
        let (room, mut mem) = staged(33);
        assert_eq!(room.ramp.direction, 1);

        let mut willy = Willy {
            y: 10 * ROW_UNITS,
            cell: ATTR_BUF + 10 * COLUMNS as u16 + 8,
            ..Willy::default()
        };
        // The original probes the cell at (row+2, column+1) for a ramp that
        // climbs right. The ramp runs (12,9), (11,10), (10,11) and up, so
        // standing at (9,9) puts (11,10) under his right foot.
        willy.y = 9 * ROW_UNITS;
        willy.cell = ATTR_BUF + 9 * COLUMNS as u16 + 9;

        let offsets: Vec<u8> = (0..4)
            .map(|frame| {
                willy.frame = frame;
                willy.draw_offset(&room, &mem)
            })
            .collect();
        assert_eq!(offsets, vec![12, 8, 4, 0], "ramp offsets for a right climb");

        // Airborne, the ramp is ignored entirely.
        willy.airborne = 1;
        assert_eq!(willy.draw_offset(&room, &mem), 0);

        // Off the ramp there is no offset either.
        willy.airborne = 0;
        willy.cell = ATTR_BUF + 13 * COLUMNS as u16 + 20;
        willy.sync_cell();
        let _ = &mut mem;
        assert_eq!(willy.draw_offset(&room, &mem), 0);
    }

    #[test]
    fn the_movement_table_turns_him_around_before_moving_him() {
        // Facing right and standing, asked to go left: he turns, does not move.
        assert_eq!(MOVEMENT[4], facing::LEFT);
        // Facing left and standing, asked to go left: now he moves.
        assert_eq!(MOVEMENT[4 + 1], facing::LEFT | facing::MOVING);
        // Facing left and moving, asked to go right: he turns and stops.
        assert_eq!(MOVEMENT[8 + 3], 0);
    }

    #[test]
    fn a_cell_is_sixteen_units_of_the_y_coordinate() {
        let mut willy = Willy {
            y: 3 * ROW_UNITS,
            ..Willy::default()
        };
        willy.sync_cell();
        assert_eq!(willy.position().0, 3);
    }

    #[test]
    fn he_falls_when_there_is_nothing_underneath() {
        let (room, mut mem) = staged(0);
        // Column 2 of The Off Licence is open air from the ceiling down.
        let mut willy = Willy {
            y: 2 * ROW_UNITS,
            cell: ATTR_BUF + 2 * COLUMNS as u16 + 2,
            ..Willy::default()
        };
        let start = willy.y;
        for _ in 0..3 {
            willy.update(&room, &mut mem, Input::default());
        }
        assert!(willy.y > start, "he did not fall: y {} -> {}", start, willy.y);
        assert!(willy.airborne >= 2);
    }

    #[test]
    fn the_floor_holds_him_up() {
        let (room, mut mem) = staged(0);
        // Row 13 sits on the floor along the bottom of the room.
        let mut willy = Willy {
            y: 13 * ROW_UNITS,
            cell: ATTR_BUF + 13 * COLUMNS as u16 + 15,
            ..Willy::default()
        };
        for _ in 0..8 {
            willy.update(&room, &mut mem, Input::default());
        }
        assert_eq!(willy.airborne, 0, "he should be standing");
        assert_eq!(willy.y, 13 * ROW_UNITS);
    }

    #[test]
    fn walking_takes_four_frames_to_cross_a_cell() {
        let (room, mut mem) = staged(0);
        let mut willy = Willy {
            y: 13 * ROW_UNITS,
            cell: ATTR_BUF + 13 * COLUMNS as u16 + 15,
            ..Willy::default()
        };
        let go_right = Input {
            right: true,
            ..Input::default()
        };

        // The first press only starts him moving; the column changes on the
        // frame after the fourth animation frame.
        let start_column = willy.position().1;
        for _ in 0..5 {
            willy.update(&room, &mut mem, go_right);
        }
        assert_eq!(willy.position().1, start_column + 1);
        assert!(!willy.facing_left());
    }

    #[test]
    fn walking_off_the_left_edge_leaves_the_room() {
        let (room, mut mem) = staged(0);
        let mut willy = Willy {
            y: 13 * ROW_UNITS,
            cell: ATTR_BUF + 13 * COLUMNS as u16,
            flags: facing::LEFT | facing::MOVING,
            frame: 0,
            ..Willy::default()
        };
        let go_left = Input {
            left: true,
            ..Input::default()
        };
        assert_eq!(willy.update(&room, &mut mem, go_left), Outcome::Left);
    }

    #[test]
    fn arriving_from_the_right_puts_him_on_the_left_edge() {
        let mut willy = Willy::default();
        willy.enter_from(Outcome::Right);
        assert_eq!(willy.position().1, 0);
    }

    #[test]
    fn a_jump_can_land_on_a_higher_ledge() {
        // A floor along the left half at row 8, and a ledge two rows higher
        // along the right half. Willy stands on the floor and jumps right; he
        // has to come down on the ledge rather than sail through it.
        let room = Room::load(0);
        let mut mem = Memory::new();
        let floor = room.tile(Kind::Floor).attr;
        let background = room.tile(Kind::Background).attr;
        for cell in 0..(ROWS * COLUMNS) {
            mem.write(ATTR_BUF + cell as u16, background);
        }
        for column in 0..16 {
            mem.write(ATTR_BUF + (8 * COLUMNS + column) as u16, floor);
        }
        for column in 16..COLUMNS {
            mem.write(ATTR_BUF + (6 * COLUMNS + column) as u16, floor);
        }

        let mut willy = Willy {
            y: 6 * ROW_UNITS,
            cell: ATTR_BUF + 6 * COLUMNS as u16 + 13,
            flags: facing::MOVING,
            frame: 3,
            ..Willy::default()
        };
        let go_right = Input {
            right: true,
            ..Input::default()
        };
        willy.update(
            &room,
            &mut mem,
            Input {
                right: true,
                jump: true,
                ..Input::default()
            },
        );
        for _ in 0..JUMP_FRAMES + 8 {
            willy.update(&room, &mut mem, go_right);
        }

        assert_eq!(willy.airborne, 0, "he never landed");
        assert_eq!(
            willy.position().0,
            4,
            "he should be standing on the ledge at row 4, feet on row 6"
        );
    }

    #[test]
    fn walking_into_a_ramp_climbs_it() {
        // The Off Licence's ramp climbs to the right from (14,23).
        let (room, mut mem) = staged(0);
        assert_eq!(room.ramp.direction, 1);

        let mut willy = Willy {
            y: 13 * ROW_UNITS,
            cell: ATTR_BUF + 13 * COLUMNS as u16 + 21,
            flags: facing::MOVING,
            frame: 3,
            ..Willy::default()
        };
        let go_right = Input {
            right: true,
            ..Input::default()
        };

        let start_row = willy.position().0;
        for _ in 0..12 {
            willy.update(&room, &mut mem, go_right);
        }
        assert!(
            willy.position().0 < start_row,
            "he walked past the ramp instead of up it: row {} -> {}",
            start_row,
            willy.position().0
        );
    }

    #[test]
    fn a_jump_rises_and_comes_back_down() {
        let (room, mut mem) = staged(0);
        let mut willy = Willy {
            y: 13 * ROW_UNITS,
            cell: ATTR_BUF + 13 * COLUMNS as u16 + 15,
            ..Willy::default()
        };
        let jump = Input {
            jump: true,
            ..Input::default()
        };
        willy.update(&room, &mut mem, jump);
        assert_eq!(willy.airborne, 1);

        let mut highest = willy.y;
        for _ in 0..JUMP_FRAMES {
            willy.update(&room, &mut mem, Input::default());
            highest = highest.min(willy.y);
        }
        assert!(highest < 13 * ROW_UNITS, "he never left the ground");
    }
}
