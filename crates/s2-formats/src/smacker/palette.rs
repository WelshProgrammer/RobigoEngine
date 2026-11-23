//! Per-frame palette updates.

use super::Error;
use crate::palette::{PALETTE_SIZE, Palette};

/// Record flag: skip entries, keeping them unchanged.
const SKIP: u8 = 0x80;
/// Record flag, with [`SKIP`] clear: copy entries from the previous palette.
const COPY: u8 = 0x40;

/// Applies one palette chunk body (the bytes after its length byte) to `palette`.
///
/// Each record starts with a byte `b`:
/// - `1xxxxxxx`: keep the next `x + 1` entries unchanged;
/// - `01xxxxxx` then `s`: copy `x + 1` entries from the previous palette starting at `s`;
/// - `00rrrrrr` then `g`, `b`: one new colour, 6 bits per channel.
pub(super) fn apply(palette: &mut Palette, body: &[u8]) -> Result<(), Error> {
    let previous = palette.0;
    let mut bytes = body.iter().copied();
    let mut index = 0;
    while index < PALETTE_SIZE {
        let Some(op) = bytes.next() else { break };
        if op & SKIP != 0 {
            index += usize::from(op & !SKIP) + 1;
        } else if op & COPY != 0 {
            let count = usize::from(op & !COPY) + 1;
            let source = usize::from(bytes.next().ok_or(Error::Palette)?);
            let from = previous.get(source..source + count).ok_or(Error::Palette)?;
            let to = palette.0.get_mut(index..index + count).ok_or(Error::Palette)?;
            to.copy_from_slice(from);
            index += count;
        } else {
            let (Some(g), Some(b)) = (bytes.next(), bytes.next()) else {
                return Err(Error::Palette);
            };
            if let Some(colour) = palette.0.get_mut(index) {
                *colour = [widen(op), widen(g), widen(b)];
            }
            index += 1;
        }
    }
    Ok(())
}

/// Widens a 6-bit channel to 8 bits by repeating its top two bits at the bottom: 63 becomes 255.
fn widen(six: u8) -> u8 {
    let six = six % 64;
    (six << 2) | (six >> 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widens_like_the_vga_dac_table() {
        assert_eq!(widen(0), 0);
        assert_eq!(widen(16), 65);
        assert_eq!(widen(32), 130);
        assert_eq!(widen(48), 195);
        assert_eq!(widen(63), 255);
    }

    #[test]
    fn new_skip_and_copy() {
        let mut palette = Palette::default();
        palette.0[5] = [1, 2, 3];
        palette.0[6] = [4, 5, 6];
        apply(
            &mut palette,
            &[0x3f, 0, 0x20, 0x81, 0x41, 5, 0x82, 0x3f, 0x3f, 0x3f],
        )
        .unwrap();
        assert_eq!(palette.0[0], [255, 0, 130]);
        assert_eq!(palette.0[1], [0, 0, 0]);
        assert_eq!(palette.0[3], [1, 2, 3]);
        assert_eq!(palette.0[4], [4, 5, 6]);
        assert_eq!(palette.0[5], [1, 2, 3]);
        assert_eq!(palette.0[8], [255; 3]);
    }

    #[test]
    fn copies_read_the_previous_palette() {
        let mut palette = Palette::default();
        palette.0[0] = [9, 9, 9];
        apply(&mut palette, &[0x3f, 0x3f, 0x3f, 0x40, 0]).unwrap();
        assert_eq!(palette.0[1], [9, 9, 9]);
    }

    #[test]
    fn rejects_overruns() {
        let mut palette = Palette::default();
        assert_eq!(apply(&mut palette, &[0x7f, 0xf0]), Err(Error::Palette));
        assert_eq!(apply(&mut palette, &[0x3f, 0]), Err(Error::Palette));
        assert_eq!(apply(&mut palette, &[0xff, 0xfe, 0x41, 0]), Err(Error::Palette));
    }
}
