# Fuzzing

No parser in `s2-formats` may panic on malformed input or allocate more than the input
justifies. The libFuzzer targets in `fuzz/` check both. `fuzz/` is a Cargo
workspace of its own, so the main workspace never builds libFuzzer or needs nightly.

## Setup

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
```

All commands below run from `fuzz/`.

## Seeding

The seeds come from your own copy of the game, so `fuzz/corpus/` and `fuzz/artifacts/` are
gitignored and must never be committed. Point any of `S2_DATA_PLAIN`, `S2_DATA_GOLD` and
`S2_DATA_CD` at a data set (anything `s2_data::DataSet::discover` accepts) and run:

```sh
S2_DATA_PLAIN=... S2_DATA_GOLD=... cargo run --release -p s2-fuzz-seed
```

The tool writes `corpus/<target>/`, one file per seed named by content hash, and prints the count
per target. Pass a directory as the only argument to write elsewhere. Files too large to mutate
quickly are cut down while staying well formed:

- LST archives are also split into one-slot archives and single tagged items, and IDX/DAT pairs
  into one-entry pairs; at most 64 of each per file.
- Maps are cropped to 32×32 nodes, `.WLD` in both the tagged and the compact (RLE) layout. Each
  cropped layer, RLE-packed, seeds `map_rle`.
- Save slots use the cropped world, with the transport header and the computer players' grid
  resized to match.
- Smacker videos keep short runs of frames from eight points. The shipped videos carry about
  280 KB of Huffman trees, so a 143-byte synthetic video built in code is added too.

The tool reports any cut-down seed its parser no longer decodes. The fix for that belongs in the
cutting code, not the parser.

## Targets

Two-file and seven-file inputs are split by `s2_fuzz::Parts`: each part is a little-endian `u32`
length then that many bytes. After every successful parse, the target walks the result
through its accessors, `Debug` and error formatting, and every render path.

| Target | Parsers | Also exercised | `-max_len` | Cap (MB) |
|---|---|---|---|---|
| `lst` | `lst::Archive::parse`, `Item::parse_tagged`, `Font::parse_file` | every item's `to_rgba`/`to_indexed`, font glyphs | 131072 | 144 |
| `idx` | `idx::Index::parse` (parts: IDX, DAT) | entry names, every item rendered | 131072 | 144 |
| `bob` | `bob::Bob::parse`, `bob::RomAnim::parse` | bodies, overlays, every `composite`, animation records | 131072 | 144 |
| `iff` | `iff::parse`, `iff::read_palette`, `gouraud::ShadingTable::parse` | picture `to_rgba` | 262144 | 80 |
| `map_wld` | `map::Map::parse_wld` | header, every layer, areas, animals | 65536 | 64 |
| `map_v102` | `map::Map::parse_v102` (parts: world, area table) | as `map_wld` | 65536 | 64 |
| `map_rle` | `map::rle::decode` (input: `u24` output length, then the stream) | output length | 65536 | 64 |
| `text` | `text::StringTable::parse`, `Page::parse`, `cp437::decode` | every slot, lines | 65536 | 64 |
| `rtx` | `rtx::Script::parse` | sections and commands | 65536 | 64 |
| `music` | `music::Xmidi::parse`, `music::TimbreBank::parse` | `Sequence::to_smf` for each sequence | 131072 | 64 |
| `smacker` | `smacker::Video::parse` | every frame and audio chunk decoded, first picture rendered | 524288 | 64 |
| `save_slot` | `save::Slot::parse` (see below) | world walk, missing parts | 262144 | 64 |
| `save_meta` | `Directory`, `MissionProgress`, `WorldCampaignProgress`, `ScreenMode`, `Transport`, `Control`, `Desktops`, `Mailboxes`, `ComputerPlayers` | entry names, accessors | 65536 | 64 |

`save_slot` input starts with a presence byte. Bit `n` set means optional part `n` follows, in
the order transport, control, desktop, mails, computer. The world, the area table and each present
part come next, as `Parts`.

## Running

Build with debug assertions (`-a`) so integer overflow counts as a finding, cap single
allocations through ASan, and treat slow inputs as timeouts:

```sh
ASAN_OPTIONS=max_allocation_size_mb=64 cargo +nightly fuzz run -a smacker -- \
    -fork=8 -ignore_crashes=1 -ignore_ooms=1 -ignore_timeouts=1 \
    -max_total_time=600 -rss_limit_mb=2048 -timeout=10 -max_len=524288
```

Use each target's `-max_len` and cap from the table, passing the cap as `max_allocation_size_mb`.
libFuzzer's own `-malloc_limit_mb` does not fire with these builds, so the cap goes through
`ASAN_OPTIONS`. Each cap is the largest single allocation the format's documented
expansion allows at that `-max_len`, rounded up. LST bitmaps may hold 256 pixels per input byte
(`lst::MAX_EXPANSION`), which is 1 KB per byte once rendered as RGBA. IFF bodies may hold 64
pixels per byte.
An allocation past the cap means a parser broke its own limit. Findings land in
`artifacts/<target>/`. To reproduce and shrink one:

```sh
cargo +nightly fuzz run -a <target> artifacts/<target>/<file>
cargo +nightly fuzz tmin -a <target> artifacts/<target>/<file>
```

A regression test for a fix goes in `s2-formats` with its input built in code. Never use the
artefact itself as the input, since it may be derived from game data.

## Suggested smoke run

No CI job runs these yet. A short pass that needs no game data uses the synthetic seeds and
whatever the fuzzer finds from scratch:

```sh
cargo run --release -p s2-fuzz-seed corpus || true
for target in $(cargo +nightly fuzz list); do
    ASAN_OPTIONS=max_allocation_size_mb=144 cargo +nightly fuzz run -a "$target" -- \
        -max_total_time=60 -max_len=65536 -rss_limit_mb=2048 -timeout=10 || exit 1
done
```

The seed tool exits non-zero without game data but still writes the synthetic seeds. The
`|| true` lets the loop run anyway.
