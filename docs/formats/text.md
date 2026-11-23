# Text files (`.ENG`)

Parser: `s2_formats::text` (`StringTable`, `Page`, `cp437`).

All game text is English in `.ENG` files under `DATA/`. Two unrelated layouts share the
extension: binary string tables and plain text pages. A file is a string table when it starts with
the bytes `E7 FD`; everything else is a page.

## Which files exist

| Directory | Files | Layout | Base game | Gold Edition |
|---|---|---|---|---|
| `TXT` | `ONGAME`, `NAMES`, `MISS_000` to `MISS_010` | table | 13 | 13 |
| `TXT3` | `ONGAME` | table | – | 1 |
| `TXT2` | `CREDIT00` to `CREDIT21`, `KEYS` | page | 23 | 23 |
| `ONLINE` | `HELP0000` to `HELP0031`, `HELP1000` to `HELP1010` | page | 43 | 43 |
| `ONLINE` | `DEMO0000` to `DEMO0008` | page | 9 | – |

The Gold Edition drops the `DEMO` pages and adds `TXT3/ONGAME.ENG`, which its map editor
`S2EDIT.EXE` reads; `S2.EXE` never names `TXT3`. The base game and GOG's Gold Edition ship the
`.ENG` files alone. A game installed from the Gold Edition CD holds a German `.GER` file beside
every `.ENG` one ([differences.md](../re/differences.md#the-gold-edition-cds-install)). Robigo bakes
each language into its own folder ([mods.md](../design/mods.md#languages)).

S2.EXE builds the paths from these strings, then appends the language suffix `.ENG`:
`DATA\TXT\MISS_%3.3d` at `0x170503`, `DATA\TXT\ONGAME` at `0x170517`, `DATA\TXT\NAMES` at
`0x170527`. `!LOAD_MISSION_TEXTS n` in a mission script picks `MISS_nnn` (see `rtx.md`).

## String tables

Little-endian throughout.

| Offset | Size | Field |
|---|---|---|
| 0 | 2 | type tag `0xFDE7` |
| 2 | 2 | slot count *n* |
| 4 | 2 | block tag, always `1` |
| 6 | 4 | block size *s* in bytes |
| 10 | *s* | block |

The block starts with *n* `u32` offsets, one per slot, measured from the start of the block (byte
10 of the file). Offset 0 means the slot is empty. Every other offset points at a NUL-terminated
string inside the block. In every shipped table the file ends exactly at the end of the block.

S2.EXE reads the file as a serialised object. The leading `u16` picks a class from a registry:
`0xFDE7` is registered at `0x78253`, with its factory at `0x7826E`. The class reads the slot count,
then a nested object. That object's own `u16` tag 1 means "raw byte block", a `u32` size plus the
bytes, and tag 0 would mean no block. The reader for nested objects is at `0x8031C`. Slot lookup
(`0x7843C`) returns nothing for an index past the count or an offset of 0.

In every shipped table:

- The strings tile the rest of the block exactly, without gaps, overlaps or shared strings.
- In `MISS_nnn` and `TXT3/ONGAME` the strings are stored in slot order. In `TXT/ONGAME` and
  `NAMES` they are not, so the order of the strings says nothing about slot numbers.
- A present slot can hold a zero-length string (only the NUL). `MISS_000` has 10 of these,
  `TXT3/ONGAME` has 16. The API keeps "empty slot" (`None`) and "empty string" (`Some("")`)
  apart.

The parser rejects a wrong tag, a block tag other than 0 or 1, and a block that runs past the end
of the input. It also rejects an offset past the end of the block and a string with no NUL before
the end of the block. It ignores bytes after the block, as the game does. It accepts an offset that
points back into the offset table, as the game would, though no shipped file has one. It does not
require the strings to tile the block, because another release shares one string between several
slots.

The parsers accept what S2.EXE accepts. Where the game's handling of an oddity is not yet known,
the parser accepts the input and reports the oddity. Each top-level type has a `quirks()` method
listing what was unusual and where. The shipped files produce none, the Gold Edition's included,
and the real-data tests assert that. Input is still an error when its bytes cannot be decoded at
all, or when the game itself treats it as fatal. It is also an error when accepting it would let a
small file allocate without bound.

`StringTable::quirks` reports:

| Quirk | What the parser does | Why |
|---|---|---|
| `UnsizedBlock` | a block size of 0 takes the rest of the file as the block | other tools read a size of 0 this way |
| `NoBlock` | a block tag of 0, the serialiser's null object, has no size or bytes after it; every slot is empty | the game's object reader treats tag 0 as "no object" |

### Slot counts

| File | Slots | Present: base game | Gold Edition |
|---|---|---|---|
| `TXT/ONGAME` | 600 | 405 | 405 |
| `TXT/NAMES` | 300 | 165 | 165 |
| `TXT/MISS_000`, `MISS_001` | 120 | 34 | 34 |
| `TXT/MISS_002` | 120 | 20 | 20 |
| `TXT/MISS_003` | 50 | 20 | 8 |
| `TXT/MISS_004` to `MISS_007` | 50 | 8 | 8 |
| `TXT/MISS_008`, `MISS_009` | 50 | 4 | 4 |
| `TXT/MISS_010` | 50 | 6 | 6 |
| `TXT3/ONGAME` | 220 | – | 201 |

The Gold Edition's `TXT/ONGAME` has different content, is 6 bytes longer and keeps the same slots
present.

In the base game, `MISS_003` holds exactly the same twenty strings in the same slots as
`MISS_002`, only in a 50-slot table. Chapter 3's script (`MIS_0002.RTX`) loads `MISS_003`, so the
base game shows chapter 2's texts in chapter 3. The Gold Edition replaces the file with 8 strings
of its own.

### Codes inside strings

- Every `@` is a line break, a single `@` included. Runs of 2 up to 52 are common and act as
  vertical spacing in the mission briefings, and `text::lines` keeps the empty lines a run
  produces. Some tools write `@@` for a break, but the game has no such convention.
- C `printf` conversions that the game fills in: `%s`, `%d`, `%ld` and `%2.2ld`.
- No byte below `0x20` appears in any string, so there are no other control codes. `<`, `>`,
  `*` and `_` appear only as literal text.
- A few strings in `ONGAME`, `NAMES` and `MISS_000` are still in German.

## Text pages

Plain text up to the first `0x1A` (DOS end of file). Some files put a CR LF after the `0x1A`.
`DEMO0001`, `DEMO0003`, `DEMO0005` and `DEMO0007` hold only `0x1A`, and `CREDIT21` is one
space. Lines end in CR LF or, in most `CREDIT` files, in a bare CR. Pages contain no `@` and no
`printf` conversions.

`Page` keeps the bytes before the `0x1A` and splits lines on CR LF, CR or LF.

## Character set

The decoder is code page 437 (`text::cp437`). Bytes below `0x80` decode as ASCII and the upper half
as the IBM PC glyphs. `decode` maps every byte to one `char` and cannot fail; `encode_char` goes the
other way.

The only non-ASCII bytes in any `.ENG` file, in any edition, are `0x81` ü, `0x84` ä, `0x8E` Ä,
`0x94` ö, `0x9A` Ü, `0xE1` ß and `0xF8` °. All seven decode identically in code pages 437 and 850,
and the files cannot tell the two apart. 437 is the default DOS code page and the safer reading.
`0xF8` appears where a copyright sign is meant, which code page 437 lacks. The game draws text
with its own bitmap fonts. The font files, not a code page, decide the glyph each byte shows, so
check `0xF8` there once the fonts are decoded.
