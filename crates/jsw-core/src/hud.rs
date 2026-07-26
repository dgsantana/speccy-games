//! The bottom third of the screen: the room's name, the item count, the clock
//! and Willy's remaining lives.
//!
//! The original prints the name at (16,0) and "Items collected 000 Time 00:00 m"
//! at (19,0), and draws a Willy sprite per life along row 21. The colours come
//! from a table at 39424 that is copied over the bottom third of the attribute
//! file.

use speccy::memory::{ATTR_FILE, DISPLAY, DrawMode, Memory, display_row_offset};

/// Character row the room's name is printed on.
pub const NAME_ROW: usize = 16;
/// Character row the item count and clock are printed on.
pub const STATUS_ROW: usize = 19;
/// Character row the lives are drawn on.
pub const LIVES_ROW: usize = 21;

/// The game starts at seven in the morning.
pub const START_HOUR: u8 = 7;

/// Minutes of game time in a frame counter's full turn: the original's minute
/// counter is a byte, and the clock advances when it wraps.
pub const FRAMES_PER_MINUTE: u16 = 256;

/// The clock, counted in minutes from seven in the morning.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Clock {
    /// Minutes since 7:00am.
    pub minutes: u16,
}

impl Clock {
    /// The hour on a twelve-hour dial, the minute, and whether it is afternoon.
    pub fn reading(&self) -> (u8, u8, bool) {
        let total = u16::from(START_HOUR) * 60 + self.minutes;
        let hour24 = (total / 60) % 24;
        let minute = (total % 60) as u8;
        let afternoon = hour24 >= 12;
        let hour = match hour24 % 12 {
            0 => 12,
            h => h as u8,
        };
        (hour, minute, afternoon)
    }

    /// Advance a minute.
    pub fn tick(&mut self) {
        self.minutes = self.minutes.saturating_add(1);
    }

    /// Whether it has reached one in the morning, at which point the original
    /// ends the game.
    pub fn past_bedtime(&self) -> bool {
        // 7am to 1am is eighteen hours.
        self.minutes >= 18 * 60
    }

    /// The clock as the original prints it: two columns for the hour, then the
    /// minutes, then am or pm.
    pub fn text(&self) -> String {
        let (hour, minute, afternoon) = self.reading();
        let half = if afternoon { 'p' } else { 'a' };
        format!("{hour:>2}:{minute:02}{half}m")
    }
}

/// Print the room name, the item count and the clock, and draw the lives.
pub fn draw(
    mem: &mut Memory,
    name: &[u8; 32],
    collected: usize,
    clock: &Clock,
    lives: u8,
    life_frame: usize,
) {
    // The colours of the whole bottom third come from the table.
    for (index, &attr) in jsw_data::hud::ATTRS.iter().enumerate() {
        mem.write(ATTR_FILE + (NAME_ROW * 32 + index) as u16, attr);
    }

    // The name is already padded to 32 characters by the room definition.
    let at = row_addr(NAME_ROW);
    for (column, &byte) in name.iter().enumerate() {
        mem.print_char(byte, at + column as u16);
    }

    let status = format!("Items collected {collected:03} Time {}", clock.text());
    mem.print_str(&status, row_addr(STATUS_ROW));

    draw_lives(mem, lives, life_frame);
}

/// A Willy sprite for each life, two columns apart, as the original's 35211 does.
fn draw_lives(mem: &mut Memory, lives: u8, life_frame: usize) {
    let at = row_addr(LIVES_ROW);
    for life in 0..lives as usize {
        let column = life * 2;
        if column >= 32 {
            break;
        }
        let frame = life_frame % 4;
        let sprite: [u8; 32] = jsw_data::sprites::WILLY[frame * 32..(frame + 1) * 32]
            .try_into()
            .expect("a Willy frame is 32 bytes");
        mem.draw_16x16(&sprite, at + column as u16, DrawMode::Overwrite);
    }
}

/// Display-file address of the leftmost cell of a character row.
fn row_addr(row: usize) -> u16 {
    DISPLAY + display_row_offset(row * 8) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_starts_at_seven_in_the_morning() {
        let clock = Clock::default();
        assert_eq!(clock.reading(), (7, 0, false));
        assert_eq!(clock.text(), " 7:00am");
    }

    #[test]
    fn the_clock_runs_through_noon_and_midnight() {
        let mut clock = Clock::default();
        for _ in 0..5 * 60 {
            clock.tick();
        }
        assert_eq!(clock.text(), "12:00pm", "noon");

        for _ in 0..60 {
            clock.tick();
        }
        assert_eq!(clock.text(), " 1:00pm");

        // Seven in the morning to midnight is seventeen hours.
        let mut midnight = Clock::default();
        for _ in 0..17 * 60 {
            midnight.tick();
        }
        assert_eq!(midnight.text(), "12:00am");
        assert!(!midnight.past_bedtime());

        for _ in 0..60 {
            midnight.tick();
        }
        assert_eq!(midnight.text(), " 1:00am");
        assert!(midnight.past_bedtime());
    }

    #[test]
    fn the_status_line_is_thirty_two_characters() {
        let clock = Clock::default();
        let status = format!("Items collected {:03} Time {}", 0, clock.text());
        assert_eq!(status.len(), 32, "{status:?}");
        assert_eq!(status, "Items collected 000 Time  7:00am");
    }

    #[test]
    fn drawing_the_hud_puts_the_name_and_status_on_the_screen() {
        let mut mem = Memory::new();
        let room = crate::room::Room::load(33);
        draw(&mut mem, &room.name, 3, &Clock::default(), 7, 0);

        let pixels: u32 = (4096..6144)
            .map(|i| mem.read(DISPLAY + i).count_ones())
            .sum();
        assert!(pixels > 200, "the bottom third is nearly empty: {pixels}");

        // The name's row is coloured from the table's first entry.
        assert_eq!(
            mem.read(ATTR_FILE + (NAME_ROW * 32) as u16),
            jsw_data::hud::ATTRS[0]
        );
    }
}
