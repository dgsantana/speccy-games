//! What the player is asking for this frame.

/// The controls, filled in by the front end and read by whichever game cares
/// about which field. Manic Miner never looks at `down`.
///
/// Left and right can both be held; the original treated that the same way it
/// treated a conveyor pulling against a keypress.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub jump: bool,
    /// Enter: start, or choose the highlighted game.
    pub start: bool,
    /// P: pause.
    pub pause: bool,
    /// M: toggle the music.
    pub mute: bool,
    /// Escape or Q: leave the game and go back to the picker. At the picker it
    /// leaves the program.
    pub back: bool,
}
