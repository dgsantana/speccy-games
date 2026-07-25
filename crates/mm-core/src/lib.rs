//! The Manic Miner engine.
//!
//! Everything here is independent of any window, GPU or audio device. [`Game`]
//! owns the whole world; call [`Game::update`] once per original frame and read
//! the resulting screen with [`speccy::Frame::render`].
//!
//! The machine itself — the address space, the display file, the palette, the
//! beeper — lives in [`speccy`] and is shared with the other ports. What is
//! here is only what Matthew Smith wrote.
//!
//! ```no_run
//! use mm_core::Game;
//! use speccy::{Frame, Input};
//!
//! let mut game = Game::new();
//! let mut frame = Frame::new();
//! game.update(Input::default());
//! frame.render(&game.mem, false);
//! ```

pub mod cartridge;
pub mod cavern;
pub mod game;
pub mod guardian;
pub mod layout;
pub mod score;
pub mod special;
pub mod willy;

pub use cavern::Cavern;
pub use game::{FRAMES_PER_SECOND, Game, Mode};
pub use mm_data::CAVERN_COUNT;
pub use willy::Willy;
