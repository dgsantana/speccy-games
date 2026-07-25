//! The debug keys, and the two locks in front of them.
//!
//! The `debug` feature decides whether any of this is compiled; `--debug` on the
//! command line decides whether a build that has it does anything with it. Both
//! have to be open, so a normal build cannot cheat and a debug build still plays
//! straight unless asked not to.

#[cfg(feature = "debug")]
mod on {
    use macroquad::prelude::*;
    use mm_core::{CAVERN_COUNT, Game};

    pub fn enabled() -> bool {
        if !std::env::args().any(|arg| arg == "--debug") {
            return false;
        }
        println!("Debug mode. F1 next cavern, F2 previous, F3 guardians, F4 lives, F5 air.");
        true
    }

    pub fn read_keys(game: &mut Game) {
        let sheet = game.cavern.sheet;
        if is_key_pressed(KeyCode::F1) {
            jump(game, sheet + 1);
        }
        if is_key_pressed(KeyCode::F2) {
            jump(game, sheet + CAVERN_COUNT - 1);
        }
        if is_key_pressed(KeyCode::F3) {
            game.debug.no_guardians = !game.debug.no_guardians;
            report("guardians", !game.debug.no_guardians);
        }
        if is_key_pressed(KeyCode::F4) {
            game.debug.invulnerable = !game.debug.invulnerable;
            report("lives", !game.debug.invulnerable);
        }
        if is_key_pressed(KeyCode::F5) {
            game.debug.frozen_air = !game.debug.frozen_air;
            report("air", !game.debug.frozen_air);
        }
    }

    fn jump(game: &mut Game, sheet: usize) {
        game.goto_cavern(sheet);
        if game.cavern.sheet == sheet % CAVERN_COUNT {
            println!("cavern {}: {}", game.cavern.sheet, game.cavern.name.trim());
        }
    }

    fn report(what: &str, on: bool) {
        println!("{what} {}", if on { "on" } else { "off" });
    }
}

#[cfg(not(feature = "debug"))]
mod on {
    use mm_core::Game;

    pub fn enabled() -> bool {
        if std::env::args().any(|arg| arg == "--debug") {
            println!("--debug needs a build with the debug feature: cargo run --features debug");
        }
        false
    }

    pub fn read_keys(_game: &mut Game) {}
}

pub use on::{enabled, read_keys};
