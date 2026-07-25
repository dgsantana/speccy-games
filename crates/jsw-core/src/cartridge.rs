//! Plugging Jet Set Willy into the shell.

use speccy::{Cartridge, Input, Memory, SoundQueue};

use crate::game::Game;

impl Cartridge for Game {
    fn update(&mut self, input: Input) {
        Game::update(self, input);
    }

    fn memory(&self) -> &Memory {
        &self.mem
    }

    fn sounds(&mut self) -> &mut SoundQueue {
        &mut self.sounds
    }

    fn border(&self) -> u8 {
        Game::border(self)
    }

    fn finished(&self) -> bool {
        self.quit
    }

    #[cfg(feature = "debug")]
    fn debug(&mut self) -> Option<&mut dyn speccy::DebugSwitches> {
        Some(self)
    }
}

#[cfg(feature = "debug")]
impl speccy::DebugSwitches for Game {
    fn switches(&mut self) -> &mut speccy::Debug {
        &mut self.debug
    }

    fn goto_level(&mut self, level: usize) {
        self.goto_room(level);
    }

    fn level(&self) -> usize {
        self.room.number
    }

    fn level_count(&self) -> usize {
        jsw_data::ROOM_COUNT
    }

    fn level_name(&self) -> &str {
        // The name is bytes, and three of the 64 rooms hold code rather than
        // text, so this is the room number's stand-in until 2c prints properly.
        "room"
    }
}
