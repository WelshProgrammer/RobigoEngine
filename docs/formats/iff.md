# IFF pictures and palettes (`.LBM`, `.BBM`)

Parser: `s2_formats::iff` (`parse`, `read_palette`).

Every picture, terrain texture sheet and palette in both editions is a Deluxe Paint IFF file in
the PC-specific `FORM PBM ` variant. The parser also reads planar `FORM ILBM`, which no shipped
file uses.

## Container

Standard EA IFF 85: `FORM`, a big-endian `u32` length, the form type, then chunks. Each chunk is
a four-byte id, a big-endian `u32` length and the data, followed by one zero byte when the length
is odd. In every shipped file the `FORM` length is exactly the file length minus 8.

The parsers accept what S2.EXE accepts. Where S2.EXE's handling of an oddity is not yet known,
the parser accepts the input and reports the oddity. Each top-level type has a `quirks()` method
listing every oddity and where it was found. The shipped files produce none, the Gold Edition's
included, and the real-data tests assert that. Input is an error when its bytes cannot be decoded,
when the game itself treats it as fatal, or when accepting it would let a small file allocate
without bound.

`Picture::quirks` reports one quirk, `FormLength`: a `FORM` length longer than the file is
clamped to the file and the chunks are read up to its end. A chunk that runs past the end of the
clamped form is still an error, as is a missing `BMHD` or `BODY`.

Chunks in shipped files, in file order:

| Chunk | Meaning | Parser |
|---|---|---|
| `BMHD` | 20-byte bitmap header | `Picture::header` |
| `CMAP` | 256 RGB triples, 768 bytes | `Picture::palette` |
| `DPPS` | Deluxe Paint perspective state, 110 bytes | skipped, id recorded |
| `GRAB` | brush hotspot, 4 bytes (only some `.BBM`) | `Picture::hotspot` |
| `CRNG` | colour range, 8 bytes, 16 per file | `Picture::ranges` |
| `TINY` | Deluxe Paint thumbnail | skipped, id recorded |
| `BODY` | pixels | `Picture::image` |

Chunks of odd length occur (`TINY` and `BODY` in many files) and are padded as the standard
says. The parser tolerates a missing pad byte on the final chunk.

## `BMHD`

Big-endian: width, height (`u16`); x, y (`i16`); planes, masking, compression, pad (`u8`);
transparent colour (`u16`); x and y aspect (`u8`); page width and height (`i16`).

Observed values:

- planes: always 8.
- masking: 0 for every `.LBM`; 2 (transparent colour) for `PAL6`, `PAL7` and `PALETTI8`.
- compression: 0 for every `.BBM`, 1 (`ByteRun1`) for every `.LBM`.
- transparent colour: 0, 96, 254 or 255. It is set even when masking is 0 and means nothing
  unless masking is 2.
- aspect and page size: either 5:6 on 320x200 or 1:1 on 640x480, whichever Deluxe Paint mode
  the file was last saved in. Nothing depends on them.

## `BODY`

`PBM `: one byte per pixel, each row padded to an even number of bytes. Every shipped picture
has an even width except the brush palettes `PAL7.BBM` (5x6) and `PALETTI8.BBM` (5x11), and
those two carry the pad byte: 36 and 66 body bytes.

`ILBM`: for each row, every bitplane in turn (least significant first), each padded to a whole
number of 16-bit words, then an extra mask plane when masking is 1. The mask plane is read and
dropped. `PBM ` with masking 1 is rejected: chunky rows have nowhere to put a mask.

`ByteRun1`: a control byte *n* is followed by *n* + 1 literal bytes when *n* is 0 to 127, or by
one byte repeated 257 - *n* times when *n* is 129 to 255; 128 does nothing. The decoder reads
the body as one stream across rows, and a run may cross a row boundary. A run longer than the
space left in the image is cut short, and anything after the last row is ignored. Input that
ends before the last row is an error. Before allocating, the parser checks that the image fits
in the body at the best possible ratio (64 output bytes per input byte), which stops a damaged
header requesting gigabytes.

## `CMAP`

8-bit RGB triples. Every value in the shipped files is a 6-bit VGA DAC value *v* stored as
`v * 4 + 3` (and 0 as 0), so `value >> 2` recovers the DAC value exactly. The parser keeps the
bytes as stored. A short `CMAP` leaves the remaining entries black; anything past 256 entries is
ignored.

## `CRNG`

Pad (`u16`), rate (`u16`, 16384 = one step per 1/60 s), flags (`u16`, bit 0 active, bit 1
reverse), low and high palette index (`u8`). All 1,120 ranges across both editions have flags 0,
and none would cycle in Deluxe Paint. Some carry non-zero rates on indices 240-247 and 248-251
anyway, and some have junk in the pad word (for example 7488). The game animates its palette
itself; these ranges are editor leftovers.

## Files

The base game and the Gold Edition, all decoding with no errors (`tests/real_iff.rs`):

| Path | Size | Notes |
|---|---|---|
| `GFX/PALETTE/PAL5.BBM`, `PAL6.BBM`, `PAL7.BBM` | 8x8, 8x8, 5x6 | the game's palettes; greenland, wasteland, winter |
| `GFX/PALETTE/PALETTI0.BBM`, `PALETTI1.BBM`, `PALETTI8.BBM` | 8x8, 8x8, 5x11 | from the original CD; `PALETTI0`/`1` have the same colours as `PAL5`/`6` |
| `GFX/TEXTURES/TEX5.LBM`, `TEX6.LBM`, `TEX7.LBM` | 256x256 | the game's terrain sheets |
| `GFX/TEXTURES/TEXTUR_0.LBM`, `TEXTUR_3.LBM` | 256x256 | CD-era sheets; differ from `TEX5`/`TEX6` in 11,008 and 28,058 pixels |
| `GFX/PICS/INSTALL.LBM` | 320x200 | |
| `GFX/PICS/SETUP*.LBM` | 640x480 | 13 in the base game, 20 with the Gold Edition |
| `GFX/PICS2/CREDIT00.LBM` | 640x480 | |
| `GFX/PICS/MISSION/*.LBM` | 640x480 | Gold Edition only, 9 files |
| `GFX/PICS/WORLD.LBM`, `WORLDMSK.LBM` | 512x340 | Gold Edition only |

The base game has 26 files, the Gold Edition 44. The Gold Edition adds `SETUP666`, `667`, `810`,
`811`, `899`, `901`, `990`, `998`, drops `SETUP997`, and changes `SETUP996`. `SETUP810` and
`SETUP811` have the same pixels as `MISSION/GREEN` and `MISSION/NASIA` with different palettes.

Each picture carries its own palette. `TEX6` and `TEX7` carry exactly `PAL6` and `PAL7`, but
`TEX5`'s palette differs from `PAL5` in 25 entries: 23 blues between indices 128 and 159,
plus 254 and 255.
