# How the screen works

Notes on the ZX Spectrum display model this port reproduces, and where each
piece of it lives in the code. Written for whoever next has to work out why a
sprite is drawn eight pixels off.

## The two files

The Spectrum's screen is two separate regions of memory.

The **display file** holds 6144 bytes of monochrome pixels at 16384. One byte is
eight horizontal pixels, bit 7 leftmost.

The **attribute file** holds 768 bytes at 22528, one per 8x8 cell, 32 columns by
24 rows. A byte packs four fields:

| Bits | Field |
| --- | --- |
| 7 | FLASH: swap ink and paper about three times a second |
| 6 | BRIGHT: use the upper half of the palette |
| 5-3 | PAPER: colour of an unset pixel |
| 2-0 | INK: colour of a set pixel |

Colour only exists per cell, which is why sprites passing over a background of a
different colour take on its paper — the famous attribute clash, faithfully
reproduced here.

`mm_core::display::Frame::render` walks both files and produces RGBA.

## Why the display file is not in row order

A row's address is not `16384 + y * 32`. The file is ordered by third of the
screen, then by pixel row within a character, then by character row:

```
offset = ((y & 0xc0) << 5) | ((y & 0x07) << 8) | ((y & 0x38) << 2)
```

The practical consequence is that **moving down one pixel row means adding 256**,
which on a Z80 is a single `INC H`. Eight of those and you have crossed into the
next third rather than the next character row, so sprite-drawing code has to
notice and correct. `mm_core::speccy::next_pixel_row` does the increment and
`Memory::draw_16x16` does the correction.

## The working buffers

The game does not draw straight to the screen. It keeps four buffers:

| Address | Size | Contents |
| --- | --- | --- |
| 23552 | 512 | Attributes of the playing area being built this frame |
| 24064 | 512 | Attributes of the empty cavern |
| 24576 | 4096 | Pixels of the playing area being built this frame |
| 28672 | 4096 | Pixels of the empty cavern |

Each frame the empty-cavern pair is copied over the working pair, everything
that moves is drawn on top, and the result is copied to the display and
attribute files. The playing area is the top sixteen character rows; the bottom
eight hold the cavern name, the air bar, the scores and the remaining lives, and
are drawn once when the cavern loads.

The empty-cavern pixel buffer is also where the game *stores state*: crumbling
floors wear away in it, conveyor belts scroll in it, and the Kong Beast caverns
erode a wall in it. That is why those routines write to 28672 rather than to the
frame being built.

## Tiles

A cavern has eight tile kinds: background, floor, crumbling floor, wall,
conveyor, two nasties, and an "extra" that does different jobs in different
caverns. Each is nine bytes — an attribute byte then eight rows of bitmap.

The attribute byte doubles as the tile's identity. A cavern's 512-byte layout is
a grid of attribute bytes, and the engine works out which tile a cell holds by
matching that byte against the eight tiles. This is also how collision works:
Willy is standing on a floor because the cell below him carries the floor tile's
attribute byte. `Cavern::kind_of` is the lookup.

Two consequences worth knowing. Tile identity is per cavern, so the same byte
means different things in different caverns. And an attribute byte the engine
does not recognise draws nothing at all, leaving whatever was in the buffer —
which is exactly how The Final Barrier shows the title screen's sky behind its
tiles.

## Positions in the data tables

Item, portal, conveyor and guardian positions arrive from the data tables as
absolute addresses into one of the buffers above, not as coordinates. A guardian
knows its attribute-buffer address and the *high byte* of its pixel-buffer
address; combining that high byte with the attribute address's low byte gives
the pixel position. `mm_core::guardian::sprite_address` and
`attribute_address` do this conversion.
