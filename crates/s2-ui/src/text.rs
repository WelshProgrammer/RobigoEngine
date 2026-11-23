//! Glyphs and the text a label draws.

use s2_assets::Sprite;
use s2_formats::lst::{FIRST_GLYPH, Font, GLYPH_COUNT};

/// Which of the original's bitmap fonts draws a run of text. The three shipped fonts have line
/// heights of 9, 11 and 14 pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontId {
    /// The 9-pixel font, named `font09` in `RESOURCE.DAT`.
    Small,
    /// The 11-pixel font, named `font11`.
    Medium,
    /// The 14-pixel font, named `font14`, which `FONT14.FNT` also holds.
    Large,
}

impl FontId {
    /// The font a window style record names, where 0 is `font11`, 1 is `font14` and 2 is
    /// `font09`. Any other index names no font.
    #[must_use]
    pub const fn from_style(index: u16) -> Option<Self> {
        match index {
            0 => Some(Self::Medium),
            1 => Some(Self::Large),
            2 => Some(Self::Small),
            _ => None,
        }
    }
}

/// How a run of text is spaced, holding the pen fields the original keeps that change where a
/// glyph lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord, Hash)]
pub struct Pen {
    /// Pixels added after each glyph.
    pub spacing: u16,
    /// Pixels added to the line height between lines.
    pub line_gap: u16,
    /// The cell every glyph is centred in. Zero draws proportionally.
    pub cell: u16,
}

impl Pen {
    /// Glyphs at their own widths, with nothing added.
    pub const TIGHT: Self = Self {
        spacing: 0,
        line_gap: 0,
        cell: 0,
    };

    /// Glyphs centred in cells of `cell` pixels, which the pen advances by whatever a glyph
    /// measures.
    #[must_use]
    pub const fn cells(cell: u16) -> Self {
        Self { cell, ..Self::TIGHT }
    }

    /// Whether every glyph sits in a cell of the same width.
    #[must_use]
    pub const fn is_fixed(self) -> bool {
        self.cell != 0
    }
}

/// The highest character code [`Glyphs::measure`] counts, which `0x93a98` stops at. Codes above
/// it draw at their own width and measure as nothing.
pub const LAST_MEASURED: u8 = 191;

/// The glyphs of one bitmap font, ready to blit.
///
/// A glyph's pixels are player-colour shades, so the colour a run draws in is the base the
/// painter adds to them. A glyph advances the pen by its own width, as `0x93a0c` does, and a
/// character the font has no glyph for draws nothing.
#[derive(Debug, Clone)]
pub struct Glyphs {
    line_height: u32,
    widths: Vec<u16>,
    slots: Vec<Option<Sprite>>,
}

impl Glyphs {
    /// The glyphs of `font`, which advance by their own widths.
    #[must_use]
    pub fn from_font(font: &Font) -> Self {
        Self::from_sprites(
            font.dy(),
            font.glyphs()
                .map(|(code, glyph)| (code, Sprite::from_player_bitmap(glyph))),
        )
    }

    /// A font of `line_height` whose glyphs are `glyphs` by character code, each advancing by its
    /// own width. A code below the first a font holds is ignored.
    #[must_use]
    pub fn from_sprites(line_height: u8, glyphs: impl IntoIterator<Item = (u8, Sprite)>) -> Self {
        let mut slots: Vec<Option<Sprite>> = vec![None; usize::from(u8::MAX - FIRST_GLYPH) + 1];
        for (code, sprite) in glyphs {
            if let Some(slot) = code.checked_sub(FIRST_GLYPH) {
                slots[usize::from(slot)] = Some(sprite);
            }
        }
        let widths = slots
            .iter()
            .map(|slot| slot.as_ref().map_or(0, Sprite::width))
            .collect();
        Self {
            line_height: u32::from(line_height),
            widths,
            slots,
        }
    }

    /// A font with no glyphs at all, which draws nothing and measures every character as
    /// `width`. A stand-in for a build with no data to read a font from.
    #[must_use]
    pub fn fixed(line_height: u32, width: u16) -> Self {
        Self {
            line_height,
            widths: vec![width; GLYPH_COUNT],
            slots: vec![None; GLYPH_COUNT],
        }
    }

    /// How far one line of text sits below the one above, before the pen's own gap.
    #[must_use]
    pub const fn line_height(&self) -> u32 {
        self.line_height
    }

    /// How far a line break moves the pen down.
    #[must_use]
    pub const fn line_advance(&self, pen: Pen) -> u32 {
        self.line_height + pen.line_gap as u32
    }

    /// The glyph for `code`, if the font has one.
    #[must_use]
    pub fn glyph(&self, code: u8) -> Option<&Sprite> {
        let slot = code.checked_sub(FIRST_GLYPH)?;
        self.slots.get(usize::from(slot))?.as_ref()
    }

    /// How wide the glyph for `code` draws, which is 0 for a character the font lacks.
    #[must_use]
    pub fn width(&self, code: u8) -> u32 {
        code.checked_sub(FIRST_GLYPH)
            .and_then(|slot| self.widths.get(usize::from(slot)))
            .map_or(0, |&width| u32::from(width))
    }

    /// How far the pen moves after drawing `code`.
    #[must_use]
    pub fn advance(&self, code: u8, pen: Pen) -> u32 {
        if pen.is_fixed() {
            u32::from(pen.cell)
        } else {
            self.width(code).saturating_add(u32::from(pen.spacing))
        }
    }

    /// How far right of the pen the glyph for `code` draws, which centres it in a fixed cell.
    #[must_use]
    pub fn inset(&self, code: u8, pen: Pen) -> i32 {
        if !pen.is_fixed() {
            return 0;
        }
        let cell = i32::from(pen.cell);
        let width = i32::try_from(self.width(code)).unwrap_or(cell);
        (cell - width) / 2
    }

    /// How wide `text` draws, in cp437 bytes, as `0x93a98` answers it.
    ///
    /// A fixed cell gives one cell per byte. Otherwise the codes from the first glyph slot to
    /// [`LAST_MEASURED`] count and every other byte is skipped, which leaves a line break or a
    /// tab adding nothing even though drawing them moves the pen.
    #[must_use]
    pub fn measure(&self, text: &[u8], pen: Pen) -> u32 {
        if pen.is_fixed() {
            return u32::try_from(text.len())
                .unwrap_or(u32::MAX)
                .saturating_mul(u32::from(pen.cell));
        }
        text.iter()
            .filter(|&&code| (FIRST_GLYPH..=LAST_MEASURED).contains(&code))
            .fold(0, |width, &code| width.saturating_add(self.advance(code, pen)))
    }
}

/// The three fonts a window can draw in.
#[derive(Debug, Clone)]
pub struct Fonts {
    small: Glyphs,
    medium: Glyphs,
    large: Glyphs,
}

impl Fonts {
    /// The three fonts, smallest first.
    #[must_use]
    pub const fn new(small: Glyphs, medium: Glyphs, large: Glyphs) -> Self {
        Self { small, medium, large }
    }

    /// The glyphs of `id`.
    #[must_use]
    pub const fn get(&self, id: FontId) -> &Glyphs {
        match id {
            FontId::Small => &self.small,
            FontId::Medium => &self.medium,
            FontId::Large => &self.large,
        }
    }
}

/// What a label shows: a slot in the game's text table, or a number worked out as the window
/// draws.
///
/// Both stay plain values so an op list can be compared and tested without a text table. Runs
/// the player types arrive with the screens that need them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Label {
    /// Slot `0` upwards of the text table the game loaded for its language.
    Slot(u16),
    /// A number in decimal, with a minus sign when negative.
    Number(i32),
    /// Two numbers with a slash between them, which is the `%d/%d` the buildings screen shows
    /// (`0x17109e`).
    Pair(i32, i32),
    /// A run a window printed as it drew, held in the frame's [`Runs`].
    Run(u16),
    /// A slot of the names table, which the original reaches by setting bit 16 of the slot.
    Named(u16),
    /// A number with a per-cent sign after it, which is the `%d%%` a bar that carries its own
    /// number shows (`0x170cb4`).
    PerCent(i32),
    /// A slot holding one `%s` with another slot's text in its place, which is what `0x505e8`
    /// builds for a geologist's find (`0x65504`).
    Filled(u16, u16),
    /// Robigo's own text, for the windows S2 does not have. Its characters are ASCII, which code
    /// page 437 holds as they are.
    Text(&'static str),
    /// Two numbers with an `x` between them, as a window's size shows.
    Size(i32, i32),
    /// A number of megabytes, as a download's size shows.
    Megabytes(i32),
    /// A slot holding one `%d` or `%ld` with a number in its place, as the settings screen's
    /// `Player %ld` takes the player's number.
    Numbered(u16, i32),
}

/// The widest a number can draw: eleven digits with a sign.
const NUMBER_LEN: usize = 12;

/// What [`Label::Filled`] replaces in its format slot.
const MARK: &[u8] = b"%s";

impl Label {
    /// Calls `with` on the label's cp437 bytes, taking a slot's text from `labels`.
    pub fn with_text<R>(self, labels: &impl Labels, with: impl FnOnce(&[u8]) -> R) -> R {
        match self {
            Self::Slot(slot) => with(labels.slot(slot)),
            Self::Number(value) => {
                let mut buffer = [0u8; NUMBER_LEN];
                let at = write_number(value, &mut buffer, NUMBER_LEN);
                with(&buffer[at..])
            }
            Self::Run(index) => with(labels.run(index)),
            Self::Named(slot) => with(labels.named(slot)),
            Self::PerCent(value) => {
                let mut buffer = [0u8; NUMBER_LEN + 1];
                buffer[NUMBER_LEN] = b'%';
                let at = write_number(value, &mut buffer, NUMBER_LEN);
                with(&buffer[at..])
            }
            Self::Filled(format, slot) => {
                let (format, text) = (labels.slot(format), labels.slot(slot));
                let mut filled = Vec::with_capacity(format.len() + text.len());
                match format.windows(MARK.len()).position(|pair| pair == MARK) {
                    Some(at) => {
                        filled.extend_from_slice(&format[..at]);
                        filled.extend_from_slice(text);
                        filled.extend_from_slice(&format[at + MARK.len()..]);
                    }
                    None => filled.extend_from_slice(format),
                }
                with(&filled)
            }
            Self::Pair(left, right) => with(&joined(left, b'/', right)),
            Self::Size(across, down) => with(&joined(across, b'x', down)),
            Self::Text(text) => with(text.as_bytes()),
            Self::Numbered(format, value) => with(&numbered(labels.slot(format), value)),
            Self::Megabytes(value) => {
                const UNIT: &[u8] = b" MB";
                let mut buffer = [0u8; NUMBER_LEN + UNIT.len()];
                buffer[NUMBER_LEN..].copy_from_slice(UNIT);
                let at = write_number(value, &mut buffer, NUMBER_LEN);
                with(&buffer[at..])
            }
        }
    }
}

/// `format` with its first `%d` or `%ld` replaced by `value` in decimal.
fn numbered(format: &[u8], value: i32) -> Vec<u8> {
    let mut buffer = [0u8; NUMBER_LEN];
    let at = write_number(value, &mut buffer, NUMBER_LEN);
    let marks: [&[u8]; 2] = [b"%ld", b"%d"];
    let found = marks.iter().find_map(|mark| {
        format
            .windows(mark.len())
            .position(|run| run == *mark)
            .map(|place| (place, mark.len()))
    });
    let Some((place, len)) = found else {
        return format.to_vec();
    };
    let mut out = Vec::with_capacity(format.len() + NUMBER_LEN);
    out.extend_from_slice(&format[..place]);
    out.extend_from_slice(&buffer[at..]);
    out.extend_from_slice(&format[place + len..]);
    out
}

/// `left` and `right` in decimal with `between` between them.
fn joined(left: i32, between: u8, right: i32) -> Vec<u8> {
    let mut buffer = [0u8; NUMBER_LEN * 2 + 1];
    let mut at = write_number(right, &mut buffer, NUMBER_LEN * 2 + 1);
    at -= 1;
    buffer[at] = between;
    at = write_number(left, &mut buffer, at);
    buffer[at..].to_vec()
}

/// Writes `value` in decimal into `buffer` ending at `end`, and gives back where it starts.
fn write_number(value: i32, buffer: &mut [u8], end: usize) -> usize {
    let mut at = end;
    let mut rest = value.unsigned_abs();
    loop {
        at -= 1;
        buffer[at] = b'0' + u8::try_from(rest % 10).unwrap_or_default();
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    if value < 0 {
        at -= 1;
        buffer[at] = b'-';
    }
    at
}

/// Where the text of a slot comes from.
///
/// The game's own table is `ONGAME.ENG` and the edition the player owns supplies it, which keeps
/// every string out of this crate.
pub trait Labels {
    /// The cp437 bytes of `slot`, or an empty run when the table has no such slot.
    fn slot(&self, slot: u16) -> &[u8];

    /// The bytes of the run a [`Label::Run`] names. A table with no runs draws nothing for one.
    fn run(&self, _index: u16) -> &[u8] {
        &[]
    }

    /// The cp437 bytes of `slot` in the names table. A source with no names draws nothing.
    fn named(&self, _slot: u16) -> &[u8] {
        &[]
    }
}

/// The runs of text a frame's windows printed, which a [`Label::Run`] points into.
///
/// `S2.EXE` prints into a buffer on the stack and draws it straight away. Robigo collects the
/// operations first, so the bytes live here until the frame is drawn.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Runs {
    bytes: Vec<u8>,
    spans: Vec<(usize, usize)>,
}

impl Runs {
    /// No runs.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bytes: Vec::new(),
            spans: Vec::new(),
        }
    }

    /// Keeps `text` and gives back the label that draws it. Past `u16::MAX` runs the label
    /// points at nothing.
    pub fn add(&mut self, text: &[u8]) -> Label {
        let at = self.bytes.len();
        self.bytes.extend_from_slice(text);
        self.spans.push((at, text.len()));
        Label::Run(u16::try_from(self.spans.len() - 1).unwrap_or(u16::MAX))
    }

    /// The bytes of run `index`, or nothing where there is no such run.
    #[must_use]
    pub fn get(&self, index: u16) -> &[u8] {
        self.spans
            .get(usize::from(index))
            .and_then(|&(at, len)| self.bytes.get(at..at + len))
            .unwrap_or(&[])
    }

    /// How many runs are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    /// Whether no run is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// Drops every run, keeping the buffers.
    pub fn clear(&mut self) {
        self.bytes.clear();
        self.spans.clear();
    }
}

/// A text table and a frame's runs together, which is what [`paint`](crate::paint::paint) reads.
#[derive(Debug, Clone, Copy)]
pub struct WithRuns<'a, L> {
    /// The game's own text table.
    pub labels: &'a L,
    /// The runs the frame's windows printed.
    pub runs: &'a Runs,
}

impl<L: Labels> Labels for WithRuns<'_, L> {
    fn slot(&self, slot: u16) -> &[u8] {
        self.labels.slot(slot)
    }

    fn run(&self, index: u16) -> &[u8] {
        self.runs.get(index)
    }

    fn named(&self, slot: u16) -> &[u8] {
        self.labels.named(slot)
    }
}

/// A table with no text in it, which draws every slot as nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoLabels;

impl Labels for NoLabels {
    fn slot(&self, _slot: u16) -> &[u8] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(label: Label) -> Vec<u8> {
        label.with_text(&NoLabels, <[u8]>::to_vec)
    }

    #[test]
    fn a_number_draws_in_decimal_with_its_sign() {
        assert_eq!(text(Label::Number(0)), b"0");
        assert_eq!(text(Label::Number(7)), b"7");
        assert_eq!(text(Label::Number(1996)), b"1996");
        assert_eq!(text(Label::Number(-42)), b"-42");
        assert_eq!(text(Label::Number(i32::MAX)), b"2147483647");
        assert_eq!(text(Label::Number(i32::MIN)), b"-2147483648");
        assert_eq!(text(Label::Pair(0, 0)), b"0/0");
        assert_eq!(text(Label::Pair(12, 3)), b"12/3");
        assert_eq!(text(Label::Pair(i32::MIN, i32::MAX)), b"-2147483648/2147483647");
    }

    #[test]
    fn a_slot_comes_from_the_table() {
        struct Two;
        impl Labels for Two {
            fn slot(&self, slot: u16) -> &[u8] {
                match slot {
                    0 => b"first",
                    1 => b"second",
                    _ => &[],
                }
            }
        }
        assert_eq!(Label::Slot(1).with_text(&Two, <[u8]>::to_vec), b"second");
        assert_eq!(Label::Slot(9).with_text(&Two, <[u8]>::to_vec), b"");
    }

    #[test]
    fn a_filled_slot_takes_another_slots_text_in_place_of_its_mark() {
        struct Three;
        impl Labels for Three {
            fn slot(&self, slot: u16) -> &[u8] {
                match slot {
                    0 => b"%s found.",
                    1 => b"Water",
                    2 => b"nothing to fill",
                    _ => &[],
                }
            }
        }
        let fill = |label: Label| label.with_text(&Three, <[u8]>::to_vec);
        assert_eq!(fill(Label::Filled(0, 1)), b"Water found.");
        assert_eq!(
            fill(Label::Filled(2, 1)),
            b"nothing to fill",
            "a format without the mark takes nothing in"
        );
        assert_eq!(fill(Label::Filled(9, 1)), b"");
    }

    #[test]
    fn a_numbered_slot_takes_the_number_in_place_of_its_mark() {
        struct Formats;
        impl Labels for Formats {
            fn slot(&self, slot: u16) -> &[u8] {
                match slot {
                    0 => b"Player %ld",
                    1 => b"%d left",
                    _ => b"no mark",
                }
            }
        }
        let fill = |label: Label| label.with_text(&Formats, <[u8]>::to_vec);
        assert_eq!(fill(Label::Numbered(0, 3)), b"Player 3");
        assert_eq!(fill(Label::Numbered(1, 12)), b"12 left");
        assert_eq!(fill(Label::Numbered(2, 5)), b"no mark");
    }

    #[test]
    fn a_glyphless_font_measures_by_its_stand_in_width() {
        let glyphs = Glyphs::fixed(9, 8);
        assert_eq!(glyphs.line_height(), 9);
        assert_eq!(glyphs.advance(b'a', Pen::TIGHT), 8);
        assert_eq!(glyphs.measure(b"abc", Pen::TIGHT), 24);
        assert_eq!(glyphs.measure(b"", Pen::TIGHT), 0);
        assert!(glyphs.glyph(b'a').is_none());
        assert!(
            glyphs.glyph(0).is_none(),
            "codes below the first slot have no glyph"
        );
    }

    #[test]
    fn spacing_lands_after_every_glyph_and_the_gap_between_lines() {
        let glyphs = Glyphs::fixed(11, 6);
        let pen = Pen {
            spacing: 1,
            line_gap: 2,
            cell: 0,
        };
        assert_eq!(glyphs.advance(b'a', pen), 7);
        assert_eq!(glyphs.measure(b"abcd", pen), 28);
        assert_eq!(glyphs.line_advance(pen), 13);
        assert_eq!(glyphs.line_advance(Pen::TIGHT), 11);
    }

    #[test]
    fn a_cell_takes_over_from_the_glyph_width() {
        let glyphs = Glyphs::fixed(9, 5);
        let pen = Pen::cells(10);
        assert!(pen.is_fixed());
        assert_eq!(glyphs.advance(b'a', pen), 10, "the cell, whatever the glyph");
        assert_eq!(glyphs.inset(b'a', pen), 2, "centred, truncated towards zero");
        assert_eq!(glyphs.inset(b'a', Pen::TIGHT), 0);
        assert_eq!(glyphs.measure(b"a\nb", pen), 30, "a fixed cell counts every byte");
    }

    #[test]
    fn measuring_skips_what_drawing_still_moves_the_pen_for() {
        let glyphs = Glyphs::fixed(9, 4);
        assert_eq!(glyphs.measure(b"ab", Pen::TIGHT), 8);
        assert_eq!(
            glyphs.measure(b"a\tb", Pen::TIGHT),
            8,
            "a tab draws a space and measures as nothing"
        );
        assert_eq!(glyphs.measure(b"a\nb", Pen::TIGHT), 8);
        assert_eq!(
            glyphs.measure(&[b'a', LAST_MEASURED, LAST_MEASURED + 1], Pen::TIGHT),
            8,
            "codes past the last measured one add nothing"
        );
    }
}
