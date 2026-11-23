# Savegames: `SAVE/*nnn.DAT`, `DIR.DAT`, `MISSION.DAT`, `RES.DAT`

Parser: `s2_formats::save` (`Slot::parse`, `Directory::parse`, `MissionProgress::parse`,
`WorldCampaignProgress::parse`, `ScreenMode::parse`). Integers are little-endian.

Addresses are from the Gold Edition's executable (`s2-gold-151.elf`) unless marked "patched"
(`s2-151.elf`, the build [s2-exe-layout.md](../re/s2-exe-layout.md) describes). The top-level
save code sits 4 bytes higher in the Gold Edition's build than in the patched one. Everything
else moves by different amounts.

This document covers the container. What the transport, control and computer-player state
means belongs to RE issue #31, with open questions listed at the end.

## What is on disk

| Install | `SAVE/` contents |
| --- | --- |
| base game | `MISSION.DAT` (13 bytes) |
| with the Gold Edition's data, as shipped | `MISSION.DAT` (30 bytes), `DIR.DAT`, `RES.DAT`, slot 900 (six files, no `CNTRL900.DAT`), slot 901 (all seven) |

The Gold Edition's `S2.EXE` also knows `save\mission2.dat`, but neither edition ships one. Starting
a World Campaign continent and saving does not create it. It probably appears once a continent is
won.

The two slots the Gold Edition ships were both saved on `MISS203.WLD`, the fourth campaign map.
Their used area table entries and area layers equal the map's. Slot 900 also keeps the map's heights
and first terrain layer, while slot 901 (game clock 104,652) has drifted further.

Saves written by the Gold Edition's own executable cover a campaign chapter, Unlimited Play maps, a
split-screen game, a World Campaign continent, and one game saved three times a few minutes
apart. Every one has all seven files, a 578-byte control block and no quirks. Saves
belong to the player and change whenever they play, so the real-data test checks rules every save
follows and pins no bytes.

## Slot files

The save routine (`0x14927`, patched `0x14923`) checks for 500,000 bytes of
free disk space, then writes, in this order:

| Order | File | Writer | Reader | Parsed as |
| --- | --- | --- | --- | --- |
| 1 | `WORLDnnn.DAT` | `0x14d70` | | `map::Map` |
| 2 | `TRANSnnn.DAT` | `0x14c9f` | | `Transport` (header + raw) |
| 3 | `CONTInnn.DAT` | `0x14e28` | | `map::Map` area table |
| 4 | `CNTRLnnn.DAT` | `0x15454` | `0x15515` | `Control` (raw, game clock named) |
| 5 | `DESKTnnn.DAT` | `0x13ccd` | `0x13bf5` | `Desktops` (fully framed) |
| 6 | `MAILSnnn.DAT` | `0x14f76` | | `Mailboxes` (fully framed) |
| 7 | `COMPYnnn.DAT` | `0x1509f` | `0x15132` | `ComputerPlayers` (grid framed, players raw) |

With its export flag set (`BX != 0`) the same routine writes only `WORLD` and `CONTI`, which is
how the `DATA\MAPS\WORLDnnn.DAT` pairs were made. For a savegame all seven are written, so a
slot without `CNTRL` was not produced by the game. The loader (patched `0x14aed`) and the main
loop (`0x105db`, which opens `CNTRLnnn.DAT` before anything
else) both expect it. `Slot::parse` accepts a missing `CNTRL`, or any other missing part except
the world and area table, and reports it through `Slot::missing`.

Every file is written through a stream object whose virtual method `+0x28` writes `n` raw bytes.
Nested objects go through the generic object writer (`0x81254`), which writes a `u16` class tag from
the object's method `+0x14`, then calls the object's serialiser at `+0x1c`. Nothing in a slot is
compressed except the world's layers.

### Slot numbers

Directory entry `n` is saved as files numbered `900 + n`. The autosave tick (`0x4caab`) counts up
to the interval the player picked. When it fires, it fills directory entry `[0x1f7bed]`, writes
`DIR.DAT`, then calls the save routine with `900 + [0x1f7bed]`. Confidence is high for autosaves.
The manual save path goes through an indirect call that has not been traced, so confidence that
it uses the same numbering is medium.

### `WORLDnnn.DAT` and `CONTInnn.DAT`

The world is the compact layer container from `map.md`: class `0x2711`, `0xffffffff`, origin,
size, then 14 `u32`-length layers. `Map::parse_v102(world, conti)` reads it unchanged, and
`CONTI` is the 2,250-byte area table. In both shipped slots all 14 layers are RLE-packed, so this
is the first real data through the RLE decoder. Every used area's node count matches the area
layer exactly in both slots, which checks the decoder end to end.

Slots 6 and 7 do not hold map data in a save. Compared with `MISS203.WLD`, slot 900 matches 0%
and 0.1% of nodes there, against 93% to 100% for every other layer. Their runtime meaning is a
question for RE issue #21, and the parser keeps the map names.

### `TRANSnnn.DAT` (confidence: framing high, contents low)

One serialised object of class `0x2b5c` (registered at `0x251ab`, class-id method `0x276dc`,
vtable `0x199da0`, serialiser `0x26ed8`):

| Offset | Size | Field |
| --- | --- | --- |
| `0x00` | 2 | class tag `0x2b5c` |
| `0x02` | 2 | map width (checked against the world) |
| `0x04` | 2 | map height (checked against the world) |
| `0x06` | 2 | pool size from the first section's object (1000 in both saves) |
| `0x08` | 2 | pool size from the second section's object (2000 in both saves) |
| `0x0a` | rest | pool sections, kept as `Transport::body` |

The serialiser then writes seventeen sections in a fixed order, followed by an object from
`[0x20118e]` and a run of `u16` scalars. Three kinds of section are visible:

- The first section (`0x28850`) writes four `u16`: a capacity of 1000, then three counts
  (4, 4, 3 in slot 900 and 489, 417, 491 in slot 901). For each of the 1000 entries it then
  writes a `u16` 0 (empty) or `u16` 1 followed by a variable-length record from `0x28334`. The
  section's length depends on record contents, so it cannot be skipped without decoding.
- Generic pools (`0x797a0`, used eleven times) write seven `u32` header fields and then a nested
  tagged object.
- Bespoke sections at `0x29980`, `0x2d160`, `0x3d1d9`, `0x29b8d` and `0x31f15`.

### `CNTRLnnn.DAT` (confidence: length and clock high, rest unknown)

A block copied straight from memory: 578 (`0x242`) bytes from `0x1f7ba8` (patched `0x1f78e0`). The
reader is `0x15511`. Bytes 109 to 172 are the eight player slot records
([players.md](../re/players.md#the-slot-record-confirmed)). The first `u32` is the game clock,
which it copies to `0x2011c0` and `0x2010e4` (patched `0x200c9c` and `0x200bc0`). It also clears the pause word, which is where the original's
stacking pause overlay comes from
([windows.md](../re/windows.md#what-that-costs-when-the-word-is-cleared-elsewhere)). The game stores
the byte at offset `0x20` from the word at `0x218a18` and restores it on load.

The control block in the Gold Edition's shipped slot is 358 bytes, the length an older executable
wrote ([differences.md](../re/differences.md#savegames)), so **the executable the Gold Edition ships
did not write the saves it ships**. Its reader asks for 578 bytes and does not check the count it
gets; the read wrapper at `0x7fefb` only checks the DOS carry flag. It would load the 358 bytes and
leave the rest of the block as it was. By the same reasoning it would load any shorter file. The
parser accepts any length up to 578 for that reason, and reports the layout it saw
(`ControlLayout::V10x`, `V151` or `Other`) and the length (`Control::len`). A length other than 358
or 578 is reported as `Quirk::ControlLength` in `Slot::quirks`. A block longer than
578 bytes is an error. `Control::game_time` reads 0 when the block is too short to hold the clock.

The clock is the game's step counter (`0x2011c0`, patched `0x200c9c`; see
[rng.md](../re/rng.md)), one unit per simulation step. At normal speed one step runs every 8
ticks of the 60 Hz timer, 450 a minute. One game saved at 0:01, 0:03 and 0:06 (played at a mix of
normal and fast speed) has clocks of 763, 1,615 and 3,269. Taking the name's minutes as rounded
down, all three agree on 468 to 538 steps per minute of the name's time, which leaves out 450.
The name's time is not the clock divided by the normal-speed rate. It may be play time on the
wall clock, since fast speed runs more steps per minute. Its source is still open.

The parsers accept what S2.EXE accepts. Where the game's handling of an oddity is not yet known, the
parser accepts the input and reports the oddity. Each top-level type has a `quirks()` method listing
what was unusual and where. The files the base game and the Gold Edition ship produce none, and the
real-data tests assert that. Input is still an error when its bytes cannot be decoded at all, or
when the game itself treats it as fatal. It is also an error when accepting it would let a small
file allocate without bound.

`Slot::quirks` lists the control-length quirk and any world quirks (`Quirk::World`, see
`map.md`).

### `DESKTnnn.DAT` (confidence: high)

One 916-byte record per desktop. The writer (`0x13ccd`) serialises `[desk + 0x14]` and
`[desk + 0x18]` when each is non-null, so a file holds one record, or two in split-screen.
Each record (`0x467cc`):

| Offset | Size | Field |
| --- | --- | --- |
| `0x000` | 4 | the dword at `+0x49a`, which decides whether the small diary opens |
| `0x004` | 50 × 18 | one record a window kind, from `+0x10e` |
| `0x388` | 12 | the bytes at `+0xcd`: two `u32` and a `u16`, each set to 1 at set-up, then two spare |

Window record: `u8` remembered, `u8` count, `i16` x, `i16` y, `u16` width, `u16` height, 8 bytes.

A record's place in the array is the window kind the view's own list carries
([screens.md](../re/screens.md#the-screens-confirmed) names them), so slot `0x03` holds the game
menu and slot `0x17` the ship register. Closing a window (`0x461d0`) finds its entry in the list at
the view's `+0xf9`, copies the window's rectangle into `+0x02` of the kind's slot, sets the first
byte and takes 1 off the second. Opening one (`0x46160`) adds 1 to the second byte and, when the
first is set, moves the window to the remembered x and y through its vtable `+0x28`. The byte
at `0x1fbdc0 + kind` guards the copy and is 1 for every kind from 0 to `0x30`, so every window's
position is remembered. Nothing writes the last 8 bytes of a record, which is why the saves hold
`0xcd` fill there.

The dword at `+0x49a` is zeroed when the view is set up (`0x43909`) and written whole by
`0x454f8`. `0x45500` opens the small diary, kind `0x11`, only when it is not zero, and the factory
case at `0x45bed` passes it to the window. It is a text reference: the high word names the table, 2
for the mission's own `MISS_nnn.ENG`, and the low word the slot of the chapter's current task,
which the letter's fill stores there ([screens.md](../re/screens.md#the-diary-confirmed)). The
saves hold `0x00020001` and `0x00020005`. The three
values from `+0xcd` are written once at `0x43929` and never read again.

### `MAILSnnn.DAT` (confidence: framing high)

Written by `0x219e9` from the object at `[0x201b6c]`:

```text
u32 counter                         (1 and 75 in the saves)
repeat 8 times, once per player:
    u8 present                      (0 or 1)
    if present: u32 count, count × 28-byte message records
```

Both saves have player 0's mailbox present and empty, and no other mailboxes. The parser rejects
a presence byte other than 0 or 1 and any bytes after the eighth player.

### `COMPYnnn.DAT` (confidence: framing high, players medium)

Written by `0x6a11f` (patched `0x693a7`) from `0x1509f`, read by `0x6a046` (patched `0x692ce`) from
`0x15132`. What the fields mean is in [ai.md](../re/ai.md#save-and-load), with patched addresses.
First the shared grid (`0x689e2`, built at `0x68a23`):

| Offset | Size | Field |
| --- | --- | --- |
| `0x00` | 4 | columns: map width / 8 |
| `0x04` | 4 | rows: map height / 8 |
| `0x08` | 4 | cells: columns × rows |
| `0x0c` | 4 | 64 when built |
| `0x10` | 4 | grid pass cursor, column (0 in the saves) |
| `0x14` | 4 | grid pass cursor, row (14 and 3 in the saves) |
| `0x18` | cells × 21 | one record per 8 × 8-node cell |

Then a record (`0x6a433`) for each slot in the game, in ascending order: every slot that passes the
in-game check at `0x165a6`, whoever plays it, the people's slots included.

```text
634     player block (job lists, handler table, player number)
22 ×    u16 present (0 or 1)
        if 1: u16 task type, then the task's own fields (its method +0x30; none for most types)
300 ×   u16 present (0 or 1)
        if 1: u16 job kind, 22-byte job base, then the kind's fields (8 to 40 bytes)
```

Task and job sizes vary by type and kind, so the parser keeps everything after the grid as
`ComputerPlayers::players`.

## `DIR.DAT` (confidence: framing high)

631 bytes, read by `0x4c85d` and written by `0x4c935`. `0x4c4f8` builds an empty one when the
file is missing.

```text
u32 counter                        (incremented on every save)
repeat 11 times:
    u16 used                       (1 when the slot holds a save)
    u8  kind                       (copied from [[0x201192] + 0x1c]; 2 seen only on split screen)
    u32 sequence                   (the counter after this save, so higher is newer)
    50  name                       (NUL-terminated CP437)
```

The game always generates the name, `'<map title>'-<h>:<mm>` with the elapsed in-game time (format
string at `0x170c91`). The player cannot type one. In the saves examined, `kind` was 1 for every
single-screen game (campaign, Unlimited Play and World Campaign alike) and 2 for the one
split-screen game, whose `DESKT` file holds two desktops. The `DIR.DAT` the Gold Edition ships has
every entry unused even though slots 900 and 901 exist, and the name fields hold `0xcd` after the
leading NUL. Names are CP437, decoded with `s2_formats::text::cp437`.

## `MISSION.DAT` (confidence: high)

Roman campaign progress, one ASCII `'0'` or `'1'` per chapter. Two routines use it: `0x321ea`
updates it and the screen constructor `0x5e10e` reads it for the chapter list
([screens.md](../re/screens.md#the-chapter-screens-confirmed)). Both open the existing file and read up
to 30 bytes into a buffer, ignoring anything after them. The updater sets up to three bytes to
`'1'`, then recreates the file (DOS create, which truncates) and writes all 30 bytes back, so the
game never leaves a file longer than 30 bytes.

The base game's file is 13 bytes: ten flags, CR, LF and `0x1a`, with chapters 1 and 2 open. The Gold
Edition's 30-byte file has four chapters open. Its last 17 bytes are whatever the buffer held and
look like noise. A longer file comes from an external edit or another version of the game; one seen
in a Gold Edition install had CR LF added after byte 30. The parser requires at least 10 bytes,
keeps the first 30 and checks only the ten chapter flags.

## `MISSION2.DAT` (Gold Edition only, confirmed)

The World Campaign's progress: 19 bytes copied to and from `0x201ba4` (`0x171e9` reads,
`0x172ac` writes). Nine little-endian words, one per continent in the table's order, are 1 once
the continent is won; the last byte is the continent the world screen marks as chosen
([world-campaign.md](../re/world-campaign.md#mission2dat-confirmed)). A file made by hand with two
words set showed both continents won on the world screen.

## `RES.DAT` (confidence: medium)

A screen-mode digit. The start-up code (`0x103a4`) reads one byte, subtracts `'0'` and stores the
result at `0x201196`. It then lowers the mode to what memory allows (`0x23634`): 0 up to 6,000,000
bytes, 1 up to 6,500,000, otherwise 2. Three modes fit 640 × 480, 800 × 600 and 1024 × 768, but the
path from `0x201196` to the video code has not been traced. The Gold Edition's file is `'2'`
followed by a NUL that the reader ignores.

## Questions for RE #31

1. **Transport sections.** Record layouts for the first section (`0x28334`, 36-byte in-memory
   records), the generic pools (`0x797a0`: what the seven `u32` fields are and which class the
   nested object has), and the bespoke sections `0x29980`, `0x2d160`, `0x3d1d9`, `0x29b8d`,
   `0x31f15`. Which section is flags, roads, buildings, settlers and wares?
2. **Pool sizes.** What are 1000 (`[section1 + 4]`) and 2000 (`[section2 + 6]`)? Fixed limits or
   saved state?
3. **Control block.** Field layout of the 578-byte block, and what the byte at `0x20` (from
   `0x218a18`) holds.
4. **Missing `CNTRL`.** What happens when `CNTRLnnn.DAT` is absent: an error from the open
   routine `0x7fcf6`, or a fallback?
5. **Mail records.** Layout of the 28-byte record and the meaning of the leading counter.
6. **Computer players.** Settled in [ai.md](../re/ai.md): the grid cell, the player block, the
   task and job serialisers. The parser still keeps the players raw.
7. **Directory.** Confirm that `kind` is the screen layout (1 single, 2 split), and whether
   manual saves use slot `900 + n` like the autosave.
8. **Runtime layers.** What map slots 6 and 7 hold during play, and why slot 901's terrain
   layers match the map on only 60% to 65% of nodes.
9. **`MISSION2.DAT`.** Layout of the 19 bytes.
