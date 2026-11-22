//! A screen the game can draw before it has any of the player's data: a line of text in a small
//! font of its own and a progress bar, for the set-up of a first start.

use s2_formats::palette::Palette;
use s2_render::Frame;

/// The screen's size, the one S2 draws at.
pub(crate) const SIZE: (u32, u32) = (640, 480);

/// The palette indices the screen draws in.
const GROUND: u8 = 0;
const INK: u8 = 1;
const TROUGH: u8 = 2;
const FILLED: u8 = 3;
const LIT: u8 = 4;
const SHADED: u8 = 5;
const FAINT: u8 = 6;

/// The colours of those indices, as red, green and blue.
const COLOURS: [[u8; 3]; 7] = [
    [0, 0, 0],
    [252, 216, 84],
    [24, 36, 24],
    [196, 148, 36],
    [140, 156, 140],
    [60, 72, 60],
    [168, 164, 140],
];

/// A glyph's size in its own dots, and how many screen pixels a dot covers.
const GLYPH: (i32, i32) = (5, 7);
const DOT: i32 = 2;

/// How far one letter's left edge is from the next's, in dots.
const ADVANCE: i32 = 6;

/// The letters whose glyphs hang below the line, and how many dots lower they are drawn.
const HANGING: &str = "gpqy";
const HANG: i32 = 2;

/// The bar's size, how far its top sits under the screen's middle, and how far under that
/// middle the second line's top does.
const BAR: (i32, i32) = (360, 18);
const BAR_UNDER: i32 = 30;
const DETAIL_UNDER: i32 = 64;

/// The font: each glyph's seven rows of five dots, top first, `#` for a lit dot. A hanging
/// letter's rows start at the top of the small letters.
const GLYPHS: [(char, &str); 70] = [
    ('A', ".###. #...# #...# ##### #...# #...# #...#"),
    ('B', "####. #...# #...# ####. #...# #...# ####."),
    ('C', ".###. #...# #.... #.... #.... #...# .###."),
    ('D', "####. #...# #...# #...# #...# #...# ####."),
    ('E', "##### #.... #.... ####. #.... #.... #####"),
    ('F', "##### #.... #.... ####. #.... #.... #...."),
    ('G', ".###. #...# #.... #.### #...# #...# .####"),
    ('H', "#...# #...# #...# ##### #...# #...# #...#"),
    ('I', ".###. ..#.. ..#.. ..#.. ..#.. ..#.. .###."),
    ('J', "..### ...#. ...#. ...#. ...#. #..#. .##.."),
    ('K', "#...# #..#. #.#.. ##... #.#.. #..#. #...#"),
    ('L', "#.... #.... #.... #.... #.... #.... #####"),
    ('M', "#...# ##.## #.#.# #.#.# #...# #...# #...#"),
    ('N', "#...# ##..# #.#.# #..## #...# #...# #...#"),
    ('O', ".###. #...# #...# #...# #...# #...# .###."),
    ('P', "####. #...# #...# ####. #.... #.... #...."),
    ('Q', ".###. #...# #...# #...# #.#.# #..#. .##.#"),
    ('R', "####. #...# #...# ####. #.#.. #..#. #...#"),
    ('S', ".#### #.... #.... .###. ....# ....# ####."),
    ('T', "##### ..#.. ..#.. ..#.. ..#.. ..#.. ..#.."),
    ('U', "#...# #...# #...# #...# #...# #...# .###."),
    ('V', "#...# #...# #...# #...# #...# .#.#. ..#.."),
    ('W', "#...# #...# #...# #.#.# #.#.# ##.## #...#"),
    ('X', "#...# #...# .#.#. ..#.. .#.#. #...# #...#"),
    ('Y', "#...# #...# .#.#. ..#.. ..#.. ..#.. ..#.."),
    ('Z', "##### ....# ...#. ..#.. .#... #.... #####"),
    ('a', "..... ..... .###. ....# .#### #...# .####"),
    ('b', "#.... #.... #.##. ##..# #...# #...# ####."),
    ('c', "..... ..... .###. #.... #.... #...# .###."),
    ('d', "....# ....# .##.# #..## #...# #...# .####"),
    ('e', "..... ..... .###. #...# ##### #.... .###."),
    ('f', "..##. .#..# .#... ###.. .#... .#... .#..."),
    ('g', ".#### #...# #...# #...# .#### ....# .###."),
    ('h', "#.... #.... #.##. ##..# #...# #...# #...#"),
    ('i', "..#.. ..... .##.. ..#.. ..#.. ..#.. .###."),
    ('j', "...#. ..... ..##. ...#. ...#. #..#. .##.."),
    ('k', "#.... #.... #..#. #.#.. ##... #.#.. #..#."),
    ('l', ".##.. ..#.. ..#.. ..#.. ..#.. ..#.. .###."),
    ('m', "..... ..... ##.#. #.#.# #.#.# #...# #...#"),
    ('n', "..... ..... #.##. ##..# #...# #...# #...#"),
    ('o', "..... ..... .###. #...# #...# #...# .###."),
    ('p', "#.##. ##..# #...# #...# ####. #.... #...."),
    ('q', ".##.# #..## #...# #...# .#### ....# ....#"),
    ('r', "..... ..... #.##. ##..# #.... #.... #...."),
    ('s', "..... ..... .#### #.... .###. ....# ####."),
    ('t', ".#... .#... ###.. .#... .#... .#..# ..##."),
    ('u', "..... ..... #...# #...# #...# #..## .##.#"),
    ('v', "..... ..... #...# #...# #...# .#.#. ..#.."),
    ('w', "..... ..... #...# #...# #.#.# #.#.# .#.#."),
    ('x', "..... ..... #...# .#.#. ..#.. .#.#. #...#"),
    ('y', "#...# #...# #...# #...# .#### ....# .###."),
    ('z', "..... ..... ##### ...#. ..#.. .#... #####"),
    ('0', ".###. #...# #..## #.#.# ##..# #...# .###."),
    ('1', "..#.. .##.. ..#.. ..#.. ..#.. ..#.. .###."),
    ('2', ".###. #...# ....# ...#. ..#.. .#... #####"),
    ('3', "##### ...#. ..#.. ...#. ....# #...# .###."),
    ('4', "...#. ..##. .#.#. #..#. ##### ...#. ...#."),
    ('5', "##### #.... ####. ....# ....# #...# .###."),
    ('6', "..##. .#... #.... ####. #...# #...# .###."),
    ('7', "##### ....# ...#. ..#.. .#... .#... .#..."),
    ('8', ".###. #...# #...# .###. #...# #...# .###."),
    ('9', ".###. #...# #...# .#### ....# ...#. .##.."),
    ('.', "..... ..... ..... ..... ..... .##.. .##.."),
    (',', "..... ..... ..... ..... .##.. ..#.. .#..."),
    ('%', "##..# ##..# ...#. ..#.. .#... #..## #..##"),
    (':', "..... .##.. .##.. ..... .##.. .##.. ....."),
    ('-', "..... ..... ..... ##### ..... ..... ....."),
    ('\'', ".##.. ..#.. .#... ..... ..... ..... ....."),
    ('_', "..... ..... ..... ..... ..... ..... #####"),
    ('/', "....# ....# ...#. ..#.. .#... #.... #...."),
];

/// The palette the screen is drawn in.
pub(crate) fn palette() -> Palette {
    let mut colours = [[0; 3]; 256];
    colours[..COLOURS.len()].copy_from_slice(&COLOURS);
    Palette(colours)
}

/// The screen saying `text` on one line, with a bar under it filled to `per_cent` when there is
/// one, and `detail` fainter on a line under that. A letter the font lacks leaves a gap, and a
/// line too long for the screen runs off both its sides.
pub(crate) fn screen(text: &str, per_cent: Option<u64>, detail: &str) -> Frame {
    let mut frame = Frame::new(SIZE.0, SIZE.1, GROUND);
    let (width, height) = (SIZE.0.cast_signed(), SIZE.1.cast_signed());
    line(&mut frame, text, height / 2 - GLYPH.1 * DOT, INK);
    line(&mut frame, detail, height / 2 + DETAIL_UNDER, FAINT);
    if let Some(per_cent) = per_cent {
        let at = ((width - BAR.0) / 2, height / 2 + BAR_UNDER);
        fill(&mut frame, (at.0 - 1, at.1 - 1), (BAR.0 + 2, BAR.1 + 2), LIT);
        fill(&mut frame, (at.0 - 1, at.1 - 1), (BAR.0 + 1, BAR.1 + 1), SHADED);
        fill(&mut frame, at, BAR, TROUGH);
        let done = i32::try_from(per_cent.min(100)).unwrap_or(100);
        fill(&mut frame, at, (BAR.0 * done / 100, BAR.1), FILLED);
    }
    frame
}

/// Draws `text` in `index` on one line centred across the screen, its top at `top`.
fn line(frame: &mut Frame, text: &str, top: i32, index: u8) {
    let letters = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);
    let across = letters.saturating_mul(ADVANCE * DOT);
    for (place, letter) in (0..).zip(text.chars()) {
        let left = (SIZE.0.cast_signed() - across) / 2 + place * ADVANCE * DOT;
        let top = if HANGING.contains(letter) {
            top + HANG * DOT
        } else {
            top
        };
        for (row, dots) in (0..).zip(glyph(letter).split(' ')) {
            for (column, dot) in (0..).zip(dots.chars()) {
                if dot == '#' {
                    fill(frame, (left + column * DOT, top + row * DOT), (DOT, DOT), index);
                }
            }
        }
    }
}

/// The rows of `letter`'s glyph, or of a blank for a letter the font lacks.
fn glyph(letter: char) -> &'static str {
    GLYPHS
        .iter()
        .find(|(of, _)| *of == letter)
        .map_or("", |(_, rows)| rows)
}

/// Fills the `size` rectangle at `at` with `index`.
fn fill(frame: &mut Frame, at: (i32, i32), size: (i32, i32), index: u8) {
    for y in at.1..at.1 + size.1 {
        for x in at.0..at.0 + size.0 {
            frame.set(x, y, index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(frame: &Frame, index: u8) -> usize {
        frame
            .pixels()
            .iter()
            .map(|&pixel| usize::from(pixel == index))
            .sum()
    }

    #[test]
    fn every_glyph_is_seven_rows_of_five_dots() {
        for (letter, rows) in GLYPHS {
            let rows: Vec<&str> = rows.split(' ').collect();
            assert_eq!(rows.len(), 7, "{letter}");
            assert!(
                rows.iter()
                    .all(|row| row.len() == 5 && row.chars().all(|dot| dot == '#' || dot == '.')),
                "{letter}"
            );
        }
        let every = ('a'..='z').chain('A'..='Z').chain('0'..='9');
        assert!(every.into_iter().all(|letter| !glyph(letter).is_empty()));
    }

    #[test]
    fn the_text_is_drawn_dot_for_dot_and_a_space_draws_nothing() {
        let dots = GLYPHS[0].1.chars().filter(|&dot| dot == '#').count();
        assert_eq!(count(&screen("A", None, ""), INK), dots * 4, "each dot is 2 by 2");
        assert_eq!(count(&screen(" ", None, ""), INK), 0);
        assert_eq!(
            count(&screen("A", None, ""), TROUGH),
            0,
            "no bar without a per cent"
        );
        assert_eq!(
            count(&screen("", None, "A"), FAINT),
            dots * 4,
            "the second line is fainter"
        );
    }

    #[test]
    fn the_bar_fills_by_the_per_cent() {
        let area = usize::try_from(BAR.0 * BAR.1).unwrap();
        assert_eq!(count(&screen("", Some(0), ""), TROUGH), area);
        assert_eq!(count(&screen("", Some(50), ""), FILLED), area / 2);
        assert_eq!(
            count(&screen("", Some(250), ""), FILLED),
            area,
            "held at the whole bar"
        );
    }
}
