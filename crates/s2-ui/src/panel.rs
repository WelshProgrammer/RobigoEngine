//! The panel the diary and the two text viewers show their text in.
//!
//! The original keeps 600 line pointers and wraps text into them as it arrives, dropping the
//! oldest line once the 600 are full. The wrap is greedy and counts characters rather than
//! pixels, so the same text breaks in the same places here. A pair of `@` in the text is a line
//! break, which is how the mission tables write one.

use crate::geometry::Rect;

/// How many lines the panel holds before the oldest goes.
pub const MOST_LINES: usize = 600;

/// The character cell the line width is counted in.
const CELL: u32 = 8;

/// What splits one line of the text from the next.
const BREAK: [u8; 2] = *b"\r\n";

/// What the panel turns into [`BREAK`] before it splits the text (`0x59ae7`).
const PAIRED_AT: [u8; 2] = *b"@@";

/// How many characters a line gives up, which the flags byte at `+0x97f` picks (`0x59987`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gutter {
    /// Bit 2 of the flags: the line takes the whole width.
    None,
    /// Bit 0: four characters go.
    Four,
    /// Neither bit: ten go.
    Ten,
}

impl Gutter {
    /// The flags bit that gives the line the whole width.
    const WHOLE_BIT: u8 = 1 << 2;
    /// The flags bit that takes four characters off.
    const FOUR_BIT: u8 = 1 << 0;

    /// The gutter the flags byte names. Bit 2 wins over bit 0.
    #[must_use]
    pub const fn of(flags: u8) -> Self {
        if flags & Self::WHOLE_BIT != 0 {
            Self::None
        } else if flags & Self::FOUR_BIT != 0 {
            Self::Four
        } else {
            Self::Ten
        }
    }

    /// How many characters it takes off a line.
    #[must_use]
    pub const fn characters(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Four => 4,
            Self::Ten => 10,
        }
    }

    /// How many characters fit on a line of a panel `width` pixels wide.
    #[must_use]
    pub const fn columns(self, width: u32) -> usize {
        let cells = width / CELL;
        (cells.saturating_sub(self.characters())) as usize
    }
}

/// The lines of text a panel holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextPanel {
    lines: Vec<Vec<u8>>,
    first: usize,
}

impl TextPanel {
    /// An empty panel.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            lines: Vec::new(),
            first: 0,
        }
    }

    /// Drops every line and goes back to the top (`0x599d2`).
    pub fn clear(&mut self) {
        self.lines.clear();
        self.first = 0;
    }

    /// The lines held, oldest first.
    #[must_use]
    pub fn lines(&self) -> &[Vec<u8>] {
        &self.lines
    }

    /// Which line is drawn at the top.
    #[must_use]
    pub const fn first(&self) -> usize {
        self.first
    }

    /// Draws from `line` down, held to the lines there are.
    pub fn show_from(&mut self, line: usize) {
        self.first = line.min(self.lines.len().saturating_sub(1));
    }

    /// The lines on show when `rows` of them fit.
    #[must_use]
    pub fn shown(&self, rows: usize) -> &[Vec<u8>] {
        let end = self.first.saturating_add(rows).min(self.lines.len());
        self.lines.get(self.first..end).unwrap_or_default()
    }

    /// Takes `text` in, splitting it on CRLF and wrapping each line to `columns` characters
    /// (`0x59606`). Each `@@`, taken from the left, counts as a CRLF, and a lone `@` stays a
    /// character (`0x59ae7`). A line of exactly `columns` characters is kept whole.
    pub fn push(&mut self, text: &[u8], columns: usize) {
        let text = paired_ats_broken(text);
        let mut rest = text.as_slice();
        while let Some(at) = find(rest, &BREAK) {
            self.wrap(&rest[..at], columns);
            rest = &rest[at + BREAK.len()..];
        }
        self.wrap(rest, columns);
    }

    /// Wraps one line, which holds no CRLF, into as many as it takes.
    fn wrap(&mut self, line: &[u8], columns: usize) {
        let mut rest = line;
        loop {
            if columns == 0 || rest.len() <= columns {
                self.add(rest);
                return;
            }
            let mut at = columns;
            while at > 0 && rest[at] != b' ' {
                at -= 1;
            }
            if at == 0 {
                at = columns;
            }
            self.add(&rest[..at]);
            rest = &rest[at..];
            while rest.first() == Some(&b' ') {
                rest = &rest[1..];
            }
        }
    }

    /// Keeps one line, dropping the oldest once 600 are held.
    fn add(&mut self, line: &[u8]) {
        self.lines.push(line.to_vec());
        if self.lines.len() > MOST_LINES {
            self.lines.remove(0);
        }
    }

    /// How many characters fit on a line of `area` with `gutter`.
    #[must_use]
    pub const fn columns(area: Rect, gutter: Gutter) -> usize {
        gutter.columns(area.width)
    }
}

/// `text` with every `@@` from the left replaced by a CRLF.
fn paired_ats_broken(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = find(rest, &PAIRED_AT) {
        out.extend_from_slice(&rest[..at]);
        out.extend_from_slice(&BREAK);
        rest = &rest[at + PAIRED_AT.len()..];
    }
    out.extend_from_slice(rest);
    out
}

/// Where `needle` first sits in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&at| &haystack[at..at + needle.len()] == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(panel: &TextPanel) -> Vec<String> {
        panel
            .lines()
            .iter()
            .map(|line| String::from_utf8_lossy(line).into_owned())
            .collect()
    }

    #[test]
    fn the_flags_pick_how_many_characters_a_line_gives_up() {
        assert_eq!(Gutter::of(Gutter::WHOLE_BIT), Gutter::None);
        assert_eq!(
            Gutter::of(Gutter::WHOLE_BIT | Gutter::FOUR_BIT),
            Gutter::None,
            "bit 2 wins over bit 0"
        );
        assert_eq!(Gutter::of(Gutter::FOUR_BIT), Gutter::Four);
        assert_eq!(Gutter::of(Gutter::WHOLE_BIT | (1 << 3) | (1 << 4)), Gutter::None);
        assert_eq!(Gutter::of(0), Gutter::Ten);
        assert_eq!(Gutter::None.columns(400), 50);
        assert_eq!(Gutter::Four.columns(400), 46);
        assert_eq!(Gutter::Ten.columns(400), 40);
        assert_eq!(Gutter::Ten.columns(16), 0, "a narrow panel fits nothing");
    }

    #[test]
    fn crlf_starts_a_new_line_and_a_lone_return_does_not() {
        let mut panel = TextPanel::new();
        panel.push(b"one\r\ntwo\rthree", 40);
        assert_eq!(lines(&panel), ["one", "two\rthree"]);
    }

    #[test]
    fn a_pair_of_at_signs_breaks_the_line_and_a_lone_one_stays() {
        let mut panel = TextPanel::new();
        panel.push(b"Day one:@@@@@@Rain@at noon@@@", 40);
        assert_eq!(lines(&panel), ["Day one:", "", "", "Rain@at noon", "@"]);
    }

    #[test]
    fn a_text_that_ends_on_a_break_leaves_an_empty_line() {
        let mut panel = TextPanel::new();
        panel.push(b"\r\n\r\n", 40);
        assert_eq!(lines(&panel), ["", "", ""]);
    }

    #[test]
    fn a_long_line_breaks_at_the_last_space_that_fits() {
        let mut panel = TextPanel::new();
        panel.push(b"the quick brown fox", 10);
        assert_eq!(lines(&panel), ["the quick", "brown fox"]);
    }

    #[test]
    fn a_line_with_no_space_breaks_at_the_count() {
        let mut panel = TextPanel::new();
        panel.push(b"abcdefghijklmno", 10);
        assert_eq!(lines(&panel), ["abcdefghij", "klmno"]);
    }

    #[test]
    fn a_line_of_exactly_the_count_is_kept_whole() {
        let mut panel = TextPanel::new();
        panel.push(b"abcdefghij", 10);
        assert_eq!(lines(&panel), ["abcdefghij"]);
    }

    #[test]
    fn the_oldest_line_goes_once_six_hundred_are_held() {
        let mut panel = TextPanel::new();
        for line in 0..MOST_LINES + 2 {
            panel.push(line.to_string().as_bytes(), 40);
        }
        assert_eq!(panel.lines().len(), MOST_LINES);
        assert_eq!(lines(&panel)[0], "2");
    }

    #[test]
    fn what_is_shown_starts_at_the_top_line_and_stops_at_the_last() {
        let mut panel = TextPanel::new();
        panel.push(b"one\r\ntwo\r\nthree\r\nfour", 40);
        assert_eq!(panel.shown(2).len(), 2);
        panel.show_from(3);
        assert_eq!(panel.first(), 3);
        assert_eq!(panel.shown(5).len(), 1);
        panel.show_from(99);
        assert_eq!(panel.first(), 3, "a line past the end holds at the last");
        panel.clear();
        assert_eq!(panel.first(), 0);
        let found = panel.shown(5);
        assert!(found.is_empty(), "{found:?}");
    }
}
