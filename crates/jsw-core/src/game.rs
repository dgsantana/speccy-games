//! The game as a state machine, advanced one Spectrum frame at a time.
//!
//! [`Game::update`] is the original's main loop at 34762, in its order: move the
//! guardians and the ropes, move Willy, draw him, then Maria and the toilet, the
//! guardians, the conveyor and the items. That order matters more than it looks.
//! Items are drawn last because collecting one is decided by finding white ink
//! in its cell; ropes are drawn before them because a rope finds Willy by
//! looking for pixels already on the screen.
//!
//! The sequences the original runs as blocking loops — the title screen, dying,
//! the foot coming down at the end of the game — are [`Mode`] variants advanced
//! a step per frame, because a blocking loop would freeze the window. A game
//! starts on its title screen and goes back to it when the night ends, either
//! way round; only Escape leaves for the picker.

use speccy::layout::{
    ATTR_BACK, ATTR_BUF, COLUMNS, PLAY_ATTRS, PLAY_PIXELS, ROWS, SCREEN_BACK, SCREEN_BUF,
    cell_offset,
};
use speccy::memory::{ATTR_FILE, DISPLAY, Memory};
use speccy::sound::SoundQueue;

use crate::bedroom::{self, Quest};
use crate::entity::Entities;
use crate::gameover;
use crate::item::Items;
use crate::room::Room;
use crate::willy::{self, Outcome, Willy};

/// Jet Set Willy runs at the same pace Manic Miner does.
pub const FRAMES_PER_SECOND: f32 = 17.0;

/// The room Willy starts in: The Bathroom.
pub const START_ROOM: usize = 33;

/// Frames the death sequence lasts: the loop at 35708 fills the attribute file
/// with 71 down to 64, one value a pass, so it is one frame per ink colour.
const DEATH_FRAMES: u8 = 7;

/// Lives a new game starts with, from the original's 34784.
pub const STARTING_LIVES: u8 = 7;

/// Duration units elapsed per frame, in 1/256ths.
///
/// The theme tune is far slower than the game: half a note of it lasts 146ms
/// against a 59ms frame, so notes cannot advance once a frame. This clock keeps
/// the fraction, so the tempo is right on average and the error never piles up.
const TUNE_UNITS_PER_FRAME: u32 = (256.0 / (FRAMES_PER_SECOND * 256.0 / 62_500.0)) as u32;

/// What the game is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The title screen: play the theme, then scroll the message round and
    /// round until Enter is pressed. `half` counts halves of a note of the
    /// tune, `scroll` characters of the message.
    Title {
        half: usize,
        scroll: usize,
    },
    Playing,
    /// Willy has had a fatal accident; the original flashes him and drops him
    /// back into the room. Counts down.
    Dying(u8),
    /// Out of lives: the foot is coming down on the barrel. Counts the foot's
    /// distance from the top of the screen.
    GameOver(u8),
    /// The foot has landed and "Game Over" is glistening. Counts down.
    GameOverMessage(u8),
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
    /// How far he is through the night: the original's mode byte at 34271.
    pub quest: Quest,
    pub sounds: SoundQueue,
    /// Where the in-game tune has got to. The original's note index at 34274
    /// counts frames, and each note of the tune lasts two of them.
    note_index: u8,
    /// Whether the in-game tune has been switched off, which is bit 1 of the
    /// original's music flags at 34275.
    pub music_off: bool,
    /// Set when the player asks to leave, sending the shell back to the picker.
    pub quit: bool,
    pub paused: bool,
    /// Developer switches. Only exists with the `debug` feature.
    #[cfg(feature = "debug")]
    pub debug: speccy::Debug,
    /// Edge detection so a held key does not repeat.
    prev_input: speccy::Input,
    /// How far into the current half-note of the theme tune, in 1/256ths of a
    /// duration unit. Only the title screen uses it.
    tune_clock: u32,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    /// A new game, sitting on its title screen as the original does.
    pub fn new() -> Self {
        let mut game = Self::started();
        game.show_title();
        game
    }

    /// A game already under way, in The Bathroom at seven in the morning. This
    /// is what pressing Enter on the title screen leads to.
    pub fn started() -> Self {
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
            quest: Quest::default(),
            sounds: SoundQueue::default(),
            note_index: 0,
            music_off: false,
            quit: false,
            paused: false,
            #[cfg(feature = "debug")]
            debug: speccy::Debug::default(),
            prev_input: speccy::Input::default(),
            tune_clock: 0,
        };
        game.enter_room(START_ROOM);
        game
    }

    /// Back to the title screen, which is where the original goes when the game
    /// is over and when one in the morning comes round: everything is set up
    /// again from 34762, and the picture is drawn.
    fn show_title(&mut self) {
        let paused = self.paused;
        let music_off = self.music_off;
        #[cfg(feature = "debug")]
        let debug = self.debug;
        *self = Self::started();
        self.paused = paused;
        self.music_off = music_off;
        #[cfg(feature = "debug")]
        {
            self.debug = debug;
        }
        self.mode = Mode::Title { half: 0, scroll: 0 };
        crate::title::draw(&mut self.mem);
    }

    /// The border colour of the room being played, or of the colours running
    /// through the screen while the game is paused.
    pub fn border(&self) -> u8 {
        if self.paused {
            self.mem.read(ATTR_FILE) & 7
        } else if matches!(self.mode, Mode::Title { .. }) {
            // The title screen leaves the border black; the original only ever
            // touches it there as a side effect of making a sound.
            0
        } else {
            self.room.border
        }
    }

    /// Run the ink and paper colours of the whole screen forward: the routine at
    /// 39112, which the original uses while the game is paused and while the
    /// instructions scroll across the title screen.
    ///
    /// The original does this as fast as the delay loop allows; here it is once
    /// a frame, so the screen washes through the colours rather than shimmering.
    fn cycle_attrs(&mut self) {
        for at in ATTR_FILE..ATTR_FILE + 768 {
            let attr = self.mem.read(at);
            let ink = attr.wrapping_add(3) & 7;
            // Three on for the paper too, and bright goes out: 184 keeps the
            // paper and the flash bit and nothing else.
            self.mem.write(at, (attr.wrapping_add(24) & 184) | ink);
        }
    }

    /// Load a room, draw it into the empty-room buffers, and put its name up.
    fn enter_room(&mut self, number: usize) {
        self.room = Room::load(number % jsw_data::ROOM_COUNT);
        if let Some(seen) = self.visited.get_mut(self.room.number) {
            *seen = true;
        }
        self.entities = Entities::load(&self.room);
        // Not one of the seven bytes saved on entry: the original clears it
        // here, at 35245, so a rope in the room he arrives in starts empty.
        self.willy.rope = 0;
        self.willy_on_entry = self.willy;
        self.mem.fill(SCREEN_BACK, PLAY_PIXELS, 0);
        self.mem.fill(ATTR_BACK, PLAY_ATTRS, 0);
        self.room.draw(&mut self.mem);

        // The bottom third of the display file, cleared as the original clears
        // it at 35283. The name and the clock are redrawn every frame, but the
        // lives are only ever drawn - so without this the sprite for a life
        // just lost would stay on the screen for good.
        self.mem
            .fill(DISPLAY + (crate::hud::NAME_ROW * 8 * 32) as u16, 2048, 0);
    }

    /// Advance one frame.
    pub fn update(&mut self, input: speccy::Input) {
        let pressed = speccy::Input {
            pause: input.pause && !self.prev_input.pause,
            mute: input.mute && !self.prev_input.mute,
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
        if pressed.mute {
            self.music_off = !self.music_off;
        }
        if self.paused {
            self.cycle_attrs();
            return;
        }

        self.sync_debug();

        // The game over sequence has the screen to itself: it draws straight
        // into the display file, so none of the room's buffers are copied. So
        // does the title screen.
        match self.mode {
            Mode::Title { half, scroll } => {
                self.update_title(pressed, half, scroll);
                return;
            }
            Mode::GameOver(distance) => {
                self.update_game_over(distance);
                return;
            }
            Mode::GameOverMessage(step) => {
                self.update_game_over_message(step);
                return;
            }
            _ => {}
        }

        // Dying happens on the screen as it stands: the original's loop at
        // 35708 only recolours the attribute file, so the room, the guardians
        // and Willy stay exactly where they were when he was hit. Copying the
        // empty room in would wipe all of them off.
        if let Mode::Dying(step) = self.mode {
            self.update_dying(step);
            return;
        }

        // Every frame starts from the empty room and redraws what moves.
        self.mem.copy(ATTR_BACK, ATTR_BUF, PLAY_ATTRS);
        self.mem.copy(SCREEN_BACK, SCREEN_BUF, PLAY_PIXELS);

        self.entities.step();

        // With his head down the toilet nothing moves him at all.
        if self.quest == Quest::HeadDownTheToilet {
            self.finish_the_frame();
            return;
        }

        let wanted = if self.quest.on_the_errand() {
            Willy::errand_input()
        } else {
            willy::Input {
                left: input.left,
                right: input.right,
                jump: input.jump,
            }
        };
        let outcome = self
            .willy
            .update(&self.room, &mut self.mem, wanted, &mut self.sounds);
        if outcome == Outcome::Died {
            self.kill();
            self.present();
            return;
        }
        // A rope can carry him off the top of the room without any of the
        // jumping code running, so the original tests his height again here,
        // at 35025, having already moved everything.
        let outcome = if outcome == Outcome::None && self.willy.y >= 225 {
            Outcome::Above
        } else {
            outcome
        };
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

        // He arrives at the toilet by reaching its column, wherever he is
        // standing: the routine at 39850.
        if self.quest == Quest::ToTheToilet
            && self.room.number == bedroom::BATHROOM
            && bedroom::reached_toilet(&self.willy)
        {
            // The counter is reset so his head stays down it for a whole game
            // minute at least.
            self.minute = 0;
            self.quest = Quest::HeadDownTheToilet;
        }

        self.finish_the_frame();
    }

    /// Everything the main loop does after Willy has been drawn, which is also
    /// all it does once his head is down the toilet.
    fn finish_the_frame(&mut self) {
        // Maria and the toilet come before the guardians, as they do at 35048.
        if self.room.number == bedroom::BEDROOM {
            let caught = bedroom::bed(&mut self.quest, &self.willy, self.minute, &mut self.mem);
            if caught {
                self.kill();
                self.present();
                return;
            }
        } else if self.room.number == bedroom::BATHROOM {
            bedroom::toilet(self.quest, self.minute, &mut self.mem);
        }

        // Guardians are drawn over Willy and report touching him, which is how
        // the original detects the collision. A rope in the room reads the
        // pixels drawn so far to find him, so nothing but Willy may be on the
        // screen when this runs - which is why the items come after it, as they
        // do in the original at 35056.
        let has_room_above = self.room.exits.up as usize != self.room.number;
        if self.entities.draw(
            &mut self.mem,
            &mut self.willy,
            has_room_above,
            &mut self.sounds,
        ) {
            self.kill();
            self.present();
            return;
        }

        // The belt turns in the empty-room buffer, so it keeps moving without
        // being redrawn.
        self.room.move_conveyor(&mut self.mem);

        // Collecting an item is decided by finding white ink in its cell, and
        // only Willy's drawing puts white ink there.
        let taken = self.items.draw(
            self.room.number,
            self.minute,
            &self.room.item,
            &mut self.mem,
        );
        for _ in 0..taken {
            // The blip at 37897 is a sweep, not a note: the original counts
            // down from 128 in twos and delays 144 less the counter each pass,
            // so the pitch falls the whole way through. Four notes are enough
            // to hear it as one chirp.
            for pitch in [16, 48, 80, 112] {
                self.sounds.note(pitch, 5);
            }
        }
        if taken > 0 && self.items.remaining() == 0 {
            // Every item in: Maria stands aside. The original sets the mode
            // byte here, at 37888.
            self.quest = Quest::AllCollected;
        }

        // The clock moves on when the frame counter wraps, as the original's
        // 35401 does.
        self.minute = self.minute.wrapping_add(1);
        if self.minute == 0 {
            self.clock.tick();
            if self.clock.past_bedtime() {
                // One in the morning, and the original drops back to its title
                // screen whatever Willy has managed.
                self.show_title();
                return;
            }
        }

        // On his way to the toilet he moves at twice his usual pace, which the
        // original arranges by forcing his animation frame odd at 35377.
        if self.quest.on_the_errand() {
            self.willy.frame |= 1;
        }

        self.play_the_tune();

        self.draw_hud();
        self.present();
    }

    /// The title screen, from 34965: the theme tune, and then the message
    /// scrolling across the bottom with the colours running through the
    /// picture. Enter starts the game at any point, and reaching the end of the
    /// message starts the whole thing again.
    fn update_title(&mut self, input: speccy::Input, half: usize, scroll: usize) {
        if input.start {
            self.mode = Mode::Playing;
            // Entering the room clears the bottom third of the screen, which
            // the title screen has been scrolling its message across.
            self.enter_room(START_ROOM);
            self.present();
            return;
        }

        if let Some((pitch, duration)) = crate::title::note(half) {
            // True on the first frame of each half-note: the clock never
            // carries a whole frame of credit into a new one.
            if self.tune_clock < TUNE_UNITS_PER_FRAME && !self.music_off {
                self.sounds.note(pitch, duration);
            }
            self.tune_clock += TUNE_UNITS_PER_FRAME;
            let length = u32::from(duration) * 256;
            if self.tune_clock >= length {
                self.tune_clock -= length;
                self.mode = Mode::Title {
                    half: half + 1,
                    scroll,
                };
            }
            return;
        }

        // The tune is over, so the message scrolls. The original cycles the
        // colours of the whole screen as it goes, which is what makes the
        // picture crawl.
        self.cycle_attrs();
        crate::title::scroll(&mut self.mem, scroll, &mut self.sounds);
        if scroll + 1 == crate::title::SCROLL_STEPS {
            self.show_title();
        } else {
            self.mode = Mode::Title {
                half,
                scroll: scroll + 1,
            };
        }
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
    ///
    /// The loop at 35708. `step` is the ink colour, counting 7 down to 0: each
    /// pass fills the top two thirds of the *attribute file* - not the working
    /// buffer - with 64 + step, so the picture already on the screen fades from
    /// white to black without being redrawn.
    fn update_dying(&mut self, step: u8) {
        self.mem.fill(ATTR_FILE, PLAY_ATTRS, 64 | step);

        // The note's pitch and its length both come from the ink colour: the
        // original derives a delay of 63 - 8*step and repeats it 8 + 32*step
        // times, so the sound falls and shortens as the screen darkens. One of
        // our duration units is 256 iterations of that delay loop.
        let pitch = ((7 - step) << 3) | 7;
        let iterations = u32::from(8 + 32 * u16::from(step)) * u32::from(pitch);
        self.sounds.note(pitch, ((iterations / 256) as u8).max(1));

        if step > 0 {
            self.mode = Mode::Dying(step - 1);
            return;
        }

        // The original looks for a life remaining before taking one away, at
        // 35720, so the seven it starts with are worth eight deaths.
        if self.lives == 0 && !self.invulnerable() {
            gameover::open(&mut self.mem);
            self.mode = Mode::GameOver(0);
            return;
        }
        if !self.invulnerable() {
            self.lives -= 1;
        }

        // Back to the doorway he came in by, not to where the game started.
        self.willy = self.willy_on_entry;
        self.willy.airborne = 0;
        self.mode = Mode::Playing;
        let room = self.room.number;
        self.enter_room(room);
    }

    /// A note of the in-game tune, from 39221.
    ///
    /// The index counts frames and the tune is read at half that, so each note
    /// lasts two frames. Its pitch is lifted by however many lives are left,
    /// which is why the tune climbs as the night goes badly.
    fn play_the_tune(&mut self) {
        if self.music_off {
            return;
        }
        self.note_index = self.note_index.wrapping_add(1);
        let note = jsw_data::music::GAME[usize::from(self.note_index & 126) >> 1];
        let lift = 28u8.wrapping_sub(self.lives.wrapping_mul(4));
        self.sounds.note(note.wrapping_add(lift), 2);
    }

    /// One step of the foot's descent onto the barrel.
    fn update_game_over(&mut self, distance: u8) {
        let landed = gameover::descend(distance, &mut self.mem, &mut self.sounds);
        self.mode = if landed {
            gameover::message(&mut self.mem);
            Mode::GameOverMessage(gameover::GLISTEN_FRAMES)
        } else {
            Mode::GameOver(distance + gameover::STEP)
        };
    }

    /// The message glistening, and then back to the title screen, which is
    /// where the original goes from here.
    fn update_game_over_message(&mut self, step: u8) {
        gameover::glisten(step, &mut self.mem);
        if step == 0 {
            self.show_title();
        } else {
            self.mode = Mode::GameOverMessage(step - 1);
        }
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
            let reaches = if cell_row == 2 {
                drawn_y & 15 != 0
            } else {
                true
            };
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
        let game = Game::started();
        assert_eq!(game.room.number, START_ROOM);
        assert_eq!(game.room.title, "The Bathroom");
    }

    #[test]
    fn the_room_is_drawn_to_the_display_file() {
        let mut game = Game::started();
        game.update(speccy::Input::default());
        let pixels: u32 = (0..4096)
            .map(|i| game.mem.read(DISPLAY + i).count_ones())
            .sum();
        assert!(pixels > 500, "only {pixels} pixels reached the screen");
    }

    #[test]
    fn walking_off_an_edge_changes_room() {
        let mut game = Game::started();
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
            let mut game = Game::started();
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
        let mut game = Game::started();
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
        let mut game = Game::started();
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
    fn dying_leaves_the_picture_alone_and_fades_it() {
        // The original's death loop only writes the attribute file, so whatever
        // was on the screen when he was hit - the room, the guardians, the
        // items and Willy - stays there while the ink fades to black. Blitting
        // the empty room in first wiped the guardians off instead.
        let mut game = Game::started();
        game.enter_room(28); // First Landing, with guardians in it.
        for _ in 0..4 {
            game.update(speccy::Input::default());
            game.sounds.clear();
        }
        let picture: Vec<u8> = (0..4096).map(|i| game.mem.read(DISPLAY + i)).collect();

        game.kill();
        let mut inks = Vec::new();
        while let Mode::Dying(_) = game.mode {
            game.update(speccy::Input::default());
            game.sounds.clear();
            inks.push(game.mem.read(ATTR_FILE) & 7);
            let now: Vec<u8> = (0..4096).map(|i| game.mem.read(DISPLAY + i)).collect();
            assert_eq!(now, picture, "the death sequence redrew the screen");
        }

        assert_eq!(
            inks,
            vec![7, 6, 5, 4, 3, 2, 1, 0],
            "the ink should fade out"
        );
    }

    #[test]
    fn standing_in_a_nasty_kills_him() {
        let mut game = Game::started();
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
        let mut game = Game::started();
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
        let mut game = Game::started();
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
        let mut game = Game::started();
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

    /// A rope room, with Willy dropped just under the rope's path.
    fn under_a_rope(number: usize) -> Game {
        let mut game = Game::started();
        game.goto_room(number);
        game.willy = Willy {
            y: 5 * willy::ROW_UNITS,
            cell: ATTR_BUF + 5 * COLUMNS as u16 + 17,
            ..Willy::default()
        };
        game
    }

    /// On the Roof, whose exit upwards is itself: there is nothing above it.
    fn on_the_roof() -> Game {
        under_a_rope(18)
    }

    /// Play until the rope has him, or give up.
    fn until_caught(game: &mut Game) {
        for _ in 0..8 {
            game.update(speccy::Input::default());
            game.sounds.clear();
            if game.willy.rope != 0 {
                return;
            }
        }
    }

    #[test]
    fn falling_past_a_rope_catches_it() {
        let mut game = on_the_roof();
        until_caught(&mut game);

        assert!(game.willy.on_rope(), "he fell straight past the rope");
        assert_eq!(game.mode, Mode::Playing, "he died on the way");
        let caught = game.willy.position();

        // The swing carries him about while he does nothing. Sideways, mostly:
        // a segment near the top of the rope hardly changes height.
        let mut moved = false;
        for _ in 0..12 {
            game.update(speccy::Input::default());
            game.sounds.clear();
            moved |= game.willy.position() != caught;
        }
        assert!(moved, "the rope swung without taking him with it");
        assert!(game.willy.on_rope(), "he let go by himself");
    }

    #[test]
    fn jumping_off_a_rope_drops_him_back_into_the_room() {
        let mut game = on_the_roof();
        until_caught(&mut game);
        assert!(game.willy.on_rope(), "he never caught the rope");

        game.update(speccy::Input {
            jump: true,
            ..speccy::Input::default()
        });
        game.sounds.clear();
        assert!(!game.willy.on_rope(), "he is still hanging there");
        assert_eq!(game.willy.airborne, 1, "he should be jumping");

        // And the rope leaves him alone all the way down.
        for _ in 0..12 {
            game.update(speccy::Input::default());
            game.sounds.clear();
            assert!(!game.willy.on_rope(), "the rope grabbed him mid-jump");
        }
    }

    #[test]
    fn climbing_a_rope_carries_him_into_the_room_above() {
        // Cold Store's rope has the Swimming Pool above it.
        let mut game = under_a_rope(25);
        assert_eq!(game.room.exits.up, 31);
        until_caught(&mut game);
        assert!(game.willy.on_rope(), "he never caught the rope");

        // Facing against the swing climbs. Which way that is depends on which
        // way the rope happens to be going, so try one and then the other.
        let mut left = speccy::Input {
            left: true,
            ..speccy::Input::default()
        };
        let right = speccy::Input {
            right: true,
            ..speccy::Input::default()
        };
        let mut highest = game.willy.rope;
        for _ in 0..60 {
            let before = game.willy.rope;
            game.update(if game.willy.rope > highest {
                right
            } else {
                left
            });
            game.sounds.clear();
            if game.room.number != 25 {
                break;
            }
            if game.willy.rope > before {
                // Going the wrong way; turn around.
                left = right;
            }
            highest = highest.min(game.willy.rope);
        }

        assert_eq!(game.room.number, 31, "he never climbed out of the room");
        assert_eq!(game.willy.rope, 0, "the rope kept hold of him");
    }

    #[test]
    fn there_is_nothing_to_climb_to_above_the_roof() {
        // On the Roof's exit upwards is itself, so the rope holds him back.
        let mut game = on_the_roof();
        assert_eq!(usize::from(game.room.exits.up), game.room.number);
        until_caught(&mut game);
        assert!(game.willy.on_rope());

        let mut left = speccy::Input {
            left: true,
            ..speccy::Input::default()
        };
        let right = speccy::Input {
            right: true,
            ..speccy::Input::default()
        };
        for _ in 0..60 {
            let before = game.willy.rope;
            game.update(left);
            game.sounds.clear();
            if game.willy.rope > before {
                left = right;
            }
            assert_eq!(game.room.number, 18, "he climbed off the roof");
        }
        assert!(
            game.willy.rope >= 12,
            "he climbed past the top of the rope: {}",
            game.willy.rope
        );
    }

    #[test]
    fn a_rope_is_forgotten_on_the_way_out_of_the_room() {
        let mut game = on_the_roof();
        until_caught(&mut game);
        assert!(game.willy.on_rope());

        game.goto_room(33);
        assert_eq!(game.willy.rope, 0, "he brought the rope with him");
    }

    #[test]
    fn a_new_game_has_every_item_to_find() {
        let game = Game::started();
        assert_eq!(game.items.remaining(), 83);
        assert_eq!(game.items.collected, 0);
    }

    #[test]
    #[cfg(feature = "debug")]
    fn invulnerability_costs_no_lives_and_never_ends_the_game() {
        let mut game = Game::started();
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
    fn the_last_item_sends_maria_away() {
        let mut game = Game::started();
        // Take everything but one in the Watch Tower, as walking into each
        // would. The last one has to be walked into, because the mode only
        // changes where the original changes it: in the item-drawing pass.
        game.goto_room(50);
        let item = (crate::item::FIRST..256)
            .find(|&n| game.items.room_of(n) == 50)
            .expect("the Watch Tower has items");
        for other in crate::item::FIRST..256 {
            if other != item {
                game.items.take(other);
            }
        }
        assert_eq!(game.items.remaining(), 1);
        assert_eq!(game.quest, Quest::Collecting);

        let cell = game.items.cell_of(item);
        game.willy = Willy {
            y: ((cell - ATTR_BUF) / 32) as u8 * willy::ROW_UNITS,
            cell,
            ..Willy::default()
        };

        game.update(speccy::Input::default());
        assert_eq!(game.items.remaining(), 0);
        assert_eq!(game.quest, Quest::AllCollected);
    }

    #[test]
    fn reaching_the_bed_sends_him_running_for_the_toilet() {
        let mut game = Game::started();
        game.goto_room(bedroom::BEDROOM);
        game.quest = Quest::AllCollected;
        // On the bed itself, at the left-hand end of the room. Row 13 is solid
        // all the way across, so the walkable floor is the one above it.
        game.willy = Willy {
            y: 12 * willy::ROW_UNITS,
            cell: ATTR_BUF + 12 * COLUMNS as u16 + 5,
            ..Willy::default()
        };

        game.update(speccy::Input::default());
        game.sounds.clear();
        assert_eq!(game.quest, Quest::ToTheToilet);

        // From now on he goes right whatever is held, and will not jump.
        let go_left = speccy::Input {
            left: true,
            jump: true,
            ..speccy::Input::default()
        };
        let start = game.willy.position().1;
        for _ in 0..12 {
            game.update(go_left);
            game.sounds.clear();
            if game.room.number != bedroom::BEDROOM {
                break;
            }
        }
        assert!(
            game.willy.position().1 > start || game.room.number != bedroom::BEDROOM,
            "he went the wrong way, or nowhere"
        );
        assert_eq!(game.willy.airborne, 0, "he jumped when he should not");
    }

    #[test]
    fn reaching_the_toilet_ends_the_errand() {
        let mut game = Game::started();
        game.goto_room(bedroom::BATHROOM);
        game.quest = Quest::ToTheToilet;
        game.willy = Willy {
            y: 13 * willy::ROW_UNITS,
            cell: ATTR_BUF + 13 * COLUMNS as u16 + 28,
            ..Willy::default()
        };

        game.update(speccy::Input::default());
        game.sounds.clear();
        assert_eq!(game.quest, Quest::HeadDownTheToilet);
        assert_eq!(game.minute, 1, "the minute counter should start afresh");

        // Nothing moves him after that.
        let where_he_is = (game.willy.position(), game.willy.y);
        for _ in 0..20 {
            game.update(speccy::Input {
                left: true,
                jump: true,
                ..speccy::Input::default()
            });
            game.sounds.clear();
        }
        assert_eq!((game.willy.position(), game.willy.y), where_he_is);
        assert_eq!(game.room.number, bedroom::BATHROOM);
    }

    #[test]
    fn a_new_game_waits_on_the_title_screen() {
        let mut game = Game::new();
        assert!(matches!(game.mode, Mode::Title { half: 0, scroll: 0 }));

        // The tune plays first, a note at a time, and nothing else moves.
        let mut notes = 0;
        for _ in 0..40 {
            game.update(speccy::Input::default());
            notes += game.sounds.drain().count();
            assert!(matches!(game.mode, Mode::Title { scroll: 0, .. }));
        }
        assert!(notes > 0, "the theme tune never played");

        // Enter starts the game in The Bathroom.
        game.update(speccy::Input {
            start: true,
            ..speccy::Input::default()
        });
        game.sounds.clear();
        assert_eq!(game.mode, Mode::Playing);
        assert_eq!(game.room.number, START_ROOM);
        assert_eq!(game.lives, STARTING_LIVES);
    }

    #[test]
    fn the_message_scrolls_once_the_tune_is_over_and_then_starts_again() {
        let mut game = Game::new();
        // Straight to the end of the tune.
        game.mode = Mode::Title {
            half: crate::title::THEME_HALVES,
            scroll: 0,
        };

        for step in 0..crate::title::SCROLL_STEPS - 1 {
            game.update(speccy::Input::default());
            game.sounds.clear();
            assert_eq!(
                game.mode,
                Mode::Title {
                    half: crate::title::THEME_HALVES,
                    scroll: step + 1,
                }
            );
        }

        // The last step starts the whole title screen over, tune and all.
        game.update(speccy::Input::default());
        game.sounds.clear();
        assert!(matches!(game.mode, Mode::Title { half: 0, scroll: 0 }));
    }

    #[test]
    fn one_in_the_morning_ends_the_night() {
        let mut game = Game::started();
        // A minute short of bedtime, and a frame short of the minute.
        game.clock.minutes = 18 * 60 - 1;
        game.minute = 255;

        game.update(speccy::Input::default());
        game.sounds.clear();
        assert!(
            matches!(game.mode, Mode::Title { .. }),
            "the game carried on past one in the morning"
        );
        // Back to the title screen means a new night: the clock starts again.
        assert!(!game.clock.past_bedtime());
    }

    #[test]
    fn maria_kills_him_in_the_bedroom() {
        let mut game = Game::started();
        game.goto_room(bedroom::BEDROOM);
        // Maria stands at (11,14); put Willy in her.
        game.willy = Willy {
            y: 11 * willy::ROW_UNITS,
            cell: ATTR_BUF + 11 * COLUMNS as u16 + 14,
            ..Willy::default()
        };

        game.update(speccy::Input::default());
        game.sounds.clear();
        assert_eq!(game.mode, Mode::Dying(DEATH_FRAMES), "Maria let him past");
    }

    /// Pixels drawn along the row the lives are shown on.
    fn lives_shown(game: &Game) -> u32 {
        let row = crate::hud::LIVES_ROW;
        (0..8)
            .flat_map(|pixel_row| (0..COLUMNS).map(move |column| (pixel_row, column)))
            .map(|(pixel_row, column)| {
                let at = DISPLAY
                    + speccy::memory::display_row_offset(row * 8 + pixel_row) as u16
                    + column as u16;
                game.mem.read(at).count_ones()
            })
            .sum()
    }

    /// Kill him and run out the death sequence.
    fn die(game: &mut Game) {
        game.kill();
        for _ in 0..DEATH_FRAMES + 2 {
            game.update(speccy::Input::default());
            game.sounds.clear();
        }
    }

    #[test]
    fn losing_a_life_rubs_one_off_the_row() {
        let mut game = Game::started();
        game.update(speccy::Input::default());
        game.sounds.clear();
        let before = lives_shown(&game);
        assert!(before > 0, "no lives were drawn to begin with");

        die(&mut game);
        game.update(speccy::Input::default());
        game.sounds.clear();

        assert_eq!(game.lives, STARTING_LIVES - 1);
        assert!(
            lives_shown(&game) < before,
            "the row still shows {before} pixels' worth of lives"
        );
    }

    #[test]
    fn the_count_runs_all_the_way_down_before_the_game_ends() {
        // The original checks for lives remaining before taking one away, so
        // the seven it starts with are worth eight deaths.
        let mut game = Game::started();
        for death in 0..STARTING_LIVES {
            die(&mut game);
            assert_eq!(game.mode, Mode::Playing, "the game ended at death {death}");
            assert_eq!(game.lives, STARTING_LIVES - 1 - death);
        }
        assert_eq!(game.lives, 0);

        die(&mut game);
        assert!(
            matches!(game.mode, Mode::GameOver(_)),
            "the last death did not end the game: {:?}",
            game.mode
        );
    }

    #[test]
    fn the_game_over_screen_is_not_drawn_over_by_the_room() {
        let mut game = Game::started();
        game.lives = 0;
        die(&mut game);
        assert!(matches!(game.mode, Mode::GameOver(_)));

        // Willy stands on the barrel in the middle of an otherwise blank
        // screen. The room would fill far more of it than this.
        let lit: u32 = (0..4096)
            .map(|i| game.mem.read(DISPLAY + i).count_ones())
            .sum();
        assert!(lit > 0, "the game over screen is blank");
        assert!(lit < 1000, "the room was drawn over it: {lit} pixels");

        // And he is still there once the foot starts coming down.
        let willy: u32 = (0..8)
            .map(|line| {
                let at = DISPLAY
                    + speccy::memory::display_row_offset(12 * 8 + line) as u16
                    + 15;
                game.mem.read(at).count_ones()
            })
            .sum();
        assert!(willy > 0, "Willy is missing from the game over screen");
    }

    #[test]
    fn the_last_life_brings_the_foot_down() {
        let mut game = Game::started();
        // None left, so this death is the one that ends it.
        game.lives = 0;
        game.kill();

        // Count the frames of each phase, from the accident to the title screen
        // the original goes back to.
        let (mut dying, mut foot, mut glisten) = (0, 0, 0);
        for _ in 0..300 {
            if matches!(game.mode, Mode::Title { .. }) {
                break;
            }
            // Counted before the frame runs, so each total is the number of
            // frames actually spent in that phase.
            match game.mode {
                Mode::Dying(_) => dying += 1,
                Mode::GameOver(_) => foot += 1,
                Mode::GameOverMessage(_) => {
                    glisten += 1;
                    let lit: u32 = (0..4096)
                        .map(|i| game.mem.read(DISPLAY + i).count_ones())
                        .sum();
                    assert!(lit > 0, "the screen went blank");
                }
                Mode::Playing | Mode::Title { .. } => panic!("he came back to life"),
            }
            game.update(speccy::Input::default());
            game.sounds.clear();
        }

        assert!(
            matches!(game.mode, Mode::Title { .. }),
            "the game over sequence never finished"
        );
        assert_eq!(dying, usize::from(DEATH_FRAMES) + 1, "the death sequence");
        assert_eq!(
            foot,
            usize::from(gameover::LANDS_AT / gameover::STEP),
            "steps of the foot's descent"
        );
        assert_eq!(
            glisten,
            usize::from(gameover::GLISTEN_FRAMES) + 1,
            "frames of the message glistening"
        );
    }

    #[test]
    fn the_tune_plays_a_note_a_frame_until_it_is_switched_off() {
        let mut game = Game::started();

        // Two frames a note. The index is incremented before it is halved, so
        // the very first frame is a note on its own and the pairs run from the
        // second.
        let mut pitches = Vec::new();
        for _ in 0..5 {
            game.update(speccy::Input::default());
            pitches.push(
                game.sounds
                    .drain()
                    .filter_map(|sound| match sound {
                        speccy::sound::Sound::Note { pitch, .. } => Some(pitch),
                        _ => None,
                    })
                    .last()
                    .expect("a note a frame"),
            );
        }
        assert_eq!(pitches[1], pitches[2], "a note lasts two frames");
        assert_eq!(pitches[3], pitches[4]);
        assert_ne!(pitches[2], pitches[3], "the tune never moved on");

        // M switches it off, and on again.
        let mute = speccy::Input {
            mute: true,
            ..speccy::Input::default()
        };
        game.update(mute);
        assert!(game.music_off);
        game.sounds.clear();
        game.update(speccy::Input::default());
        assert_eq!(game.sounds.drain().count(), 0, "the tune played on");

        game.update(mute);
        assert!(!game.music_off);
        game.sounds.clear();
        game.update(speccy::Input::default());
        assert!(
            game.sounds.drain().count() > 0,
            "the tune did not come back"
        );
    }

    #[test]
    fn the_tune_climbs_as_the_lives_run_out() {
        // The pitch byte is a delay, so fewer lives means a bigger number and a
        // lower note.
        let mut low = Game::started();
        low.lives = 1;
        low.update(speccy::Input::default());
        let with_one = last_pitch(&mut low);

        let mut high = Game::started();
        high.lives = 7;
        high.update(speccy::Input::default());
        let with_seven = last_pitch(&mut high);

        assert!(
            with_one > with_seven,
            "one life {with_one}, seven lives {with_seven}"
        );
    }

    fn last_pitch(game: &mut Game) -> u8 {
        game.sounds
            .drain()
            .filter_map(|sound| match sound {
                speccy::sound::Sound::Note { pitch, .. } => Some(pitch),
                _ => None,
            })
            .last()
            .expect("a note")
    }

    #[test]
    fn pausing_runs_the_colours_through_the_screen() {
        let mut game = Game::started();
        game.update(speccy::Input::default());
        game.sounds.clear();
        let before: Vec<u8> = (0..768).map(|i| game.mem.read(ATTR_FILE + i)).collect();
        let room_border = game.border();

        let pause = speccy::Input {
            pause: true,
            ..speccy::Input::default()
        };
        game.update(pause);
        assert!(game.paused);
        let after: Vec<u8> = (0..768).map(|i| game.mem.read(ATTR_FILE + i)).collect();
        assert_ne!(before, after, "the screen stayed the same colour");

        // Nothing bright survives the cycling, and the border follows the ink.
        for attr in &after {
            assert_eq!(attr & 64, 0, "a bright cell came through the cycling");
        }
        assert_eq!(game.border(), game.mem.read(ATTR_FILE) & 7);

        // Eight passes bring every colour back where it started: three at a
        // time through eight colours.
        for _ in 0..7 {
            game.update(speccy::Input::default());
        }
        let full_circle: Vec<u8> = (0..768).map(|i| game.mem.read(ATTR_FILE + i)).collect();
        let unbright: Vec<u8> = before.iter().map(|attr| attr & 191).collect();
        assert_eq!(full_circle, unbright, "the colours did not come back round");

        // And unpausing gives the room its own border again.
        game.update(pause);
        assert!(!game.paused);
        game.update(speccy::Input::default());
        assert_eq!(game.border(), room_border);
    }

    #[test]
    fn escape_asks_to_leave() {
        let mut game = Game::started();
        game.update(speccy::Input {
            back: true,
            ..speccy::Input::default()
        });
        assert!(game.quit);
    }
}
