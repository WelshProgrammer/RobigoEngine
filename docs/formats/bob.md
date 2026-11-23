# BOB composite sprites and ROM_ANIM.DAT

Parser: `s2_formats::bob`. Worked out from the base game's and the Gold Edition's data files, then
checked against the S2.EXE loader and draw code. Addresses are `S2.EXE`'s, as
[s2-exe-layout.md](../re/s2-exe-layout.md) describes. The files are byte-identical in both data
sets. All integers are little-endian.

Confidence: **confirmed** means read in S2.EXE and matching the data; **observed** means it holds
across the shipped files and renders correctly, with no S2.EXE check yet; **guess** is a provisional
name.

## Files

| File | Overlays | Links | Items |
|---|---|---|---|
| `DATA/BOBS/CARRIER.BOB` | 602 | 3,264 | 34 |
| `DATA/BOBS/CARRIER2.BOB` | 738 | 3,264 | 34 |
| `DATA/BOBS/JOBS.BOB` | 2,269 | 8,928 | 93 |
| `DATA/ANIMDAT/ROM_ANIM.DAT` | | | 6,400 records |

S2.EXE loads `CARRIER.BOB` and `JOBS.BOB` by name (call site 0x15d5c). `CARRIER2.BOB` has the same
layout with its own pixel data (several body colours use different palette indices). S2.EXE does
not contain its name, with or without the Gold Edition.

## Serialised objects

The engine's object serialiser writes both formats. Every object starts with a `u16` class tag
that picks its reader (registrations at 0x770fc, 0x77d4c and 0x23632):

| Tag | Object | Body |
|---|---|---|
| `0x01F6` | BOB file | see below |
| `0x01F5` | pixel block | `u16` length, then that many bytes (reader 0x772a9) |
| `0x01F4` | part | see below (reader 0x77b32) |
| `0x5277` | animation script | `u16` count, then `count` six-byte records (reader 0x23726) |

## BOB layout (confirmed, reader 0x77e1d)

| Order | Field |
|---|---|
| 1 | tag `0x01F6` |
| 2 | body pixel block (tag `0x01F5`) |
| 3 | 96 body parts (tag `0x01F4` each), no count |
| 4 | six overlay pixel blocks, one per direction |
| 5 | `u16` overlay count, then that many parts |
| 6 | `u16` link count, then that many 4-byte links |

Nothing follows the last link.

### Part

| Offset | Type | Field |
|---|---|---|
| 0 | `u16` | tag `0x01F4` |
| 2 | `u8` | height `h` |
| 3 | `h` × `u16` | row offsets into the owning pixel block |
| 3+2h | `u8` | hotspot row |

Rows use the type 4 player-colour encoding from `lst.md` and are always 32 pixels wide. The width
is not stored. The engine assumes 32 (confirmed at the draw routine 0x778be, which subtracts 16 from
x and the hotspot row from y). Parts share row offsets, and many point at the same empty row.

Bodies decode from the body block. An overlay decodes from the block of the direction it is drawn
in, which comes from the link's position, not from the part (confirmed, 0x780af). Each overlay in
the carrier files is linked from one direction only. In `JOBS.BOB` overlay 91 (one row high) is
linked from directions 2 and 3 and decodes to the same pixels in both. The parser decodes a shared
overlay in every direction that links it and compares them (see [Quirks](#quirks)).

### Bodies (confirmed, loop at 0x77e73)

Body index = `type * 48 + direction * 8 + frame`, with two body types, six directions and eight
walking frames. Bodies have no head; the overlay draws it.

| Field | Values | Confidence |
|---|---|---|
| body type | 0 slim, 1 fat (wider figure) | confirmed index, guess name |
| direction | 0 E, 1 SE, 2 SW, 3 W, 4 NW, 5 NE | confirmed index, observed compass names |
| frame | 0 to 7, a walking cycle | confirmed |

### Links (confirmed, 0x7802e)

Link index = `item * 96 + frame * 12 + type * 6 + direction`.

| Offset | Type | Field |
|---|---|---|
| 0 | `u16` | overlay index |
| 2 | `i16` | added to the overlay's draw row; -1, 0 or 1 in the shipped files |

The engine draws the body, then the overlay at the same anchor with the row adjusted. Items are
wares in the carrier files. In `JOBS.BOB` they are figure types: S2.EXE holds a per-nation table of
72 eight-byte rows at 0x1fad94 whose byte 0 picks the file (0 carrier, 1 jobs), byte 1 the item
and byte 2 the body type (read at 0x237ab). The table lives in S2.EXE, in none of these files.

## Quirks

The parsers accept what S2.EXE accepts. Where the game's handling of an oddity is not yet known,
the parser accepts the input and reports the oddity. Each top-level type has a `quirks()` method
listing what was unusual and where. The shipped files produce none, the Gold Edition's included,
and the real-data tests assert that. Input is still an error when its bytes cannot be decoded at
all, or when the game itself treats it as fatal. It is also an error when accepting it would let a
small file allocate without bound.

`Bob::quirks` and `RomAnim::quirks` report:

| Quirk | What the parser does | Why |
|---|---|---|
| `Part` | a row quirk from the type 4 decoder (`lst.md`), with the body or overlay index | |
| `UnusedOverlay` | an overlay no link names stays undecoded, as `Overlay::Unlinked` with its height, hotspot row and row offsets | nothing says which direction's block it reads, and the engine never draws it |
| `OverlayDirections` | an overlay that decodes differently (or not at all) in another linked direction keeps the decode from its first link's direction | the engine reads whichever block it draws in; the first link is the stable choice |
| `LinkCount` | a link table that is not a whole number of items keeps every link; `Bob::items` counts whole items only | the links that are there still make sense |
| `TrailingBytes` | bytes after the last link, or after the last `ROM_ANIM.DAT` record, are ignored | the counts say where the data ends |

`Bob::overlays` returns `Overlay` values, and `Bob::overlay(i)` gives the decoded bitmap of a linked
one. These remain errors: a wrong class tag, input cut short, and a link naming an overlay past the
list (the engine would read past its overlay table). The same goes for a body, or an overlay in its
first direction, whose rows cannot be read from the block. Parts share their blocks, so the
`RowReads` limit from `lst.md` applies to the whole file: the rows of every part together may read
at most 256 bytes per byte of the file. The shipped files read about 1 to 1.5 bytes per byte.

## ROM_ANIM.DAT (confirmed, step routine 0x23926, draw routine 0x23761)

Tag `0x5277`, `u16` count (6,400), then six-byte records, and nothing after them. The last 819
records are all `0xFF`.

| Offset | Type | Field |
|---|---|---|
| 0 | `u8` | kind: 0 draws a `ROM_BOBS.LST` bitmap, anything else a BOB figure |
| 1 | `u16` | argument |
| 3 | `i8` | dx, added to the draw column |
| 4 | `i8` | dy, added to the draw row |
| 5 | `u8` | op |

Each animating settler holds a current record index. A step advances it by one and acts on the op.
Commands keep running until the step reaches a frame.

| Op | Meaning | Argument | Count |
|---|---|---|---|
| 0 | call: save the index, continue at record `arg` | record index | 946 |
| 1 | end of animation: the settler's animation state is freed | `0xFFFF` | 293 |
| 2 | return: continue after the saved call | `0xFFFF` | 71 |
| 3 | change the map object under the settler (engine passes `arg - 1` to 0x3d784) | 1 to 5 | 5 |
| 4 | play sound effect `arg` when in view (0x23b0e) | 1 to 57 | 250 |
| 5 to 255 | frame | see below | 4,016 non-blank, all `0xFF` |

Frames with kind 0 draw `ROM_BOBS.LST` item `arg` (3,211 frames; every such id names a type 4
bitmap). Frames with a non-zero kind (always 1, 378 frames) draw a BOB figure: the argument's low
byte is a row of the engine table above, the high byte a direction, and the walking frame is the
record index modulo 8. Command records carry dx and dy too. Nothing reads them there, and they
usually repeat the neighbouring frame's values.

Call targets all fall inside the non-blank records. Calls come in long runs: records 49 to 76 all
call record 78 or 88, for example. That looks like a jump table the engine indexes.

## Open questions for S2.EXE

- Kind-0 frames with argument `0xFFFF` (195) and `0xFFC3` to `0xFFCA` (232, each directly after an
  end record): does the draw routine (0x86ee4) skip them, and are the `0xFFCx` records the entry
  slots that the engine starts an animation from?
- Which engine tables index the call runs, and what an animation id is.
- What op 3 changes on the map (0x3d784 tests the node's object type against `0xC400`).
- What each sound number is. Op 4 passes its argument to `0x23b0e` as an effect id, which reads
  item `id + 0x32` of `SOUND.LST` ([sound.md](../re/sound.md#which-ids-have-a-sample-confirmed));
  which sound each of 1 to 57 holds was not listened to.
- Whether anything loads `CARRIER2.BOB`, perhaps through a built path; no executable names it.
- Whether "slim" and "fat" match the game's own terms for the two body types.
