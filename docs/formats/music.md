# XMIDI songs and Miles timbre banks

Parser: `s2_formats::music` (`Xmidi::parse`, `Sequence::to_smf`, `Sequence::from_smf`,
`TimbreBank::parse`).

Settlers II plays MIDI music through the Miles Sound System (AIL 3.03, linked into S2.EXE). Songs
are XMIDI, Miles' IFF-wrapped MIDI variant. FM instruments come from a Miles Global Timbre Library.

## Files

| File | Contents |
|---|---|
| `DATA/SOUNDDAT/SNG/SNG_0001.DAT` .. `SNG_0014.DAT` | one XMIDI song each, 4 to 13 KB |
| `DATA/SOUNDDAT/SOUND.LST` | one more XMIDI as an item (at byte 14), 365.8 s long |
| `DRIVERS/SETT_SND.AD`, `DRIVERS/SETT_SND.OPL` | timbre bank, 3,622 bytes |

All of these are byte-identical in the base game and the Gold Edition. S2.EXE holds only
the base name `SETT_SND` (0x1714c8). The suffix comes from the Miles driver in use: `.AD` for the
OPL2 (AdLib) driver and `.OPL` for the OPL3 one. The two files are byte-identical, so OPL3 cards
get the same two-operator voices as OPL2 cards.

## XMIDI container

Big-endian IFF chunks, as in EA IFF 85:

```
FORM <len> XDIR
  INFO <2>          u16 little-endian sequence count
CAT  <len> XMID
  FORM <len> XMID   one per sequence
    TIMB <len>      optional: u16 LE count, then count × (patch u8, bank u8)
    RBRN <len>      optional: u16 LE count, then count × (id u16 LE, offset u32 LE into EVNT)
    EVNT <len>      the event stream
```

A file may also be a lone `FORM XMID` with no directory, and the parser accepts both. The count in
`INFO` is little-endian although the chunk lengths are big-endian. Bytes after the `CAT` chunk
are ignored, so an XMIDI cut loosely out of a container still parses.

Every shipped song has exactly one sequence, a `TIMB` chunk, no `RBRN` chunk, and no odd-length
chunks. The parser still skips a zero pad byte after an odd-length chunk.

## Events

The `EVNT` body is MIDI with three differences:

- **Delays.** A byte below `0x80` in status position is a delay of that many ticks; consecutive
  delay bytes add up. The Miles sequencer reads one delay byte at a time and waits at least one
  tick for each, so a zero byte also costs a tick. None occur in the shipped songs.
- **Notes.** A note-on (`0x9n key velocity`) is followed by a duration as a standard MIDI
  variable-length quantity. The stream has no note-offs. Miles queues the note-off and sends it
  `max(duration, 1)` ticks later, before any stream events of that tick. Sixty-nine notes in the
  `SNG_*.DAT` files have duration 0 and sound for one tick.
- **No running status.** Every event carries its status byte.

Sysex (`0xF0`, `0xF7`) and meta (`0xFF type`) events carry a variable-length size as in SMF.
`FF 2F` ends the stream.

Time runs at a fixed 120 ticks per second. Tempo meta events (`FF 51`) only feed the beat and bar
counters, because the delays already have the tempo baked in. SNG_0004, for example, states 545,454
µs per quarter (110 bpm), and its loop starts at tick 261, four quarters (261.8 ticks) at 120 Hz.

The parser converts each sequence into a list of events at absolute ticks, sorted by tick and
then stream order, with note-offs first within a tick.

### Miles controllers

The AIL 3.03 sequencer (controller handler at 0xa32b0, stream service at 0xa3c20) treats these
controller numbers specially:

| CC | Meaning |
|---|---|
| 108 | prefix callback: the application's callback returns the value for the next controller on this channel |
| 109 | sequence branch: jump to the `RBRN` entry with this id (calls `AIL_branch_index`) |
| 110 | channel lock: 64 and up takes a free physical channel for this sequence, below 64 releases it |
| 111 | channel lock protect: 64 and up protects, below 64 unprotects |
| 112, 113, 114 | voice protect, timbre protect, patch bank select; forwarded to the driver |
| 115 | indirect prefix: the next controller on this channel takes its value from an application array at this index |
| 116 | for-loop: push a loop with this count (0 = forever) and the stream position just after the event; at most 4 nest, a fifth is ignored |
| 117 | next-loop: below 64 ignored; otherwise take the innermost loop, decrement a non-zero count and jump back while it stays non-zero, always jump back for count 0 |
| 118 | clear the beat and bar counters |
| 119 | call the application's trigger callback with the value |

Older Miles documentation puts the sequence branch on controller 120. This build handles it on
109 and passes 120 through as an ordinary controller. 108 and 109 do not appear in that older
list.

The jump on a next-loop happens mid-tick. The events right after the for-loop play in the same
tick as the next-loop, and one pass of the loop lasts `tick(next) - tick(for)` ticks. Notes still
queued at the jump keep counting down. At `FF 2F`, Miles stops, or restarts from the top of `EVNT`
on the following tick if the sequence loop count allows.

### The note queue and the end (confirmed)

The sequence record holds a 32-slot note queue: channels at `+0x558` (−1 for a free slot), keys at
`+0x5d8`, ticks left at `+0x658` and the number in use at `+0x554`. A tick runs each time the
accumulator at `+0x58`, which gains the tempo at `+0x48` on every call of the service, reaches 100
(`0xa3c9f`); S2 leaves the tempo at the 100 `0xa5075` sets, so a tick runs on every call.

- At the top of a tick, before any stream event, the service walks the slots from the first,
  takes one from each count and sends a note-off with velocity 0 for each that is no longer
  positive (`0xa3cdf`). Note-offs due in the same tick therefore go out in slot order.
- A note-on takes the lowest free slot with its duration as read (`0xa411e`). A duration of 0
  ends at the next tick, as 1 does.
- With all 32 slots taken, a note-on stops the sequence and marks it done (`0xa414a`).
- `0xa5054` starts every sequence with a loop count of 1 at `+0x2c`. At `FF 2F` the count drops
  to 0 and the sequence stops (`0xa3e65`), so a song without a next-loop plays once.
- Stopping (`0xa5300`, through `0xa3740`) sends the note-off of every queued note, slot by slot,
  then frees the queue.

The parser types these as `Event::Miles` with a `MilesControl`.

### What the songs use

| Song | Length | Ticks | Notes | Loop start | Loop end | TIMB | Channels with notes |
|---|---|---|---|---|---|---|---|
| SNG_0001 | 58.2 s | 6,984 | 858 | 72 | 6,984 | 8 | 0-5, 9 |
| SNG_0002 | 62.9 s | 7,542 | 877 | 54 | 7,542 | 11 | 0-3, 9 |
| SNG_0003 | 79.7 s | 9,558 | 786 | 1,206 | 9,558 | 20 | 0-9 |
| SNG_0004 | 170.2 s | 20,421 | 1,769 | 261 | 20,421 | 9 | 0-6, 9 |
| SNG_0005 | 138.0 s | 16,560 | 1,569 | 216 | 16,560 | 14 | 0-2, 4-9 |
| SNG_0006 | 84.6 s | 10,147 | 1,188 | 67 | 10,147 | 12 | 0-4, 9 |
| SNG_0007 | 244.5 s | 29,340 | 2,219 | 6,120 | none | 5 | 0-7 |
| SNG_0008 | 122.0 s | 14,639 | 1,312 | 239 | 14,639 | 10 | 0-7, 9 |
| SNG_0009 | 131.0 s | 15,719 | 1,725 | 1,079 | 15,719 | 9 | 1-5, 7-10 |
| SNG_0010 | 130.8 s | 15,696 | 1,358 | 288 | 15,696 | 15 | 0-7, 9 |
| SNG_0011 | 91.2 s | 10,944 | 954 | 576 | 10,944 | 19 | 0-9 |
| SNG_0012 | 92.6 s | 11,108 | 1,218 | 1,234 | 11,108 | 19 | 0-7, 9 |
| SNG_0013 | 110.2 s | 13,230 | 1,683 | 270 | 13,230 | 9 | 0-1, 3-7, 9 |
| SNG_0014 | 177.8 s | 21,330 | 2,333 | 270 | none | 14 | 0, 5-9 |
| SOUND.LST | 365.8 s | 43,891 | 5,181 | 1,001 | 43,641 | 6 | 1-2, 4-8 |

Channels are zero-based; 9 is percussion. The loop controllers sit on channel 10 (channel 11 in
SNG_0009), which carries no notes.

- The only Miles controllers used are 116 and 117. Every song has one for-loop with count 0
  (forever). All but SNG_0007 and SNG_0014 close it with a next-loop of value 90 on the song's
  last tick. The `SOUND.LST` song closes it 250 ticks before its end, and its tail never plays
  while looping.
  SNG_0007 and SNG_0014 have no next-loop, so they play once to `FF 2F` unless the sequence loop
  count restarts them.
- Standard controllers: bank select 0 and 32, volume 7, pan 10, effects 91, 93 and 94, data
  increment 96 (SNG_0011), and reset-all-controllers 121.
- At most 12 notes wait for their note-off at once; Miles' per-sequence queue holds 32.
- Every `SNG_*.DAT` but SNG_0013 opens with a Yamaha XG System On sysex
  (`43 10 4C 00 00 7E 00`), and most carry further XG parameter changes and XG display-text
  messages holding a composer credit. SNG_0013 opens with a Roland GS reset
  (`41 10 42 12 40 00 7F 00 41`) instead. The `SOUND.LST` song has no sysex. The sequencer hands
  sysex to the driver. What the OPL drivers do with it is not checked yet.

### `TIMB`

Each entry names a (patch, bank) timbre the song needs loaded. Bank 0 holds the melodic patches
from the songs' program changes, including program changes on the percussion channel. Bank 127
holds one entry per percussion key the song plays on channel 9. Every entry in every song exists
in `SETT_SND.OPL`, and every program change and percussion key in the event streams is listed in
its song's `TIMB`. The songs never use controller 114, so program changes always select bank 0.

## SMF export

`Sequence::to_smf` writes a format 0 Standard MIDI File with 60 ticks per quarter and a single
500,000 µs tempo, so one SMF tick is one XMIDI tick (1/120 s). Durations become `0x8n` note-offs
with velocity 64. The original tempo events are dropped because the delays already include them.
Miles controllers 108..=119 stay as controller messages, which General MIDI leaves undefined. A
for-loop also gets a `loopStart` marker (`FF 06`) just before it, and a next-loop of 64 or more a
`loopEnd` marker, which some MIDI players use for looping. Sysex, other meta events and standard
controllers pass through, so a GS or XG synth gets the reset messages.

`Sequence::from_smf` reads format 0 and 1 files whose division counts ticks per quarter note, and
refuses format 2 and SMPTE time. It follows tempo events, turns every time into 1/120 s ticks
rounded to the nearest, and honours running status. A format 1 file's tracks merge by time, the
earlier track first within a tick. Controllers 108..=119 come back as Miles controls. Reading back
what `to_smf` wrote gives the sequence's events exactly, the markers and the one tempo event
aside, for all 15 shipped songs.

## Global Timbre Library

```
directory: entries of (patch u8, bank u8, offset u32 LE), ended by a bank byte of 0xFF
records:   at each offset, u16 LE length (counting itself), then the body
```

`SETT_SND` has 181 entries: bank 0 patches 0..127 (the General MIDI melodic set) and bank 127
patches 35..87 (the General MIDI percussion keys). The directory ends with `FF FF` at 0x43e. The
181 records follow in directory order, back to back, and the last ends exactly at the end of the
file. Every record is 14 bytes, a two-operator OPL2 voice:

| Byte | Field |
|---|---|
| 0-1 | length, 14 |
| 2 | transpose |
| 3-7 | modulator: registers `0x20`, `0x40`, `0x60`, `0x80`, `0xE0` |
| 8 | feedback and connection, register `0xC0` |
| 9-13 | carrier: registers `0x20`, `0x40`, `0x60`, `0x80`, `0xE0` |

The `0xC0` byte never sets bits 4-7 (the OPL3 stereo bits). The transpose byte is 0 for every
melodic timbre. For the percussion timbres it ranges over 24..84, which fits Miles' use of that
byte as the fixed key a rhythm-bank timbre plays at. The parser keeps the register bytes as they
are, names the fields above, and decodes the multiplier, total level, feedback and connection.
Records of any other length (four-operator OPL3 voices in other games) are kept as raw bytes.
