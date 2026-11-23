# Data, the base mod and mods

How Robigo gets from the player's copy of Settlers II to the pictures, sounds, texts and tables it
plays with. The original data is baked once into the base mod, and every other mod extends or
overrides it. Issue #87 tracks the work; music (#64) builds on it.

Today the game reads the original files directly through `s2-data`, and the tables read out of
`S2.EXE` live in the code as `ORIGINAL` consts. Each direct read stays until its asset type moves to
the bake.

## Why bake

The original formats suit a 1996 engine: sprites packed into `.LST` and `.IDX`/`.DAT` archives,
texts in CP437 string tables, music as XMIDI inside a sound archive, tables compiled into the
executable. A mod author would need Robigo's decoders to change any of them. Baking turns them into
files anyone can open: PNG, WAV, FLAC and TOML. A mod is then a folder of the same kinds of files.

The bake also lets two renderings of one asset sit side by side. The songs are rendered to audio
as S2's FM driver plays them on an OPL3, and through a sound font as General MIDI, so the player
picks a rendering in the options and the game only ever plays audio files.

Nothing baked enters the repository. The base mod is Blue Byte's data in another shape, so the game
makes it on the player's machine from their own copy and it stays there.

## The player's copy

The game bakes from the player's copy in a folder named `RETAIL`: beside the binary, or in the
preferences folder (`~/.local/share/robigo/RETAIL/`, `%APPDATA%\robigo\RETAIL\`) for an install
whose own folder cannot be written. The installer or the release archive ships the folder with the
note in `dist/RETAIL/RETAIL.txt`, and the player copies their game in by hand or the installer does
it for them. It takes:

- the files of the CD, an installed game or a GOG installation, which the folder is read as;
- or a CD image with its `.cue` sheet, or an `.iso`, read when the folder itself is not a game.

`RETAIL/PATCH` holds the official v1.51 patch for a copy older than v1.51, such as the CD's
v1.03, and is layered over the copy when it holds anything besides its note
(`dist/RETAIL/PATCH/PATCH.txt`). The Gold Edition and GOG's copy are v1.51 already. The game only
reads these folders.

A start with no copy in either place plays from the base mod baked last as it is, if this
version's baker made it. With a copy, the bake is checked against it and made again when the copy
or the baker changed. `robigo play DATA` still takes a copy from anywhere, for development.

### The first start

With no copy and no base mod of this baker's, the game sets itself up before the title:

1. A dialog asks for the folder the game is in, and the system's folder dialog picks it: an
   installed game, a GOG installation, or a folder that holds a CD image. A folder with no
   Settlers 2 in it is asked for again, as is one that holds the v1.51 patch alone, which has
   an `S2.EXE` and none of the game's `DATA/RESOURCE.DAT`.
2. For a copy older than v1.51, a second dialog asks for the folder the official v1.51 patch
   was unpacked into, and says the archive has to be unpacked first. A folder that does not
   bring the copy to v1.51 is asked for again.
3. The game offers to keep a copy in the preferences folder's `RETAIL`, naming its size, the
   patch's files going into `RETAIL/PATCH`. An
   installed game is copied whole; of a folder that holds a CD image, only the sheets, images
   and audio tracks at its top are. A later start then finds the copy and bakes from it again
   after an update, with no questions. Declined, the bake reads the picked folder this once.
4. The base mod is baked, the sound font fetched and the General MIDI songs rendered. The window
   shows a line of text and a progress bar for each, drawn in a small font Robigo carries itself,
   since none of the game's art is there yet.

A system with no folder dialog gets the dialog that says where to copy the game by hand. A copy
that cannot be baked, one older than v1.51 with no patch say, ends in a dialog that gives the
reason and names the `PATCH` folder.

## Where things live

Mods live in the platform's data directory, beside the settings file:
`~/.local/share/robigo/mods/` on Linux and `%APPDATA%\robigo\mods\` on Windows
(`s2_platform::preferences_dir`). The base mod is `mods/base/`; every other folder in `mods/` is a
mod the player installed.

```text
mods/
  base/
    mod.toml
    bake.toml            what it was baked from and by which baker
    sprites/             one folder an archive, named after its path below DATA
      map_0_z/700.png    one PNG an item, named after its slot
      map_0_z.toml       one row a slot: kind and origin
      mbob/rom_z/250.png
      mbob/rom_z.toml
      io/66.png
      io.toml
    interface/           one folder a named .IDX/.DAT pair, named after the pair
      io/iocon_045.png   one PNG a picture, named after its entry
      io.toml            one row an entry, in the index's order
      resource/handa.png
      resource.toml
    fonts/
      font09/65.png      one PNG a glyph, named after its character code
      font09.toml        line_height, then one row a glyph
    terrain/
      greenland/palette.toml
      greenland/texture.png
      greenland/shading.png
    pictures/
      pics/setup010.png  one indexed PNG a full-screen picture, carrying its own palette
      pics2/credit00.png
    bobs/
      jobs/bodies/0.png  the 96 bodies, then the overlays, one PNG a part
      jobs/bodies.toml
      jobs/overlays/0.png
      jobs/overlays.toml
      jobs/links.toml    one row an item
    sounds/
      effects/61.wav     one sample an effect id
    music/
      opl/1.flac         one rendering a song, by the number S2 plays it as, tagged with its loop
      score/1.xmi        the song's XMIDI file as it is, for rendering through a sound font
    texts/
      en/txt/ongame.toml slots, then one row a filled slot
      en/online/help1000.txt
      de/txt/ongame.toml the German texts, from a copy that has them
      readme.txt
    tables/
      effects.toml       one row an effect id
      messages.toml      one row a message id
      terrain/types.toml
      terrain/textures.toml
      terrain/borders.toml
      buildings/romans.toml  doors and what buildings draw over themselves
      yards.toml
      objects.toml
      styles.toml        one row a window style
      pictures.toml      the interface item each picture draws
    sim/
      buildings.toml     one row a building type
      economy.toml
      military.toml
      cargo.toml
      missions.toml      one row a mission with rules besides its script
      waypoints/romans.toml
      fights/romans/walked_up.toml
      fights/deaths.toml
      animations.toml    one row a record of ROM_ANIM.DAT
    maps/maps3/omap00.wld  each map folder's files, copied as they are
    missions/mis_0000.rtx  each mission script of DATA/MISSIONS, copied as it is
    video/intro.smk      each .SMK file of VIDEO, copied as it is
    cd/2.flac            one FLAC an audio track, by its number on the disc, when the data is an image
  gold/                  the Gold Edition's extras, when the data has them
    mod.toml
    sprites/map_2_z/700.png
    interface/io.toml
  some-mod/
    mod.toml
    sprites/mbob/rom_z/250.png
    sim/buildings.toml
```

## What each original file becomes

The rule for every format: the baked form must reproduce the original's output exactly. A base
mod made from S2 draws the same pixels and plays the same samples as reading S2's files
directly, and a test proves it (below).

| Original | Baked | Kept exactly |
|---|---|---|
| `.LST`, `.IDX`/`.DAT` bitmaps | one 8-bit indexed PNG per item, carrying the landscape palette the archive is drawn with; the first palette index the item leaves unused is fully transparent | every pixel index |
| item origins and kinds | one TOML per archive, a row per slot: `kind` (`colour`, `player` or `shadow`) and `origin` | origin and kind |
| player-colour items | indices 128 to 131 are the owner's four shades; a fixed colour in that range refuses to bake | which pixels take the player's ramp |
| shadow items | indexed PNG of the coverage mask, index 1 where the shadow table darkens | which pixels the table reaches |
| landscape palettes (`PAL5.BBM` to `PAL7.BBM`) | `terrain/<landscape>/palette.toml`, a row an index, each `[red, green, blue]` | the file's 8-bit colours, which the game turns into DAC values as before |
| terrain sheets (`TEX5.LBM` to `TEX7.LBM`) | `terrain/<landscape>/texture.png`, 256 by 256, indexed | every texel's palette index |
| shading tables (`GOU5.DAT` to `GOU7.DAT`) | `terrain/<landscape>/shading.png`, 256 by 256, indexed: a row a light level, a column a palette index | every entry |
| full-screen pictures (`GFX/PICS`, `GFX/PICS2` and `GFX/PICS/MISSION`) | `pictures/<folder>/<name>.png`, indexed, carrying the picture's own palette | every pixel index and the palette's 8-bit colours |
| `.BOB` settler parts | `bodies/` and `overlays/`, a player-colour PNG a part with a TOML of rows, and `links.toml`, a row an item holding its 96 `overlay` numbers and `dy` offsets, frame by body type by direction | each part's pixels and origin, every link; overlays no link names are left out |
| `.IDX`/`.DAT` named pairs | one indexed PNG per picture, named after its entry, with a TOML of rows keyed by name in the index's order | every pixel index, origin and name |
| fonts in `RESOURCE` | PNG per glyph, its player shades as indices 128 to 131, plus a TOML of `line_height` and the glyph rows | line height, each glyph's runs; a glyph advances by its own width |
| `SOUND.LST` effects | WAV, unsigned 8-bit mono at the stored rate | every sample |
| XMIDI songs (`SNG_0001.DAT` to `SNG_0014.DAT`) with the FM bank `SETT_SND.OPL` | `music/opl/1.flac` to `music/opl/14.flac`, rendered at 49,716 Hz | every register write S2's driver makes, in order |
| string tables (`DATA/TXT`, `DATA/TXT3`) | `texts/en/txt/ongame.toml`: `slots`, the slot count, then a row a filled slot holding its string in UTF-8, `@` line breaks kept | every slot's bytes once turned back into code page 437 |
| pages (`DATA/TXT2`, `DATA/ONLINE`, `README.TXT`) | `texts/en/online/help1000.txt` and so on, UTF-8 | every byte, bare CR and CR LF line ends and the end-of-file marker included, which the reading windows treat as S2 does |
| maps, `.SMK` videos | copied as they are | byte for byte |
| `ROM_ANIM.DAT` | `sim/animations.toml`, a row a record keyed by its number from 0, each `[kind, argument, column offset, row offset, operation]` | the file byte for byte, so a replay's `script` hash is unchanged |
| CD tracks | `cd/2.flac` to `cd/9.flac`, ripped from a `.bin/.cue` or the GOG image | every sample |
| `S2.EXE` tables (`ORIGINAL` consts) | TOML under `sim/` for the tables the sim reads, `tables/` for the rest | every value |

The game draws text with its own fonts, a glyph a code page 437 byte, so it turns baked text
back into code page 437 as it loads it. A character code page 437 lacks is refused with the file,
the row and the character: "texts/en/txt/ongame.toml, row 120: code page 437 has no '❄'".

### Languages

A text sits in a folder named after its language. The bake puts S2's `.ENG` files in `texts/en`
and its `.GER` files in `texts/de`. Every copy of the game has the English ones; a game installed
from the Gold Edition CD has the German ones as well. `README.TXT` has no language and stays at
`texts/readme.txt`.

The `language` setting names the folder the game reads, `en` unless the player changes it. A
string table is read slot by slot: the language's row where it has one, the English row where it
does not. A page is read from the language's folder when the file is there and from `texts/en`
otherwise. A mod adds a language by shipping `texts/<language>/`, and translates its own strings
by shipping the same file in more than one folder.

The game's fonts cover code page 437, so a language whose letters fall outside it also needs fonts
and a way to map its text onto them, which nothing does yet. Where `S2.EXE` branches on its
language word for something other than a file's ending
([startup.md](../re/startup.md#the-language-word-0x200c30-confirmed)), Robigo still follows the
English game.

The `S2.EXE` tables are the one part not read from the player's files. They come from Robigo's own
code, where the RE put them, so the baker writes them out without needing the executable.

Maps and videos stay as they are because mod tools for them already use the original formats, and
nothing in them is awkward to read.

## Mods

A mod is a folder with a `mod.toml`:

```toml
id = "bigger-storehouses"
name = "Bigger storehouses"
description = "Storehouses hold twice as many wares."
version = "1.2.0"
Robigo = "0.9"
depends = ["base"]
```

`robigo` names the release the mod was written for: `0.9` fits every 0.9 release and `0.9.3` that
one alone. A mod written for another release still loads, and the game logs a warning naming both.
The baked mods leave it out, since a new baker bakes them again. `description` and `robigo` are
optional.

`depends` lists the mods it builds on; the base mod is always first. The player's mod list in the
settings decides the order of the rest, and a mod loads after everything it depends on.

Two ways a mod changes what is below it:

- **Files replace files by path.** `sprites/mbob/rom_z/250.png` in a mod replaces the base mod's
  picture of that name. A path the base mod lacks adds a new item.
- **Tables merge by key.** A table file holds keyed rows (`[headquarters]`, `[woodcutter]`), and a
  mod's file changes only the rows and fields it names. Changing one building type's capacity is a
  three-line file. A row can be removed by setting `removed = true`.

Once the game merges the stack, each table goes through its `new()`, which refuses a row that
cannot work (a building type with no group, a sound with no sample). The game names the mod that
failed, with the row and the reason.

## The Gold Edition's extras

The Gold Edition is the base game plus extras, and the extras bake into a second mod, `gold`
("Settlers 2 - Gold"), that depends on `base` ("Settlers 2"). Data without them, such as S2 installed from the original CD, bakes no
`gold` mod. `s2_mods::is_gold` lists what goes into `gold`, by data path. The list comes from
comparing the original CD and the official patch with a Gold Edition install:

| Extra | Files |
|---|---|
| World Campaign | `DATA/MAPS2` (nine maps), `DATA/TXT3`, `GFX/PICS/MISSION`, `GFX/PICS/WORLD.LBM`, `GFX/PICS/WORLDMSK.LBM` |
| winter objects and buildings | `DATA/MAP_2_Y.LST`, `DATA/MAP_2_Z.LST`, the ten `DATA/MBOB/W*.LST` |
| map editor | `DATA/IO/EDITIO`, `DATA/EDITRES`, `DATA/EDITBOB.LST`, `DATA/MAP00.LST` to `MAP02.LST`, `DATA/MAPBOBS*.LST` |
| new maps | `DATA/MAPS4` (twelve maps) |
| interface items | `iobob_03` and `iobob_04`, added to `DATA/IO/IO` as rows of `gold/interface/io.toml` |
| other | the four `DATA/MBOB/*_BOBS.LST`, eight `GFX/PICS/SETUP*.LBM` screens |

The official patch adds the winter terrain (`TEX7.LBM`, `PAL7.BBM`, `GOU7.DAT`) and no winter
objects, so the base game draws winter ground with nothing on it. Winter counts as a Gold extra.

The Gold Edition also changes 13 files the base game has, among them `ONGAME.ENG`, `IO.IDX` and
`IO.DAT` (`iobob_01` and `iobob_02`), `CREDITS.LST`, `MISS203.WLD` and `OMAP00.WLD`. A Gold
Edition install has no base-game copy of them, so its base mod bakes the Gold Edition's copies, and
leaving `gold` out keeps them. Shipping the difference would ship Blue Byte data.

The base game's `CREDITS.LST` is the one sprite archive the bake changes. Six of its portraits
are drawn for another palette's ramp of greys and show in blotches of sepia in S2
([screens.md](../re/screens.md#the-credits-confirmed)). The bake draws them in the nearest colours of the
palette the credits show in, which is what the Gold Edition's copy holds.

`gold` loads first after `base` whenever it is baked. The settings' `switched_off` list leaves any
installed mod out by id, and `gold` in it shows the game as a player without the Gold Edition sees
it; `robigo view --without gold` does the same for one run. `base` and `gold` are the baker's ids,
and a bake replaces both folders.

Without `gold`:

- the campaign screen offers the Roman campaign (`DATA/MAPS/MISS200` to `MISS209`) and not the
  World Campaign;
- a map whose header names the winter landscape cannot be started, and the free-play list shows it
  disabled;
- the `DATA/MAPS4` maps are not listed, and the map editor is not offered;
- the World Campaign's and the new maps are in no loaded mod, so the game cannot open them.

The game has no campaign screen or map list yet, so the first three rules wait for them.

Two things can still ask for an extra the game does not have, and both fail with a message that
names the mod: an original savegame (`SAVE/WORLDnnn.DAT`) and a map file from outside the mods,
when their landscape is winter. "the winter landscape requires the 'Settlers 2 - Gold' mod
enabled (it comes with the Gold Edition)".

## The sprite atlas

The base mod holds thousands of small PNGs, and opening and decoding each at every start would slow
it down. After merging the stack, the game packs every sprite into atlas pages (`etagere`, already in the plan) and caches the pages
with a hash of the mod list and each mod's version. The next start loads the cache. A change to
the mod list or to any mod's files rebuilds it, which is what Factorio does.

The renderer draws from the atlas and never from the PNGs, so a mod's picture costs nothing extra
at run time.

## Sound effects

`tables/effects.toml` has a row an effect id: `volume`, `jitter`, `rate` and an optional
`playback`. The game mixes at 44,100 Hz, and each sound plays one of two ways:

- `original`: an unsigned 8-bit mono WAV at the row's rate, moved by the jitter, stepped to
  11,025 Hz by the nearest byte as S2 does. The original sounds add up into one 11,025 Hz stream,
  which is what S2 sends the Sound Blaster frame for frame (`s2-audio`'s real-data test checks
  it against recordings), and a fixed windowed-sinc filter takes that stream to 44,100 Hz.
- `smooth`: a WAV, Ogg Vorbis or FLAC file at any rate, 8 or 16 bits, mono or stereo, widened to
  16-bit stereo on load. It plays from its own rate through a cubic interpolation straight to
  44,100 Hz, moved by the jitter in proportion: a row with `rate = 11000` that jitters to 11,550
  plays a 44.1 kHz file at 46,305 Hz. The row's `rate` sets no pitch of its own on this path.

A row without `playback` plays the base mod's samples the original way and any other mod's
samples smooth, so an HD sounds mod only needs to drop its files in `sounds/effects/` as
`<id>.wav`, `<id>.ogg` or `<id>.flac`. A mod that sets `playback = "original"` for a sample that
is not unsigned 8-bit mono WAV is refused with the file. Both paths share the volume, the master
and the scale: a 16-bit sample at full scale is as loud as an 8-bit one.

## Music

The game plays no MIDI. The bake renders each of the 14 songs the MIDI player offers into
`music/opl/<n>.flac`: the song's first sequence, the one S2 plays, goes through the Miles
sequencer and the OPL3 driver S2 ships into an emulated OPL3, Nuked-OPL3, at the chip's own
49,716 Hz. The driver makes every register write S2 makes for each song, in the same order,
which `s2-audio`'s real-data test checks against the OPL register writes recorded from S2
playing each song under DOSBox-X.

Twelve songs loop forever. The chip's envelope timer and vibrato never repeat, so no two passes of
a loop come out alike, and a file cannot simply hold one pass. The bake renders three passes and
keeps the song up to a seam in the third: one pass of frames, from the first point where the second
and third passes agree to within 16 sample units over 256 frames, repeats from there. The file's
`LOOPSTART` and `LOOPLENGTH` tags name the frames, so going round the loop sounds as going on
would. Where the passes never agree that closely, the last 2,048 frames of the repeat are blended
evenly into the 2,048 before its start, so its last frame leads into its first as the frame before
the start does; three of the OPL3's loops need that, none of them more than 69 sample units out
before the blend. Songs 7 and 14 play once, ring out and end, as in S2. The 14 files come to about
210 MB.

The game streams the song the jukebox asks for and goes round its loop by the tags. The game
menu's music volume scales the song; in S2 it scales each note's level on the chip, which a gain
over the rendering only approaches.

A mod replaces a song with its own `music/opl/<n>.flac`, `.ogg` or `.wav`, tagged the same way to
loop.

### General MIDI

With the setting `"music": "general-midi"` the songs play from `music/midi/<n>.flac` instead: the
same sequence through a General MIDI synth (rustysynth) playing a sound font, at 44,100 Hz, with
every channel at volume and expression 127, pan 64, the bend centred and a bend range of 2 before
the song starts. The synth's chorus and vibrato never repeat either, so most of these loops end in
a blend. Both renderings are made at the start, whichever plays, so switching between them never
waits. The General MIDI rendering is a step of its own after the bake, recorded in
`music/midi/render.toml` with the sound font it used, so changing the sound font renders the 14
songs again, about 10 seconds, and leaves the rest of the base mod alone. It renders from
`music/score/<n>.xmi`, the songs' XMIDI files the bake copies into the base mod, so it needs no
game data. A song the General MIDI
rendering lacks, after a sound font that could not be fetched say, plays from the OPL3 one.

The setting `"sound_font"` picks the sound font, and the options window lists the two Robigo
offers, those the loaded mods ship and a file the settings named at the start. The `"music"` setting and the options
pick the rendering that plays, `opl3` or `general-midi`; a change starts the playing song again
from the other folder.

| Setting | Sound font | Licence | Size |
|---|---|---|---|
| `generaluser-gs` (the default) | GeneralUser GS 2.0.3 by S. Christian Collins | GeneralUser GS licence v2.0 | 32 MB |
| `musescore-general` | MuseScore General 0.2, after Frank Wen's FluidR3 | MIT | 216 MB |
| a path | the player's own SF2 file | the player's | |
| `mod:<id>` | the `music/soundfont.sf2` the loaded mod `<id>` ships | the mod's | |

Robigo keeps the two it offers as GitHub release assets, each in a release of its own beside its
licence, and fetches one the first time it is wanted into `soundfonts/` beside the mods folder. A fetched file is used only
when its SHA-256 digest matches the one the code holds. The record names an offered sound font by
that digest and a file of the player's by its path, size and time of change, so a changed file
renders again.

GeneralUser GS ships with the game and is never asked about. Any other offered sound font that is
not in `soundfonts/` at its size, and has no rendering made with it, is asked about at the start
in the system's own dialog, which gives its size. Download fetches it, with the per-cent in the
window's title, then renders the songs and carries on loading. Not now plays the songs with
GeneralUser GS and writes that to the settings file, so the next start does not ask again. A
download that fails says why in a second dialog, where Try again fetches it once more and Not now
does what it does in the first.
Picking such a sound font on the options window opens a notice that says the next start asks.

A mod offers a sound font by shipping `music/soundfont.sf2`. The options window lists it by the
mod's name, after the two Robigo offers, and nothing plays through it until the player picks it.
The songs are rendered once, with the sound font the setting names, so a mod's sound font that is
not picked costs no rendering. A start that finds the setting naming a mod that is no longer
loaded, or one without a sound font, plays the songs with GeneralUser GS and writes that to the
settings file.

The `SOUND.LST` archive also holds an XMIDI song in slot 0. The song player reads only the `SNG`
files ([music.md](../re/music.md#song-files-confirmed)) and the effects start at slot 51, so
nothing plays it and the bake leaves it out.

## CD audio

The disc's audio tracks are 2 to 9; track 1 holds the game's files. When the data is a disc
image, a `.bin/.cue` pair or GOG's `.gog/.ins`, the bake rips each audio track from its
`INDEX 01` to where the next track starts, less any pregap the sheet marks, into
`cd/<track>.flac`, 44,100 Hz 16-bit stereo, and the bake test decodes every file back to the
image's samples. GOG's image holds the same audio as the
retail disc, starting 1,763 to 1,775 frames later depending on the track, and the bake keeps
whichever the player has as it is. An installed copy has no CD audio, so its bake has no `cd/`.
A player's own tracks go in a mod as `cd/<track>.flac`, `.ogg` or `.wav`.

The game plays the tracks the loaded mods hold, read as they play, and the CD player lists them.
Without mods it reads the image's raw audio. A track at 44,100 Hz reaches the mix sample for
sample; one at another rate goes through the cubic the smooth effects use. The disc adds in at its
own level, as a CD drive's audio reaches a Sound Blaster's mixer: S2 sets the music volume on the
MIDI driver alone, so the game menu's music volume leaves the disc as it is.

Ripping is most of a bake's time: the eight tracks take about three seconds on 24 threads, one
task a track, with every bake job its own task so the tracks never queue on one thread. Reading a
physical disc is not supported. A drive seeks too slowly for tracks read side by side, so a rip
from one would read the tracks one after another and only encode them in parallel.

## The sim's tables

The sim reads seven tables, and the animation script, from `sim/`:

| File | Holds |
|---|---|
| `buildings.toml` | a row a building type: `category`, graphics `group`, `worker`, `boards`, `stones`, `size`, `levelled`, `output`, `duration`, `rated`, `stoppable`, `sight`, `strength`, `soldiers`, and an `inputs` table of the wares a work cycle uses with how many of each the building stocks, in input slot order |
| `economy.toml` | `construction`, a site's weight for boards and stones; `type_weights` by building type; the metalworks' `tools` by ware; `transport`, each group's place in the list from 0; and the `distribution` pages in window order, each `wares` and `consumers` |
| `military.toml` | the settings a game starts with: `recruiting`, `rank`, `defence`, `attack`, `occupation_centre`, `occupation_middle`, `occupation_border` |
| `cargo.toml` | how much ships weigh each ware, under `wares`, and each job, under `people` |
| `missions.toml` | a row a campaign mission number: `computers_never_build`, the building types its computer players never order. The original bans the catapult in mission 1, the second chapter |
| `waypoints/<nation>.toml` | a row a building type: the builder's `first` waypoint and the six `x` and `y` offsets |
| `fights/<nation>/<side>.toml` | a row a rank: the `attack`, `block`, both `dodge`, `hit` and `ready` animations and the `hit_offset` given to the opponent; the sides are `walked_up` and `waited` |
| `fights/deaths.toml` | the death animation of each side |

Rows and keys name things in snake case: building types (`woodcutter`, `pig_farm`,
`lookout_tower`), wares (`iron_ore`, `rod_and_line`), jobs (`pig_breeder`, `private_first_class`),
nations (`romans`, `vikings`, `japanese`, `africans`) and transport groups (`wood`, `boards`,
`stones`, `food`, `water`, `beer`, `coal`, `iron_ore`, `gold`, `iron`, `coins`, `tools`, `weapons`,
`boats`). The full lists are `BuildingKind::NAMES`, `WareKind::NAMES`, `Job::NAMES` and
`TRANSPORT_GROUP_NAMES` in `s2-systems`. A distribution page names building sites as
`construction`.

A mod's row merges field by field, so `[storehouse]` with `boards = 5` changes the storehouse's
boards and nothing else. A value that is itself a table or a list, such as a building's `inputs` or
a page's `consumers`, is replaced whole. Every table needs a row for every building type, ware or
job it covers; a name the game does not have, a field it does not know or a value out of range is
refused with the file and the row.

A mod can change what the original kinds of building, ware and job do, but not add new kinds: the
sprites, the interface and the sim all assume S2's set.

## The renderer's and interface's tables

`tables/` holds the tables that only change what the player sees and hears, so a mod can change
them without touching a replay's `tables` hash:

| File | Holds |
|---|---|
| `terrain/types.toml` | a row a terrain type, by number: its border `class`, its `texture` entry and the border `strip` it paints into its neighbours in greenland, wasteland and winter |
| `terrain/textures.toml` | a row a texture entry, by number: the six `uv` corners in 8.8 texels, whether it is drawn `flat`, and the `colour` the map screen gives it |
| `terrain/borders.toml` | a row a landscape: 14 strings of 14 `x` or `.`, whether a triangle of the row's class takes a border from a neighbour of the column's |
| `buildings/<nation>.toml` | a row a building type: its `door`, where its small `flags` and the `stopped` sign go, and its `work` glow, `smoke` and military `banner` where it has them, each `[x, y]` from the building's position |
| `yards.toml` | a row a nation: where its pig farm's `pigs`, donkey breeder's `donkeys` and the `harbour`'s expedition boards, stones and builder go |
| `objects.toml` | the eight `flag_slots` a flag draws waiting wares in and the six `ship_pennants`, by direction |
| `messages.toml` | a row a message id, by number: the `category` list it also goes on and the `effect` it plays |
| `styles.toml` | a row a window style, 0 to 6: the `background`, `alternate` and `frame` items it draws from, its `font` (`small`, `medium` or `large`) and its text's `colour` and `other_colour` |
| `pictures.toml` | the interface item each picture draws, keyed by the picture's name: `confirm = "io014"` |

Terrain types, texture entries, message ids and window styles are keyed by the numbers `S2.EXE`
uses. A picture's name is its `Picture` constant in snake case; the runs of tool, military and
statistic buttons are named after what they stand for, such as `tool_tongs_more` and
`statistic_fights`. The full list is `s2_ui::picture::NAMES`. The skin keeps seven styles, so
`styles.toml` needs all seven rows.

## Baking

The game bakes on its first launch, and again when the source data or the baker changes. `robigo
bake DATA [--patch DIR]` does the same on demand. Both show a progress bar over the files written,
which rayon writes in parallel.

Only v1.51 and the Gold Edition bake. The bake refuses data from the original CD until the official
patch is layered over it, and the game then reads the original files.

`bake.toml` records what the base mod came from: the data set's edition and release
(`s2_data::Edition`), the 64-bit content hash of every source file (the hash a replay header uses,
`s2_sim::replay::content_hash`), and the baker's version. The videos and the disc image are copied
or ripped as they are, so the record holds a hash of each video's size and of the image's track
table in place of tens of megabytes, which keeps the check at start near ten milliseconds. At start
the game compares that record with the data set it was given and re-bakes when they differ.

The baker decodes the source files with the parsers in `s2-formats` and the asset loaders in
`s2-assets`, and writes the formats above.

## Determinism and replays

A replay's header has a `mods` line: the ids of the mods the game was played with, in load order.
Every save holds its replay, so every save names its mods. A save or replay loads only with the
same mods in the same order, since a later mod's rows win the merge. Versions are not compared:
a mod updated since the save loads, and there are no migrations. Otherwise the game refuses with
the mods by name:

- "Can't load this save, the mods it needs are not enabled: Settlers 2 - Gold"
- "Can't load this save, it was made without: Bigger storehouses"
- "Can't load this save, its mods were loaded in another order: Settlers 2 - Gold, Settlers 2"

A replay without the line, such as one written by hand, plays with any mods.

The header's `tables` line is a hash of the merged sim tables, taken over the TOML the game would
write for them, so it changes when a table's values do and not when a mod only rewrites a file's
layout. A mod update that changes pictures, sounds or texts leaves it alone. A replay whose hash
differs is refused: replaying the same commands by other rules plays a different game. A save
whose hash differs loads its state as saved, with a warning that the rules have changed and the
game may be damaged, and the player checks it before overwriting the save.

A save holds the building types it was made with beside its state. On such a load `Game::repair`
compares them with the game's own and takes out of the state what no longer belongs. The log
lists each thing taken, and a "Save migrated" window lists what the player's own buildings lost,
building by building:

- A building whose type is gone from the table, or is of another category, is demolished.
- A stopped workplace whose type can no longer be stopped works again.
- A workplace whose type has another worker loses the old one, who leaves as a lost settler and
  walks to a storehouse. Every order at its flag goes and the new worker is ordered.
- A finished workplace or military building loses the stock of an input slot whose ware changed,
  and any stock above the slot's capacity. The wares come off the owner's count.
- Its orders for wares it no longer takes or has no room for are cancelled. A ware already on its
  way is stranded as one for a burnt building is, and goes to a storehouse.
- A military building keeps as many soldiers as its type holds, and at least one. The rest walk
  home, and with no storehouse by road they are removed, weakest first.
- A site keeps as many boards and stones as its type costs, and its orders past that are
  cancelled. A site whose cost has risen orders the difference.

A state repaired by the tables it was made with comes back unchanged. A save from before the
building types were stored is repaired as far as the state alone shows: stock above capacity,
orders and soldiers.

A save holding a building whose type has changed size is refused, naming the type: the ground
it stands on and what was allowed round it were settled by its size. A save holding a finished
building whose type has changed strength is refused too, unless it is a military building nobody
has occupied: its land was claimed by the old reach, and demolishing it would release by the
new. Nothing in the state
depends on the other tables. The waypoints and fight animations are fixed in count, and a
builder walks to wherever its next waypoint now is. The military and economy tables hold the
defaults a new game starts from, and a saved game keeps the player's own settings. The cargo
weights are read each time ships weigh their work.

When the save is also from an older state format, its
state can't be read and replaying it would use the new rules, so it is refused.

The animation script has its own `script` hash, of `ROM_ANIM.DAT`'s bytes. The base mod rebuilds
the file byte for byte, so replays recorded before the bake still play.

The base mod's tables hold exactly the `ORIGINAL` values, so a replay made with the base mod alone
matches one made before the tables moved.

## Proving the base mod matches

For each asset type, a test bakes the real data (when `S2_DATA_PLAIN` points at it), loads the
base mod, and compares what it gives the game with what the direct read gives: the same sprite
pixels and origins, the same samples, the same strings, the same table rows. The test stores no
Blue Byte data; it compares two in-memory results.

## Order of work

1. This design, and a decision row in the plan.
2. One asset type end to end: the sound effects. `SOUND.LST` becomes WAVs, `s2-audio`'s
   `Effects::ORIGINAL` becomes `tables/effects.toml`, and the game plays effects from the mod stack.
   That covers a baked asset, a baked table, the merge, the loader and the proof test in one small
   piece, and leads straight into #64.
3. The rest of the assets, one archive family at a time, each with its proof test.
4. The sprite atlas, once the sprites come from mods.
5. The sim tables, with the replay header change.
6. Music (#64): the songs rendered at the bake, as the OPL3 and as a sound font plays them, with
   the rendering picked in the options.

## Open

- How a mod declares that it changes the sim, for multiplayer later: probably by whether it
  touches `sim/` at all.
