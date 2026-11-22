//! The looks a window draws in, as the original's table of them holds it.
//!
//! `S2.EXE` keeps seven records at `0x1fc10c` and every window, button and label picks one by
//! index. A record names its items rather than numbering them, because the original finds an
//! `IO.DAT` item by name through a registry.

use std::borrow::Cow;

use crate::text::FontId;

/// One window style: the items its background and frame draw from, the font its text uses, and
/// the two colours that text draws in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    /// `IO.DAT` item name for the background tile.
    pub background: Cow<'static, str>,
    /// `IO.DAT` item name for the background tile in the other state.
    pub alternate: Cow<'static, str>,
    /// `IO.DAT` item name for the frame.
    pub frame: Cow<'static, str>,
    /// Which font the text draws in.
    pub font: FontId,
    /// The colour text draws in.
    pub colour: u8,
    /// The colour text draws in when the control reports the other state.
    pub other_colour: u8,
}

/// What a style's text draws in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lettering {
    /// The font.
    pub font: FontId,
    /// The colour.
    pub colour: u8,
    /// The colour when the control reports the other state.
    pub other_colour: u8,
}

impl Style {
    /// The item names, background first and frame last.
    #[must_use]
    pub fn names(&self) -> [&str; 3] {
        [&self.background, &self.alternate, &self.frame]
    }

    /// What its text draws in.
    #[must_use]
    pub const fn lettering(&self) -> Lettering {
        Lettering {
            font: self.font,
            colour: self.colour,
            other_colour: self.other_colour,
        }
    }
}

/// The table of styles, one a style index below [`NO_FRAME`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Styles(Cow<'static, [Style; COUNT]>);

impl Styles {
    /// The original's table.
    pub const ORIGINAL: Self = Self(Cow::Borrowed(&ORIGINAL));

    /// A custom table: `styles[index]` is style `index`.
    #[must_use]
    pub fn new(styles: [Style; COUNT]) -> Self {
        Self(Cow::Owned(styles))
    }

    /// Style `index`, which is nothing for [`NO_FRAME`] and for anything past the table.
    #[must_use]
    pub fn get(&self, index: u16) -> Option<&Style> {
        self.0.get(usize::from(index))
    }

    /// What style `index`'s text draws in, which is nothing for [`NO_FRAME`] and for anything
    /// past the table.
    #[must_use]
    pub fn lettering(&self, index: u16) -> Option<Lettering> {
        self.get(index).map(Style::lettering)
    }

    /// What the text of style 0, the one most controls take, draws in.
    #[must_use]
    pub fn first(&self) -> Lettering {
        self.0[0].lettering()
    }

    /// Every style, by index.
    #[must_use]
    pub fn all(&self) -> &[Style; COUNT] {
        &self.0
    }
}

impl Default for Styles {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

/// The palette index most styles draw text in.
const TEXT: u8 = 1;

/// The palette index most styles draw text in for the other state.
const TEXT_OTHER: u8 = 3;

/// How many styles the original's table holds.
pub const COUNT: usize = 7;

/// The seven styles the original fills in, in the order of its own table.
pub const ORIGINAL: [Style; COUNT] = [
    Style {
        background: Cow::Borrowed("back_01"),
        alternate: Cow::Borrowed("back_00"),
        frame: Cow::Borrowed("rahm_00"),
        font: FontId::Medium,
        colour: TEXT,
        other_colour: TEXT_OTHER,
    },
    Style {
        background: Cow::Borrowed("back_03"),
        alternate: Cow::Borrowed("back_02"),
        frame: Cow::Borrowed("rahm_01"),
        font: FontId::Medium,
        colour: TEXT,
        other_colour: TEXT_OTHER,
    },
    Style {
        background: Cow::Borrowed("back_05"),
        alternate: Cow::Borrowed("back_04"),
        frame: Cow::Borrowed("rahm_02"),
        font: FontId::Medium,
        colour: TEXT,
        other_colour: TEXT_OTHER,
    },
    Style {
        background: Cow::Borrowed("back_07"),
        alternate: Cow::Borrowed("back_06"),
        frame: Cow::Borrowed("rahm_03"),
        font: FontId::Medium,
        colour: TEXT,
        other_colour: TEXT_OTHER,
    },
    Style {
        background: Cow::Borrowed("back_01"),
        alternate: Cow::Borrowed("back_00"),
        frame: Cow::Borrowed("rahm_00"),
        font: FontId::Medium,
        colour: 98,
        other_colour: 96,
    },
    Style {
        background: Cow::Borrowed("back_01"),
        alternate: Cow::Borrowed("back_00"),
        frame: Cow::Borrowed("rahm_00"),
        font: FontId::Large,
        colour: TEXT,
        other_colour: TEXT_OTHER,
    },
    Style {
        background: Cow::Borrowed("back_01"),
        alternate: Cow::Borrowed("back_00"),
        frame: Cow::Borrowed("rahm_00"),
        font: FontId::Small,
        colour: TEXT,
        other_colour: TEXT_OTHER,
    },
];

/// The style index the original keeps no record for. `0x51292` matches it ahead of the table and
/// takes a path that fills flat palette indices in place of the items a record would name.
pub const NO_FRAME: u16 = 7;

/// The palette indices that path draws with (`0x515c2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flat {
    /// The face.
    pub face: u8,
    /// The face when the control reports the other state.
    pub other: u8,
    /// The strip along the edges.
    pub edge: u8,
}

/// What [`NO_FRAME`] fills with: the face from `0x51883`, its other state from `0x5187c` and the
/// edges from `0x515f4`.
pub const FLAT: Flat = Flat {
    face: 228,
    other: 230,
    edge: 232,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_runs_out_before_the_sentinel() {
        let styles = Styles::ORIGINAL;
        assert!(styles.get(0).is_some());
        assert!(styles.get(6).is_some());
        assert_eq!(styles.get(NO_FRAME), None);
        assert_eq!(styles.get(u16::MAX), None);
    }

    #[test]
    fn the_three_fonts_each_have_a_style() {
        let fonts: Vec<FontId> = ORIGINAL.iter().map(|style| style.font).collect();
        assert!(fonts.contains(&FontId::Small));
        assert!(fonts.contains(&FontId::Medium));
        assert!(fonts.contains(&FontId::Large));
    }

    #[test]
    fn every_style_names_a_background_and_a_frame() {
        for style in &ORIGINAL {
            let [background, alternate, frame] = style.names();
            assert!(background.starts_with("back_"));
            assert!(alternate.starts_with("back_"));
            assert!(frame.starts_with("rahm_"));
        }
    }
}
