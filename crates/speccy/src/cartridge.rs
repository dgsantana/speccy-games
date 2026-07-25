//! What the front end needs from a game, and nothing more.

use crate::input::Input;
use crate::memory::Memory;
use crate::sound::SoundQueue;

/// A game the shell can run.
///
/// The name avoids colliding with the games' own `Game` types, and it is what
/// the machine took: the shell owns the screen, the speaker and the keyboard,
/// and hands them to whichever cartridge is plugged in.
pub trait Cartridge {
    /// Advance one Spectrum frame.
    fn update(&mut self, input: Input);

    /// The memory whose display and attribute files are drawn this frame.
    fn memory(&self) -> &Memory;

    /// Sounds queued during the frame, drained by the front end.
    fn sounds(&mut self) -> &mut SoundQueue;

    /// The colour to paint around the screen.
    fn border(&self) -> u8;

    /// The game asking to be put away, sending the shell back to the picker.
    fn finished(&self) -> bool;

    /// The debug switches this game offers, if it has any.
    #[cfg(feature = "debug")]
    fn debug(&mut self) -> Option<&mut dyn crate::debug::DebugSwitches> {
        None
    }
}
