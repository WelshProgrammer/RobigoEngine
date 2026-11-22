//! Plain text pages (`ONLINE/HELPnnnn.ENG`, `ONLINE/DEMOnnnn.ENG`, `TXT2/CREDITnn.ENG`,
//! `TXT2/KEYS.ENG`).

use super::cp437;

/// The DOS end-of-file marker; anything after it is not part of the page.
pub const EOF_MARKER: u8 = 0x1A;

/// A plain CP437 text page.
///
/// Lines end in CR LF or a bare CR (both appear in the shipped credits); a bare LF is accepted
/// too. An [`EOF_MARKER`] byte ends the page, and some files carry a stray CR LF after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    raw: Vec<u8>,
}

impl Page {
    /// Takes the page text from a file, stopping at the first [`EOF_MARKER`]. Never fails.
    #[must_use]
    pub fn parse(data: &[u8]) -> Self {
        let end = data.iter().position(|&b| b == EOF_MARKER).unwrap_or(data.len());
        Self {
            raw: data.get(..end).unwrap_or_default().to_vec(),
        }
    }

    /// The page bytes before the end-of-file marker, line ends included.
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    /// The raw bytes of each line, without line ends. A final line end does not start another
    /// line.
    pub fn raw_lines(&self) -> impl Iterator<Item = &[u8]> {
        RawLines {
            rest: Some(&self.raw),
        }
    }

    /// Each line decoded from CP437.
    pub fn lines(&self) -> impl Iterator<Item = String> {
        self.raw_lines().map(cp437::decode)
    }

    /// The whole page decoded, with every line end turned into `\n`.
    #[must_use]
    pub fn text(&self) -> String {
        self.lines().collect::<Vec<_>>().join("\n")
    }
}

struct RawLines<'a> {
    rest: Option<&'a [u8]>,
}

impl<'a> Iterator for RawLines<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        let rest = self.rest?;
        let Some(end) = rest.iter().position(|&b| b == b'\r' || b == b'\n') else {
            self.rest = None;
            return (!rest.is_empty()).then_some(rest);
        };
        let skip = if rest.get(end..end + 2) == Some(b"\r\n") {
            2
        } else {
            1
        };
        let after = rest.get(end + skip..).unwrap_or_default();
        self.rest = (!after.is_empty()).then_some(after);
        rest.get(..end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(data: &[u8]) -> Vec<String> {
        Page::parse(data).lines().collect()
    }

    #[test]
    fn splits_on_every_line_end_style() {
        assert_eq!(lines(b"a\r\nb\rc\nd"), ["a", "b", "c", "d"]);
        assert_eq!(lines(b"a\r\r\rb\r "), ["a", "", "", "b", " "]);
        assert_eq!(lines(b"a\r\n\r\nb\r\n"), ["a", "", "b"]);
    }

    #[test]
    fn stops_at_the_eof_marker() {
        let page = Page::parse(b"one\r\ntwo\r\n\x1a\r\n");
        assert_eq!(page.raw(), b"one\r\ntwo\r\n");
        assert_eq!(page.text(), "one\ntwo");
        assert!(
            Page::parse(b"\x1a").raw().is_empty(),
            "{:?}",
            Page::parse(b"\x1a").raw()
        );
        assert_eq!(Page::parse(b"\x1a").lines().count(), 0);
    }

    #[test]
    fn decodes_cp437() {
        assert_eq!(Page::parse(b"G\x81nter").text(), "Günter");
    }
}
