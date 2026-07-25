//! The game as a state machine, advanced one original frame at a time.
//!
//! The 1983 code ran its death, cavern-change and game-over sequences as blocking
//! loops. Here each of those is a [`Mode`] that advances by one step per call to
//! [`Game::update`], so the front end always stays responsive and every sequence
//! runs at the same 17 frames per second as the game itself.

use crate::cavern::Cavern;
use crate::guardian::Guardians;
use crate::input::Input;
use crate::score::Score;
use crate::sound::{Sound, SoundQueue};
use crate::special::{self, Specials};
use crate::speccy::{
    ATTR_BACK, ATTR_BUF, ATTR_FILE, DISPLAY, DISPLAY_LEN, DrawMode, Memory, PLAY_ATTRS,
    PLAY_PIXELS, SCREEN_BACK, SCREEN_BUF, addr_of, rot_l, rot_r, screen_row_addr,
};
use crate::willy::{self, Willy};

/// The game runs at the Spectrum's own pace: 17 frames per second.
pub const FRAMES_PER_SECOND: f32 = 17.0;

/// Notes in the title tune.
const THEME_NOTES: usize = mm_data::music::THEME_TUNE.len();
/// Characters of the scrolling message, which is longer than the screen is wide.
const SCROLL_STEPS: usize = 292;

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
    /// Cavern to start in, for the original's teleport cheat.
    pub start_cavern: usize,
    /// In-game tune on or off.
    pub music_on: bool,
    pub paused: bool,
    /// Set when the player asks to quit.
    pub quit: bool,
    /// Counts down an extra-life screen flash.
    flash: u8,
    /// Index into the in-game tune, which also drives the lives animation.
    note_index: u8,
    /// Attribute of the last item drawn; zero once every item has been collected.
    item_attr: u8,
    /// Edge detection so a held key does not repeat.
    prev_input: Input,
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
            start_cavern: 0,
            music_on: true,
            paused: false,
            quit: false,
            flash: 0,
            note_index: 0,
            item_attr: 0,
            prev_input: Input::default(),
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

    /// Advance one frame.
    pub fn update(&mut self, input: Input) {
        let pressed = Input {
            left: input.left,
            right: input.right,
            jump: input.jump,
            start: input.start && !self.prev_input.start,
            pause: input.pause && !self.prev_input.pause,
            mute: input.mute && !self.prev_input.mute,
            quit: input.quit,
        };
        self.prev_input = input;

        if pressed.quit {
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
            let [duration, low, high] = mm_data::music::THEME_TUNE[note];
            // Light the two piano keys this note plays, and unlight the previous pair.
            if note > 0 {
                let [_, prev_low, prev_high] = mm_data::music::THEME_TUNE[note - 1];
                self.mem.write(piano_key(prev_low), 56);
                self.mem.write(piano_key(prev_high), 56);
            }
            self.mem.write(piano_key(low), 80);
            self.mem.write(piano_key(high), 40);
            self.sounds.push(Sound::Chord { low, high, duration });
            self.mode = Mode::Title {
                note: note + 1,
                scroll,
            };
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
        if alive {
            alive = !willy::draw(&self.willy, &self.cavern, &mut self.mem);
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
            if died {
                self.willy.kill();
            } else if self.check_portal() {
                self.mode = Mode::Cleared {
                    phase: ClearPhase::Cycle(63),
                };
                return;
            }
        }

        self.present();

        let out_of_air = self.cavern.decrease_air(&mut self.mem);
        if out_of_air || self.willy.is_dead() {
            self.mode = Mode::Dying { ink: 71 };
            return;
        }

        if self.music_on {
            self.note_index = self.note_index.wrapping_add(1);
            let index = rot_r(self.note_index & 126, 1) as usize;
            self.sounds
                .note(mm_data::music::GAME_TUNE[index % 64], 32);
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

            let screen = addr_of(item.screen_msb, crate::speccy::lsb(item.attr_addr));
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
        if !self.cheat {
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
                crate::speccy::msb(base).wrapping_sub(32),
                crate::speccy::lsb(base) | 15,
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
