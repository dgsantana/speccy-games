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
use crate::item::Items;
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
    /// Willy as he was on entering this room. The original saves these seven
    /// bytes at 35146 and puts them back when he dies, which is why dying
    /// returns him to the doorway rather than to where the game began.
    willy_on_entry: Willy,
    pub entities: Entities,
    pub items: Items,
    /// Frames since the clock last moved. The original keeps this as a byte and
    /// advances the clock when it wraps, which also paces the item colours.
    pub minute: u8,
    /// The time of day, from seven in the morning.
    pub clock: crate::hud::Clock,
    /// Rooms Willy has been in, for the debug map. Cheap enough to keep whether
    /// or not anything looks at it.
    visited: Vec<bool>,
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
            willy_on_entry: Willy::default(),
            entities: Entities::default(),
            items: Items::new(),
            minute: 0,
            clock: crate::hud::Clock::default(),
            visited: vec![false; jsw_data::ROOM_COUNT],
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
        if let Some(seen) = self.visited.get_mut(self.room.number) {
            *seen = true;
        }
        self.entities = Entities::load(&self.room);
        self.willy_on_entry = self.willy;
        self.mem.fill(SCREEN_BACK, PLAY_PIXELS, 0);
        self.mem.fill(ATTR_BACK, PLAY_ATTRS, 0);
        self.room.draw(&mut self.mem);
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
        if self.take_exit(outcome) {
            // The original draws the new room and re-enters the main loop, so
            // nothing is drawn against the room just left. Without this the
            // working buffers still hold the old room, and Willy is checked
            // against its attributes - which killed him on the way into any
            // room whose neighbour used 255 for a tile he landed on.
            self.mem.copy(ATTR_BACK, ATTR_BUF, PLAY_ATTRS);
            self.mem.copy(SCREEN_BACK, SCREEN_BUF, PLAY_PIXELS);
        }

        // Drawing Willy also reports him standing in a nasty, because the
        // original finds that out while colouring the cells he covers.
        if self.draw_willy() {
            self.kill();
            self.present();
            return;
        }
        // Items come after Willy, because collecting one is decided by finding
        // white ink in its cell and only Willy's drawing puts white ink there.
        let taken = self.items.draw(
            self.room.number,
            self.minute,
            &self.room.item,
            &mut self.mem,
        );
        for _ in 0..taken {
            // A short high blip per item, as the original's 37897 makes.
            self.sounds.note(16, 4);
        }

        // Guardians are drawn over Willy and report touching him, which is how
        // the original detects the collision.
        if self.entities.draw(&mut self.mem) {
            self.kill();
        }
        // The clock moves on when the frame counter wraps, as the original's
        // 35401 does.
        self.minute = self.minute.wrapping_add(1);
        if self.minute == 0 {
            self.clock.tick();
        }
        self.draw_hud();
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
            let _ = self.draw_willy();
            return;
        }

        if !self.invulnerable() {
            self.lives = self.lives.saturating_sub(1);
        }
        if self.lives == 0 {
            self.mode = Mode::GameOver;
            return;
        }

        // Back to the doorway he came in by, not to where the game started.
        self.willy = self.willy_on_entry;
        self.willy.airborne = 0;
        self.mode = Mode::Playing;
        let room = self.room.number;
        self.enter_room(room);
    }

    /// Follow whichever edge Willy walked off. Reports whether the room changed.
    fn take_exit(&mut self, outcome: Outcome) -> bool {
        let next = match outcome {
            Outcome::Left => self.room.exits.left,
            Outcome::Right => self.room.exits.right,
            Outcome::Above => self.room.exits.up,
            Outcome::Below => self.room.exits.down,
            Outcome::None | Outcome::Died => return false,
        };
        self.willy.enter_from(outcome);
        self.enter_room(next as usize);
        true
    }

    fn draw_willy(&mut self) -> bool {
        let (row, column) = self.willy.position();
        if column + 1 >= COLUMNS {
            return false;
        }

        // On a ramp the drawing routine adds a sub-cell height, which is what
        // makes a climb look smooth rather than blocky.
        let drawn_y = self
            .willy
            .y
            .wrapping_add(self.willy.draw_offset(&self.room, &self.mem));

        // Six cells, three rows of two. The bottom row is only recoloured when
        // his sprite actually reaches into it, which the original decides by
        // looking at the low nibble of his y-coordinate.
        let mut hit_a_nasty = false;
        for cell_row in 0..3usize {
            let reaches = if cell_row == 2 { drawn_y & 15 != 0 } else { true };
            for cell_column in column..(column + 2).min(COLUMNS) {
                let at_row = row + cell_row;
                if at_row >= ROWS {
                    continue;
                }
                hit_a_nasty |= self.colour_cell(at_row, cell_column, reaches);
            }
        }

        let frame = self.willy.sprite_frame();
        let sprite: [u8; 32] = jsw_data::sprites::WILLY[frame * 32..(frame + 1) * 32]
            .try_into()
            .expect("a Willy frame is 32 bytes");

        // His sprite hangs from a pixel offset inside the cell, so it is drawn
        // one pixel row at a time rather than as a tidy 16x16 block.
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

        hit_a_nasty
    }

    /// Colour one cell of Willy's sprite, the routine at 38430. Reports whether
    /// the cell holds a nasty, which kills him.
    ///
    /// Only cells holding the room's background tile are touched, and only their
    /// ink is changed: that is why the bath keeps its colour when Willy sits in
    /// it, and why the floor under his feet is not repainted.
    fn colour_cell(&mut self, row: usize, column: usize, reaches: bool) -> bool {
        let at = ATTR_BUF + (row * COLUMNS + column) as u16;
        let here = self.mem.read(at);
        let background = self.room.tile(crate::room::Kind::Background).attr;

        if here == background && reaches {
            // White ink, and whatever paper and brightness the background has.
            self.mem.write(at, background | 7);
        }

        here == self.room.tile(crate::room::Kind::Nasty).attr
    }
    /// Replace the screen with a map of the mansion, if that switch is on.
    #[inline]
    #[allow(clippy::unused_self)]
    fn draw_map(&mut self) {
        #[cfg(feature = "debug")]
        if self.debug.map {
            let name = self.room.name;
            let number = self.room.number;
            // Lent out and given back, rather than copied every frame.
            let visited = std::mem::take(&mut self.visited);
            crate::map::draw(&mut self.mem, number, &visited, &name);
            self.visited = visited;
        }
    }

    /// The bottom third of the screen: room name, items, clock and lives.
    fn draw_hud(&mut self) {
        let name = self.room.name;
        let frame = (self.minute / 8) as usize;
        crate::hud::draw(
            &mut self.mem,
            &name,
            self.items.collected,
            &self.clock,
            self.lives,
            frame,
        );
    }

    /// Copy the working buffers to the screen the front end reads.
    ///
    /// The map, if it is up, goes on last: it replaces the whole screen, so
    /// anything drawn after it would show through.
    fn present(&mut self) {
        self.blit();
        self.draw_map();
    }

    /// The playing area, from the working buffers to the display file.
    fn blit(&mut self) {
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

    /// Whether losing a life costs one. Folds to false without the `debug`
    /// feature, so the engine is unchanged.
    #[inline]
    #[allow(clippy::unused_self)]
    fn invulnerable(&self) -> bool {
        #[cfg(feature = "debug")]
        {
            self.debug.invulnerable
        }
        #[cfg(not(feature = "debug"))]
        {
            false
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

    /// Rooms a debug jump will visit: the ones that are rooms.
    ///
    /// The last three of the sixty-four blocks hold code, and jumping into one
    /// shows a screen of rubbish with a rubbish name.
    #[cfg(feature = "debug")]
    pub fn real_room_count() -> usize {
        (0..jsw_data::ROOM_COUNT)
            .find(|&n| !Room::load(n).is_real())
            .unwrap_or(jsw_data::ROOM_COUNT)
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
    fn willy_takes_white_ink_and_leaves_the_paper_alone() {
        let mut game = Game::new();
        let background = game.room.tile(crate::room::Kind::Background).attr;

        // Half a cell down, so his sprite reaches into a third row of cells.
        game.willy = Willy {
            y: 5 * willy::ROW_UNITS + 8,
            cell: ATTR_BUF + 5 * COLUMNS as u16 + 10,
            ..Willy::default()
        };
        // Give one of the cells he covers something that is not background, as
        // the bath is in The Bathroom.
        let bath = ATTR_BUF + 6 * COLUMNS as u16 + 10;
        game.mem.write(bath, 56);

        let _ = game.draw_willy();

        for row in 5..=7 {
            let at = ATTR_BUF + (row * COLUMNS + 10) as u16;
            if at == bath {
                assert_eq!(
                    game.mem.read(at),
                    56,
                    "a cell that is not background must be left alone"
                );
            } else {
                assert_eq!(
                    game.mem.read(at),
                    background | 7,
                    "row {row} should have white ink over the background's paper"
                );
            }
        }
    }

    #[test]
    fn the_third_row_is_only_coloured_when_he_reaches_it() {
        let mut game = Game::new();
        let background = game.room.tile(crate::room::Kind::Background).attr;

        // Cell-aligned: his sixteen pixels fit in two rows exactly.
        game.willy = Willy {
            y: 5 * willy::ROW_UNITS,
            cell: ATTR_BUF + 5 * COLUMNS as u16 + 10,
            ..Willy::default()
        };
        let third = ATTR_BUF + 7 * COLUMNS as u16 + 10;
        game.mem.write(third, background);
        let _ = game.draw_willy();
        assert_eq!(
            game.mem.read(third),
            background,
            "the row below him was repainted even though he does not reach it"
        );
    }

    #[test]
    fn standing_in_a_nasty_kills_him() {
        let mut game = Game::new();
        let nasty = game.room.tile(crate::room::Kind::Nasty).attr;
        game.willy = Willy {
            y: 5 * willy::ROW_UNITS,
            cell: ATTR_BUF + 5 * COLUMNS as u16 + 10,
            ..Willy::default()
        };
        game.mem.write(ATTR_BUF + 5 * COLUMNS as u16 + 10, nasty);
        assert!(game.draw_willy(), "a nasty under him went unnoticed");
    }

    #[test]
    fn walking_into_the_next_room_does_not_kill_him() {
        // The frame a room changes on used to leave the previous room's
        // attributes in the working buffer, and The Bathroom uses 255 for its
        // nasty, so stepping left into room 34 killed him on arrival.
        let mut game = Game::new();
        let go_left = speccy::Input {
            left: true,
            ..speccy::Input::default()
        };
        for _ in 0..90 {
            game.update(go_left);
            game.sounds.clear();
        }
        assert_ne!(game.room.number, START_ROOM, "he never left The Bathroom");
        assert_eq!(game.mode, Mode::Playing, "he died on the way in");
        assert_eq!(game.lives, STARTING_LIVES);
    }

    #[test]
    fn dying_returns_him_to_the_door_he_came_in_by() {
        let mut game = Game::new();
        let go_left = speccy::Input {
            left: true,
            ..speccy::Input::default()
        };

        // Walk until the room changes, and note the doorway he arrives at.
        let mut arrived = game.willy.position();
        for _ in 0..90 {
            let before = game.room.number;
            game.update(go_left);
            game.sounds.clear();
            if game.room.number != before {
                arrived = game.willy.position();
                break;
            }
        }
        let room = game.room.number;
        assert_ne!(room, START_ROOM, "he never left The Bathroom");

        // Wander a little, then die.
        for _ in 0..8 {
            game.update(go_left);
            game.sounds.clear();
        }
        game.kill();
        for _ in 0..DEATH_FRAMES + 2 {
            game.update(speccy::Input::default());
            game.sounds.clear();
        }

        assert_eq!(game.room.number, room);
        assert_eq!(
            game.willy.position(),
            arrived,
            "he should reappear where he entered the room"
        );
    }

    #[test]
    fn walking_into_an_item_collects_it() {
        let mut game = Game::new();
        // The Watch Tower holds the first four items.
        game.goto_room(50);
        let item = (crate::item::FIRST..256)
            .find(|&n| game.items.room_of(n) == 50)
            .expect("the Watch Tower has items");

        // Put Willy on the item's own cell, so his drawing forces white ink
        // into it exactly as walking into it would.
        let cell = game.items.cell_of(item);
        let offset = cell - ATTR_BUF;
        game.willy = Willy {
            y: (offset / 32) as u8 * willy::ROW_UNITS,
            cell,
            ..Willy::default()
        };

        assert!(game.items.present(item));
        game.update(speccy::Input::default());
        assert!(!game.items.present(item), "he walked through it");
        assert_eq!(game.items.collected, 1);
        assert_eq!(game.items.remaining(), 82);
    }

    #[test]
    fn a_new_game_has_every_item_to_find() {
        let game = Game::new();
        assert_eq!(game.items.remaining(), 83);
        assert_eq!(game.items.collected, 0);
    }

    #[test]
    #[cfg(feature = "debug")]
    fn invulnerability_costs_no_lives_and_never_ends_the_game() {
        let mut game = Game::new();
        game.debug.invulnerable = true;
        game.lives = 1;

        // Die repeatedly. The death sequence still runs - the original's cheat
        // does not stop that either - but the count must not move, and the game
        // must never reach its end.
        for _ in 0..4 {
            game.kill();
            for _ in 0..DEATH_FRAMES + 2 {
                game.update(speccy::Input::default());
                game.sounds.clear();
            }
            assert_eq!(game.mode, Mode::Playing, "the game ended anyway");
            assert_eq!(game.lives, 1);
        }
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
