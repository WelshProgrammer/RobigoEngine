//! Cue sheet parsing, covering both `.cue` files and GOG's `.ins` files (same syntax).

use crate::sector::Geometry;

/// Sectors per second of Red Book audio, which is also the frame rate of cue `mm:ss:ff` times.
pub(crate) const FRAMES_PER_SECOND: u32 = 75;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CueSheet {
    pub files: Vec<CueFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CueFile {
    /// File name exactly as written in the sheet, relative to the sheet's folder.
    pub name: String,
    /// True for `BINARY`; `MOTOROLA`, `WAVE`, `MP3` and friends are not raw little-endian sectors.
    pub binary: bool,
    pub tracks: Vec<CueTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CueTrack {
    pub number: u8,
    pub mode: TrackMode,
    /// `INDEX 00`, the start of the pregap, in sectors from the start of the file.
    pub pregap: Option<u32>,
    /// `INDEX 01`, where the track proper begins, in sectors from the start of the file.
    pub start: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TrackMode {
    Audio,
    Data(Geometry),
    Unsupported(String),
}

/// A parse failure: one-based line number (0 for whole-sheet problems) and a reason.
pub(crate) type CueError = (usize, String);

struct Pending {
    line: usize,
    number: u8,
    mode: TrackMode,
    pregap: Option<u32>,
    start: Option<u32>,
}

/// Parses the text of a cue sheet.
pub(crate) fn parse(text: &str) -> Result<CueSheet, CueError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut files: Vec<(String, bool, Vec<Pending>)> = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = n + 1;
        let fail = |reason: String| (line, reason);
        let raw = raw.trim();
        let (keyword, rest) = raw.split_once(char::is_whitespace).unwrap_or((raw, ""));
        let rest = rest.trim();
        match keyword.to_ascii_uppercase().as_str() {
            "FILE" => {
                let (name, kind) = file_args(rest).ok_or_else(|| fail(format!("cannot read FILE {rest}")))?;
                files.push((name, kind.eq_ignore_ascii_case("BINARY"), Vec::new()));
            }
            "TRACK" => {
                let (_, _, tracks) = files
                    .last_mut()
                    .ok_or_else(|| fail("TRACK appears before any FILE".into()))?;
                let mut words = rest.split_whitespace();
                let number = words
                    .next()
                    .and_then(|w| w.parse::<u8>().ok())
                    .filter(|n| (1..=99).contains(n))
                    .ok_or_else(|| fail(format!("bad track number in TRACK {rest}")))?;
                let mode = track_mode(words.next().unwrap_or_default());
                tracks.push(Pending {
                    line,
                    number,
                    mode,
                    pregap: None,
                    start: None,
                });
            }
            "INDEX" => {
                let track = files
                    .last_mut()
                    .and_then(|(_, _, tracks)| tracks.last_mut())
                    .ok_or_else(|| fail("INDEX appears before any TRACK".into()))?;
                let mut words = rest.split_whitespace();
                let index = words.next().and_then(|w| w.parse::<u8>().ok());
                let at = words.next().and_then(msf);
                match (index, at) {
                    (Some(0), Some(at)) => track.pregap = Some(at),
                    (Some(1), Some(at)) => track.start = Some(at),
                    (Some(_), Some(_)) => {}
                    _ => return Err(fail(format!("cannot read INDEX {rest}"))),
                }
            }
            _ => {}
        }
    }
    finish(files)
}

fn finish(files: Vec<(String, bool, Vec<Pending>)>) -> Result<CueSheet, CueError> {
    let mut last_number = 0;
    let mut out = Vec::with_capacity(files.len());
    for (name, binary, pending) in files {
        let mut tracks: Vec<CueTrack> = Vec::with_capacity(pending.len());
        for p in pending {
            let start = p
                .start
                .ok_or((p.line, format!("track {} has no INDEX 01", p.number)))?;
            if p.number <= last_number {
                return Err((p.line, format!("track {} is out of order", p.number)));
            }
            if p.pregap.is_some_and(|g| g > start) || tracks.last().is_some_and(|t| t.start > start) {
                return Err((
                    p.line,
                    format!("track {} starts before the one in front of it", p.number),
                ));
            }
            last_number = p.number;
            tracks.push(CueTrack {
                number: p.number,
                mode: p.mode,
                pregap: p.pregap,
                start,
            });
        }
        out.push(CueFile { name, binary, tracks });
    }
    if out.iter().all(|f| f.tracks.is_empty()) {
        return Err((0, "the sheet lists no tracks".into()));
    }
    Ok(CueSheet { files: out })
}

/// Splits `"name with spaces.bin" BINARY` (quotes optional) into name and type.
fn file_args(rest: &str) -> Option<(String, &str)> {
    let (name, kind) = match rest.strip_prefix('"') {
        Some(quoted) => quoted.split_once('"')?,
        None => rest.rsplit_once(char::is_whitespace)?,
    };
    let name = name.trim();
    (!name.is_empty()).then(|| (name.to_owned(), kind.trim()))
}

fn track_mode(word: &str) -> TrackMode {
    match word.to_ascii_uppercase().as_str() {
        "AUDIO" => TrackMode::Audio,
        "MODE1/2048" => TrackMode::Data(Geometry::COOKED),
        "MODE1/2352" => TrackMode::Data(Geometry::MODE1_RAW),
        "MODE2/2352" => TrackMode::Data(Geometry::MODE2_RAW),
        "MODE2/2336" => TrackMode::Data(Geometry::MODE2_2336),
        _ => TrackMode::Unsupported(word.to_owned()),
    }
}

/// Parses `mm:ss:ff` into a sector count.
fn msf(text: &str) -> Option<u32> {
    let mut parts = text.split(':').map(|p| p.parse::<u32>().ok());
    let (m, s, f) = (parts.next()??, parts.next()??, parts.next()??);
    (parts.next().is_none() && s < 60 && f < FRAMES_PER_SECOND).then(|| (m * 60 + s) * FRAMES_PER_SECOND + f)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASTER: &str = r#"FILE "Settlers2MASTER.bin" BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 00 33:38:00
    INDEX 01 33:40:00
  TRACK 03 AUDIO
    INDEX 01 36:15:67
"#;

    #[test]
    fn parses_single_file_sheet() {
        let sheet = parse(MASTER).unwrap();
        assert_eq!(sheet.files.len(), 1);
        let file = &sheet.files[0];
        assert_eq!(file.name, "Settlers2MASTER.bin");
        assert!(file.binary);
        assert_eq!(file.tracks[0].mode, TrackMode::Data(Geometry::MODE1_RAW));
        assert_eq!(file.tracks[1].mode, TrackMode::Audio);
        assert_eq!(file.tracks[1].pregap, Some((33 * 60 + 38) * 75));
        assert_eq!(file.tracks[1].start, (33 * 60 + 40) * 75);
        assert_eq!(file.tracks[2].start, (36 * 60 + 15) * 75 + 67);
    }

    #[test]
    fn parses_multi_file_sheet_with_noise() {
        let text = "\u{feff}REM GENRE Strategy\r\nCATALOG 0000000000000\r\n\
                    file Track01.bin binary\r\n track 1 mode1/2048\r\n  index 1 00:00:00\r\n\
                    FILE \"Track 02.wav\" WAVE\r\n TRACK 2 AUDIO\r\n  FLAGS DCP\r\n  PREGAP 00:02:00\r\n\
                    INDEX 01 00:00:00\r\n";
        let sheet = parse(text).unwrap();
        assert_eq!(sheet.files[0].name, "Track01.bin");
        assert_eq!(sheet.files[0].tracks[0].mode, TrackMode::Data(Geometry::COOKED));
        assert_eq!(sheet.files[1].name, "Track 02.wav");
        assert!(!sheet.files[1].binary);
        assert_eq!(sheet.files[1].tracks[0].pregap, None);
    }

    #[test]
    fn reports_line_numbers() {
        assert_eq!(parse("TRACK 01 AUDIO").unwrap_err().0, 1);
        assert_eq!(
            parse("FILE a.bin BINARY\nTRACK 01 AUDIO\nINDEX 01 00:61:00")
                .unwrap_err()
                .0,
            3
        );
        assert_eq!(parse("FILE a.bin BINARY\nTRACK 01 AUDIO\n").unwrap_err().0, 2);
        let backwards = "FILE a BINARY\nTRACK 1 AUDIO\nINDEX 01 00:05:00\nTRACK 2 AUDIO\nINDEX 01 00:01:00";
        assert_eq!(parse(backwards).unwrap_err().0, 4);
        assert_eq!(parse("REM nothing here").unwrap_err().0, 0);
    }

    #[test]
    fn reads_msf() {
        assert_eq!(msf("00:02:21"), Some(171));
        assert_eq!(msf("01:00:00"), Some(4500));
        assert_eq!(msf("00:00:75"), None);
        assert_eq!(msf("1:2"), None);
    }
}
