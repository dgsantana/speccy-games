//! The picker: which game to put in the machine.
//!
//! It draws into a [`Memory`] with the ROM font and reads an [`Input`], exactly
//! as a game does, so the front end renders it through the same texture path
//! and there is no second way of drawing anything.

use speccy::memory::{ATTR_FILE, DISPLAY, display_row_offset};
use speccy::{Cartridge, Input, Memory};

/// One line of the catalogue.
pub struct Entry {
    pub title: &'static str,
    /// The year it came out, or what to show instead until it is written.
    pub year: &'static str,
    /// How to start it. `None` means the port does not exist yet: the row is
    /// drawn dim and cannot be chosen.
    pub launch: Option<fn() -> Box<dyn Cartridge>>,
}

/// What the games are. Adding a port means writing its crate and filling in a
/// `launch`; nothing else here changes.
pub const CATALOGUE: &[Entry] = &[
    Entry {
        title: "MANIC MINER",
        year: "1983",
        launch: Some(|| Box::new(mm_core::Game::new())),
    },
    Entry {
        title: "JET SET WILLY",
        year: "1984",
        launch: Some(|| Box::new(jsw_core::Game::new())),
    },
    Entry {
        title: "JET SET WILLY II",
        year: "1985",
        launch: Some(|| Box::new(jsw_core::Game::new_jsw2())),
    },
    Entry {
        title: "MATCH POINT",
        year: "SOON",
        launch: None,
    },
];

/// Bright white on black, for the heading.
const INK_TITLE: u8 = 64 | 7;
/// Bright yellow on black: the row the cursor is on.
const INK_SELECTED: u8 = 64 | 6;
/// Plain white on black: a game that can be played.
const INK_ENTRY: u8 = 7;
/// Blue on black, dim enough to read as unavailable.
const INK_ABSENT: u8 = 1;

/// Character row of the first catalogue entry; they are two rows apart.
const FIRST_ROW: usize = 9;
const ROW_STEP: usize = 2;

/// What the picker wants the shell to do after a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Keep showing the picker.
    Stay,
    /// Start the game at this index of [`CATALOGUE`].
    Play(usize),
    /// Leave the program.
    Quit,
}

/// The start screen.
#[derive(Debug)]
pub struct Menu {
    mem: Memory,
    cursor: usize,
    /// Edge detection, so a held key moves the cursor once.
    prev: Input,
}

impl Menu {
    /// Open the picker with the cursor on `cursor`, which is how coming back
    /// from a game lands on the game just left.
    pub fn new(cursor: usize) -> Self {
        let mut menu = Self {
            mem: Memory::new(),
            cursor: cursor.min(CATALOGUE.len() - 1),
            prev: Input::default(),
        };
        menu.draw();
        menu
    }

    pub fn memory(&self) -> &Memory {
        &self.mem
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Read the keyboard and redraw. The cursor wraps at both ends.
    pub fn update(&mut self, input: Input) -> Choice {
        let pressed = Input {
            up: input.up && !self.prev.up,
            down: input.down && !self.prev.down,
            start: input.start && !self.prev.start,
            back: input.back && !self.prev.back,
            ..Input::default()
        };
        self.prev = input;

        let last = CATALOGUE.len() - 1;
        if pressed.up {
            self.cursor = if self.cursor == 0 { last } else { self.cursor - 1 };
        }
        if pressed.down {
            self.cursor = if self.cursor == last { 0 } else { self.cursor + 1 };
        }
        self.draw();

        if pressed.back {
            return Choice::Quit;
        }
        // Enter on a game that is not written yet does nothing at all.
        if pressed.start && CATALOGUE[self.cursor].launch.is_some() {
            return Choice::Play(self.cursor);
        }
        Choice::Stay
    }

    fn draw(&mut self) {
        self.mem.fill(DISPLAY, 6144, 0);
        self.mem.fill(ATTR_FILE, 768, 0);

        centre(&mut self.mem, 3, "S P E C C Y   A R C A D E", INK_TITLE);

        for (index, entry) in CATALOGUE.iter().enumerate() {
            let row = FIRST_ROW + index * ROW_STEP;
            let selected = index == self.cursor;
            let ink = match (selected, entry.launch.is_some()) {
                (true, _) => INK_SELECTED,
                (false, true) => INK_ENTRY,
                (false, false) => INK_ABSENT,
            };

            let cursor = if selected { ">" } else { " " };
            let line = format!(
                "{cursor} {}  {:<20}{}",
                index + 1,
                entry.title,
                entry.year
            );
            write_at(&mut self.mem, row, 2, &line, ink);
        }

        centre(&mut self.mem, 20, "ENTER TO PLAY", INK_ENTRY);
        centre(&mut self.mem, 21, "ESC TO QUIT", INK_ENTRY);
    }
}

/// Print `text` at a character row and column, colouring the cells it covers.
fn write_at(mem: &mut Memory, row: usize, col: usize, text: &str, ink: u8) {
    let addr = DISPLAY + (display_row_offset(row * 8) + col) as u16;
    mem.print_str(text, addr);

    let attr = ATTR_FILE + (row * 32 + col) as u16;
    for i in 0..text.len().min(32 - col) {
        mem.write(attr + i as u16, ink);
    }
}

/// The same, centred across the 32 columns.
fn centre(mem: &mut Memory, row: usize, text: &str, ink: u8) {
    let col = (32 - text.len().min(32)) / 2;
    write_at(mem, row, col, text, ink);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(menu: &mut Menu, input: Input) -> Choice {
        // A key has to be seen released before it counts as pressed again.
        menu.update(Input::default());
        menu.update(input)
    }

    #[test]
    fn the_cursor_wraps_past_the_top() {
        let mut menu = Menu::new(0);
        press(
            &mut menu,
            Input {
                up: true,
                ..Input::default()
            },
        );
        assert_eq!(menu.cursor(), CATALOGUE.len() - 1);
    }

    #[test]
    fn the_cursor_wraps_past_the_bottom() {
        let mut menu = Menu::new(CATALOGUE.len() - 1);
        press(
            &mut menu,
            Input {
                down: true,
                ..Input::default()
            },
        );
        assert_eq!(menu.cursor(), 0);
    }

    #[test]
    fn enter_on_a_written_game_starts_it() {
        let mut menu = Menu::new(0);
        let choice = press(
            &mut menu,
            Input {
                start: true,
                ..Input::default()
            },
        );
        assert_eq!(choice, Choice::Play(0));
    }

    #[test]
    fn enter_on_a_game_that_is_not_written_does_nothing() {
        let absent = CATALOGUE
            .iter()
            .position(|entry| entry.launch.is_none())
            .expect("the catalogue lists a game that is not written yet");
        let mut menu = Menu::new(absent);
        let choice = press(
            &mut menu,
            Input {
                start: true,
                ..Input::default()
            },
        );
        assert_eq!(choice, Choice::Stay);
    }

    #[test]
    fn escape_leaves_the_program() {
        let mut menu = Menu::new(0);
        let choice = press(
            &mut menu,
            Input {
                back: true,
                ..Input::default()
            },
        );
        assert_eq!(choice, Choice::Quit);
    }

    #[test]
    fn every_entry_is_drawn_into_the_display_file() {
        let menu = Menu::new(0);
        let pixels: u32 = (0..6144)
            .map(|i| menu.memory().read(DISPLAY + i).count_ones())
            .sum();
        assert!(pixels > 200, "the picker drew almost nothing: {pixels} bits");

        // Every catalogue row has coloured cells, dim or not.
        for index in 0..CATALOGUE.len() {
            let row = FIRST_ROW + index * ROW_STEP;
            let attr = ATTR_FILE + (row * 32 + 2) as u16;
            assert_ne!(menu.memory().read(attr), 0, "row {row} has no colour");
        }
    }
}
