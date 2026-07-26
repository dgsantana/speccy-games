# manic-miner-rs

ZX Spectrum games ported to Rust 2024 and drawn with
[macroquad](https://macroquad.rs), behind a start screen that picks between
them.

Matthew Smith's *Manic Miner* (1983) is the one that is finished: all twenty
caverns, the original tiles and sprites, the real attribute colours, and a
square-wave beeper playing The Blue Danube on the title screen and In the Hall
of the Mountain King underground. Jet Set Willy I to III and Match Point are
listed on the picker and are not written yet.

Everything renders into a virtual Spectrum display file, the start screen
included, so what you see is what the 1983 machine drew.

## Playing

```bash
cargo run --release
```

Up and Down move through the list, Enter starts a game, Escape leaves it again.
Inside a game:

| Key | Action |
| --- | --- |
| Left / Right, or A / D | Walk |
| Space or Up | Jump |
| Enter | Start |
| P | Pause |
| M | Music on or off |
| Q or Escape | Back to the picker |

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
| F1 / F2 | Next / previous level |
| F3 | Guardians on or off |
| F4 | Lives on or off |
| F5 | Air drain on or off |
| F6 | Map of the house, in Jet Set Willy |

The keys act during play only, and nothing is drawn on the Spectrum screen —
each toggle prints a line to the terminal instead. Jumping to a cavern leaves
the score and lives alone, so it shows you the cavern rather than resuming a
game there. Eugene and Kong keep moving with the guardians switched off, since
their caverns cannot be finished otherwise; they just cannot kill you.

## How it is put together

The machine is one crate and each game is another. Nothing below the front end
knows about windows, so all of it can be tested without one.

| Crate | What it does |
| --- | --- |
| `manic-miner-rs` | macroquad front end: window, input, the picker |
| `speccy` | The machine: memory map, display, palette, sound, ROM font |
| `speccy-audio` | cpal square-wave beeper |
| `mm-core` | Manic Miner: caverns, Willy, guardians, game loop |
| `mm-data` | Manic Miner byte tables, generated and committed |

A game is a `speccy::Cartridge` — five methods the front end calls each frame —
so adding one means writing its crate and adding a row to the picker's
catalogue.

`speccy` keeps the Spectrum's memory map because the games' own logic depends on
it: sprites move down the screen by incrementing the high byte of an address.
[CLAUDE.md](CLAUDE.md) explains that in more detail.

To see the engine's output without a window:

```bash
cargo run -p mm-core --example dump_frames -- /tmp/frames
```

## Provenance and licence

The Rust code is original and MIT licensed. Manic Miner's data — cavern layouts,
tile and sprite bitmaps, guardian definitions and the tunes — is derived from the
published [Manic Miner disassembly](https://skoolkid.github.io/manicminer/) by
way of [Michael R. Cook's C++ port](https://github.com/mrcook/manic-miner), and
remains Copyright © 1983 Matthew Smith. This is a hobby preservation project,
not a commercial release.
