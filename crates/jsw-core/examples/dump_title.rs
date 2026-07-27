//! Render the title screen to a binary PPM file, for eyeballing the picture the
//! attributes and triangles build.
//!
//! ```text
//! cargo run -p jsw-core --example dump_title -- out/ 60
//! ```
//!
//! The second argument is how many frames to run first, which is what moves the
//! tune along and, once it is over, scrolls the message.

use std::io::Write;

use jsw_core::Game;
use speccy::{Frame, HEIGHT, Input, WIDTH};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".to_owned());
    let frames: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(0);
    std::fs::create_dir_all(&dir).expect("could not create output directory");

    let mut game = Game::new();
    for _ in 0..frames {
        game.update(Input::default());
        game.sounds.clear();
    }

    let mut frame = Frame::new();
    frame.render(&game.mem, false);
    let path = format!("{dir}/title-{frames:04}.ppm");
    let mut file = std::fs::File::create(&path).expect("could not write frame");
    write!(file, "P6\n{WIDTH} {HEIGHT}\n255\n").unwrap();
    for pixel in frame.pixels.chunks_exact(4) {
        file.write_all(&pixel[..3]).unwrap();
    }
    println!("wrote {path}");
}
