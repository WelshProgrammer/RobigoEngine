# Gouraud shading tables (`GOU*.DAT`)

Parser: `s2_formats::gouraud::ShadingTable`.

A shading table is 65,536 bytes with no header: 256 rows of 256 bytes. The row is a light level
and the column a palette index; the byte is the palette index to draw for that colour at that
light. `ShadingTable::shade(level, index)` is that lookup. Any other file size is an error.

## Files

All in `DATA/TEXTURES/`, identical in the base game and the Gold Edition:

| File | Pairs with | Notes |
|---|---|---|
| `GOU5.DAT` | `PAL5.BBM`, `TEX5.LBM` | greenland |
| `GOU6.DAT` | `PAL6.BBM`, `TEX6.LBM` | wasteland |
| `GOU7.DAT` | `PAL7.BBM`, `TEX7.LBM` | winter |
| `GOURAUD0.DAT`, `GOURAUD1.DAT` | CD-era palettes | also on the original CD |
| `GOURAUD.DAT` | | byte-identical to `GOURAUD0.DAT` |

Each table is paired with the palette and sheet of the same number. The pairing has not been
traced in `S2.EXE`.

## Layout found in every table

- Row 64 is the identity: `shade(64, i) == i` for all 256 indices (`NEUTRAL_LEVEL`).
- Rows below 64 darken and rows above brighten. Measured against `PAL5`, a row's mean brightness
  climbs steadily from row 0 to row 129. A single column can step back down, because each entry is
  the nearest colour the palette has.
- Rows 0 to 129 are all distinct (`MAX_LEVEL` = 129).
- Rows 130 to 255 are identical filler: every byte is 204, a light beige in all three of the
  game's palettes. Whether the engine reads them is not confirmed.
- Index 0 maps to itself at every level 0 to 129; no other index does.

Each of `GOU5` to `GOU7` was built against its own palette, and of rows 0 to 129 any two share
only row 64. Row 65 matches the identity in 239 or 240 of 256 entries, row 63 in 235 or 236.
