//! Print a room's entity buffers, for checking guardians against the original.

use jsw_core::{Entities, Room};

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
        let entities = Entities::load(&room);
        println!("room {number} {}", room.title);
        for (slot, spec) in room.entities.iter().enumerate() {
            if spec.definition == 255 {
                println!("  {slot}: terminator");
                break;
            }
            let b = entities.buffers[slot];
            println!(
                "  {slot}: def={:3} x={:3} buffer={b:?} kind={:?} page={} (0x{:02X})",
                spec.definition,
                spec.x,
                jsw_core::entity::Kind::of_public(b[0]),
                b[5],
                b[5]
            );
        }
    }
}
