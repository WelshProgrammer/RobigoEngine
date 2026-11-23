# Smacker video (`.SMK`)

The intro film is a Smacker file, RAD Game Tools' palettised video format. Decoder:
`s2_formats::smacker`. This note describes the format in Robigo's own words, from the public format
description and from checking the decoder's output against ffmpeg run as a black box.

All integers are little-endian. All bitstreams are read least significant bit first: bit 0 of
byte 0 is the first bit, and a multi-bit field read from the stream has its first bit as its
lowest bit.

## Files in Settlers II

| File | Where | Size | Notes |
| --- | --- | --- | --- |
| `VIDEO/INTRO.SMK` | base game, Gold Edition | 16,383,164 | identical in both |
| `VIDEO/INTRO2.SMK` | base game (not the Gold Edition) | 30,322,032 | higher-rate encode of the same film |

Both files share every header field except the tree sizes:

- `SMK2`, 320x200, no scaling flags, no ring frame.
- 2,224 frames at 66 ms each (stored as -6600), so exactly 500/33 ≈ 15.15 fps and 146.784 s.
- One audio track: 22,050 Hz, stereo, 8-bit, Smacker DPCM compression.
- No frame has its keyframe bit set. Seven frames carry a palette chunk.
- Frames 0 to 2,208 carry audio, and the last 15 frames are silent. The first chunk decodes to
  47,012 bytes (about 1.07 s, the pre-roll). Later chunks hold 2,908 or 2,912 bytes, one frame's
  worth.
  The total is 3,236,586 stereo sample frames.
- The decoded audio of the two files is identical, sample for sample. Only the video differs.
- Frame data ends exactly at the end of each file.

The Robigo decoder matches ffmpeg on every frame (packed RGB per frame) and every audio sample of both
files. On a desktop machine in release mode it decodes INTRO.SMK at about 5,600 frames per second
and INTRO2.SMK at about 3,400, against the 15 the film needs.

## Layout

```text
header              104 bytes
frame sizes         4 bytes per frame
frame types         1 byte per frame
Huffman trees       trees_size bytes
frame data          each frame's bytes, back to back
```

When the ring-frame flag is set, both tables (and the frame data) have one extra entry after the
last frame.

### Header

| Offset | Type | Field |
| --- | --- | --- |
| 0 | 4 bytes | signature: `SMK2` or `SMK4` |
| 4 | u32 | width |
| 8 | u32 | height |
| 12 | u32 | frame count, not counting the ring frame |
| 16 | i32 | frame rate (below) |
| 20 | u32 | flags: bit 0 ring frame, bit 1 Y-interlaced, bit 2 Y-doubled |
| 24 | u32 × 7 | largest decoded audio chunk per track, in bytes |
| 52 | u32 | size of the tree blob |
| 56 | u32 × 4 | buffer sizes for the MMAP, MCLR, FULL and TYPE trees (only useful to the original player) |
| 72 | u32 × 7 | audio format per track (below) |
| 100 | u32 | unused |

Frame rate: a positive value is milliseconds per frame, a negative value is units of 10 µs per
frame (so -6600 is 66 ms), and zero means 100 ms.

Audio format word, per track:

| Bits | Meaning |
| --- | --- |
| 0 to 23 | sample rate in Hz |
| 26 | Bink audio uses the DCT variant |
| 27 | track is Bink audio |
| 28 | stereo |
| 29 | 16-bit samples (otherwise 8-bit) |
| 30 | track present |
| 31 | Smacker DPCM compression (otherwise raw PCM) |

The interlaced and doubled flags say how to display the coded rows. Interlaced puts each coded row
on every other display line, and doubled shows each coded row twice. The coded picture is always
the header's width by height. The decoder leaves scaling to the presenter.

### Frame tables

A frame size entry holds the frame's byte length with its low two bits used as flags. Bit 0 marks
a keyframe and bit 1 is unknown. Mask both off to get the length.

A frame type byte has bit 0 set when the frame starts with a palette chunk and bit `n + 1` set when
it carries a chunk for audio track `n`.

## Huffman trees

Smacker uses two kinds of tree, both stored as a pre-order walk of the tree's shape:

- read a bit; 1 means a branch, whose 0 side is stored first and then its 1 side;
- 0 means a leaf, followed by the leaf's value.

Decoding walks from the root, taking one bit per branch, until it reaches a leaf.

**Byte trees** have 8-bit leaf values written as 8 raw bits. A byte tree is stored as a presence
bit. When it is set, the shape follows, then one more bit (always 0). An absent tree decodes to 0
without reading anything.

**Word trees** produce 16-bit values and are stored as:

1. a presence bit (absent trees decode to 0);
2. a byte tree for low bytes, then one for high bytes;
3. three 16-bit escape values;
4. the shape, where each leaf's value is a low byte coded with the first byte tree followed by a
   high byte coded with the second;
5. one closing bit.

A leaf whose value equals escape value `k` stands for "the `k`th most recent value from this
tree" instead of that value. Each word tree keeps the three most recent values it produced.
After decoding value `v` (whether from a plain leaf or a recent-value lookup), if `v` differs from
the newest entry, the entries shift down one place and `v` becomes the newest. All three entries
are reset to 0 at the start of every frame.

The tree blob after the frame tables holds four word trees, in order: MMAP (mono block bitmaps),
MCLR (mono block colours), FULL (full block pixels), TYPE (block types and runs).

The audio chunks carry their own byte trees, built the same way.

## Frame data

A frame holds, in order: the palette chunk if the type byte says so, one chunk for each audio track
it flags (track 0 first), then the video bitstream, which runs to the end of the frame.

### Palette chunk

The first byte times 4 is the chunk's length, including that byte. The rest is a list of records
that walk the 256 palette entries from 0 upwards, building the new palette from the previous one:

| First byte | Following | Effect |
| --- | --- | --- |
| `1nnnnnnn` | nothing | keep the next `n + 1` entries as they were |
| `01nnnnnn` | `s` | copy `n + 1` entries from the previous palette, starting at entry `s` |
| `00rrrrrr` | `g`, `b` | one new colour; each channel's low 6 bits |

Copies read from the palette as it was before this chunk, not from entries already rewritten.
Records end once all 256 entries are covered, and any remaining bytes are padding. Six-bit channels
widen to 8 bits by repeating their top two bits at the bottom (`v << 2 | v >> 4`), so 0x3f
becomes 0xff. The palette starts all black.

### Audio chunk

A u32 length (counting its own 4 bytes) and then the chunk body.

Raw tracks store PCM directly: unsigned 8-bit samples or signed 16-bit little-endian samples,
interleaved left then right.

DPCM tracks start the body with a u32 giving the decoded size in bytes, then a bitstream:

1. One bit: data present. When clear the chunk decodes to nothing.
2. One bit each for stereo and 16-bit, which must match the track header.
3. Byte trees: one per channel for 8-bit audio; two per channel (low byte, high byte) for 16-bit.
   Left channel first.
4. The first sample of each channel, raw, right channel first. 8-bit audio stores one byte per
   channel. 16-bit audio stores the high byte and then the low byte.
5. For every later sample, interleaved left then right: decode a delta with that channel's tree
   (for 16-bit, the low byte then the high byte) and add it to the channel's previous sample,
   wrapping at 8 or 16 bits.

The first samples count towards the decoded size, so a chunk holds `size / (channels × bytes per
sample)` sample frames including the first.

### Video

The picture is cut into 4x4 blocks, numbered in raster order. Every frame starts from the previous
frame's picture (black before the first) and decodes until every block is covered:

1. Decode a TYPE value. Bits 0 to 1 are the block kind, bits 2 to 7 a run code, bits 8 to 15 a
   colour for solid blocks.
2. The run code gives the number of consecutive blocks the kind covers: codes 0 to 58 mean 1 to
   59 blocks; 59 to 63 mean 128, 256, 512, 1024 and 2048. A run is cut short at the last block.
3. Each kind fills its blocks:
   - **0, mono**: per block, an MCLR value (high byte for set bits, low byte for clear bits)
     and then an MMAP value, one bit per pixel. Bit 0 is the top-left pixel; each row takes the
     next four bits, left to right.
   - **1, full**: per block, two FULL values per row, top row first. The first value gives the
     row's right two pixels and the second the left two; within a value the low byte is the left
     pixel of the pair.
   - **2, void**: blocks keep the previous frame's pixels.
   - **3, solid**: blocks fill with the colour from the TYPE value.

`SMK4` files add two variants of the full block, chosen per block by bits read before the FULL
values. A 1 bit selects the doubled variant. Otherwise a second 1 bit selects the half variant,
and two 0 bits leave the normal block. Doubled reads one FULL value per half (top then bottom),
low byte for the left 2x2 square and high byte for the right. Half reads one row (two values, as
above) for each pair of display rows and repeats it on both.

Pictures whose sides are not multiples of 4 round the block grid up and clip the blocks.

## What has not been checked against real data

Neither intro file uses these, and no other Smacker file is available to compare against. They
are implemented from the description, and only Robigo's synthetic tests cover them:

- the `SMK4` doubled and half full-block variants;
- 16-bit and mono DPCM audio, and raw PCM tracks;
- picture sizes that are not multiples of 4;
- the interlaced and doubled display flags, and the ring frame;
- Bink audio, which the decoder reports as unsupported.
