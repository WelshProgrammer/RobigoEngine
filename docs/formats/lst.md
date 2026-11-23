# LST archives and item encodings

Parser: `s2_formats::lst`. Worked out from the base game's and the Gold Edition's data files and
checked against every archive in both. Nothing here is copied from other projects.
All integers are little-endian.

## Where the format turns up

| Container | Layout |
|---|---|
| `DATA/**/*.LST` (30 in the base game, 52 with the Gold Edition) | archive header, then slots |
| `DATA/IO/FONT14.FNT` | one tagged item (type 3) |
| `DATA/IO/IO.DAT`, `DATA/RESOURCE.DAT` | tagged items at offsets listed in the `.IDX` files |

`DRIVERS/AILDRVR.LST` is a Miles Sound System driver catalogue in plain text, not an archive.
The base game has no `GFX/` LST files and keeps `CREDITS.LST` in `DATA/`.

A *tagged item* is a `u16` type id followed by the item body. All 57 entries in
`RESOURCE.DAT` and 262 in `IO.DAT` decode with the bodies below.

## Archive

| Offset | Type | Field |
|---|---|---|
| 0 | `u16` | magic `0x4E20` (bytes `20 4E`) |
| 2 | `u32` | slot count |
| 6 | slots | `count` slots back to back |

Each slot starts with a `u16` flag. A flag of 0 means an empty slot with nothing after it, and 1
means a `u16` type id and the item body follow. No other flag value occurs in the shipped files, and
the last slot ends exactly at the end of the file. See [Quirks](#quirks) for what the parser does
otherwise.

## Item types

| Id | Item | Where |
|---|---|---|
| 1 | sound | `SOUND.LST` |
| 2 | bitmap with transparency | most archives, `IO.DAT`, `RESOURCE.DAT` |
| 3 | font | `FONT14.FNT`, `RESOURCE.DAT` (never inside an LST) |
| 4 | player-colour bitmap | `ROM_BOBS.LST`, `BOAT.LST`, map and building sets, font glyphs |
| 5 | palette | slot 0 of each map set, `EDITBOB.LST`, `RESOURCE.DAT` |
| 7 | shadow | map, building, `BOOT*` and `MIS*BOBS` sets |
| 14 | raw bitmap | `IO.LST`, `IO.DAT`, `RESOURCE.DAT` |

No other ids appear in either edition.

### Type 1: sound

`u32` length, then that many bytes. `SOUND.LST` declares 200 slots and fills 55 of them: slot 0
holds a complete XMIDI file (`FORM`, size, `XDIR`…) and slots 51 to 114 hold headerless unsigned
8-bit mono PCM centred on `0x80`. Ten slots inside that range are empty and one PCM item is only 2
bytes long. The game reads slot `n + 0x32` for effect id `n`
([sound.md](../re/sound.md#which-ids-have-a-sample-confirmed)). The item does not store the sample
rate; the effect table in
[sound.md](../re/sound.md#the-effect-table-0x1f972c-confirmed) supplies one per effect id.

### Types 2, 4 and 7: shared header

| Offset | Type | Field |
|---|---|---|
| 0 | `i16` | nx: hotspot column |
| 2 | `i16` | ny: hotspot row |
| 4 | `u32` | always 0 |
| 8 | `u16` | width |
| 10 | `u16` | height |
| 12 | `u16` | always 1 |
| 14 | `u32` | data length |
| 18 | bytes | data |

The data opens with a `u16` per row giving that row's offset from the start of the data. The
largest data block in the shipped files is 57,811 bytes, so the offsets never overflow. Rows are
stored in order with no sharing.

### Type 2: draw/skip rows

Each row repeats `n` (pixels to draw), `n` palette indices, `m` (pixels to skip) and ends with
`0xFF` where the next `n` would be. Every row covers exactly the bitmap's width. The data block
carries one extra `0xFF` after the last row.

### Type 7: shadow rows

The type 2 row encoding without the index bytes: `n` shadowed pixels, `m` skipped, ending with
`0xFF`, plus the same extra trailing `0xFF`. A render of a tree and its shadow confirmed the
draw-first order.

### Type 4: player-colour rows

Rows have no terminator. Commands run until they fill the width. The command byte's top two
bits pick the kind and its low six bits give a count `n` (0 to 63):

| Bits | Meaning | Bytes after the command |
|---|---|---|
| `00` | `n` transparent pixels | none |
| `01` | `n` palette indices | `n` |
| `10` | `n` pixels of player shade `s` | `s` (always 0 to 3) |
| `11` | `n` pixels of palette index `c` | `c` |

Every palette in the LST files holds a bright-to-dark blue ramp at indices 128 to 131, the
default player colour. Shade `s` draws as index `128 + s` there. Fixed-colour pixels in type 4
bitmaps never use 128 to 131. Shades land on the soldiers' shields and plumes in `ROM_BOBS.LST`.

### Type 5: palette

A `u16` that is always `0x0100`, then 256 RGB triples with 8 bits per channel. These are not
6-bit VGA values.

### Type 14: raw bitmap

| Offset | Type | Field |
|---|---|---|
| 0 | `u16` | always 1 |
| 2 | `u32` | pixel count `len` |
| 6 | bytes | `len` palette indices, row by row |
| 6+len | `i16`, `i16` | nx, ny |
| 10+len | `u16`, `u16` | width, height (`width * height == len`) |
| 14+len | 8 bytes | `00 00 02 01 F4 06 70 00` in every item |

The dimensions come after the pixels, unlike the other bitmap types.

### Type 3: font

| Offset | Type | Field |
|---|---|---|
| 0 | `u8` | dx |
| 1 | `u8` | dy |
| 2 | 224 slots | glyphs for character codes 32 to 255 |

Each glyph slot is a tagged item: `u16` 0 for no glyph, or 4 followed by a type 4 body. The shipped
fonts use only type 4. The parser also takes type 2 glyphs and turns them into player bitmaps with
fixed colours only (opaque pixels become `Pixel::Colour`), so that a font from another release or
tool loads the same way. The three fonts (`font09`, `font11`, `font14`) each have 115 glyphs, all at
origin (0, 0) and all `dy` pixels tall (9, 11, 14). dx is 8, 10 and 12 (dy minus 1, 1 and 2) and
matches no glyph width. The engine reads dy back as the line height and never looks at dx again
([text.md](../re/text.md#the-font-object-confirmed)). `FONT14.FNT` and the `font14` entry in
`RESOURCE.DAT` are the same bytes.

## Quirks

The parsers accept what S2.EXE accepts. Where the game's handling of an oddity is not yet known,
the parser accepts the input and reports the oddity. Each top-level type has a `quirks()` method
listing what was unusual and where. The shipped files produce none, the Gold Edition's included,
and the real-data tests assert that. Input is still an error when its bytes cannot be decoded at
all, or when the game itself treats it as fatal. It is also an error when accepting it would let a
small file allocate without bound.

`Archive::quirks`, `Font::quirks`, `Item::quirks` and the per-bitmap `quirks()` report:

| Quirk | What the parser does | Why |
|---|---|---|
| `SlotFlag` | a flag other than 0 or 1 reads as an empty slot; the next slot starts right after it | another loader treats it this way; never seen in the shipped data |
| `TrailingBytes` | bytes after the last slot (or after a stand-alone font) are kept in `Archive::trailing` | map editors note that original files can end in padding zeros |
| `NoData` | a type 2, 4 or 7 bitmap with a size but a data length of 0 decodes fully transparent | other loaders accept it and editors write it |
| `RowOverrun` | a row whose runs pass the width is clipped (type 2 and 7 skip or draw runs, any type 4 command) | the pixels past the edge cannot be shown in the bitmap |
| `RowShort` | a type 2 or 7 row that ends before the width is transparent to the end | the missing pixels are not drawn |
| `Shade` | a type 4 shade byte above 3 is kept as found (`Shade::from_byte`) | the engine adds the shade to the player's base index, so it would draw `base + s` |
| `Item`, `Glyph` | wrap any of the above with the slot index or character code | |

`PlayerColour::index` resolves an out-of-range shade `s` to the first index plus `s` (wrapping
past 255), matching that base-plus-shade lookup. `Shade::is_valid` tells the two apart.

Without a limit, a bitmap with no data could claim 65,535 by 65,535 pixels from an 18-byte header.
Every such bitmap in an archive draws on one shared budget of 256 pixels per byte of the archive (or
DAT, or item), and `TooLarge` is the error past it. Bitmaps with data keep their own 256-to-1 limit.

Row offsets may point anywhere in the data, so every row of a tall bitmap could read all of it again
and make decoding quadratic in the input. A bitmap's rows together may read at most 256 bytes per
byte of input, and `RowReads` is the error past that. No shipped bitmap's rows read more than its
data once over.

These remain errors: an unknown item type (its length is unknown), a row start past the data, a row
that runs off the end of the data, and a font glyph of a type other than 0, 2 or 4.

## Open questions for S2.EXE

- How the engine maps shades to other players' colours, and where their ramps sit.
- Whether the always-0 `u32` and always-1 `u16` in the bitmap header, the `0x0100` in palettes
  and the 8-byte tail on raw bitmaps carry meaning or are leftovers from the authoring tool.
- Whether the engine treats a skip count of `0xFF` as end of row. The shipped data never
  contains one, and the parser reads it as a count.
