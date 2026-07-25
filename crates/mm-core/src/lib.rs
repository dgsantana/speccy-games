//! The Manic Miner engine.
//!
//! Everything here is independent of any window, GPU or audio device. [`Game`]
//! owns the whole world; call [`Game::update`] once per original frame and read
//! the resulting screen with [`Frame::render`].
//!
//! ```no_run
//! use mm_core::{Frame, Game, Input};
//!
//! let mut game = Game::new();
//! let mut frame = Frame::new();
//! game.update(Input::default());
//! frame.render(&game.mem, false);
//! ```

pub mod cavern;
#[cfg(feature = "debug")]
pub mod debug;
pub mod display;
pub mod game;
pub mod guardian;
pub mod input;
pub mod score;
pub mod sound;
pub mod special;
pub mod speccy;
pub mod willy;

pub use cavern::Cavern;
#[cfg(feature = "debug")]
pub use debug::Debug;
pub use display::{Attribute, Frame, PALETTE};
pub use game::{FRAMES_PER_SECOND, Game, Mode};
pub use input::Input;
pub use mm_data::CAVERN_COUNT;
pub use sound::{Sound, SoundQueue};
pub use speccy::{HEIGHT, WIDTH};
pub use willy::Willy;
