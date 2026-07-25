# Working on manic-miner-rs

Faithful Rust ports of ZX Spectrum games, rendered with macroquad, behind a
start screen that picks between them. Matthew Smith's *Manic Miner* (1983) is
the one that is written; Jet Set Willy I to III and Match Point are listed on
the picker and are not. This file is the orientation guide for anyone — human or
agent — picking the project up.

## Layout

```
manic-miner-rs        bin    window, input, the picker, the app state machine
  crates/speccy       lib    the machine: memory, display, palette, sound, font, Cartridge
  crates/speccy-audio lib    cpal beeper synth
  crates/mm-core      lib    Manic Miner: caverns, Willy, guardians, game loop
    crates/mm-data    lib    Manic Miner byte tables, no dependencies
tools/gen_data.py            regenerates mm-data from the reference C arrays
```

The workspace root carries `[workspace.package]`, `[workspace.dependencies]`
and `[workspace.lints]`; every member inherits with `field.workspace = true`.
Add a dependency to the root table first, then reference it by
`name.workspace = true`.

Neither `speccy` nor a game crate may depend on a window, GPU or audio device,
so all of them are testable headlessly. Keep it that way — anything
platform-specific belongs in the binary or in `speccy-audio`.

## Adding a game

A game is a `speccy::Cartridge`: `update`, `memory`, `sounds`, `border`,
`finished`. Write the crate, implement the trait, then fill in a `launch` in
`CATALOGUE` in `src/menu.rs`. An entry whose `launch` is `None` is drawn dim and
cannot be chosen, which is how a game appears before it is written.

Anything the machine does belongs in `speccy`; anything one game happens to do
belongs in that game. Manic Miner's working buffers are the example — the
addresses are Matthew Smith's choice, so they live in `mm-core::layout`, not in
`speccy::memory` beside the real display file.

## The display model

`speccy::Memory` is a flat 64K array with the Spectrum's buffers as named
windows into it: the display file at 16384 and the attribute file at 22528. A
game adds its own — Manic Miner's playing-area attribute and pixel buffers at
23552 and 24576, and the empty-cavern copies of both at 24064 and 28672, are in
`mm-core::layout`.

This looks unusual for Rust, and it is deliberate. The game moves sprites by
incrementing the *high* byte of an address, because that is how the Spectrum
display file is laid out; item, portal, conveyor and guardian positions all
arrive from the data tables as absolute addresses. Modelling the address space
directly is what lets the ported routines be checked line by line against the
disassembly. `next_pixel_row`, `next_cell` and `add_lsb` encode the Z80's
wrapping rules; use them rather than plain arithmetic whenever you are walking
a sprite or a tile.

`speccy::Frame::render` expands the display and attribute files into RGBA.

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

The `debug` feature adds code and tests of its own, and both shapes have to
pass:

```bash
cargo test --workspace --features debug
cargo clippy --workspace --all-targets --features debug
cargo run --features debug -- --debug    # F1-F5, see README
```

The switches themselves are `speccy::Debug`, reached through the
`speccy::DebugSwitches` trait so the keys mean the same thing in every game.
Inside Manic Miner they are read through the `invulnerable`, `air_drains`,
`guardians_live` and `sync_debug` helpers in `game.rs`, which fold to constants
when the feature is off. Put the `cfg` there rather than in the game loop.

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
- Sequences that an original ran as blocking loops (dying, changing cavern,
  game over) are `Mode` variants advanced one step per frame. Do not reintroduce
  a blocking loop; it would freeze the window.
- Escape and Q mean "put this game away and go back to the picker". Only the
  picker leaves the program.

## Provenance

Game data is derived from the published Manic Miner disassembly. Manic Miner is
Copyright © 1983 Matthew Smith. The Rust code here is original work, and each
game added keeps its own data and its own copyright note the same way.
