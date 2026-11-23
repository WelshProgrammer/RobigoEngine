//! Block-coded picture data.
//!
//! The picture is cut into 4x4 blocks in raster order. A TYPE value gives a block kind, a run
//! length and (for solid blocks) a colour; the run covers that many consecutive blocks.

use super::{
    Error, Version,
    bits::BitReader,
    huffman::{Recent, WordTree},
};
use crate::image::IndexedImage;

/// Blocks covered by the run code in bits 2..8 of a TYPE value: 1 to 59, then 128 to 2048.
fn run_length(code: u16) -> usize {
    match code {
        0..=58 => usize::from(code) + 1,
        _ => 128 << (code - 59).min(4),
    }
}

const MONO: u16 = 0;
const FULL: u16 = 1;
const VOID: u16 = 2;

/// The four 16-bit trees every file carries, in stored order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Trees {
    /// Mono block bitmaps.
    mmap: WordTree,
    /// Mono block colour pairs.
    mclr: WordTree,
    /// Full block pixel pairs.
    full: WordTree,
    /// Block kinds and run lengths.
    kind: WordTree,
}

impl Trees {
    pub(super) fn read(blob: &[u8]) -> Result<Self, Error> {
        let mut bits = BitReader::new(blob);
        Ok(Self {
            mmap: WordTree::read(&mut bits)?,
            mclr: WordTree::read(&mut bits)?,
            full: WordTree::read(&mut bits)?,
            kind: WordTree::read(&mut bits)?,
        })
    }
}

/// Per-frame decoding state: the bitstream and each tree's recent-value cache.
struct Blocks<'t, 'd> {
    trees: &'t Trees,
    version: Version,
    bits: BitReader<'d>,
    recent: [Recent; 4],
}

impl Blocks<'_, '_> {
    fn mono(&mut self) -> Result<[u8; 16], Error> {
        let [lo, hi] = self
            .trees
            .mclr
            .decode(&mut self.bits, &mut self.recent[1])?
            .to_le_bytes();
        let map = self.trees.mmap.decode(&mut self.bits, &mut self.recent[0])?;
        let mut block = [0; 16];
        for (i, pixel) in block.iter_mut().enumerate() {
            *pixel = if map >> i & 1 == 0 { lo } else { hi };
        }
        Ok(block)
    }

    fn pair(&mut self) -> Result<[u8; 2], Error> {
        self.trees
            .full
            .decode(&mut self.bits, &mut self.recent[2])
            .map(u16::to_le_bytes)
    }

    /// Reads one row of four pixels: the right pair is stored first.
    fn row(&mut self) -> Result<[u8; 4], Error> {
        let [c, d] = self.pair()?;
        let [a, b] = self.pair()?;
        Ok([a, b, c, d])
    }

    fn full(&mut self) -> Result<[u8; 16], Error> {
        let mut block = [0; 16];
        let smk4 = self.version == Version::Smk4;
        let double = smk4 && self.bits.bit()?;
        let half = smk4 && !double && self.bits.bit()?;
        if double {
            for half in block.as_chunks_mut::<8>().0 {
                let [left, right] = self.pair()?;
                half.copy_from_slice(&[left, left, right, right, left, left, right, right]);
            }
        } else if half {
            for half in block.as_chunks_mut::<8>().0 {
                let row = self.row()?;
                half[..4].copy_from_slice(&row);
                half[4..].copy_from_slice(&row);
            }
        } else {
            for row in block.as_chunks_mut::<4>().0 {
                row.copy_from_slice(&self.row()?);
            }
        }
        Ok(block)
    }
}

/// Decodes one frame's picture data over `image`, which holds the previous frame.
pub(super) fn decode(
    trees: &Trees,
    version: Version,
    data: &[u8],
    image: &mut IndexedImage,
) -> Result<(), Error> {
    let width = usize::from(image.width());
    let height = usize::from(image.height());
    let columns = width.div_ceil(4);
    let total = columns * height.div_ceil(4);
    let pixels = image.pixels_mut();
    let mut blocks = Blocks {
        trees,
        version,
        bits: BitReader::new(data),
        recent: [[0; 3]; 4],
    };
    let mut index = 0;
    while index < total {
        // TYPE value: bits 0 to 1 the block kind, 2 to 7 the run code, 8 to 15 a solid colour.
        let word = trees.kind.decode(&mut blocks.bits, &mut blocks.recent[3])?;
        let end = total.min(index + run_length((word >> 2) % 64));
        let at = |block: usize| ((block % columns) * 4, (block / columns) * 4);
        match word & 3 {
            MONO => {
                for block in index..end {
                    let pixels4x4 = blocks.mono()?;
                    put(pixels, width, height, at(block), &pixels4x4);
                }
            }
            FULL => {
                for block in index..end {
                    let pixels4x4 = blocks.full()?;
                    put(pixels, width, height, at(block), &pixels4x4);
                }
            }
            VOID => {}
            _ => {
                let pixels4x4 = [word.to_le_bytes()[1]; 16];
                for block in index..end {
                    put(pixels, width, height, at(block), &pixels4x4);
                }
            }
        }
        index = end;
    }
    Ok(())
}

/// Writes a 4x4 block with its top-left corner at `(x, y)`, clipped to the picture.
fn put(pixels: &mut [u8], width: usize, height: usize, (x, y): (usize, usize), block: &[u8; 16]) {
    let span = width.saturating_sub(x).min(4);
    for (row, source) in (y..height.min(y + 4)).zip(block.as_chunks::<4>().0) {
        let start = row * width + x;
        if let (Some(target), Some(source)) = (pixels.get_mut(start..start + span), source.get(..span)) {
            target.copy_from_slice(source);
        }
    }
}
