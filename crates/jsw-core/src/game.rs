//! The game as a state machine, advanced one Spectrum frame at a time.
//!
//! Milestone 2a: the mansion and Willy in it. Nothing kills him yet — no
//! guardians, no items, no lives — so the loop is the original's main loop with
//! the entity and item passes not yet written.

use speccy::layout::{
    ATTR_BACK, ATTR_BUF, COLUMNS, PLAY_ATTRS, PLAY_PIXELS, ROWS, SCREEN_BACK, SCREEN_BUF,
    cell_offset,
};
use speccy::memory::{ATTR_FILE, DISPLAY, Memory};
use speccy::sound::SoundQueue;

use crate::entity::Entities;
use crate::room::Room;
use crate::willy::{self, Outcome, Willy};

/// Jet Set Willy runs at the same pace Manic Miner does.
pub const FRAMES_PER_SECOND: f32 = 17.0;

/// The room Willy starts in: The Bathroom.
pub const START_ROOM: usize = 33;

/// Frames the death sequence lasts.
const DEATH_FRAMES: u8 = 16;

/// Lives a new game starts with, from the original's 34784.
pub const STARTING_LIVES: u8 = 7;

/// What the game is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Playing,
    /// Willy has had a fatal accident; the original flashes him and drops him
    /// back into the room. Counts down.
    Dying(u8),
    /// Out of lives.
    GameOver,
}

/// The whole game.
#[derive(Debug)]
pub struct Game {
    pub mem: Memory,
    pub room: Room,
    pub willy: Willy,
    pub entities: Entities,
    pub lives: u8,
    pub mode: Mode,
    pub sounds: SoundQueue,
    /// Set when the player asks to leave, sending the shell back to the picker.
    pub quit: bool,
    pub paused: bool,
    /// Developer switches. Only exists with the `debug` feature.
    #[cfg(feature = "debug")]
    pub debug: speccy::Debug,
    /// Edge detection so a held key does not repeat.
    prev_input: speccy::Input,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub fn new() -> Self {
        let mut game = Self {
            mem: Memory::new(),
            room: Room::load(START_ROOM),
            willy: Willy::default(),
            entities: Entities::default(),
            lives: STARTING_LIVES,
            mode: Mode::Playing,
            sounds: SoundQueue::default(),
            quit: false,
            paused: false,
            #[cfg(feature = "debug")]
            debug: speccy::Debug::default(),
            prev_input: speccy::Input::default(),
        };
        game.enter_room(START_ROOM);
        game
    }

    /// The border colour of the room being played.
    pub fn border(&self) -> u8 {
        self.room.border
    }

    /// Load a room, draw it into the empty-room buffers, and put its name up.
    fn enter_room(&mut self, number: usize) {
        self.room = Room::load(number % jsw_data::ROOM_COUNT);
        self.entities = Entities::load(&self.room);
        self.mem.fill(SCREEN_BACK, PLAY_PIXELS, 0);
        self.mem.fill(ATTR_BACK, PLAY_ATTRS, 0);
        self.room.draw(&mut self.mem);
        self.draw_room_name();
    }

    /// The room name sits on the bottom two rows, where the original puts it.
    fn draw_room_name(&mut self) {
        let addr = DISPLAY + 4096 + 32 * 4;
        for (index, &byte) in self.room.name.iter().enumerate() {
            self.mem.print_char(byte, addr + index as u16);
        }
        for index in 0..32u16 {
            self.mem.write(ATTR_FILE + 16 * 32 + index, 71);
        }
    }

    /// Advance one frame.
    pub fn update(&mut self, input: speccy::Input) {
        let pressed = speccy::Input {
            pause: input.pause && !self.prev_input.pause,
            ..input
        };
        self.prev_input = input;

        if pressed.back {
            self.quit = true;
            return;
        }
        if pressed.pause {
            self.paused = !self.paused;
        }
        if self.paused {
            return;
        }

        self.sync_debug();

        // Every frame starts from the empty room and redraws what moves.
        self.mem.copy(ATTR_BACK, ATTR_BUF, PLAY_ATTRS);
        self.mem.copy(SCREEN_BACK, SCREEN_BUF, PLAY_PIXELS);

        match self.mode {
            Mode::Dying(step) => {
                self.update_dying(step);
                self.present();
                return;
            }
            Mode::GameOver => {
                self.present();
                return;
            }
            Mode::Playing => {}
        }

        self.entities.step();

        let outcome = self.willy.update(
            &self.room,
            &mut self.mem,
            willy::Input {
                left: input.left,
                right: input.right,
                jump: input.jump,
            },
        );
        if outcome == Outcome::Died {
            self.kill();
            self.present();
            return;
        }
        self.take_exit(outcome);

        self.draw_willy();
        // Guardians are drawn over Willy and report touching him, which is how
        // the original detects the collision.
        if self.entities.draw(&mut self.mem) {
            self.kill();
        }
        self.present();
    }

    /// A fatal accident: the original sets the airborne indicator to 255 and
    /// drops back into the main loop, which starts the death sequence.
    fn kill(&mut self) {
        if self.mode != Mode::Playing {
            return;
        }
        self.willy.airborne = 255;
        self.mode = Mode::Dying(DEATH_FRAMES);
    }

    /// Flash through the death sequence, then put him back in the room he died
    /// in, one life the poorer.
    fn update_dying(&mut self, step: u8) {
        // A rising note per step, as the original's death sound does.
        self.sounds.note(step.wrapping_mul(4) | 7, 8);

        if step > 0 {
            self.mode = Mode::Dying(step - 1);
            self.draw_willy();
            return;
        }

        self.lives = self.lives.saturating_sub(1);
        if self.lives == 0 {
            self.mode = Mode::GameOver;
            return;
        }

        self.willy = Willy::default();
        self.mode = Mode::Playing;
        let room = self.room.number;
        self.enter_room(room);
    }

    /// Follow whichever edge Willy walked off.
    fn take_exit(&mut self, outcome: Outcome) {
        let next = match outcome {
            Outcome::Left => self.room.exits.left,
            Outcome::Right => self.room.exits.right,
            Outcome::Above => self.room.exits.up,
            Outcome::Below => self.room.exits.down,
            // Dying costs a life, which milestone 2b brings in. Until then he
            // simply survives it.
            Outcome::None | Outcome::Died => return,
        };
        self.willy.enter_from(outcome);
        self.enter_room(next as usize);
    }

    fn draw_willy(&mut self) {
        let (row, column) = self.willy.position();
        if column + 1 >= COLUMNS {
            return;
        }

        let frame = self.willy.sprite_frame();
        let sprite: [u8; 32] = jsw_data::sprites::WILLY[frame * 32..(frame + 1) * 32]
            .try_into()
            .expect("a Willy frame is 32 bytes");

        // His sprite hangs from a pixel offset inside the cell, so it is drawn
        // one pixel row at a time rather than as a tidy 16x16 block. On a ramp
        // the offset also carries the sub-cell height the drawing routine adds,
        // which is what makes a climb look smooth.
        let drawn_y = self
            .willy
            .y
            .wrapping_add(self.willy.draw_offset(&self.room, &self.mem));
        let pixel_offset = (drawn_y % willy::ROW_UNITS) as usize / 2;
        for (line, pair) in sprite.chunks_exact(2).enumerate() {
            let y = row * 8 + pixel_offset + line;
            if y >= ROWS * 8 {
                break;
            }
            let at = SCREEN_BUF + cell_offset(y / 8, y % 8, column) as u16;
            self.mem.write(at, self.mem.read(at) | pair[0]);
            self.mem.write(at + 1, self.mem.read(at + 1) | pair[1]);
        }

        // The original colours six cells, three rows of two, whether or not his
        // sprite reaches the third row: sixteen pixels starting part way down a
        // cell cover three rows, and colouring only two leaves his legs drawn
        // in the background's ink, which is usually black on black.
        for cell_row in row..(row + 3).min(ROWS) {
            for cell_column in column..(column + 2).min(COLUMNS) {
                self.mem
                    .write(ATTR_BUF + (cell_row * COLUMNS + cell_column) as u16, 71);
            }
        }
    }

    /// Copy the working buffers to the screen the front end reads.
    fn present(&mut self) {
        self.mem.copy(ATTR_BUF, ATTR_FILE, PLAY_ATTRS);
        for row in 0..ROWS {
            for pixel_row in 0..8 {
                let offset = cell_offset(row, pixel_row, 0);
                for column in 0..COLUMNS {
                    let byte = self.mem.read(SCREEN_BUF + (offset + column) as u16);
                    self.mem.write(DISPLAY + (offset + column) as u16, byte);
                }
            }
        }
    }

    /// Push the debug switches into the parts that read them.
    #[inline]
    #[allow(clippy::unused_self)]
    fn sync_debug(&mut self) {
        #[cfg(feature = "debug")]
        {
            self.entities.disabled = self.debug.no_guardians;
        }
    }

    /// Enter a room directly, for looking at one without walking there.
    ///
    /// Not a cheat in itself — the room dumper uses it too — so it is always
    /// available. What the `debug` feature adds is a key bound to it.
    pub fn goto_room(&mut self, number: usize) {
        self.enter_room(number % jsw_data::ROOM_COUNT);
        self.willy = Willy::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_game_starts_in_the_bathroom() {
        let game = Game::new();
        assert_eq!(game.room.number, START_ROOM);
        assert_eq!(game.room.title, "The Bathroom");
    }

    #[test]
    fn the_room_is_drawn_to_the_display_file() {
        let mut game = Game::new();
        game.update(speccy::Input::default());
        let pixels: u32 = (0..4096)
            .map(|i| game.mem.read(DISPLAY + i).count_ones())
            .sum();
        assert!(pixels > 500, "only {pixels} pixels reached the screen");
    }

    #[test]
    fn walking_off_an_edge_changes_room() {
        let mut game = Game::new();
        let start = game.room.number;
        // Put him at the left edge, already moving, so the next step leaves.
        game.willy = Willy {
            y: 13 * willy::ROW_UNITS,
            cell: ATTR_BUF + 13 * COLUMNS as u16,
            flags: willy::facing::LEFT | willy::facing::MOVING,
            frame: 0,
            ..Willy::default()
        };
        game.update(speccy::Input {
            left: true,
            ..speccy::Input::default()
        });
        assert_ne!(game.room.number, start, "he stayed put");
        assert_eq!(game.room.number, usize::from(Room::load(start).exits.left));
    }

    #[test]
    fn every_room_can_be_entered_and_stepped() {
        for number in 0..jsw_data::ROOM_COUNT {
            let mut game = Game::new();
            game.room = Room::load(number);
            game.enter_room(number);
            for _ in 0..20 {
                game.update(speccy::Input::default());
                game.sounds.clear();
            }
        }
    }

    #[test]
    fn a_sprite_straddling_a_boundary_is_coloured_over_three_rows() {
        let mut game = Game::new();
        // Half a cell down: his sixteen pixels now cover rows 5, 6 and 7.
        game.willy = Willy {
            y: 5 * willy::ROW_UNITS + 8,
            cell: ATTR_BUF + 5 * COLUMNS as u16 + 10,
            ..Willy::default()
        };
        game.draw_willy();

        for row in 5..=7 {
            assert_eq!(
                game.mem.read(ATTR_BUF + (row * COLUMNS + 10) as u16),
                71,
                "row {row} of his sprite was left in the background's colour"
            );
        }
    }

    #[test]
    fn a_guardian_costs_a_life_and_puts_him_back_in_the_room() {
        let mut game = Game::new();
        let room = game.room.number;
        assert_eq!(game.lives, STARTING_LIVES);

        // A fatal accident, however he came by it.
        game.kill();
        assert!(matches!(game.mode, Mode::Dying(_)));

        // Run the sequence out.
        for _ in 0..DEATH_FRAMES + 2 {
            game.update(speccy::Input::default());
            game.sounds.clear();
        }
        assert_eq!(game.mode, Mode::Playing);
        assert_eq!(game.lives, STARTING_LIVES - 1);
        assert_eq!(game.room.number, room, "he should reappear where he died");
    }

    #[test]
    fn running_out_of_lives_ends_the_game() {
        let mut game = Game::new();
        game.lives = 1;
        game.kill();
        for _ in 0..DEATH_FRAMES + 2 {
            game.update(speccy::Input::default());
            game.sounds.clear();
        }
        assert_eq!(game.mode, Mode::GameOver);
        assert_eq!(game.lives, 0);
    }

    #[test]
    fn entering_a_room_loads_its_guardians() {
        let mut game = Game::new();
        game.goto_room(0);
        // The Off Licence names three.
        assert_eq!(game.entities.kinds().len(), 3);
    }

    #[test]
    fn escape_asks_to_leave() {
        let mut game = Game::new();
        game.update(speccy::Input {
            back: true,
            ..speccy::Input::default()
        });
        assert!(game.quit);
    }
}
