# Contributing to Robigo

Robigo is a modern engine for The Settlers II (Blue Byte, 1996), written in Rust. It plays the base game
after its official patch from the player's own copy, on Linux and Windows, with macOS support written but
untested. See [docs/building.md](docs/building.md) for how to build it.

## Rules

- **No Blue Byte data in the repository.** Not in tests, fixtures or docs. Tests use synthetic data, or
  read real data only when an environment variable points at a copy, and assert hashes, sizes or magic
  numbers. The one exception is the handful of screenshots of Robigo running in `docs/shots/`, which the
  README shows.
- **The output matches the original.** What the player sees and hears (pixels, sounds, timings) and the
  sim state match S2 exactly. How Robigo gets there is up to you: rework, cache, parallelise or move work to
  the GPU, and keep only the quirks whose side-effects change the output.
- **The sim is deterministic.** In `s2-sim`: no floats, no `HashMap` iteration, no wall-clock time, one
  sim-owned RNG, and commands as the only input. Its `clippy.toml` rejects floats, hash collections and
  clocks.
- **No decompiler output.** Studying S2.EXE with a disassembler or decompiler is fine; what they print
  never goes in the repository. Write the behaviour you take from S2.EXE in your own words, in the doc
  comment of the code that reproduces it, with the S2.EXE address so anyone with the executable can check
  it.
- **Docs stand on their own.** Nothing committed points at paths on your machine or at local dumps. Say
  what was observed in the game data or S2.EXE and name the file, so any reader with their own copy can
  check it.
- **Call the original "Settlers 2" or "S2".** S2 means v1.51, the version Robigo replicates, and the Gold
  Edition is the same game plus extra mission data, so docs and comments say what S2 does. Name a version
  only where versions differ: the official patch, or file layouts older releases wrote.

## Layout

A Cargo workspace. The root package `robigo` is the game binary; the libraries live in `crates/`:

- `s2-data`: finds and reads the player's data (an installed game, a CD image, the Gold Edition's mission
  data, patch layers).
- `s2-formats`: parsers for every original file format. They take `&[u8]`, never panic, and are fuzzed
  (`fuzz/`, [docs/tools/fuzzing.md](docs/tools/fuzzing.md)).
- `s2-platform`: a thin safe wrapper over SDL3 (`sdl3-sys`) and window handles for wgpu.
- `s2-render`: the indexed-colour renderer. Terrain is drawn on the CPU to match S2.EXE pixel for pixel
  and cached in tiles; wgpu does the palette resolve and scaling.
- `s2-assets`: the palette as the DAC shows it, texture sheets, shading tables, palette cycling and object
  atlases.
- `s2-systems`: the parts of the game world other crates need without the sim: the node grid, players,
  typed ids for buildings, wares and jobs, and the data tables a mod replaces. `s2-sim` re-exports it.
- `s2-sim`: the deterministic game world: the node grid, objects, buildings, flags, figures, territory and
  the game's RNG. The renderer reads it and never changes it.
- `s2-ui`: the painter, the skin, the widget core and the game's windows, over `s2-render`, `s2-systems`
  and a read-only view of `s2-sim`.
- `s2-audio`: the sound effects' request path, its slots and its voices. No device code: a backend takes a
  playing slot and pushes bytes.
- `s2-mods`: the base mod baked from the player's data, and the mod stack over it: files replaced by path,
  TOML tables merged by key ([docs/design/mods.md](docs/design/mods.md)).

## Tests against the real game

Most tests use synthetic data. The rest run only when these point at your own copy, and skip otherwise:

| Variable | Points at |
| --- | --- |
| `S2_DATA_PLAIN` | an installed game with the official patch |
| `S2_DATA_GOLD` | a Gold Edition install |
| `S2_DATA_CUE`, `S2_DATA_ISO` | the original CD image |
| `S2_DATA_CD` | the CD's files copied to a folder |
| `S2_DATA_PATCH` | the official patch's files |
| `S2_CAPTURES` | screenshots of S2 taken under DOSBox, for `s2-render`'s pixel tests |
| `S2_OPL_CAPTURES` | OPL register writes recorded from S2 under DOSBox-X |
| `S2_AI_TRACE` | a trace of the computer players read from S2 running under DOSBox-X |
| `S2_SOUND_FONT` | a SoundFont for the music rendering test |

## Code and comments

- Public functions, methods and types get a doc block saying what a caller needs to know, and it stops
  there.
- Comments go only where the code's structure doesn't already say it: bit fields, S2.EXE quirks. Say what
  happens and why. Leave out comments that restate the code or record what changed.
- No hex except for the disassembly: S2.EXE addresses, DOSBox memory addresses and offsets into S2.EXE.
  Ids, sizes, counts, offsets, coordinates, timings, palette indices and table values are decimal, or
  better, a named constant or enum. A value an external spec writes in hex (MIDI, OPL, LE/MZ/ELF, Smacker,
  IFF, SDL) and a hash or checksum digest may be hex inside a named constant. Masks and sentinels get names
  (`u8::MAX`, bitflags, `% 64`), never a bare `0x3f` or `0xff`.
- The workspace enables `clippy::pedantic` and CI denies warnings. Fix the lint; a scoped `#[allow]` needs
  a one-line reason.
- Keep non-platform crates free of Unix-only APIs, since CI builds and tests Windows too.

## Commits and pull requests

- Conventional commits (`feat(sim): ...`, `fix(render): ...`, `docs: ...`), brief, one concern each.
- End the message with `Closes #N` for each issue the change finishes, or `Refs #N` for partial work.
- A wording-only pass over docs goes in its own commit.

Before opening a pull request, run:

```sh
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
