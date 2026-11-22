# IDX/DAT named indexes

Parser: `s2_formats::idx`. Worked out from the base game's and the Gold Edition's data files and
checked against every pair in both. Nothing here is copied from other projects. All integers are
little-endian.

## Where the format turns up

| Pair | Base game | Gold Edition | Contents |
|---|---|---|---|
| `DATA/IO/IO` | 262 entries | 264 entries | interface graphics |
| `DATA/RESOURCE` | 57 entries | 57 entries | fonts, palette, frames, icons |
| `DATA/IO/EDITIO` | absent | 149 entries | map editor interface graphics |
| `DATA/EDITRES` | absent | 57 entries | map editor fonts, palette, frames, icons |

The `.IDX` names and locates items; the `.DAT` beside it holds them.

## Index file

| Offset | Type | Field |
|---|---|---|
| 0 | `u32` | entry count |
| 4 | entries | `count` entries of 28 bytes |

The file is exactly `4 + 28 * count` bytes long.

### Entry

| Offset | Type | Field |
|---|---|---|
| 0 | 16 bytes | name, NUL-padded |
| 16 | `u32` | offset of the item in the DAT |
| 20 | 6 bytes | reserved, see below |
| 26 | `u16` | item type id |

Names are lower-case ASCII in every shipped file, at most 9 characters, and unique within each
index even ignoring case. The bytes after the terminating NUL are always zero. The game's text
is CP437, so the parser keeps names as raw bytes and offers a `&str` view only for pure ASCII.

The game finds an entry by name. `0x50128` hands a name to the registry lookup `0x8dee5`, and both
the window style records and the action menu button records store names rather than positions
([windows.md](../re/windows.md#window-styles-confirmed)).

The reserved bytes are zero in most entries. In 38 of the 846 entries across both data sets they
hold a 32-bit value that looks like a memory address (`0x007B3EFC`, `0x8482F520` and similar) and
then a `u16` of 1 or 2. The base game's `IO.IDX` has 13 such entries and its `RESOURCE.IDX` none.
The Gold Edition's `RESOURCE.IDX` has 18, and those 18 entries are the only difference between it
and the base game's file.

S2.EXE writes to these bytes itself. After a play session in the Gold Edition, `RESOURCE.IDX` and
`IO.IDX` had been rewritten and more entries carried non-zero reserved bytes, while every item in
the DAT decoded the same. The values are runtime state the game stores back into the index, probably
where it loaded an item and a load count. The shipped values are leftovers from whichever session
last wrote the file. The parser keeps them as found without interpreting them, and tests never pin
them.

## Data file

Each entry's offset points at a tagged item: a `u16` type id and the item body, encoded as
inside an LST archive (see `lst.md`). The tag always equals the type id in the index entry.

In every shipped pair the items sit back to back in index order, starting at offset 0, with no
gaps, no overlaps and no bytes left over at the end. The parser requires neither the order nor
the packing. Sharing and overlaps are covered below.

## Quirks

The parsers accept what S2.EXE accepts. Where the game's handling of an oddity is not yet known,
the parser accepts the input and reports the oddity. Each top-level type has a `quirks()` method
listing what was unusual and where. The shipped files produce none, the Gold Edition's included,
and the real-data tests assert that. Input is still an error when its bytes cannot be decoded at
all, or when the game itself treats it as fatal. It is also an error when accepting it would let a
small file allocate without bound.

`Index::quirks` reports:

| Quirk | What the parser does | Why |
|---|---|---|
| `TrailingBytes` | bytes in the index past the announced entries are ignored | the count says how many entries there are |
| `TypeMismatch` | an entry whose index type differs from the DAT tag is kept with no item (`Entry::is_skipped`, item `Item::Empty`, size 0) | another loader skips such entries; what the game does is open |
| `SharedOffset` | entries that point at the same offset share one decoded item | nothing about the bytes is wrong |
| `Overlap` | an item that runs into the next entry's item is kept whole | the bytes still decode |
| `Item` | an LST quirk inside an entry's item, with the entry index | |

Sharing and overlaps could make a small pair decode the same bytes many times. Each distinct
offset is decoded once. The items at distinct offsets may cover at most `MAX_COVERAGE` (4) times
the DAT's length in total, and past that the parser stops with `Error::Coverage`. Bitmaps with
no data draw on one budget for the whole DAT (see `lst.md`).

These remain errors: an index shorter than its count, an offset past the end of the DAT, and an item
that fails to decode.

## Contents

| Pair | Bitmap (2) | Font (3) | Palette (5) | Raw bitmap (14) |
|---|---|---|---|---|
| `IO`, base game | 244 | | | 18 |
| `IO`, Gold Edition | 246 | | | 18 |
| `EDITIO` | 131 | | | 18 |
| `RESOURCE`, `EDITRES` | 32 | 3 | 1 | 21 |

`RESOURCE` opens with `font11`, `roemerpal` (the palette), `font09` and `font14`. Its decoded
items are identical in both data sets, and `font14` is the same font as `DATA/IO/FONT14.FNT`.

`EDITRES` has the same 57 names in the same order as `RESOURCE`; 21 of its items differ. Its
three fonts carry 123 glyphs instead of 115, adding codes 165 to 168, 173, 181, 214 and 233.

The Gold Edition's `IO` adds `iobob_03` and `iobob_04` after `iobob_02` and changes `iobob_01`
and `iobob_02`; the other 260 items match the base game's. `EDITIO` shares the `back_*` entries with
`IO`, and its `iocon_*` numbering runs out of order (`iocon_251`…`iocon_286` come before
`iocon_208`).

## Open questions for S2.EXE

- Whether the game reads the reserved bytes or the index type id at all, or only the tag in
  the DAT.
