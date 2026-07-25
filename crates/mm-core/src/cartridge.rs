//! Plugging Manic Miner into the shell.

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
        self.goto_cavern(level);
    }

    fn level(&self) -> usize {
        self.cavern.sheet
    }

    fn level_count(&self) -> usize {
        mm_data::CAVERN_COUNT
    }

    fn level_name(&self) -> &str {
        self.cavern.name.trim()
    }
}
