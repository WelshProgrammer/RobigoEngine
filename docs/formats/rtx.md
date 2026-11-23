# Mission scripts (`.RTX`)

Parser: `s2_formats::rtx` (`Script::parse`, `Keyword`, `Op`).

`DATA/MISSIONS/` holds 31 scripts, byte-identical in the base game and the Gold Edition:
`MIS_0000` to `MIS_0009` (the Roman campaign's ten chapters), `MIS_0100`, and `MIS_1000` to
`MIS_1019` (Unlimited Play maps). S2.EXE opens them through the format string
`data\missions\mis_%4.4d.rtx` (`0x170A98`).

This note covers syntax only. What each command does at run time is in
[missions.md](../re/missions.md).

## Layout

CP437 text with CR LF line ends and a single `0x1A` after the last line. Sections start with a
header line `[NAME]`. Every script has `[GLOBAL_MAP_SETTINGS]` followed by `[MAPCOMMANDS]`. The
first holds the map number, object set, AI players, free-play setup and mission text table. The
second holds start-up wares, people, buildings and animals, then numbered events. The nine campaign
scripts other than `MIS_0000` end with an empty `[ENDE]` section, which the game never reads.

Lines starting with `!` are commands: a keyword and a fixed number of integers. Lines starting
with `#` are comments, including four commented-out commands (`#!…`).

## How S2.EXE reads a script

The reader is a small generic script engine. Its error texts talk about groups, sets, labels and
strings, most of which RTX files do not use.

- **Loading** (`0x78BB3`): the whole file goes into memory with a CR LF prepended, so the first
  real line is read like any other.
- **Lines** (`0x78D49`): a line is the text between one LF and the next, minus the byte just
  before the LF (meant to be the CR, but dropped whatever it is). Text after the last LF is never
  read. The line is copied into a 256-byte buffer without a length check and handled as a C
  string, so a NUL ends it early. A line counter feeds the error messages. The prepended
  CR LF makes it report file line *n* as line *n* + 1.
- **Character classes**: a 128-entry table at `0x1FED94`, indexed with the byte's top bit masked
  off. There are three class bits: letters and `_`, digits, and "blank" (bytes `0x01` to
  `0x20`, plus `(`).
- **Line kinds** (`0x78DCE`): skip blank bytes, then look at the first other byte. NUL means an
  empty line, `!` a command, `"` a string line, `#` a comment, `[` a section header. Otherwise a
  digit or letter gives two more kinds that RTX scripts never act on, and anything else stops the
  game with "Undefined Symbol".
- **Sections** (`0x78E7F`): to run a section, the game reads from the top until a `[` line whose
  first identifier equals the wanted name (case-sensitive), failing with "Group not found" if
  there is none. It then interprets lines up to the next `[` line. The caller passes the name:
  `GLOBAL_MAP_SETTINGS` (`0x1FD428`, used at `0x5E723` and `0x5EBCE`) or `MAPCOMMANDS`
  (`0x170324`, used at `0x10968`). The closing `]` is never checked.
- **Keywords** (`0x7921B`, `0x78B7A`): the first identifier (`[A-Za-z_][A-Za-z0-9_]*`) on the
  line, looked up in the keyword table with a plain `strcmp`. An unknown keyword is skipped
  without complaint. If the line has no identifier at all, the buffer still holds the previous
  command's keyword and that one runs again.
- **Arguments** (`0x7912D`): each argument search starts where the previous one stopped, and the
  first starts at the beginning of the line. It skips to the next byte of digit class, negates
  the value if the byte just before is `-`, then accumulates digits as `value * 10 + byte - 0x30`
  in 32 bits, wrapping on overflow. Anything between numbers is skipped, so spaces, tabs, commas
  and words all separate arguments. Anything after the last argument is ignored. If no digit is
  left, the game stops with "SCRIPT ERROR: Parameter expected in line N!". The message handler,
  `0x78D16`, switches back to text mode and prints it.

The interpreter is `0x23EBE`, a `switch` on the keyword id through the jump table at `0x23E1A`.

## What the parser does differently

`Script::parse` reproduces all of the above, with these deliberate differences:

- It strips a CR before the LF only if there is one, instead of always dropping the last byte.
- It parses every section, not only the two the game asks for, and keeps them in file order.
  `Script::section` returns the first match, as the game would find it.
- Lines the game skips or survives are accepted and reported as quirks (below).
- A `!` line without an identifier is an error. The game would run the keyword left in its
  buffer from an earlier line again. Which identifiers feed that buffer is not yet known, so the
  parser does not guess.
- An unexpected first byte is an error in any section, not only the ones the game reads.
- Line numbers in errors and in `Command::line` count from 1 in the file.

Arguments are kept as the wrapped 32-bit values. The game narrows most of them to 8 or 16 bits.

## Quirks

The parsers accept what S2.EXE accepts. Where the game's handling of an oddity is not yet known,
the parser accepts the input and reports the oddity. Each top-level type has a `quirks()` method
listing what was unusual and where. The shipped files produce none, the Gold Edition's included,
and the real-data tests assert that. Input is still an error when its bytes cannot be decoded at
all, or when the game itself treats it as fatal. It is also an error when accepting it would let a
small file allocate without bound.

`Script::quirks` reports, with line numbers counted from 1:

| Quirk | What the parser does | Why |
|---|---|---|
| `UnknownKeyword` | the line is skipped | the game skips unknown keywords without complaint |
| `LongLine` | a line over 255 bytes is read whole | the game copies it into a 256-byte buffer unchecked and carries on |
| `CommandOutsideSection` | a `!` line before the first header, or after a nameless one, is skipped unread | the game only interprets lines after the header it searched for |
| `MissingSectionName` | a `[` line with no identifier starts no section, but ends the one before it | the game stops a section at any `[` line, and no name can match it |

"Parameter expected" (`Error::MissingArgument`) and "Undefined Symbol" (`Error::UnexpectedLine`)
stay errors, because the game stops on both. Missing arguments are only checked on commands the
game would reach.

## Keyword table

S2.EXE has a table of 42 rows of 68 bytes: a 64-byte NUL-padded name and a `u32` id. The ids run
1 to 41. `MET_ADD_AS_ACT_EVENT` appears twice, both times with id 26, which leaves 41 distinct
keywords. The table sits at `0x1F9854`.

The argument counts below are the number of reads in each `switch` case of the interpreter, and
match every line of the shipped scripts. "Uses" counts the commands in all 31 scripts. Argument
names follow the scripts, S2.EXE and [rtx2lua](https://github.com/kubaau/rtx2lua) (MIT,
by kubaau). `text` is the slot in the `MISS_nnn.ENG` table for an event's letter, -1 for none.
`text2` is a second word for the letter, -1 for none, which S2 keeps on the diary and never reads
([screens.md](../re/screens.md#the-diary-confirmed)). `event` and `next` are event ids, 255 for
none. A `?` marks a role that is not yet known.

| Id | Keyword | Arguments | Uses |
|---|---|---|---|
| 1 | `LOAD_MISSION_TEXTS` | `n` (loads `TXT/MISS_nnn.ENG`) | 10 |
| 2 | `SET_ACT_PLAYER` | `player` | 20 |
| 3 | `SET_HOUSE` | `building x y` | 35 |
| 4 | `ADD_WARE` | `ware amount` | 378 |
| 5 | `ADD_PEOPLE` | `job amount` | 839 |
| 6 | `ENABLE_HOUSE` | `building` | 713 |
| 7 | `ENABLE_ALL_HOUSES` | none | 6 |
| 8 | `DISABLE_HOUSE` | `building` | 2 |
| 9 | `ADD_ANIMAL` | `species amount x y` | 317 |
| 10 | `GLOBAL_ADD_COMPUTER_PLAYER` | `player character` | 17 |
| 11 | `GLOBAL_SET_MAP` | `map` | 31 |
| 12 | `GLOBAL_SET_BOBS` | `set` | 31 |
| 13 | `GLOBAL_SET_PATTERN` | `pattern` | 4 |
| 14 | `GLOBAL_SET_PALETTE` | `palette` | 4 |
| 15 | `GLOBAL_SET_MAPBOBS` | `set` | 4 |
| 16 | `GLOBAL_SET_HQ_BOBS` | `? ? ?` | 1 |
| 17 | `GLOBAL_SET_COMPUTER_BARRIER` | `radius x y` | 30 |
| 18 | `GLOBAL_SET_COMPUTER_ALLIANCE` | `player ally` | 25 |
| 19 | `FM_MAX_PLAYER` | `count` | 20 |
| 20 | `FM_MAX_HUMAN` | `count` | 20 |
| 21 | `FM_HQ_POS` | `x y` | 79 |
| 22 | `FM_CHARACTER` | `character` | 84 |
| 23 | `MET_POSITION_OCCUPIED` | `event next x y text text2 unused` | 15 |
| 24 | `MET_POSITION_EXPLORED` | `event next x y text text2 unused` | 9 |
| 25 | `MET_CLEAR_ACT_EVENTS` | none | 0 |
| 26 | `MET_ADD_AS_ACT_EVENT` | `event` | 63 |
| 27 | `MET_END_EVENT` | `event next times a b c d text text2` | 15 |
| 28 | `MET_HOUSE_AMOUNT` | `event next building amount text text2` | 10 |
| 29 | `MET_PEOPLE_AMOUNT` | `event next job amount text text2` | 0 |
| 30 | `MET_WARE_AMOUNT` | `event next ware amount text text2` | 1 |
| 31 | `MET_LAND_SIZE` | `event next size text text2` | 1 |
| 32 | `MET_CONTACT_TO_PLAYER` | `event next player text text2` | 8 |
| 33 | `MET_FOUND_GOLD` | `event next text text2` | 1 |
| 34 | `MET_FOUND_IRON` | `event next text text2` | 1 |
| 35 | `MET_FOUND_COAL` | `event next text text2` | 1 |
| 36 | `MET_FOUND_GRANITE` | `event next text text2` | 1 |
| 37 | `MET_FOUND_WATER` | `event next text text2` | 1 |
| 38 | `MET_HOUSE_ENABLING` | `event b1 b2 b3 b4 b5 b6` (255 = unused) | 19 |
| 39 | `MET_DIRECT_EVENT` | `event next text text2` | 10 |
| 40 | `MET_SET_FINAL_EVENT` | `event chapter chapter chapter` (255 = unused) | 10 |
| 41 | `MET_SET_MAP_ELEMENT` | `event x y element` | 10 |

`MET_END_EVENT`'s `a b c d` are events to activate (255 = unused). The two keywords no shipped
script uses are `MET_CLEAR_ACT_EVENTS` and `MET_PEOPLE_AMOUNT`; Robigo's test script for them is
described in [missions.md](../re/missions.md#events-no-chapter-uses).

In the shipped scripts every `GLOBAL_*`, `FM_*` and `LOAD_MISSION_TEXTS` command sits in
`[GLOBAL_MAP_SETTINGS]` and every other command in `[MAPCOMMANDS]`, but the interpreter accepts
any keyword in either section.

## Quirks in the shipped scripts

- 23 `FM_HQ_POS` lines carry a trailing remark such as `#3`. The game reads two numbers and
  ignores the rest, so the remark is harmless. A naive whitespace tokeniser sees three
  arguments.
- Arguments use commas as well as spaces and tabs, leading zeros (`04`, `09`), and `-1`.
- Nine scripts open with comment lines before the first header.
- Some comments are German and contain `0x81` and `0x84` (ü, ä).

## Run-time notes

[missions.md](../re/missions.md) covers what each keyword does. Three points bear on the syntax:

- `ADD_ANIMAL` runs into the `MET_CLEAR_ACT_EVENTS` case (`0x2431f`, a call to `0x31baf`), so
  every animal line also empties the active event list.
- `GLOBAL_SET_MAPBOBS` has no `case` in the interpreter, and `GLOBAL_SET_PALETTE` reads its number
  and drops it.
- `SET_HOUSE`, `ADD_WARE`, `ADD_PEOPLE`, `ADD_ANIMAL` and `MET_ADD_AS_ACT_EVENT` test the word at
  `0x1F78F0` first, which the game only ever sets to 0, so they always run.
