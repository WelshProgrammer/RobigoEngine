//! Code page 437, the DOS character set the game's text files are written in.
//!
//! Bytes below 128 decode as ASCII, control characters included, so line ends survive
//! decoding. The upper half maps to the IBM PC glyphs. Every byte value seen in the shipped
//! English text decodes the same under code page 850, so 437 is a choice the data does not
//! settle. The game draws text with its own bitmap fonts, which decide what a byte shows.

/// The first byte of the upper half, which [`HIGH`] covers.
const FIRST_HIGH: u8 = 0x80;

/// Unicode for bytes 128 to 255.
const HIGH: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}',
    '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}',
    '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}', '\u{0192}',
    '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}',
    '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}',
    '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}',
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}',
    '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}',
    '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}', '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}',
    '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

/// The character a byte stands for.
#[must_use]
pub const fn decode_byte(byte: u8) -> char {
    if byte < FIRST_HIGH {
        byte as char
    } else {
        HIGH[(byte - FIRST_HIGH) as usize]
    }
}

/// Decodes a whole byte string. Never fails: every byte maps to exactly one `char`.
#[must_use]
pub fn decode(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| decode_byte(b)).collect()
}

/// The byte that stands for `c`, if code page 437 has it.
#[must_use]
pub fn encode_char(c: char) -> Option<u8> {
    if c.is_ascii() {
        return u8::try_from(c).ok();
    }
    HIGH.iter()
        .position(|&h| h == c)
        .and_then(|i| u8::try_from(i + usize::from(FIRST_HIGH)).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_and_control_bytes_pass_through() {
        assert_eq!(decode(b"Hi@\r\n%d"), "Hi@\r\n%d");
        assert_eq!(decode_byte(0), '\0');
        assert_eq!(decode_byte(0x7f), '\u{7f}');
    }

    #[test]
    fn upper_half_matches_known_glyphs() {
        assert_eq!(decode(&[0x81, 0x84, 0x8e, 0x94, 0x9a, 0xe1]), "üäÄöÜß");
        assert_eq!(decode_byte(0xf8), '°');
        assert_eq!(decode_byte(0x9b), '¢');
        assert_eq!(decode_byte(0xff), '\u{a0}');
    }

    #[test]
    fn every_byte_round_trips() {
        for b in 0..=u8::MAX {
            assert_eq!(encode_char(decode_byte(b)), Some(b), "byte {b:#04x}");
        }
        assert_eq!(encode_char('€'), None);
    }

    #[test]
    fn upper_half_has_no_duplicates() {
        let mut seen: Vec<char> = HIGH.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), HIGH.len());
    }
}
