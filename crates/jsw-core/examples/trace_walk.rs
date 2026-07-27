//! Print Willy's position each frame while a key is held, for comparing the
//! port's movement against the original's.

use jsw_core::Game;
use speccy::Input;

fn main() {
    let mut args = std::env::args().skip(1);
    let room: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(33);
    let frames: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(40);

    let mut game = Game::started();
    game.goto_room(room);
    // Guardians would kill him before the movement is worth watching.
    game.debug.no_guardians = true;

    // Optional starting cell, for walking into a particular feature.
    if let (Some(row), Some(column)) = (
        args.next().and_then(|a| a.parse::<u16>().ok()),
        args.next().and_then(|a| a.parse::<u16>().ok()),
    ) {
        game.willy.cell = 23552 + row * 32 + column;
        game.willy.y = (row * 16) as u8;
    }
    let leftwards = std::env::args().any(|a| a == "--left");
    let idle = std::env::args().any(|a| a == "--idle");

    let input = if idle {
        Input::default()
    } else {
        Input {
            left: leftwards,
            right: !leftwards,
            ..Input::default()
        }
    };
    for frame in 0..frames {
        game.update(input);
        let (row, column) = game.willy.position();
        println!(
            "{frame:3}  room={:2} y={:3} row={row:2} col={column:2} airborne={:3} mode={:?}",
            game.room.number,
            game.willy.y,
            game.willy.airborne,
            game.mode
        );
    }
}
