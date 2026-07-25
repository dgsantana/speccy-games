//! Render a few frames to binary PPM files, for eyeballing the port's output.
//!
//! ```text
//! cargo run -p mm-core --example dump_frames -- out/
//! ```

use std::io::Write;

use mm_core::Game;
use speccy::{Frame, HEIGHT, Input, WIDTH};

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".to_owned());
    std::fs::create_dir_all(&dir).expect("could not create output directory");

    let mut game = Game::new();
    let mut frame = Frame::new();

    write_ppm(&dir, "title", &mut frame, &game);

    game.update(Input {
        start: true,
        ..Input::default()
    });
    game.update(Input::default());
    write_ppm(&dir, "cavern-00", &mut frame, &game);

    for sheet in [1usize, 4, 7, 13, 19] {
        let mut game = Game::new();
        game.start_cavern = sheet;
        game.update(Input {
            start: true,
            ..Input::default()
        });
        for _ in 0..8 {
            game.update(Input::default());
        }
        write_ppm(&dir, &format!("cavern-{sheet:02}"), &mut frame, &game);
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
