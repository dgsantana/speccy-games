//! Render rooms to binary PPM files, for eyeballing the port's output.
//!
//! ```text
//! cargo run -p jsw-core --example dump_rooms -- out/ 33 0 1
//! ```
//!
//! With no room numbers it writes every room in the mansion.

use std::io::Write;

use jsw_core::Game;
use speccy::{Frame, HEIGHT, Input, WIDTH};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".to_owned());
    std::fs::create_dir_all(&dir).expect("could not create output directory");

    let rooms: Vec<usize> = args.filter_map(|a| a.parse().ok()).collect();
    let map = std::env::args().any(|a| a == "--map");
    let jsw2 = std::env::args().any(|a| a == "--jsw2");
    let rooms = if rooms.is_empty() {
        let count = if jsw2 {
            jsw2_data::ROOM_COUNT
        } else {
            jsw_core::ROOM_COUNT
        };
        (0..count).collect()
    } else {
        rooms
    };

    let mut frame = Frame::new();
    for number in rooms {
        let mut game = if jsw2 {
            Game::new_jsw2()
        } else {
            Game::started()
        };
        if jsw2 {
            // A Jet Set Willy II game starts on its title screen; step past it
            // so a room is loaded before we ask for one.
            game.update(Input {
                start: true,
                ..Input::default()
            });
        }
        game.goto_room(number);
        game.debug.map = map;
        game.update(Input::default());
        let name = if map { "map" } else { "room" };
        write_ppm(&dir, &format!("{name}-{number:02}"), &mut frame, &game);
    }
}

fn write_ppm(dir: &str, name: &str, frame: &mut Frame, game: &Game) {
    frame.render(&game.mem, false);
    let path = format!("{dir}/{name}.ppm");
    let mut file = std::fs::File::create(&path).expect("could not write frame");
    write!(file, "P6\n{WIDTH} {HEIGHT}\n255\n").unwrap();
    for pixel in frame.pixels.chunks_exact(4) {
        file.write_all(&pixel[..3]).unwrap();
    }
    println!("wrote {path}");
}
