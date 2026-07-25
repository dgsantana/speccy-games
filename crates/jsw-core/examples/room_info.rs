//! Print what a room is made of: exits, conveyor, ramp and entity slots.

use jsw_core::Room;

fn main() {
    let rooms: Vec<usize> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let rooms = if rooms.is_empty() {
        (0..jsw_core::ROOM_COUNT).collect()
    } else {
        rooms
    };

    for number in rooms {
        let room = Room::load(number);
        println!(
            "{number:2} {:32} exits l={} r={} u={} d={}",
            room.title, room.exits.left, room.exits.right, room.exits.up, room.exits.down
        );
        if let Some(start) = room.ramp.start() {
            println!(
                "      ramp   at {start:?} len {} dir {} ({})",
                room.ramp.length,
                room.ramp.direction,
                if room.ramp.direction == 0 { "up-left" } else { "up-right" }
            );
        }
        if let Some(start) = room.conveyor.start() {
            println!(
                "      convey at {start:?} len {} dir {}",
                room.conveyor.length, room.conveyor.direction
            );
        }
    }
}
