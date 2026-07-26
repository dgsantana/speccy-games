//! Developer switches, compiled only with the `debug` feature.
//!
//! These exist to make a level reachable and survivable while working on it.
//! Nothing here is part of any game: a build without the feature contains none
//! of it, and the accessors that read it fold to constants, so the compiled
//! engines are unchanged.

/// What the developer has switched on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Debug {
    /// The player never runs out of lives. They still die; the count stays put.
    pub invulnerable: bool,
    /// The things that chase the player stop moving, drawing and killing.
    pub no_guardians: bool,
    /// Whatever is counting the level down stops. In Manic Miner that is the air.
    pub frozen_air: bool,
    /// Show a map of the levels instead of the game, for a game that has one.
    pub map: bool,
}

/// A game that can be poked at while it runs.
///
/// Implemented by the game and called by the front end's debug keys, so the
/// keys mean the same thing whichever cartridge is in.
pub trait DebugSwitches {
    /// The switches, to read and to set.
    fn switches(&mut self) -> &mut Debug;

    /// Go straight to a level: a cavern in Manic Miner, a room in Jet Set
    /// Willy. Numbers past the end wrap. Does nothing unless the game is being
    /// played.
    fn goto_level(&mut self, level: usize);

    /// The level being played, so the front end can ask for the next one.
    fn level(&self) -> usize;

    /// How many levels there are, for wrapping.
    fn level_count(&self) -> usize;

    /// What the level is called, for the line printed on a jump.
    fn level_name(&self) -> &str;
}
