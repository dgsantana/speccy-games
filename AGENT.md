# Working on manic-miner-rs

A faithful Rust port of Matthew Smith's *Manic Miner* (ZX Spectrum, 1983),
rendered with macroquad. This file is the orientation guide for anyone — human
or agent — picking the project up.

## Layout

```
manic-miner-rs      bin    macroquad front end: window, texture upload, input
  crates/mm-core    lib    engine: display model, cavern, Willy, guardians, game loop
    crates/mm-data  lib    const byte tables, no dependencies
  crates/mm-audio   lib    cpal beeper synth
tools/gen_data.py          regenerates mm-data from the reference C arrays
docs/superpowers/specs/    design documents
```

The workspace root carries `[workspace.package]`, `[workspace.dependencies]`
and `[workspace.lints]`; every member inherits with `field.workspace = true`.
Add a dependency to the root table first, then reference it by
`name.workspace = true`.

`mm-core` has no window, GPU or audio dependency, so all of it is testable
headlessly. Keep it that way — anything platform-specific belongs in the binary
or in `mm-audio`.

## The display model

`mm-core::speccy::Memory` is a flat 64K array with the Spectrum's buffers as
named windows into it: the display file at 16384, the attribute file at 22528,
the playing area's working attribute and pixel buffers at 23552 and 24576, and
the empty-cavern copies of both at 24064 and 28672.

This looks unusual for Rust, and it is deliberate. The game moves sprites by
incrementing the *high* byte of an address, because that is how the Spectrum
display file is laid out; item, portal, conveyor and guardian positions all
arrive from the data tables as absolute addresses. Modelling the address space
directly is what lets the ported routines be checked line by line against the
disassembly. `next_pixel_row`, `next_cell` and `add_lsb` encode the Z80's
wrapping rules; use them rather than plain arithmetic whenever you are walking
a sprite or a tile.

`display::Frame::render` expands the display and attribute files into RGBA.

## Regenerating the data

`tools/gen_data.py` reads Michael R. Cook's C++ port and writes the `mm-data`
modules. It is not part of the build; run it only when the tables need to change:

```bash
git clone --depth 1 https://github.com/mrcook/manic-miner.git /tmp/ref
python3 tools/gen_data.py crates/mm-data/src   # expects /tmp/ref/src alongside
```

The generated files carry a "do not edit by hand" header. `INTRO_MESSAGE` is
written by the generator too, so change it there rather than in `title.rs`.

## Checking your work

```bash
cargo test --workspace
cargo clippy --workspace --all-targets   # must be clean
cargo run --release                      # play it
```

To look at what the engine draws without opening a window:

```bash
cargo run -p mm-core --example dump_frames -- /tmp/frames
```

That writes binary PPM files for the title screen and a handful of caverns.
Comparing them against screenshots of the original is the fastest way to catch a
regression in tile or attribute handling.

## Conventions

- Rust 2024, current stable. Clippy runs with `all` and `pedantic`; the
  allowances in the root manifest are deliberate and explained in place.
- Bit masks are written in decimal (240, 224, 248) to match the disassembly.
- Comments explain *why* something is odd, not what the next line does. The odd
  things here are nearly always "because the Z80 did it this way".
- Sequences that the original ran as blocking loops (dying, changing cavern,
  game over) are `Mode` variants advanced one step per frame. Do not reintroduce
  a blocking loop; it would freeze the window.

## Provenance

Game data is derived from the published Manic Miner disassembly. Manic Miner is
Copyright © 1983 Matthew Smith. The Rust code here is original work.
