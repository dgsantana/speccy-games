//! Where the rooms sit relative to each other, and a map drawn on the screen.
//!
//! The layout is not stored anywhere in the game: it is implied by the exits.
//! Walking them outward from the room Willy starts in gives every room a place —
//! a left exit is one column west, an up exit one row north — and the mansion
//! turns out to be a tidy grid apart from a handful of rooms that loop across
//! the house.

use std::collections::{HashMap, VecDeque};
use std::sync::OnceLock;

use speccy::layout::COLUMNS;
use speccy::memory::{ATTR_FILE, DISPLAY, Memory, display_row_offset};

use crate::room::Room;

/// A room's place in the house, in room-sized steps from Willy's first room.
pub type Place = (i32, i32);

/// Most rooms name room 0 for an edge with nothing past it, so an exit to The
/// Off Licence means "no exit" — except from The Bridge, which is next door to
/// it.
pub fn leads_somewhere(from: usize, dest: usize) -> bool {
    dest != from && (dest != 0 || from == 1)
}

/// The mansion's layout, worked out once.
#[derive(Debug)]
pub struct Layout {
    /// Where each room sits.
    pub places: HashMap<usize, Place>,
    /// Exits that do not join neighbouring rooms. Real, and worth knowing about.
    pub oddities: Vec<(usize, usize)>,
    pub min: Place,
    pub max: Place,
}

impl Layout {
    /// Columns and rows the house spans.
    pub fn size(&self) -> (i32, i32) {
        (self.max.0 - self.min.0 + 1, self.max.1 - self.min.1 + 1)
    }

    /// A room's place counted from the top-left of the house.
    pub fn cell_of(&self, room: usize) -> Option<(i32, i32)> {
        self.places
            .get(&room)
            .map(|&(x, y)| (x - self.min.0, y - self.min.1))
    }
}

/// The layout, built on first use.
pub fn layout() -> &'static Layout {
    static LAYOUT: OnceLock<Layout> = OnceLock::new();
    LAYOUT.get_or_init(build)
}

fn build() -> Layout {
    let rooms: HashMap<usize, Room> = (0..jsw_data::ROOM_COUNT)
        .map(Room::load)
        .filter(Room::is_real)
        .map(|room| (room.number, room))
        .collect();

    let mut places: HashMap<usize, Place> = HashMap::new();
    let mut oddities = Vec::new();
    let mut queue = VecDeque::new();

    places.insert(crate::game::START_ROOM, (0, 0));
    queue.push_back(crate::game::START_ROOM);

    while let Some(number) = queue.pop_front() {
        let here = places[&number];
        let Some(room) = rooms.get(&number) else {
            continue;
        };
        for (dest, step) in [
            (room.exits.left, (-1, 0)),
            (room.exits.right, (1, 0)),
            (room.exits.up, (0, -1)),
            (room.exits.down, (0, 1)),
        ] {
            let dest = usize::from(dest);
            if !leads_somewhere(number, dest) || !rooms.contains_key(&dest) {
                continue;
            }
            let want = (here.0 + step.0, here.1 + step.1);
            match places.get(&dest) {
                None => {
                    places.insert(dest, want);
                    queue.push_back(dest);
                }
                Some(&had) if had != want => oddities.push((number, dest)),
                Some(_) => {}
            }
        }
    }

    let min = (
        places.values().map(|p| p.0).min().unwrap_or(0),
        places.values().map(|p| p.1).min().unwrap_or(0),
    );
    let max = (
        places.values().map(|p| p.0).max().unwrap_or(0),
        places.values().map(|p| p.1).max().unwrap_or(0),
    );

    Layout {
        places,
        oddities,
        min,
        max,
    }
}

/// Attribute of the room being played: bright white, and flashing so it is
/// obvious at a glance.
const HERE: u8 = 128 | 64 | 7;
/// A room that has been walked into.
const VISITED: u8 = 64 | 6;
/// A room that has not.
const UNVISITED: u8 = 5;

/// Draw the map over the whole screen.
///
/// Each room is a two-by-two block of cells, which fits the mansion's fifteen
/// by eight into thirty by sixteen with the bottom rows left for the name.
pub fn draw(mem: &mut Memory, current: usize, visited: &[bool], name: &[u8; 32]) {
    mem.fill(DISPLAY, 6144, 0);
    mem.fill(ATTR_FILE, 768, 0);

    let layout = layout();
    for &room in layout.places.keys() {
        let Some((x, y)) = layout.cell_of(room) else {
            continue;
        };
        let (column, row) = ((x * 2) as usize, (y * 2) as usize);
        if column + 1 >= COLUMNS || row + 1 >= 16 {
            continue;
        }

        let attr = if room == current {
            HERE
        } else if visited.get(room).copied().unwrap_or(false) {
            VISITED
        } else {
            UNVISITED
        };

        // A filled block for where Willy is, a hollow one for everywhere else,
        // so the map reads at a glance rather than needing to be studied.
        let filled = room == current;
        for cell_row in 0..2 {
            for cell_column in 0..2 {
                let r = row + cell_row;
                let c = column + cell_column;
                mem.write(ATTR_FILE + (r * 32 + c) as u16, attr);
                let at = DISPLAY + (display_row_offset(r * 8) + c) as u16;
                for pixel_row in 0..8u16 {
                    let byte = if filled {
                        255
                    } else {
                        edge(cell_row, cell_column, pixel_row)
                    };
                    mem.write(at + pixel_row * 256, byte);
                }
            }
        }
    }

    // The room's own name, so the map says where you are in words too.
    let at = DISPLAY + display_row_offset(18 * 8) as u16;
    for (column, &byte) in name.iter().enumerate() {
        mem.print_char(byte, at + column as u16);
    }
    for column in 0..32u16 {
        mem.write(ATTR_FILE + 18 * 32 + column, 64 | 7);
    }

    let legend = "F6 map   flashing = you are here";
    mem.print_str(legend, DISPLAY + display_row_offset(21 * 8) as u16);
    for column in 0..32u16 {
        mem.write(ATTR_FILE + 21 * 32 + column, 5);
    }
}

/// One pixel row of the outline of a room's two-by-two block.
fn edge(cell_row: usize, cell_column: usize, pixel_row: u16) -> u8 {
    let top = cell_row == 0 && pixel_row == 0;
    let bottom = cell_row == 1 && pixel_row == 7;
    if top || bottom {
        return 255;
    }
    match cell_column {
        0 => 0x80,
        _ => 0x01,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_real_room_finds_a_place() {
        let layout = layout();
        let real = (0..jsw_data::ROOM_COUNT)
            .filter(|&n| Room::load(n).is_real())
            .count();
        assert_eq!(layout.places.len(), real, "some rooms were not reachable");
    }

    #[test]
    fn the_house_is_fifteen_by_eight() {
        assert_eq!(layout().size(), (15, 8));
    }

    #[test]
    fn the_bathroom_sits_where_willy_starts() {
        let layout = layout();
        assert_eq!(layout.places[&crate::game::START_ROOM], (0, 0));
        // Its neighbours are one step away in the right directions.
        let bathroom = Room::load(crate::game::START_ROOM);
        assert_eq!(
            layout.places[&usize::from(bathroom.exits.left)],
            (-1, 0),
            "the room to the left is not to the left"
        );
        assert_eq!(layout.places[&usize::from(bathroom.exits.down)], (0, 1));
    }

    #[test]
    fn the_rooms_that_loop_across_the_house_are_noticed() {
        // The Back Door, Back Stairway and Wine Cellar are the famous ones.
        assert!(
            layout().oddities.len() > 10,
            "the mansion is not that tidy: {:?}",
            layout().oddities
        );
    }

    #[test]
    fn drawing_the_map_marks_where_you_are() {
        let mut mem = Memory::new();
        let room = Room::load(crate::game::START_ROOM);
        let mut visited = vec![false; jsw_data::ROOM_COUNT];
        visited[crate::game::START_ROOM] = true;
        draw(&mut mem, crate::game::START_ROOM, &visited, &room.name);

        let (x, y) = layout().cell_of(crate::game::START_ROOM).expect("placed");
        let attr = mem.read(ATTR_FILE + ((y * 2 * 32) + x * 2) as u16);
        assert_eq!(attr, HERE, "the room you are in is not marked");

        let painted = (0..768).filter(|&i| mem.read(ATTR_FILE + i) != 0).count();
        assert!(painted > 60, "the map is nearly empty: {painted} cells");
    }
}
