//! Draw the mansion: which room joins which, from the rooms' own exit lists.
//!
//! ```text
//! cargo run -p jsw-core --example map -- mansion.svg
//! ```
//!
//! Prints a grid of room numbers and writes an SVG of the whole house. Rooms are
//! placed by walking their exits from the one Willy starts in, so the layout is
//! the game's rather than anything guessed: a left exit puts a room one column
//! west, an up exit one row north, and so on.
//!
//! Most of the mansion is a tidy grid - room numbers rise westward and northward
//! - but not all of it, and the exits are not always mutual. Anything that does
//! not fit is listed rather than hidden.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fmt::Write as _;

use jsw_core::Room;

/// A room's place in the house.
type Place = (i32, i32);

/// Whether an exit goes anywhere.
///
/// A room names itself when there is no way out that side, and most of the
/// mansion names room 0 for the same purpose - so many rooms claim to lead to
/// The Off Licence when what they mean is "nowhere". Only its real neighbour,
/// The Bridge, is taken at its word.
fn leads_somewhere(from: usize, dest: usize) -> bool {
    dest != from && (dest != 0 || from == 1)
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "mansion.svg".into());

    let rooms: Vec<Room> = (0..jsw_core::ROOM_COUNT)
        .map(Room::load)
        .filter(|room| room.is_real())
        .collect();

    let (places, oddities) = place_rooms(&rooms);
    print_grid(&rooms, &places);

    if oddities.is_empty() {
        println!("\nevery exit fits the grid");
    } else {
        println!("\n{} exits that do not fit the grid:", oddities.len());
        for line in &oddities {
            println!("  {line}");
        }
    }

    let svg = draw_svg(&rooms, &places);
    std::fs::write(&path, svg).expect("could not write the map");
    println!("\nwrote {path}");
}

/// Walk the exits outward from Willy's starting room, giving each room a place.
fn place_rooms(rooms: &[Room]) -> (HashMap<usize, Place>, Vec<String>) {
    let by_number: HashMap<usize, &Room> = rooms.iter().map(|r| (r.number, r)).collect();
    let start = jsw_core::game::START_ROOM;

    let mut places: HashMap<usize, Place> = HashMap::new();
    let mut oddities = Vec::new();
    let mut queue = VecDeque::new();

    places.insert(start, (0, 0));
    queue.push_back(start);

    while let Some(number) = queue.pop_front() {
        let here = places[&number];
        let Some(room) = by_number.get(&number) else {
            continue;
        };

        for (name, dest, step) in [
            ("left", room.exits.left, (-1, 0)),
            ("right", room.exits.right, (1, 0)),
            ("up", room.exits.up, (0, -1)),
            ("down", room.exits.down, (0, 1)),
        ] {
            let dest = usize::from(dest);
            if !leads_somewhere(number, dest) || !by_number.contains_key(&dest) {
                continue;
            }
            let want = (here.0 + step.0, here.1 + step.1);
            match places.get(&dest) {
                None => {
                    places.insert(dest, want);
                    queue.push_back(dest);
                }
                Some(&had) if had != want => {
                    let name_of = |n: usize| {
                        by_number
                            .get(&n)
                            .map_or_else(|| n.to_string(), |r| r.title.clone())
                    };
                    oddities.push(format!(
                        "{} ({}) {name} to {} ({}), which sits at {:?} not {:?}",
                        number,
                        name_of(number),
                        dest,
                        name_of(dest),
                        had,
                        want
                    ));
                }
                Some(_) => {}
            }
        }
    }

    (places, oddities)
}

/// Print the house as a grid of room numbers.
fn print_grid(rooms: &[Room], places: &HashMap<usize, Place>) {
    let at: BTreeMap<Place, usize> = places.iter().map(|(&n, &p)| (p, n)).collect();
    let (min_x, max_x, min_y, max_y) = bounds(places);

    println!(
        "{} of {} rooms placed, {} columns by {} rows",
        places.len(),
        rooms.len(),
        max_x - min_x + 1,
        max_y - min_y + 1
    );

    for y in min_y..=max_y {
        let mut line = String::new();
        for x in min_x..=max_x {
            match at.get(&(x, y)) {
                Some(number) => {
                    let _ = write!(line, "{number:3} ");
                }
                None => line.push_str("  . "),
            }
        }
        println!("{}", line.trim_end());
    }

    let missing: Vec<usize> = rooms
        .iter()
        .map(|r| r.number)
        .filter(|n| !places.contains_key(n))
        .collect();
    if !missing.is_empty() {
        println!("\nnot reachable from the start: {missing:?}");
    }
}

fn bounds(places: &HashMap<usize, Place>) -> (i32, i32, i32, i32) {
    let xs: Vec<i32> = places.values().map(|p| p.0).collect();
    let ys: Vec<i32> = places.values().map(|p| p.1).collect();
    (
        xs.iter().copied().min().unwrap_or(0),
        xs.iter().copied().max().unwrap_or(0),
        ys.iter().copied().min().unwrap_or(0),
        ys.iter().copied().max().unwrap_or(0),
    )
}

/// One room's box in the drawing.
const CELL_W: i32 = 132;
const CELL_H: i32 = 74;
const PAD: i32 = 10;

fn draw_svg(rooms: &[Room], places: &HashMap<usize, Place>) -> String {
    let by_number: HashMap<usize, &Room> = rooms.iter().map(|r| (r.number, r)).collect();
    let (min_x, max_x, min_y, max_y) = bounds(places);
    let width = (max_x - min_x + 1) * CELL_W + 2 * PAD;
    let height = (max_y - min_y + 1) * CELL_H + 2 * PAD + 30;

    let corner = |place: Place| {
        (
            PAD + (place.0 - min_x) * CELL_W,
            PAD + 30 + (place.1 - min_y) * CELL_H,
        )
    };

    let mut svg = String::new();
    let _ = write!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}"
viewBox="0 0 {width} {height}" font-family="monospace">
<style>
  .room {{ fill: #12121a; stroke: #4a4a68; }}
  .start {{ fill: #1d2b1d; stroke: #6fbf6f; stroke-width: 2; }}
  .num {{ fill: #8f8fb0; font-size: 11px; }}
  .name {{ fill: #e8e8f0; font-size: 11px; }}
  .odd {{ stroke: #d08040; stroke-width: 2; fill: none; }}
  text {{ dominant-baseline: hanging; }}
</style>
<rect width="{width}" height="{height}" fill="#08080c"/>
<text x="{PAD}" y="{PAD}" fill="#b0b0d0" font-size="14px">Jet Set Willy: the mansion,
laid out by the rooms' own exits</text>
"##
    );

    // Connections that do not simply join neighbouring boxes get a line drawn.
    for room in rooms {
        let Some(&from) = places.get(&room.number) else {
            continue;
        };
        for (dest, step) in [
            (room.exits.left, (-1, 0)),
            (room.exits.right, (1, 0)),
            (room.exits.up, (0, -1)),
            (room.exits.down, (0, 1)),
        ] {
            let dest = usize::from(dest);
            if !leads_somewhere(room.number, dest) || !by_number.contains_key(&dest) {
                continue;
            }
            let Some(&to) = places.get(&dest) else {
                continue;
            };
            if (to.0 - from.0, to.1 - from.1) == step {
                continue; // a plain join between neighbours; the grid shows it
            }
            let (fx, fy) = corner(from);
            let (tx, ty) = corner(to);
            let _ = write!(
                svg,
                r##"<path class="odd" d="M {} {} Q {} {} {} {}"/>
"##,
                fx + CELL_W / 2,
                fy + CELL_H / 2,
                (fx + tx) / 2 + CELL_W / 2,
                (fy + ty) / 2 + CELL_H / 2 - 40,
                tx + CELL_W / 2,
                ty + CELL_H / 2
            );
        }
    }

    for room in rooms {
        let Some(&place) = places.get(&room.number) else {
            continue;
        };
        let (x, y) = corner(place);
        let class = if room.number == jsw_core::game::START_ROOM {
            "start"
        } else {
            "room"
        };
        let _ = write!(
            svg,
            r##"<rect class="{class}" x="{}" y="{}" width="{}" height="{}" rx="4"/>
<text class="num" x="{}" y="{}">{}</text>
"##,
            x + 3,
            y + 3,
            CELL_W - 6,
            CELL_H - 6,
            x + 9,
            y + 9,
            room.number
        );

        for (line, part) in wrap(&room.title, 17).into_iter().enumerate() {
            let _ = write!(
                svg,
                r##"<text class="name" x="{}" y="{}">{}</text>
"##,
                x + 9,
                y + 25 + line as i32 * 13,
                escape(&part)
            );
        }
    }

    svg.push_str("</svg>\n");
    svg
}

/// Break a room name into lines short enough for its box.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines.truncate(3);
    lines
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
