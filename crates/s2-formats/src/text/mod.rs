//! `.ENG` string tables and text pages.
//!
//! Two layouts share the `.ENG` extension:
//!
//! - [`StringTable`]: binary tables of numbered slots (`TXT/ONGAME`, `TXT/NAMES`,
//!   `TXT/MISS_nnn`, Gold's `TXT3/ONGAME`). Text in them uses `@` as a line break and C
//!   `printf` conversions (`%s`, `%d`, `%ld`, `%2.2ld`) that the game fills in.
//! - [`Page`]: plain text with CR LF or CR line ends (`ONLINE/*`, `TXT2/*`).
//!
//! Both are code page 437; [`cp437`] is the crate's decoder for any game text.

pub mod cp437;
mod page;
mod table;

pub use page::{EOF_MARKER, Page};
pub use table::{Error, MAGIC, Quirk, StringTable};

/// The byte (and character) that breaks a line in game text.
pub const LINE_BREAK: char = '@';

/// Splits game text at each `@`. Runs of `@` give empty lines, kept because the game uses them
/// for vertical space.
pub fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split(LINE_BREAK)
}

/// Splits raw game text at each `@` byte, without decoding.
pub fn raw_lines(text: &[u8]) -> impl Iterator<Item = &[u8]> {
    text.split(|&b| char::from(b) == LINE_BREAK)
}

/// Replaces each `@` with `\n`.
#[must_use]
pub fn with_newlines(text: &str) -> String {
    text.replace(LINE_BREAK, "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_sign_breaks_lines() {
        assert_eq!(lines("a@b@@c").collect::<Vec<_>>(), ["a", "b", "", "c"]);
        assert_eq!(lines("").collect::<Vec<_>>(), [""]);
        assert_eq!(raw_lines(b"x@\x81").collect::<Vec<_>>(), [&b"x"[..], &[0x81][..]]);
        assert_eq!(with_newlines("Name:@@Text"), "Name:\n\nText");
    }
}
