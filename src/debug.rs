//! The debug keys, and the two locks in front of them.
//!
//! The `debug` feature decides whether any of this is compiled; `--debug` on the
//! command line decides whether a build that has it does anything with it. Both
//! have to be open, so a normal build cannot cheat and a debug build still plays
//! straight unless asked not to.

#[cfg(feature = "debug")]
mod on {
    use macroquad::prelude::*;
    use speccy::Cartridge;

    pub fn enabled() -> bool {
        if !std::env::args().any(|arg| arg == "--debug") {
            return false;
        }
        println!(
            "Debug mode. F1 next level, F2 previous, F3 guardians, F4 lives, \
F5 timer, F6 map."
        );
        true
    }

    pub fn read_keys(game: &mut dyn Cartridge) {
        // A game with no switches — and the picker, which is not a game at all —
        // simply has nothing for these keys to do.
        let Some(switches) = game.debug() else {
            return;
        };

        let count = switches.level_count();
        let level = switches.level();
        if is_key_pressed(KeyCode::F1) {
            jump(switches, level + 1, count);
        }
        if is_key_pressed(KeyCode::F2) {
            jump(switches, level + count - 1, count);
        }
        if is_key_pressed(KeyCode::F3) {
            let on = !switches.switches().no_guardians;
            switches.switches().no_guardians = on;
            report("guardians", !on);
        }
        if is_key_pressed(KeyCode::F4) {
            let on = !switches.switches().invulnerable;
            switches.switches().invulnerable = on;
            report("lives", !on);
        }
        if is_key_pressed(KeyCode::F5) {
            let on = !switches.switches().frozen_air;
            switches.switches().frozen_air = on;
            report("timer", !on);
        }
        if is_key_pressed(KeyCode::F6) {
            let on = !switches.switches().map;
            switches.switches().map = on;
            report("map", on);
        }
    }

    fn jump(switches: &mut dyn speccy::DebugSwitches, level: usize, count: usize) {
        let wanted = level % count;
        switches.goto_level(wanted);
        if switches.level() == wanted {
            println!("level {wanted}: {}", switches.level_name());
        }
    }

    fn report(what: &str, on: bool) {
        println!("{what} {}", if on { "on" } else { "off" });
    }
}

#[cfg(not(feature = "debug"))]
mod on {
    use speccy::Cartridge;

    pub fn enabled() -> bool {
        if std::env::args().any(|arg| arg == "--debug") {
            println!("--debug needs a build with the debug feature: cargo run --features debug");
        }
        false
    }

    pub fn read_keys(_game: &mut dyn Cartridge) {}
}

pub use on::{enabled, read_keys};
