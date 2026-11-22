# Maps: `.WLD` and `.SWD`

Parser: `s2_formats::map` (`Map::parse_wld`, `Map::parse_v102`). Integers are little-endian.
Addresses are `S2.EXE`'s, as [s2-exe-layout.md](../re/s2-exe-layout.md) describes.
`Map::parse_v102` reads the older `WORLDnnn.DAT` files described in
[differences.md](../re/differences.md#older-map-files).

## Where the files are

| Folder | Files | Edition |
| --- | --- | --- |
| `DATA/MAPS` | `MISS200`–`MISS209.WLD`, campaign | base game (the Gold Edition changes `MISS203`) |
| `DATA/MAPS2` | nine World Campaign maps | Gold Edition mission data |
| `DATA/MAPS3` | `OMAP00`–`OMAP17.WLD`, unlimited play | base game |
| `DATA/MAPS4` | `MAP01`–`MAP12.WLD` | Gold Edition mission data |
| `DATA/MAPS` | `WORLDnnn.DAT` + `CONTInnn.DAT`, 31 pairs in the older format | base game install and original CD; not the Gold Edition |

`.SWD` (maps saved from the game) uses the `.WLD` layout.

## `.WLD` layout

| Offset | Size | Field |
| --- | --- | --- |
| `0x000` | 10 | `WORLD_V1.0` |
| `0x00a` | 20 | title, NUL-terminated CP437 (see the overflow note) |
| `0x01e` | 2 | width (unreliable) |
| `0x020` | 2 | height (unreliable) |
| `0x022` | 1 | terrain set: 0 greenland, 1 wasteland, 2 winter; others kept as `Terrain::Other` |
| `0x023` | 1 | player count |
| `0x024` | 20 | author, NUL-terminated; bytes after the NUL can be stale |
| `0x038` | 7 × 2 | headquarters x per player, `0xffff` for none |
| `0x046` | 7 × 2 | headquarters y per player |
| `0x054` | 1 | unknown, zero in every shipped map (see [Quirks](#quirks)) |
| `0x055` | 7 × 1 | each player's face, read in one game mode only |
| `0x05c` | 250 × 9 | area table |
| `0x926` | varies | layer container (two bytes later in a file with the extra-word fault) |
| after | varies | animal list |

The header reader at `0x15612` reads 10 bytes, compares them with the signature, then reads
`0x52` bytes (offsets `0x0a`–`0x5b`) into a block at `0x200ca0`. It copies the per-player byte at
`0x55` into the slot records' face byte only when the game mode at `0x1f78ea` is 2. In other
modes it uses the slot number ([players.md](../re/players.md#loading-a-maps-header-confirmed)).
Shipped values are 0 for slot 0 and small distinct numbers for the computer players. The African
map's opponents are 9, 10 and 11, the Africans' three faces, and the north Asian map's are 6, 7
and 8, the Japanese three, which fits the World Campaign's fixed opponents.

### The title overflow

The editor writes the title as an unbounded string. A title of 20 characters or more runs its
characters (and its NUL) into the width and height fields. The map-list panel (`0x5214c`)
copies the title with an unbounded string copy too, then shows the clobbered width and height.
Seven campaign maps are affected, with or without the Gold Edition's data (`MISS201`, `203`,
`204`, `205`, `207`, `208`, `209`).

The layer container holds the reliable size. Its own header and every layer header carry width
and height, and they agree in every shipped file. The parser takes the size from there and
rejects a layer that disagrees. It reads the title as follows:

- If the header's width and height match the layers, the title is cut at 20 bytes. One
  unlimited-play map has a 20-character title with no NUL, followed by a correct width.
- Otherwise the title may continue through the four width/height bytes, up to the first NUL.
- `Header::dimensions_overwritten` reports which case applied.

One campaign map has a short title but a wrong width. That byte is left over from an earlier,
longer title.

### Area table

250 entries of 9 bytes: kind (`u8`), seed x (`u16`), seed y (`u16`), node count (`u32`).

| Kind | Meaning | Confidence |
| --- | --- | --- |
| 0 | unused; other fields may hold stale values | high |
| 1 | connected land region | high |
| 2 | connected body of water; every seed sits on terrain 5 | high |

In all 77 shipped maps, every used entry's seed lies inside that entry's area, and its node
count equals the number of nodes the area layer assigns to it. `CONTInnn.DAT` is this table on
its own, 2,250 bytes read and written raw (`0x8ca` bytes at `0x200d40`, see `0x14e24`).

### Animal list

After the layers comes a list of 5-byte records (species `u8`, x `u16`, y `u16`), ended by a
species byte of `0xff`. Every shipped map has the terminator. Maps without animals hold the
terminator alone. The parser also accepts input that ends without one. Species run 1 to 6.
Every record inside the map sits on a node whose animal layer holds the same species. 72 of
2,158 records lie outside the map, and the parser keeps them as they are.

## The layer container

S2.EXE serialises objects with a 2-byte class tag that it looks up in a registry (`0x802f0`
registers, `0x8031c` reads, `0x804e4` writes). Maps use three classes:

| Tag | Object |
| --- | --- |
| `0x2711` | map: origin, size and 15 layer pointers (`0x48` bytes, vtable `0x1f0a98`) |
| `0x2710` | layer: origin, size and a byte buffer (vtable `0x1f06dc`) |
| `0x0001` | byte buffer: `u32` length then the bytes |

The map object's loader (`0x91f85`) reads a `u32` first. When it is `0xffffffff` the container is
**compact**; otherwise that `u32` was the origin and the container is **tagged**.

Tagged (every shipped `.WLD`):

```text
u16 0x2711, i16 x0, i16 y0, u16 width, u16 height
repeat per stored layer:
    u16 0x2710, i16 x0, i16 y0, u16 width, u16 height, u16 0x0001, u32 len, len bytes
```

Compact (what the save code writes for savegames):

```text
u16 0x2711, u32 0xffffffff, i16 x0, i16 y0, u16 width, u16 height
repeat per stored layer:
    u32 len, len bytes   (raw when len == width * height, otherwise RLE)
```

The origin is always (0, 0). The writer (`0x921cb`) chooses compact when the flag at `0x2184a2` is
set. The save routine at `0x14923` sets it for savegames and clears it when exporting a map to
`DATA\MAPS\WORLDnnn.DAT`.

### Which layers are stored

The map object holds 15 layer slots (`[map + 0x0c + 4 * slot]`). The game stores and loads every
slot except 13, so `.WLD` files hold 14 layers in slot order 0–12, 14. Maps load from
`MISSnnn.WLD`, `OMAPnn.WLD` and `MAPS4`. The only `DATA\MAPS\` world path is in the save routine,
and the loader at `0x14b4f` reads `SAVE\WORLDnnn.DAT` savegames.

### RLE

Compressor `0x7de0e`, decompressor `0x7df3b` (reached through `0x23348` and `0x23368`).

- The first byte is the escape. The encoder picks the byte value that occurs least often in the
  layer (the lowest value on a tie).
- `escape, n, v` with `n >= 4` expands to `n` copies of `v`. Runs are at most 255 long.
- `escape, n` with `n < 4` is one literal escape byte. The encoder always writes `escape, 0`, once
  per escape byte in the input.
- Every other byte is a literal.

The game's decoder trusts the stream and never checks the output length. The Robigo decoder stops
with an error on overrun, underrun or a sequence cut short. Savegames use RLE for every layer and
exercise the decoder.

## Quirks

The parsers accept what S2.EXE accepts. Where the game's handling of an oddity is not yet known, the
parser accepts the input and reports the oddity. Each top-level type has a `quirks()` method listing
what was unusual and where. The files the base game and the Gold Edition ship produce none, and the
real-data tests assert that. Input is still an error when its bytes cannot be decoded at all, or
when the game itself treats it as fatal. It is also an error when accepting it would let a small
file allocate without bound.

`Map::quirks` reports, in file order:

| Quirk | What the parser does | Why |
|---|---|---|
| `Terrain` | a terrain byte above 2 is kept as `Terrain::Other(byte)` | the map still decodes; which graphics it means is the game's business |
| `Unknown54` | a non-zero byte at `0x54` is kept (`Header::unknown_54`) | other map tools treat a set byte as marking the map unusable and note that some editors leave it set; S2.EXE is not yet checked, so it keeps a neutral name |
| `ExtraWord` | two zero bytes just before the container tag (tag at `0x928`) are skipped | see below |
| `ShortLayer` | the first layer of such a map is completed as described below | see below |
| `AbsentLayer` | a layer whose buffer tag is 0 (the serialiser's null object) followed by a `u32` length of 0 is left out of `Map::layers`; its header's width and height are not checked | open-source map editors write unused layers this way |
| `Trailer` | bytes after the animal list are kept in `Map::trailer` | |

A layer with buffer tag 0 and a non-zero length is still an error, as are layers whose size
disagrees with the container.

### The extra-word fault

One map in the German release ("Das Dreieck", `OMAP08`) is reported by other map tools to carry two
zero bytes before the `0x2711` tag, and a first layer whose next layer header starts two bytes
early. No copy of that file was available to check. The parser recognises exactly this layout. Two
zero bytes come before the container tag, and the `0x2710` tag after the first layer sits two bytes
before the declared end of that layer's data, not at it. Only then does it read the layer two bytes
short and the next header two bytes early. The file holds exactly as many bytes as a correct one, so
the two displaced bytes from before the tag are taken as the layer's last two values (both zero in
the reported file). Another loader repeats the third-last value there instead. What the German
S2.EXE does with the file is an open question. When the two zero bytes are present but the first
layer is complete, only `ExtraWord` is reported.

## Layers

Names are provisional unless the confidence column says otherwise. RE issue #21 owns the exact
meanings and object codes. "Global" is where `0x91dd1` publishes each slot's buffer after
loading.

| Slot | `LayerKind` | Global | Observations | Confidence |
| --- | --- | --- | --- | --- |
| 0 | `Heights` | `0x2184d8` | 0–41, smooth; bit 7 is a run-time flag, always 0 in maps | high |
| 1 | `Terrain1` | `0x2184dc` | low six bits 0–19; bit 6 (harbour site) on 72 nodes, bit 7 (unused) on 20 | confirmed |
| 2 | `Terrain2` | `0x2184bc` | low six bits as slot 1; bit 7 (border node) on 20 nodes; bits 6 and 7 are run-time flags | confirmed |
| 3 | `Roads` | `0x2184ac` | always 0 in maps | confirmed |
| 4 | `ObjectIndex` | `0x2184d4` | 0–247, mostly 0 | confirmed |
| 5 | `ObjectType` | `0x2184d0` | 0, `0x80`, `0xc4`–`0xcd` | confirmed |
| 6 | `Animals` | `0x2184b4` | 0–6; matches the animal list | high |
| 7 | `FigureHigh` | `0x2184a8` | always 0; overwritten at load (high byte of the figure list head) | confirmed |
| 8 | `BuildingSites` | `0x2184b0` | 1, 2, 4, 9, `0x0c`, `0x0d`, `0x68`, `0x78`: size code in bits 0–2, blocking flags above | confirmed |
| 9 | `Ownership` | `0x2184b8` | always 7: owner nobody, nothing seen | confirmed |
| 10 | `Territory` | `0x2184a4` | 0 or 1, a few hundred 1s across all maps; territory strength at run time, reset for a new game | confirmed |
| 11 | `Resources` | `0x2184c0` | kind in bits 5–7, mineral in bits 3–4, amount in bits 0–2: `0x20`/`0x21`, `0x40`–`0x5f`, `0x87` | confirmed |
| 12 | `Shading` | `0x2184cc` | 0–128, centred on 64 | confirmed |
| 13 | `Runtime13` | `0x2184c4` | not stored; the game uses it as scratch space for its off-road path search | inferred |
| 14 | `Areas` | `0x2184c8` | area table index per node; `0xfe` (no area) common | confirmed |

[map-layers.md](../re/map-layers.md) covers what every slot means once a game runs, and
[slots.md](../re/slots.md) explains the term. Apart from
the harbour sites (campaign maps `MISS203`, `MISS205`, `MISS207`), the top two bits of slots 1 and
2 are set only in `OMAP02`.

## Decode results

All of these decode with no errors. The integration test `crates/s2-formats/tests/real_map.rs`
pins the counts, sizes, player counts and FNV-1a hashes.

| Source | Files | Sizes | Terrain sets |
| --- | --- | --- | --- |
| base game `.WLD` | 28 | 64×64 to 240×192 | greenland, wasteland |
| `.WLD` with the Gold Edition's data | 49 | 64×64 to 240×192 | all three |
| older `WORLDnnn.DAT` pairs (base game install and CD) | 31 | 64×64 to 240×192 | n/a (no header) |

Player counts run from 2 to 7. Only the Gold Edition's World Campaign and `MAPS4` maps place animals
(2,158 records across 17 maps). Every other map's list is the terminator alone.
