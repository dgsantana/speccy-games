# manic-miner-rs

Matthew Smith's *Manic Miner* (ZX Spectrum, 1983), ported to Rust 2024 and drawn
with [macroquad](https://macroquad.rs).

All twenty caverns, the original tiles and sprites, the real attribute colours,
and a square-wave beeper playing The Blue Danube on the title screen and In the
Hall of the Mountain King underground. The engine renders into a virtual
Spectrum display file, so what you see is what the 1983 machine drew.

## Playing

```bash
cargo run --release
```

| Key | Action |
| --- | --- |
| Left / Right, or A / D | Walk |
| Space or Up | Jump |
| Enter | Start |
| P | Pause |
| M | Music on or off |
| Q or Escape | Quit |

## Debug mode

For looking at a cavern without playing up to it. It is behind two locks: a
cargo feature that decides whether the code is compiled at all, and a flag that
decides whether a build that has it does anything. A normal build has none of
it in the binary.

```bash
cargo run --features debug -- --debug
```

| Key | Action |
| --- | --- |
| F1 / F2 | Next / previous cavern |
| F3 | Guardians on or off |
| F4 | Lives on or off |
| F5 | Air drain on or off |

The keys act during play only, and nothing is drawn on the Spectrum screen —
each toggle prints a line to the terminal instead. Jumping to a cavern leaves
the score and lives alone, so it shows you the cavern rather than resuming a
game there. Eugene and Kong keep moving with the guardians switched off, since
their caverns cannot be finished otherwise; they just cannot kill you.

## How it is put together

Four crates. The engine knows nothing about windows, so it can be tested without
one.

| Crate | What it does |
| --- | --- |
| `manic-miner-rs` | macroquad front end: window, input, texture upload |
| `mm-core` | The game: display model, caverns, Willy, guardians, game loop |
| `mm-data` | Byte tables from the original, generated and committed |
| `mm-audio` | cpal square-wave beeper |

`mm-core` keeps the Spectrum's memory map because the game's own logic depends
on it — sprites move down the screen by incrementing the high byte of an
address. `AGENT.md` explains that in more detail.

To see the engine's output without a window:

```bash
cargo run -p mm-core --example dump_frames -- /tmp/frames
```

## Provenance and licence

The Rust code is original and MIT licensed. The game data — cavern layouts, tile
and sprite bitmaps, guardian definitions and the tunes — is derived from the
published [Manic Miner disassembly](https://skoolkid.github.io/manicminer/) by
way of [Michael R. Cook's C++ port](https://github.com/mrcook/manic-miner), and
remains Copyright © 1983 Matthew Smith. This is a hobby preservation project,
not a commercial release.
