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
    let rooms = if rooms.is_empty() {
        (0..jsw_core::ROOM_COUNT).collect()
    } else {
        rooms
    };

    let mut frame = Frame::new();
    for number in rooms {
        let mut game = Game::new();
        game.goto_room(number);
        game.update(Input::default());
        write_ppm(&dir, &format!("room-{number:02}"), &mut frame, &game);
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
