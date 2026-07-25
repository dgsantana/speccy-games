//! The ZX Spectrum, as much of it as a port of one of its games needs.
//!
//! A 64K address space with the display and attribute files in the right
//! places, the ROM font, the palette, and a description of what the beeper is
//! being asked to do. Nothing here knows about a window, a GPU or an audio
//! device, so all of it is testable headlessly, and nothing here is specific to
//! any one game.
//!
//! A game is a [`Cartridge`]: the shell owns the machine and hands it over.
//!
//! ```no_run
//! use speccy::{Frame, Memory};
//!
//! let mut memory = Memory::new();
//! let mut frame = Frame::new();
//! memory.print_str("Hello", 16384);
//! frame.render(&memory, false);
//! ```

pub mod cartridge;
#[cfg(feature = "debug")]
pub mod debug;
pub mod display;
pub mod font;
pub mod input;
pub mod layout;
pub mod memory;
pub mod sound;

pub use cartridge::Cartridge;
#[cfg(feature = "debug")]
pub use debug::{Debug, DebugSwitches};
pub use display::{Attribute, Frame, PALETTE};
pub use font::FONT;
pub use input::Input;
pub use layout::{ATTR_BACK, ATTR_BUF, PLAY_ATTRS, PLAY_PIXELS, SCREEN_BACK, SCREEN_BUF};
pub use memory::{HEIGHT, Memory, WIDTH};
pub use sound::{Sound, SoundQueue};
