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
//! Most of the mansion is a tidy grid, room numbers rising westward and
//! northward, but not all of it: the exits are not always mutual. Anything that
//! does not fit is listed rather than hidden.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use jsw_core::Room;
use jsw_core::map::Place;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "mansion.svg".into());

    let rooms: Vec<Room> = (0..jsw_core::ROOM_COUNT)
        .map(Room::load)
        .filter(Room::is_real)
        .collect();

    let layout = jsw_core::map::layout();
    print_grid(&rooms, &layout.places);

    let named = |n: usize| {
        rooms
            .iter()
            .find(|r| r.number == n)
            .map_or_else(|| n.to_string(), |r| r.title.clone())
    };
    if layout.oddities.is_empty() {
        println!("\nevery exit fits the grid");
    } else {
        println!("\n{} exits that do not fit the grid:", layout.oddities.len());
        for &(from, to) in &layout.oddities {
            println!("  {from} ({}) leads to {to} ({})", named(from), named(to));
        }
    }

    let svg = draw_svg(&rooms, &layout.places);
    std::fs::write(&path, svg).expect("could not write the map");
    println!("\nwrote {path}");
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
            if !jsw_core::map::leads_somewhere(room.number, dest)
                || !by_number.contains_key(&dest)
            {
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
            let _ = writeln!(
                svg,
                r#"<path class="odd" d="M {} {} Q {} {} {} {}"/>"#,
                fx + CELL_W / 2,
                fy + CELL_H / 2,
                i32::midpoint(fx, tx) + CELL_W / 2,
                i32::midpoint(fy, ty) + CELL_H / 2 - 40,
                tx + CELL_W / 2,
                ty + CELL_H / 2
            );
        }
    }

    draw_boxes(&mut svg, rooms, places, &corner);

    svg.push_str("</svg>\n");
    svg
}

/// A labelled box per room.
fn draw_boxes(
    svg: &mut String,
    rooms: &[Room],
    places: &HashMap<usize, Place>,
    corner: &impl Fn(Place) -> (i32, i32),
) {
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
            r#"<rect class="{class}" x="{}" y="{}" width="{}" height="{}" rx="4"/>
<text class="num" x="{}" y="{}">{}</text>
"#,
            x + 3,
            y + 3,
            CELL_W - 6,
            CELL_H - 6,
            x + 9,
            y + 9,
            room.number
        );

        for (line, part) in wrap(&room.title, 17).into_iter().enumerate() {
            let _ = writeln!(
                svg,
                r#"<text class="name" x="{}" y="{}">{}</text>"#,
                x + 9,
                y + 25 + line as i32 * 13,
                escape(&part)
            );
        }
    }
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
