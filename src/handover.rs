//! Not letting one key press act on two screens.
//!
//! Enter chooses a game on the picker, and the game it starts appears with no
//! idea a key was already down. Manic Miner edge-detects Enter to leave its
//! title screen, so a key still held on the next tick reads as a fresh press
//! and the title screen is skipped. Escape does the same thing in reverse: it
//! puts a game away, and the picker it returns to would read the same hold as
//! "leave the program".
//!
//! So a screen change latches the keys that mean "go" and "back", and the new
//! screen does not see either until it has been released.

use speccy::Input;

/// Keys being held through a handover, waiting to be let go.
#[derive(Debug, Clone, Copy, Default)]
pub struct Handover {
    start: bool,
    back: bool,
}

impl Handover {
    /// Latch both keys, whether or not they are down. Called when the shell
    /// swaps one screen for another.
    pub fn latch(&mut self) {
        self.start = true;
        self.back = true;
    }

    /// Hide a latched key until the frame it is released on, and every frame
    /// after that pass it through untouched.
    pub fn filter(&mut self, mut input: Input) -> Input {
        if self.start {
            self.start = input.start;
            input.start = false;
        }
        if self.back {
            self.back = input.back;
            input.back = false;
        }
        input
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(start: bool, back: bool) -> Input {
        Input {
            start,
            back,
            ..Input::default()
        }
    }

    #[test]
    fn a_key_held_through_a_handover_is_hidden() {
        let mut handover = Handover::default();
        handover.latch();
        assert!(!handover.filter(held(true, false)).start);
        assert!(!handover.filter(held(true, false)).start);
    }

    #[test]
    fn the_key_works_again_once_it_has_been_released() {
        let mut handover = Handover::default();
        handover.latch();
        handover.filter(held(true, false));
        // Released, still hidden on the frame it comes up.
        assert!(!handover.filter(held(false, false)).start);
        // Pressed again, and this time it is a real press.
        assert!(handover.filter(held(true, false)).start);
    }

    #[test]
    fn escape_is_latched_the_same_way_as_enter() {
        let mut handover = Handover::default();
        handover.latch();
        assert!(!handover.filter(held(false, true)).back);
        handover.filter(held(false, false));
        assert!(handover.filter(held(false, true)).back);
    }

    #[test]
    fn without_a_handover_nothing_is_touched() {
        let mut handover = Handover::default();
        let input = handover.filter(held(true, true));
        assert!(input.start && input.back);
    }

    #[test]
    fn latching_does_not_disturb_the_other_keys() {
        let mut handover = Handover::default();
        handover.latch();
        let input = handover.filter(Input {
            left: true,
            jump: true,
            start: true,
            ..Input::default()
        });
        assert!(input.left && input.jump, "only start and back are latched");
        assert!(!input.start);
    }
}
