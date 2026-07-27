//! Print a room's rope frame by frame: the shape it hangs in, and where along
//! it Willy is. The fastest way to see whether a swing matches the original.
//!
//! ```text
//! cargo run -p jsw-core --features debug --example trace_rope -- 18 12
//! ```
//!
//! Needs the `debug` feature, because a guardian would kill Willy long before
//! the swing is worth watching, and dying reloads the entity buffers.

use jsw_core::entity::Kind;
use jsw_core::{Game, Room};
use speccy::layout::{COLUMNS, ROWS, SCREEN_BUF, cell_offset};

fn main() {
    let mut args = std::env::args().skip(1);
    let room: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(18);
    let frames: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(8);

    let Some(slot) = jsw_core::Entities::load(&Room::load(room))
        .kinds()
        .iter()
        .position(|&kind| kind == Kind::Rope)
    else {
        eprintln!("room {room} has no rope");
        std::process::exit(1);
    };

    let mut game = Game::new();
    game.goto_room(room);
    game.debug.no_guardians = true;
    game.debug.invulnerable = true;

    // Optional starting cell, for dropping him onto the rope.
    if let (Some(row), Some(column)) = (
        args.next().and_then(|a| a.parse::<u16>().ok()),
        args.next().and_then(|a| a.parse::<u16>().ok()),
    ) {
        game.willy.cell = 23552 + row * 32 + column;
        game.willy.y = (row * 16) as u8;
    }

    for frame in 0..frames {
        game.update(speccy::Input::default());
        game.sounds.clear();

        let buffer = game.entities.buffers[slot];
        let side = if buffer[0] & 128 == 0 { "R->L" } else { "L->R" };
        println!(
            "frame {frame:3}  swing {side}  animation {:3}  willy rope={} y={} frame={}",
            buffer[1], game.willy.rope, game.willy.y, game.willy.frame
        );
        print_screen(&game);
    }
}

/// The playing area as characters, one per pixel column pair, so a rope's line
/// is visible in a terminal.
fn print_screen(game: &Game) {
    for row in 0..ROWS {
        for pixel_row in 0..8 {
            let offset = cell_offset(row, pixel_row, 0);
            let mut any = false;
            let mut line = String::new();
            for column in 0..COLUMNS {
                let byte = game.mem.read(SCREEN_BUF + (offset + column) as u16);
                for bit in (0..8).rev() {
                    line.push(if byte & (1 << bit) == 0 { ' ' } else { '#' });
                }
                any |= byte != 0;
            }
            if any {
                println!("{row:2}.{pixel_row} |{line}|");
            }
        }
    }
    println!();
}
