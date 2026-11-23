//! RTX mission scripts (`DATA/MISSIONS/MIS_nnnn.RTX`).
//!
//! A script is CP437 text split into sections by `[NAME]` lines. The game reads two of them,
//! `[GLOBAL_MAP_SETTINGS]` and `[MAPCOMMANDS]`, each from its header up to the next `[` line.
//! Inside, `!KEYWORD` lines carry a fixed number of integers; `#` lines are comments.
//!
//! The parser follows S2.EXE's script reader (`0x78D49` to `0x7921B`), so a script means
//! the same here as in the game:
//!
//! - A line is the text before an LF, minus one trailing CR. Text after the last LF is never
//!   read; the shipped files end in a lone DOS end-of-file byte there. A NUL ends a line early.
//! - Leading bytes 1 to 32 (control characters and space) and `(` are blank. The first other byte decides the line: `!`
//!   command, `[` section, `#` comment, `"`, a digit or a letter ignored. Anything else is an
//!   error, which the game reports as "Undefined Symbol".
//! - The keyword is the first identifier (`[A-Za-z_][A-Za-z0-9_]*`) on the line, matched
//!   case-sensitively.
//! - Each argument is the next run of digits after the previous one, counted from the start of
//!   the line; whatever sits between them (spaces, tabs, commas, words) is skipped. A `-` right
//!   before the digits negates. Text after the last argument is ignored, so the scripts can
//!   carry trailing remarks. Values wrap at 32 bits, as the game's do.
//! - The game looks characters up in a 128-entry class table with the top bit masked off, so a
//!   byte such as 181 (`5` with the top bit set) counts as the digit `5` but adds 181 - 48 to
//!   the value. The parser does the same.
//!
//! Where the game skips or survives a line without complaint, the parser does too and records
//! it in [`Script::quirks`]. The quirks are an unknown keyword, a line longer than the game's
//! 255-byte buffer, a command outside any section and a `[` line without a name. A missing
//! argument, an undefined first byte and a command line without a keyword stay errors.

mod keyword;
mod op;

pub use keyword::{Keyword, MAX_ARITY};
pub use op::{AmountEvent, Op, Pos, PositionEvent, SimpleEvent};

use crate::text::cp437;

/// Name of the section with map, object set and player setup.
pub const GLOBAL_MAP_SETTINGS: &str = "GLOBAL_MAP_SETTINGS";

/// Name of the section with the start-up commands and events.
pub const MAPCOMMANDS: &str = "MAPCOMMANDS";

/// Longest line, in bytes without the line end, that fits the game's line buffer.
pub const MAX_LINE_LEN: usize = 255;

/// Something the game skips or survives that the parser accepted. Line numbers count from 1.
/// The shipped scripts produce none.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Quirk {
    /// A line longer than [`MAX_LINE_LEN`], which overflows the game's line buffer. The parser
    /// reads it whole.
    LongLine {
        /// Line number.
        line: usize,
        /// Length without the line end.
        len: usize,
    },
    /// A `[` line without a name. Like any `[` line in the game, it ends the section before it.
    /// It starts no section, so the commands between it and the next named header are never
    /// read.
    MissingSectionName {
        /// Line number.
        line: usize,
    },
    /// A `!` line outside any section, which the game never reaches. The parser skips it unread.
    CommandOutsideSection {
        /// Line number.
        line: usize,
    },
    /// A keyword S2.EXE does not know. The game skips the line, and so does the parser.
    UnknownKeyword {
        /// Line number.
        line: usize,
        /// The identifier as written.
        name: String,
    },
}

/// Why a script could not be parsed. Line numbers count from 1.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The first non-blank byte starts no known kind of line.
    #[error("line {line}: unexpected byte {byte:#04x} at the start of a line")]
    UnexpectedLine {
        /// Line number.
        line: usize,
        /// The offending byte.
        byte: u8,
    },
    /// A `!` line with no identifier on it.
    #[error("line {line}: command without a keyword")]
    MissingKeyword {
        /// Line number.
        line: usize,
    },
    /// The line ran out of digits before the keyword had all its arguments.
    #[error("line {line}: {keyword} needs {} arguments, found {found}", keyword.arity())]
    MissingArgument {
        /// Line number.
        line: usize,
        /// The command being read.
        keyword: Keyword,
        /// How many arguments were found.
        found: usize,
    },
}

/// One command and the line it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Command {
    /// Line number, counting from 1.
    pub line: usize,
    /// The command with its arguments.
    pub op: Op,
}

/// A `[NAME]` section and the commands under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The first identifier on the header line.
    pub name: String,
    /// Line number of the header.
    pub line: usize,
    /// Commands up to the next header, in file order.
    pub commands: Vec<Command>,
}

/// A parsed mission script.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Script {
    /// Every section in file order, including ones the game never reads (such as `[ENDE]`).
    pub sections: Vec<Section>,
    quirks: Vec<Quirk>,
}

impl Script {
    /// Parses a whole script.
    ///
    /// # Errors
    ///
    /// The first problem the game treats as fatal, with its line number; see [`Error`].
    /// Lines the game skips go to [`Script::quirks`].
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let mut sections: Vec<Section> = Vec::new();
        let mut quirks = Vec::new();
        let mut open = false;
        for (index, line) in lines(data) {
            let number = index + 1;
            if line.len() > MAX_LINE_LEN {
                quirks.push(Quirk::LongLine {
                    line: number,
                    len: line.len(),
                });
            }
            let line = line.split(|&b| b == 0).next().unwrap_or_default();
            match classify(line) {
                Kind::Ignored => {}
                Kind::Unexpected(byte) => {
                    return Err(Error::UnexpectedLine { line: number, byte });
                }
                Kind::Section => {
                    open = if let Some(name) = identifier(line) {
                        sections.push(Section {
                            name: cp437::decode(name),
                            line: number,
                            commands: Vec::new(),
                        });
                        true
                    } else {
                        quirks.push(Quirk::MissingSectionName { line: number });
                        false
                    };
                }
                Kind::Command => match sections.last_mut().filter(|_| open) {
                    Some(section) => {
                        if let Some(op) = command(line, number, &mut quirks)? {
                            section.commands.push(Command { line: number, op });
                        }
                    }
                    None => quirks.push(Quirk::CommandOutsideSection { line: number }),
                },
            }
        }
        Ok(Self { sections, quirks })
    }

    /// What the game would skip or survive, in line order; empty for every shipped script.
    #[must_use]
    pub fn quirks(&self) -> &[Quirk] {
        &self.quirks
    }

    /// The first section called `name`, the one the game reads.
    #[must_use]
    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// The `[GLOBAL_MAP_SETTINGS]` section.
    #[must_use]
    pub fn global_settings(&self) -> Option<&Section> {
        self.section(GLOBAL_MAP_SETTINGS)
    }

    /// The `[MAPCOMMANDS]` section.
    #[must_use]
    pub fn map_commands(&self) -> Option<&Section> {
        self.section(MAPCOMMANDS)
    }

    /// Every command in every section, in file order.
    pub fn commands(&self) -> impl Iterator<Item = &Command> {
        self.sections.iter().flat_map(|s| &s.commands)
    }
}

/// The LF-terminated lines of `data` with their 0-based index, each without its LF and one
/// trailing CR. An unterminated final piece is dropped.
fn lines(data: &[u8]) -> impl Iterator<Item = (usize, &[u8])> {
    data.iter()
        .rposition(|&b| b == b'\n')
        .and_then(|end| data.get(..end))
        .into_iter()
        .flat_map(|body| body.split(|&b| b == b'\n'))
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
        .enumerate()
}

enum Kind {
    Ignored,
    Section,
    Command,
    Unexpected(u8),
}

/// The byte with its top bit cleared, as S2.EXE's 128-entry class table sees it.
const fn low(b: u8) -> u8 {
    b % 128
}

const fn is_blank(b: u8) -> bool {
    matches!(low(b), 1..=b' ' | b'(')
}

const fn is_digit(b: u8) -> bool {
    low(b).is_ascii_digit()
}

const fn is_ident_start(b: u8) -> bool {
    let c = low(b);
    c.is_ascii_alphabetic() || c == b'_'
}

const fn is_ident(b: u8) -> bool {
    is_ident_start(b) || is_digit(b)
}

fn classify(line: &[u8]) -> Kind {
    match line.iter().copied().find(|&b| !is_blank(b)) {
        Some(b'!') => Kind::Command,
        Some(b'[') => Kind::Section,
        Some(b) if !(b == b'#' || b == b'"' || is_digit(b) || is_ident_start(b)) => Kind::Unexpected(b),
        _ => Kind::Ignored,
    }
}

/// The first identifier on the line.
fn identifier(line: &[u8]) -> Option<&[u8]> {
    let start = line.iter().position(|&b| is_ident_start(b))?;
    let rest = line.get(start..)?;
    let len = rest.iter().position(|&b| !is_ident(b)).unwrap_or(rest.len());
    rest.get(..len)
}

/// The command on a `!` line, or `None` (with a quirk) for a keyword the game does not know.
fn command(line: &[u8], number: usize, quirks: &mut Vec<Quirk>) -> Result<Option<Op>, Error> {
    let name = identifier(line).ok_or(Error::MissingKeyword { line: number })?;
    let Some(keyword) = Keyword::from_name(name) else {
        quirks.push(Quirk::UnknownKeyword {
            line: number,
            name: cp437::decode(name),
        });
        return Ok(None);
    };
    let mut args = [0; MAX_ARITY];
    let mut cursor = 0;
    for (found, slot) in args.iter_mut().take(keyword.arity()).enumerate() {
        let (value, end) = number_at(line, cursor).ok_or(Error::MissingArgument {
            line: number,
            keyword,
            found,
        })?;
        *slot = value;
        cursor = end;
    }
    Ok(Some(Op::from_args(keyword, args)))
}

/// The next number at or after `from`, and the offset just past its digits.
fn number_at(line: &[u8], from: usize) -> Option<(i32, usize)> {
    let start = from + line.get(from..)?.iter().position(|&b| is_digit(b))?;
    let negative = start
        .checked_sub(1)
        .and_then(|i| line.get(i))
        .is_some_and(|&b| b == b'-');
    let digits = line.get(start..)?;
    let len = digits.iter().position(|&b| !is_digit(b)).unwrap_or(digits.len());
    // The unmasked byte goes into the sum, so 181 adds 181 - 48 (see the module doc).
    let magnitude = digits.iter().take(len).fold(0u32, |acc, &b| {
        acc.wrapping_mul(10)
            .wrapping_add(u32::from(b))
            .wrapping_sub(u32::from(b'0'))
    });
    let value = i32::from_ne_bytes(magnitude.to_ne_bytes());
    Some((if negative { value.wrapping_neg() } else { value }, start + len))
}

#[cfg(test)]
mod tests;
