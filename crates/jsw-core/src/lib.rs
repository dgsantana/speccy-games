//! The Jet Set Willy engine.
//!
//! Willy has to tidy the mansion before Maria will let him go to bed. Sixty-odd
//! rooms of it, each carrying its own tile graphics, joined at their edges.
//!
//! Like [`mm_core`](https://docs.rs/mm-core), everything here is independent of
//! any window, GPU or audio device: the machine is [`speccy`], and this crate is
//! only what Software Projects wrote.

pub mod bedroom;
pub mod cartridge;
pub mod entity;
pub mod game;
pub mod gameover;
pub mod hud;
pub mod item;
pub mod map;
pub mod room;
pub mod rope;
pub mod willy;

pub use entity::Entities;
pub use game::{FRAMES_PER_SECOND, Game};
pub use hud::Clock;
pub use item::Items;
pub use jsw_data::ROOM_COUNT;
pub use room::Room;
pub use willy::Willy;
