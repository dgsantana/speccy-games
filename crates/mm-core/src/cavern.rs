//! Cavern definitions: tiles, layout, items, portal, conveyor and air supply.

use mm_data::{TileKind, caverns};

use crate::speccy::{
    ATTR_BACK, DISPLAY, Memory, PLAY_ATTRS, SCREEN_BACK, addr_of, lsb, msb, next_cell,
    next_pixel_row, rot_l, rot_r,
};

/// An 8x8 tile: the attribute byte that identifies it in the layout, and its bitmap.
#[derive(Debug, Clone, Copy, Default)]
pub struct Tile {
    /// Attribute byte. Doubles as the tile's identity in the cavern layout.
    pub attr: u8,
    pub sprite: [u8; 8],
}

impl Tile {
    fn from_data(data: &[u8; 9]) -> Self {
        let mut sprite = [0u8; 8];
        sprite.copy_from_slice(&data[1..9]);
        Self {
            attr: data[0],
            sprite,
        }
    }
}

/// A collectable key.
#[derive(Debug, Clone, Copy, Default)]
pub struct Item {
    /// Current attribute byte; 0 once collected.
    pub attr: u8,
    /// Where the item sits in the attribute buffer.
    pub attr_addr: u16,
    /// High byte of the same position in the pixel buffer.
    pub screen_msb: u8,
}

/// The exit from a cavern, which only opens once every item has been collected.
#[derive(Debug, Clone, Copy, Default)]
pub struct Portal {
    pub attr: u8,
    pub sprite: [u8; 32],
    pub attr_addr: u16,
    pub screen_addr: u16,
}

/// The conveyor belt, of which a cavern has at most one.
#[derive(Debug, Clone, Copy, Default)]
pub struct Conveyor {
    pub tile: Tile,
    /// 0 drags Willy left, 1 drags him right.
    pub direction: u8,
    /// Leftmost cell in the pixel buffer holding the empty cavern.
    pub location: u16,
    pub length: u8,
}

/// Everything about the cavern currently being played.
#[derive(Debug)]
pub struct Cavern {
    pub sheet: usize,
    pub name: &'static str,
    /// Counts down by four every frame and drives guardian and animation timing.
    pub clock: u8,
    /// Air remaining, from 63 down to 36. It doubles as the low byte of the
    /// display address of the right-hand end of the air bar.
    pub air: u8,
    pub border: u8,
    tiles: [Tile; 8],
    pub conveyor: Conveyor,
    pub items: [Item; 5],
    pub portal: Portal,
    pub layout: [u8; PLAY_ATTRS],
}

impl Default for Cavern {
    fn default() -> Self {
        Self {
            sheet: 0,
            name: "",
            clock: 0,
            air: 63,
            border: 0,
            tiles: [Tile::default(); 8],
            conveyor: Conveyor::default(),
            items: [Item::default(); 5],
            portal: Portal::default(),
            layout: [0; PLAY_ATTRS],
        }
    }
}

impl Cavern {
    /// Read cavern `sheet` out of the data tables.
    pub fn load(sheet: usize) -> Self {
        let mut tiles = [Tile::default(); 8];
        for (slot, kind) in TileKind::ALL.iter().enumerate() {
            tiles[slot] = Tile::from_data(kind.data(sheet));
        }

        let mut items = [Item::default(); 5];
        for (slot, item) in items.iter_mut().enumerate() {
            let raw = caverns::ITEMS[sheet][slot];
            *item = Item {
                attr: raw[0] as u8,
                attr_addr: raw[1],
                screen_msb: raw[2] as u8,
            };
        }

        let mut portal_sprite = [0u8; 32];
        portal_sprite.copy_from_slice(&caverns::PORTAL_GRAPHICS[sheet]);

        let conveyor_params = caverns::CONVEYOR_PARAMS[sheet];

        Self {
            sheet,
            name: caverns::CAVERN_NAMES[sheet],
            clock: caverns::CLOCK_VALUES[sheet],
            air: caverns::AIR_SUPPLIES[sheet],
            border: caverns::BORDER_COLOURS[sheet],
            conveyor: Conveyor {
                tile: tiles[TileKind::Conveyor as usize],
                direction: conveyor_params[0] as u8,
                location: conveyor_params[1],
                length: conveyor_params[2] as u8,
            },
            tiles,
            items,
            portal: Portal {
                attr: caverns::PORTAL_ATTRS[sheet],
                sprite: portal_sprite,
                attr_addr: caverns::PORTAL_ATTR_LOCATIONS[sheet],
                screen_addr: caverns::PORTAL_SCREEN_LOCATIONS[sheet],
            },
            layout: caverns::LAYOUTS[sheet],
        }
    }

    /// The tile of a given kind in this cavern.
    #[inline]
    pub fn tile(&self, kind: TileKind) -> Tile {
        self.tiles[kind as usize]
    }

    /// The attribute byte identifying a tile kind in the layout.
    #[inline]
    pub fn tile_attr(&self, kind: TileKind) -> u8 {
        self.tiles[kind as usize].attr
    }

    /// Which tile kind, if any, an attribute byte from the layout refers to.
    pub fn kind_of(&self, attr: u8) -> Option<TileKind> {
        TileKind::ALL
            .iter()
            .copied()
            .find(|&kind| self.tiles[kind as usize].attr == attr)
    }

    /// Paint the empty cavern into the background buffers.
    pub fn draw_empty(&self, mem: &mut Memory) {
        mem.load(ATTR_BACK, &self.layout);

        let mut addr = SCREEN_BACK;
        for (cell, &attr) in self.layout.iter().enumerate() {
            // The layout covers two thirds of the screen; the second 256 cells
            // live 2048 bytes further into the buffer.
            let block = if cell > 255 { 2048 } else { 0 };
            if let Some(kind) = self.kind_of(attr) {
                let sprite = self.tiles[kind as usize].sprite;
                let mut row = addr;
                for &byte in &sprite {
                    mem.write(row + block, byte);
                    row = next_pixel_row(row);
                }
            }
            addr = next_cell(addr);
        }

        // The Final Barrier hides the title screen graphic behind its tiles.
        if self.sheet == mm_data::FINAL_BARRIER {
            mem.load(SCREEN_BACK, &mm_data::title::TITLE_SCREEN_PIXELS);
        }
    }

    /// Draw the full air bar, done once when the cavern is loaded.
    pub fn draw_air_bar(&self, mem: &mut Memory) {
        for high in 82..86u8 {
            let addr = addr_of(high, 36);
            for i in 0..u16::from(self.air - 36) {
                mem.write(addr + i, 255);
            }
        }
    }

    /// Advance the clock and burn a frame's worth of air.
    ///
    /// Returns true when the supply has run out.
    pub fn decrease_air(&mut self, mem: &mut Memory) -> bool {
        self.clock = self.clock.wrapping_sub(4);

        if self.clock == 252 {
            if self.air == 36 {
                return true;
            }
            self.air -= 1;
        }

        // The top three bits of the clock say how much of the rightmost cell of
        // the bar is still filled, drawn from the left edge of the cell.
        let filled = rot_l(self.clock & 224, 3);
        let mut pixels = 0u8;
        for _ in 0..filled {
            pixels = rot_r(pixels, 1) | 0x80;
        }

        for high in 82..86u8 {
            mem.write(addr_of(high, self.air), pixels);
        }
        false
    }

    /// Scroll the conveyor's first and third pixel rows so the belt appears to move.
    pub fn move_conveyor(&self, mem: &mut Memory) {
        if self.conveyor.length == 0 {
            return;
        }

        let top = self.conveyor.location;
        let third = addr_of(msb(top).wrapping_add(2), lsb(top));

        let (top_pixels, third_pixels) = if self.conveyor.direction == 0 {
            (rot_l(mem.read(top), 2), rot_r(mem.read(third), 2))
        } else {
            (rot_r(mem.read(top), 2), rot_l(mem.read(third), 2))
        };

        let (mut top, mut third) = (top, third);
        for _ in 0..self.conveyor.length {
            mem.write(top, top_pixels);
            mem.write(third, third_pixels);
            top = next_cell(top);
            third = next_cell(third);
        }
    }

    /// Print the cavern name, the air label and the air bar into the status area.
    pub fn draw_status(&self, mem: &mut Memory) {
        mem.fill(DISPLAY + 4096, 2048, 0);
        mem.print_str(self.name, 20480);
        mem.print_str("AIR", 20512);
        self.draw_air_bar(mem);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cavern_loads_with_a_distinct_set_of_tiles() {
        for sheet in 0..mm_data::CAVERN_COUNT {
            let cavern = Cavern::load(sheet);
            assert_eq!(cavern.name.len(), 32, "cavern {sheet} name is not 32 wide");
            assert!(cavern.air >= 36 && cavern.air <= 63, "cavern {sheet} air");
            // Background must not be confusable with anything Willy can stand on.
            let background = cavern.tile_attr(TileKind::Background);
            for kind in [TileKind::Floor, TileKind::Wall, TileKind::Crumbling] {
                assert_ne!(
                    cavern.tile_attr(kind),
                    background,
                    "cavern {sheet}: {kind:?} shares the background attribute"
                );
            }
        }
    }

    #[test]
    fn central_cavern_has_five_items_and_a_portal() {
        let cavern = Cavern::load(0);
        assert_eq!(cavern.name.trim(), "Central Cavern");
        assert!(cavern.items.iter().all(|item| item.attr != 0));
        assert_ne!(cavern.portal.attr_addr, 0);
    }

    #[test]
    fn air_runs_out_after_the_expected_number_of_frames() {
        let mut mem = Memory::new();
        let mut cavern = Cavern::load(0);
        let mut frames = 0;
        while !cavern.decrease_air(&mut mem) {
            frames += 1;
            assert!(frames < 100_000, "air never ran out");
        }
        assert_eq!(cavern.air, 36, "the bar did not empty");
        // Each unit of air is 64 frames and cavern 0 starts with 63 - 36 of them,
        // plus a part-unit because the clock does not start at zero.
        let units = (63 - 36) * 64;
        assert!(
            (units..units + 64).contains(&frames),
            "air lasted {frames} frames"
        );
    }
}
