//! The game as a state machine, advanced one original frame at a time.
//!
//! The 1983 code ran its death, cavern-change and game-over sequences as blocking
//! loops. Here each of those is a [`Mode`] that advances by one step per call to
//! [`Game::update`], so the front end always stays responsive and every sequence
//! runs at the same 17 frames per second as the game itself.

use crate::cavern::Cavern;
use crate::guardian::Guardians;
use crate::layout::{
    ATTR_BACK, ATTR_BUF, PLAY_ATTRS, PLAY_PIXELS, SCREEN_BACK, SCREEN_BUF, screen_row_addr,
};
use crate::score::Score;
use crate::special::{self, Specials};
use speccy::input::Input;
use speccy::memory::{ATTR_FILE, DISPLAY, DISPLAY_LEN, DrawMode, Memory, addr_of, rot_l, rot_r};
use speccy::sound::{Sound, SoundQueue};
use crate::willy::{self, Willy};

/// The game runs at the Spectrum's own pace: 17 frames per second.
pub const FRAMES_PER_SECOND: f32 = 17.0;

/// Notes in the title tune.
const THEME_NOTES: usize = mm_data::music::THEME_TUNE.len();
/// Characters of the scrolling message, which is longer than the screen is wide.
const SCROLL_STEPS: usize = 292;

/// How long a note lasts, per unit of its duration byte: 256 iterations of the
/// original's sound loop at 56 T-states each on a 3.5 MHz Z80.
const BEEP_UNIT_SECONDS: f32 = 256.0 * 56.0 / 3_500_000.0;

/// Duration units elapsed per frame, in 1/256ths.
///
/// The theme tune runs far slower than the game does — a note lasts 206ms or
/// 330ms against a 59ms frame — so notes cannot simply advance once per frame.
/// This clock accumulates fractional units so the average tempo is exact and
/// the error never compounds, which keeps the piano keys in step with the sound.
const TUNE_UNITS_PER_FRAME: u32 =
    (256.0 / (FRAMES_PER_SECOND * BEEP_UNIT_SECONDS)) as u32;

/// What the game is currently doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Title screen: play the theme, then scroll the message until Enter.
    Title { note: usize, scroll: usize },
    /// Normal play.
    Playing,
    /// Willy died; the playing area drains to black over seven frames.
    Dying { ink: u8 },
    /// Cavern cleared: cycle the colours, then convert the remaining air to score.
    Cleared { phase: ClearPhase },
    /// Out of lives: the boot descends, then the message glistens.
    GameOver { step: u32 },
}

/// The two halves of the cavern-cleared sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearPhase {
    /// Attribute value counting down from 63 to 1.
    Cycle(u8),
    /// Trading air for points.
    AirBonus,
}

/// The whole game.
#[derive(Debug)]
pub struct Game {
    pub mem: Memory,
    pub cavern: Cavern,
    pub willy: Willy,
    pub guardians: Guardians,
    pub specials: Specials,
    pub score: Score,
    pub sounds: SoundQueue,
    pub mode: Mode,
    /// Cheat mode: lives are never lost and the boot is shown.
    pub cheat: bool,
    /// Developer switches. Only exists with the `debug` feature.
    #[cfg(feature = "debug")]
    pub debug: speccy::Debug,
    /// Cavern to start in, for the original's teleport cheat.
    pub start_cavern: usize,
    /// In-game tune on or off.
    pub music_on: bool,
    pub paused: bool,
    /// Set when the player asks to leave, sending the shell back to the picker.
    pub quit: bool,
    /// Counts down an extra-life screen flash.
    flash: u8,
    /// Index into the in-game tune, which also drives the lives animation.
    note_index: u8,
    /// Attribute of the last item drawn; zero once every item has been collected.
    item_attr: u8,
    /// Edge detection so a held key does not repeat.
    prev_input: Input,
    /// Title-tune position, in 1/256ths of a duration unit into the current note.
    tune_clock: u32,
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
            cavern: Cavern::load(0),
            willy: Willy::default(),
            guardians: Guardians::load(0),
            specials: Specials::default(),
            score: Score::default(),
            sounds: SoundQueue::default(),
            mode: Mode::Title { note: 0, scroll: 0 },
            cheat: false,
            #[cfg(feature = "debug")]
            debug: speccy::Debug::default(),
            start_cavern: 0,
            music_on: true,
            paused: false,
            quit: false,
            flash: 0,
            note_index: 0,
            item_attr: 0,
            prev_input: Input::default(),
            tune_clock: 0,
        };
        game.draw_title_screen();
        game
    }

    /// The border colour the front end should paint around the screen.
    pub fn border(&self) -> u8 {
        match self.mode {
            Mode::Title { .. } | Mode::Dying { .. } | Mode::GameOver { .. } => 0,
            Mode::Playing | Mode::Cleared { .. } => self.cavern.border,
        }
    }

    //
    // Debug switches
    //
    // Each of these folds to a constant without the `debug` feature, so the
    // engine compiles to what it compiled to before the switches existed.
    //

    /// Whether losing a life costs one.
    #[inline]
    fn invulnerable(&self) -> bool {
        #[cfg(feature = "debug")]
        {
            self.cheat || self.debug.invulnerable
        }
        #[cfg(not(feature = "debug"))]
        {
            self.cheat
        }
    }

    /// Whether the air is being burned.
    // Reads nothing without the `debug` feature, which is the point.
    #[allow(clippy::unused_self)]
    #[inline]
    fn air_drains(&self) -> bool {
        #[cfg(feature = "debug")]
        {
            !self.debug.frozen_air
        }
        #[cfg(not(feature = "debug"))]
        {
            true
        }
    }

    /// Whether a guardian can kill Willy. Eugene and Kong keep moving whatever
    /// this says, because the caverns they are in cannot be finished otherwise.
    // Reads nothing without the `debug` feature, which is the point.
    #[allow(clippy::unused_self)]
    #[inline]
    fn guardians_live(&self) -> bool {
        #[cfg(feature = "debug")]
        {
            !self.debug.no_guardians
        }
        #[cfg(not(feature = "debug"))]
        {
            true
        }
    }

    /// Push the switches down into the parts that read them. Called every frame
    /// because entering a cavern loads a fresh set of guardians.
    // Reads nothing without the `debug` feature, which is the point.
    #[allow(clippy::unused_self)]
    #[inline]
    fn sync_debug(&mut self) {
        #[cfg(feature = "debug")]
        {
            self.guardians.disabled = self.debug.no_guardians;
        }
    }

    /// Enter a cavern directly, for looking at one without playing up to it.
    ///
    /// The score, lives and high score are left alone, so this resumes nothing:
    /// it puts Willy at the start of `sheet` with a full air supply. Sheets
    /// outside the twenty wrap around.
    #[cfg(feature = "debug")]
    pub fn goto_cavern(&mut self, sheet: usize) {
        if self.mode != Mode::Playing {
            return;
        }
        self.enter_cavern(sheet % mm_data::CAVERN_COUNT);
    }

    /// Advance one frame.
    pub fn update(&mut self, input: Input) {
        let pressed = Input {
            left: input.left,
            right: input.right,
            up: input.up,
            down: input.down,
            jump: input.jump,
            start: input.start && !self.prev_input.start,
            pause: input.pause && !self.prev_input.pause,
            mute: input.mute && !self.prev_input.mute,
            back: input.back,
        };
        self.prev_input = input;

        if pressed.back {
            self.quit = true;
            return;
        }

        match self.mode {
            Mode::Title { note, scroll } => self.update_title(pressed, note, scroll),
            Mode::Playing => self.update_playing(pressed),
            Mode::Dying { ink } => self.update_dying(ink),
            Mode::Cleared { phase } => self.update_cleared(phase),
            Mode::GameOver { step } => self.update_game_over(step),
        }
    }

    //
    // Title screen
    //

    fn draw_title_screen(&mut self) {
        self.tune_clock = 0;
        self.mem.fill(DISPLAY, DISPLAY_LEN, 0);
        self.mem.load(DISPLAY, &mm_data::title::TITLE_SCREEN_PIXELS);
        self.mem
            .load(DISPLAY + 2048, &mm_data::title::TITLE_SCREEN_ATTRS);

        // Willy stands at the right-hand side of the title graphic.
        let sprite = willy::frame_at(&mm_data::sprites::WILLY, 64);
        self.mem.draw_16x16(&sprite, 18493, DrawMode::Overwrite);

        // The Final Barrier's layout doubles as the title screen's top-third colours.
        self.mem
            .load(ATTR_FILE, &mm_data::caverns::LAYOUTS[mm_data::FINAL_BARRIER][..256]);
        self.mem
            .load(ATTR_FILE + 256, &mm_data::title::LOWER_ATTRS);
    }

    fn update_title(&mut self, input: Input, note: usize, scroll: usize) {
        if input.start {
            self.start_game();
            return;
        }

        if note < THEME_NOTES {
            let [duration, first, second] = mm_data::music::THEME_TUNE[note];

            // The clock always carries less than one frame of credit into a new
            // note, so this is true on exactly the first frame of each one.
            if self.tune_clock < TUNE_UNITS_PER_FRAME {
                if note > 0 {
                    let [_, prev_first, prev_second] = mm_data::music::THEME_TUNE[note - 1];
                    self.mem.write(piano_key(prev_first), 56);
                    self.mem.write(piano_key(prev_second), 56);
                }
                self.mem.write(piano_key(first), 80);
                self.mem.write(piano_key(second), 40);
                self.sounds.push(Sound::Chord {
                    first,
                    second,
                    duration,
                });
            }

            self.tune_clock += TUNE_UNITS_PER_FRAME;
            let note_length = u32::from(duration) * 256;
            if self.tune_clock >= note_length {
                self.tune_clock -= note_length;
                self.mode = Mode::Title {
                    note: note + 1,
                    scroll,
                };
            }
            return;
        }

        // The tune is over; scroll the message across the status area.
        let text = mm_data::title::INTRO_MESSAGE;
        let window: String = text
            .chars()
            .cycle()
            .skip(scroll)
            .take(32)
            .collect();
        self.mem.print_str(&window, 20576);

        // Willy walks on the spot in time with the scroll.
        let frame = rot_r((scroll as u8) & 6, 3);
        let sprite = willy::frame_at(&mm_data::sprites::WILLY, frame);
        self.mem.draw_16x16(&sprite, 18493, DrawMode::Overwrite);

        self.mode = Mode::Title {
            note,
            scroll: (scroll + 1) % SCROLL_STEPS,
        };
    }

    fn start_game(&mut self) {
        self.score.reset();
        self.willy = Willy::default();
        self.note_index = 0;
        self.flash = 0;
        self.mem.fill(DISPLAY, DISPLAY_LEN, 0);
        // Only the playing area's attributes are cleared. The status area keeps
        // the colours the title screen left behind, which is what the original
        // relied on to get white text on black down there.
        self.mem.fill(ATTR_FILE, PLAY_ATTRS, 0);
        self.enter_cavern(self.start_cavern);
        self.mode = Mode::Playing;
    }

    //
    // Cavern setup
    //

    /// Load a cavern and draw everything that only needs drawing once.
    fn enter_cavern(&mut self, sheet: usize) {
        self.cavern = Cavern::load(sheet);
        self.guardians = Guardians::load(sheet);
        self.specials = Specials::default();
        self.willy.enter_cavern(sheet);
        self.item_attr = 0;

        self.mem.fill(ATTR_BUF, PLAY_ATTRS, 0);
        self.mem.fill(ATTR_BACK, PLAY_ATTRS, 0);
        self.mem.fill(SCREEN_BUF, PLAY_PIXELS, 0);
        self.mem.fill(SCREEN_BACK, PLAY_PIXELS, 0);

        self.cavern.draw_empty(&mut self.mem);
        self.cavern.draw_status(&mut self.mem);
        self.mem
            .print_str("High Score 000000   Score 000000", 20576);
    }

    //
    // Playing
    //

    fn update_playing(&mut self, input: Input) {
        if input.mute {
            self.music_on = !self.music_on;
        }
        if input.pause {
            self.paused = !self.paused;
        }
        if self.paused {
            return;
        }
        self.sync_debug();

        willy::draw_lives(&self.willy, &mut self.mem, self.note_index, self.cheat);

        // Start from the empty cavern each frame; everything moving is redrawn.
        self.mem.copy(ATTR_BACK, ATTR_BUF, PLAY_ATTRS);
        self.mem.copy(SCREEN_BACK, SCREEN_BUF, PLAY_PIXELS);

        self.guardians.move_horizontal(&self.cavern);

        let mut alive = !willy::update(
            &mut self.willy,
            &self.cavern,
            &mut self.mem,
            input,
            &mut self.sounds,
        );
        if alive && willy::draw(&self.willy, &self.cavern, &mut self.mem) {
            // A nasty in one of the cells he covers, found while colouring
            // them: the original's 37471 goes to KILLWILLY, which sets the
            // airborne indicator to 255. Without that he stood in it for ever
            // and the guardians, the items and the specials went undrawn every
            // frame, because the rest of the loop is skipped once he is hit.
            self.willy.kill();
            alive = false;
        }
        if alive && self.guardians.draw_horizontal(&self.cavern, &mut self.mem) {
            self.willy.kill();
            alive = false;
        }

        if alive {
            self.cavern.move_conveyor(&mut self.mem);
            self.draw_items();

            let items_remaining = self.item_attr;
            let died = special::update(
                &mut self.specials,
                &mut self.guardians,
                &mut self.cavern,
                &mut self.willy,
                &mut self.mem,
                &mut self.score,
                &mut self.sounds,
                items_remaining,
            );
            if died && self.guardians_live() {
                self.willy.kill();
            } else if self.check_portal() {
                self.mode = Mode::Cleared {
                    phase: ClearPhase::Cycle(63),
                };
                return;
            }
        }

        self.present();

        // The clock inside `decrease_air` also drives guardian and conveyor
        // timing, so a frozen air supply still has to tick it.
        let out_of_air = if self.air_drains() {
            self.cavern.decrease_air(&mut self.mem)
        } else {
            let held = self.cavern.air;
            self.cavern.decrease_air(&mut self.mem);
            if self.cavern.air != held {
                // A unit was burned: the bar's new rightmost cell was drawn part
                // full. Fill it in before winding the supply back, or the bar
                // ends up with a notch in it.
                for high in 82..86u8 {
                    self.mem.write(addr_of(high, self.cavern.air), 255);
                }
                self.cavern.air = held;
            }
            false
        };
        if out_of_air || self.willy.is_dead() {
            self.mode = Mode::Dying { ink: 71 };
            return;
        }

        if self.music_on {
            self.note_index = self.note_index.wrapping_add(1);
            let index = rot_r(self.note_index & 126, 1) as usize;
            // The original produced this with C=3, a 12ms blip once a frame,
            // which is why the in-game tune chirps rather than sings.
            self.sounds.note(mm_data::music::GAME_TUNE[index % 64], 3);
        }
    }

    /// Copy the working buffers to the real screen and print the scores.
    fn present(&mut self) {
        self.mem.copy(SCREEN_BUF, DISPLAY, PLAY_PIXELS);

        if self.flash > 0 {
            self.flash -= 1;
            // An extra life washes the playing area through the paper colours.
            let attr = rot_l(self.flash, 3) & 56;
            self.mem.fill(ATTR_BUF, PLAY_ATTRS, attr);
        }

        self.mem.copy(ATTR_BUF, ATTR_FILE, PLAY_ATTRS);
        self.print_scores();
    }

    fn print_scores(&mut self) {
        let current = format!("{:06}", self.score.current);
        let high = format!("{:06}", self.score.high);
        self.mem.print_str(&high, 20587);
        self.mem.print_str(&current, 20602);
    }

    /// Draw the items still to be collected, and collect any Willy is touching.
    fn draw_items(&mut self) {
        self.item_attr = 0;

        for slot in 0..self.cavern.items.len() {
            let item = self.cavern.items[slot];
            if item.attr == 255 {
                break;
            }
            if item.attr == 0 {
                continue;
            }

            // Willy's sprite forces white ink onto the cell it covers, which is
            // how the game notices he is standing on an item.
            if self.mem.read(item.attr_addr) & 7 == 7 {
                if self.score.add(100) {
                    self.willy.lives += 1;
                    self.flash = 8;
                }
                self.cavern.items[slot].attr = 0;
                continue;
            }

            // Cycle the ink through magenta, green, cyan and yellow.
            let attr = (item.attr & 248) | 3;
            let attr = attr + (item.attr & 3);
            self.cavern.items[slot].attr = attr;
            self.mem.write(item.attr_addr, attr);
            self.item_attr = attr;

            let screen = addr_of(item.screen_msb, speccy::memory::lsb(item.attr_addr));
            let sprite = mm_data::caverns::ITEM_GRAPHICS[self.cavern.sheet];
            self.mem.draw_sprite(&sprite, screen);
        }

        if self.item_attr == 0 {
            // Everything collected: the portal starts flashing.
            self.cavern.portal.attr |= 0x80;
        }
    }

    /// Draw the portal, or report that Willy has stepped into an open one.
    fn check_portal(&mut self) -> bool {
        let portal = self.cavern.portal;
        if portal.attr_addr == self.willy.location && portal.attr & 0x80 != 0 {
            return true;
        }

        for offset in [0u16, 1, 32, 33] {
            self.mem.write(portal.attr_addr + offset, portal.attr);
        }
        self.mem
            .draw_16x16(&portal.sprite, portal.screen_addr, DrawMode::Overwrite);
        false
    }

    //
    // Death
    //

    fn update_dying(&mut self, ink: u8) {
        self.mem.fill(ATTR_FILE, PLAY_ATTRS, ink);

        // A short note per step, rising as the ink drains away.
        let pitch = (!ink & 7).wrapping_mul(8) | 7;
        self.sounds.note(pitch, 16);

        if ink > 65 {
            self.mode = Mode::Dying { ink: ink - 1 };
            return;
        }

        if self.willy.lives < 1 {
            self.score.record_high();
            self.start_game_over();
            return;
        }
        if !self.invulnerable() {
            self.willy.lives -= 1;
        }
        let sheet = self.cavern.sheet;
        self.enter_cavern(sheet);
        self.mode = Mode::Playing;
    }

    //
    // Cavern cleared
    //

    fn update_cleared(&mut self, phase: ClearPhase) {
        match phase {
            ClearPhase::Cycle(attr) => {
                self.mem.fill(ATTR_FILE, PLAY_ATTRS, attr);
                self.mode = if attr > 1 {
                    Mode::Cleared {
                        phase: ClearPhase::Cycle(attr - 1),
                    }
                } else {
                    Mode::Cleared {
                        phase: ClearPhase::AirBonus,
                    }
                };
            }
            ClearPhase::AirBonus => {
                // Trade the remaining air for points, several units per frame so
                // the bonus does not outlast the player's patience.
                for _ in 0..16 {
                    if self.cavern.decrease_air(&mut self.mem) {
                        let next = if self.cavern.sheet == mm_data::FINAL_BARRIER {
                            0
                        } else {
                            self.cavern.sheet + 1
                        };
                        self.enter_cavern(next);
                        self.mode = Mode::Playing;
                        return;
                    }
                    if self.score.add(1) {
                        self.willy.lives += 1;
                        self.flash = 8;
                    }
                }
                let pitch = rot_l(!(self.cavern.air & 63), 1);
                self.sounds.note(pitch, 4);
                self.print_scores();
            }
        }
    }

    //
    // Game over
    //

    fn start_game_over(&mut self) {
        self.mem.fill(DISPLAY, PLAY_PIXELS, 0);
        let sprite = willy::frame_at(&mm_data::sprites::WILLY, 64);
        self.mem.draw_16x16(&sprite, 18575, DrawMode::Overwrite);
        self.mem
            .draw_16x16(&mm_data::tiles::PLINTH, 18639, DrawMode::Overwrite);
        self.mode = Mode::GameOver { step: 0 };
    }

    fn update_game_over(&mut self, step: u32) {
        // The first 25 steps drop the boot; the rest make the message glisten.
        const BOOT_STEPS: u32 = 25;
        const GLISTEN_STEPS: u32 = 24;

        if step < BOOT_STEPS {
            let distance = (step * 4) as u8;
            let base = screen_row_addr(distance);
            let addr = addr_of(
                speccy::memory::msb(base).wrapping_sub(32),
                speccy::memory::lsb(base) | 15,
            );
            self.mem
                .draw_16x16(&mm_data::tiles::BOOT, addr, DrawMode::Overwrite);

            self.sounds.note(255 - distance, 64);

            let attr = rot_l(distance & 12, 1) | 71;
            self.mem.fill(ATTR_FILE, PLAY_ATTRS, attr);

            if step + 1 == BOOT_STEPS {
                self.mem.print_str("Game", 16586);
                self.mem.print_str("Over", 16594);
            }
            self.mode = Mode::GameOver { step: step + 1 };
            return;
        }

        if step < BOOT_STEPS + GLISTEN_STEPS {
            let phase = step - BOOT_STEPS;
            for i in 0..8u32 {
                let attr = (((phase + i) & 7) | 64) as u8;
                self.mem.write(22730 + i as u16, attr);
            }
            self.mode = Mode::GameOver { step: step + 1 };
            return;
        }

        self.sounds.push(Sound::Silence);
        self.draw_title_screen();
        self.mode = Mode::Title { note: 0, scroll: 0 };
    }
}

/// Attribute file address of the piano key a title-tune frequency lights up.
fn piano_key(frequency: u8) -> u16 {
    // The key index is 31 - (F - 8) / 8, and the keys occupy the last 32 cells
    // of the row at attribute page 89.
    let key = !rot_r(frequency.wrapping_sub(8), 3) | 224;
    addr_of(89, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_theme_tune_runs_at_the_original_tempo() {
        let mut game = Game::new();
        let mut frames = 0u32;
        let mut notes_heard = 0;
        loop {
            game.update(Input::default());
            notes_heard += game.sounds.drain().count();
            frames += 1;
            if !matches!(game.mode, Mode::Title { note, .. } if note < THEME_NOTES) {
                break;
            }
            assert!(frames < 10_000, "the tune never finished");
        }

        assert_eq!(notes_heard, THEME_NOTES, "a note was played twice or skipped");

        // The Blue Danube should take about twenty seconds, not four.
        let seconds = frames as f32 / FRAMES_PER_SECOND;
        let expected: f32 = mm_data::music::THEME_TUNE
            .iter()
            .map(|note| f32::from(note[0]) * BEEP_UNIT_SECONDS)
            .sum();
        assert!(expected > 15.0, "expected tune length was {expected}s");
        assert!(
            (seconds - expected).abs() < 0.5,
            "tune took {seconds}s, expected {expected}s"
        );
    }

    #[test]
    fn a_new_game_starts_on_the_title_screen() {
        let game = Game::new();
        assert!(matches!(game.mode, Mode::Title { note: 0, .. }));
    }

    #[test]
    fn pressing_enter_starts_central_cavern() {
        let mut game = Game::new();
        game.update(Input {
            start: true,
            ..Input::default()
        });
        assert_eq!(game.mode, Mode::Playing);
        assert_eq!(game.cavern.sheet, 0);
        assert_eq!(game.cavern.name.trim(), "Central Cavern");
        assert_eq!(game.willy.lives, 3);
    }

    #[test]
    fn the_game_survives_a_thousand_frames_of_play() {
        let mut game = Game::new();
        game.update(Input {
            start: true,
            ..Input::default()
        });
        for frame in 0..1000 {
            let input = Input {
                right: frame % 7 < 3,
                jump: frame % 23 == 0,
                ..Input::default()
            };
            game.update(input);
            game.sounds.clear();
        }
        assert!(!game.quit);
    }

    #[test]
    fn running_out_of_air_costs_a_life() {
        let mut game = Game::new();
        game.update(Input {
            start: true,
            ..Input::default()
        });
        // Skip to the last unit of air.
        game.cavern.air = 37;
        for _ in 0..200 {
            game.update(Input::default());
            game.sounds.clear();
            if game.willy.lives < 3 {
                return;
            }
        }
        panic!("Willy never lost a life: mode {:?}", game.mode);
    }

    /// Start a game in `sheet` and run frames until Willy dies, up to `limit`.
    /// Returns the frame he died on.
    #[cfg(feature = "debug")]
    fn frames_until_death(game: &mut Game, sheet: usize, limit: u32) -> Option<u32> {
        game.start_cavern = sheet;
        game.update(Input {
            start: true,
            ..Input::default()
        });
        for frame in 0..limit {
            game.update(Input::default());
            game.sounds.clear();
            if matches!(game.mode, Mode::Dying { .. }) {
                return Some(frame);
            }
        }
        None
    }

    /// Cavern 2, where a guardian walks into a standing Willy quickly enough to
    /// make a short test. Checked by `guardians_kill_without_the_switch`.
    #[cfg(feature = "debug")]
    const DEADLY_CAVERN: usize = 2;

    #[test]
    #[cfg(feature = "debug")]
    fn guardians_kill_without_the_switch() {
        let mut game = Game::new();
        assert!(frames_until_death(&mut game, DEADLY_CAVERN, 200).is_some());
    }

    #[test]
    #[cfg(feature = "debug")]
    fn switching_the_guardians_off_keeps_willy_alive() {
        let mut game = Game::new();
        game.debug.no_guardians = true;
        assert_eq!(frames_until_death(&mut game, DEADLY_CAVERN, 200), None);
        assert_eq!(game.mode, Mode::Playing);
    }

    #[test]
    #[cfg(feature = "debug")]
    fn frozen_air_never_runs_out() {
        let mut game = Game::new();
        game.debug.frozen_air = true;
        game.debug.no_guardians = true;
        game.update(Input {
            start: true,
            ..Input::default()
        });
        game.cavern.air = 37;
        for _ in 0..400 {
            game.update(Input::default());
            game.sounds.clear();
        }
        assert_eq!(game.mode, Mode::Playing);
        assert_eq!(game.cavern.air, 37);
        assert_eq!(game.willy.lives, 3);
    }

    #[test]
    #[cfg(feature = "debug")]
    fn invulnerability_costs_no_lives_but_willy_still_dies() {
        let mut game = Game::new();
        game.debug.invulnerable = true;
        assert!(frames_until_death(&mut game, DEADLY_CAVERN, 200).is_some());
        // Run out the death sequence and back into the cavern.
        for _ in 0..40 {
            game.update(Input::default());
            game.sounds.clear();
        }
        assert_eq!(game.mode, Mode::Playing);
        assert_eq!(game.willy.lives, 3);
    }

    #[test]
    fn standing_in_a_nasty_kills_him_rather_than_hiding_the_guardians() {
        let mut game = Game::new();
        game.update(Input {
            start: true,
            ..Input::default()
        });
        game.sounds.clear();

        // Put a nasty in the cell under his feet, in the empty-cavern buffer so
        // it survives the frame starting over.
        let nasty = game.cavern.tile_attr(mm_data::TileKind::Nasty1);
        let under = game.willy.location + 64;
        game.mem.write(under, nasty);
        game.mem.write(under - ATTR_BUF + ATTR_BACK, nasty);

        game.update(Input::default());
        game.sounds.clear();
        assert!(
            matches!(game.mode, Mode::Dying { .. }),
            "a nasty under him went unpunished"
        );
    }

    #[test]
    #[cfg(feature = "debug")]
    fn goto_cavern_loads_the_sheet_and_wraps() {
        let mut game = Game::new();
        game.update(Input {
            start: true,
            ..Input::default()
        });

        game.goto_cavern(3);
        assert_eq!(game.cavern.sheet, 3);
        assert_eq!(game.cavern.name.trim(), "Abandoned Uranium Workings");

        game.goto_cavern(mm_data::CAVERN_COUNT);
        assert_eq!(game.cavern.sheet, 0);
    }

    #[test]
    #[cfg(feature = "debug")]
    fn goto_cavern_does_nothing_on_the_title_screen() {
        let mut game = Game::new();
        game.goto_cavern(5);
        assert!(matches!(game.mode, Mode::Title { .. }));
        assert_eq!(game.cavern.sheet, 0);
    }

    #[test]
    fn every_cavern_can_be_entered_and_stepped() {
        for sheet in 0..mm_data::CAVERN_COUNT {
            let mut game = Game::new();
            game.start_cavern = sheet;
            game.update(Input {
                start: true,
                ..Input::default()
            });
            for _ in 0..40 {
                game.update(Input::default());
                game.sounds.clear();
            }
        }
    }
}
