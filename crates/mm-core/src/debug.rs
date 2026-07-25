//! Developer switches, compiled only with the `debug` feature.
//!
//! These exist to make a cavern reachable and survivable while working on it.
//! Nothing here is part of the game: a build without the feature contains none
//! of it, and the accessors on [`Game`](crate::Game) that read it fold to
//! constants, so the compiled engine is unchanged.

/// What the developer has switched on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Debug {
    /// Willy never runs out of lives. He still dies; the count stays put.
    pub invulnerable: bool,
    /// Guardians stop moving, drawing and killing.
    pub no_guardians: bool,
    /// The air stops draining. The cavern clock still runs.
    pub frozen_air: bool,
}
