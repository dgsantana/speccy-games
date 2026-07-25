//! What the player is asking Willy to do this frame.

/// The controls the game reads. Left and right can both be held; the original
/// treated that the same way it treated a conveyor pulling against a keypress.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    /// Enter: start the game from the title screen.
    pub start: bool,
    /// P: pause.
    pub pause: bool,
    /// M: toggle the in-game tune.
    pub mute: bool,
    /// Q: quit.
    pub quit: bool,
}
