//! Sector layouts of CD images and a reader that hides them.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

const SYNC: [u8; 12] = [0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0];
const BULK_SECTORS: u64 = 32;

/// How user bytes sit inside the sectors of an image file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Geometry {
    /// Bytes per sector in the file.
    pub sector: u64,
    /// Offset of the user bytes within each sector.
    pub offset: u64,
    /// User bytes per sector.
    pub payload: u64,
}

impl Geometry {
    pub const COOKED: Self = Self {
        sector: 2048,
        offset: 0,
        payload: 2048,
    };
    pub const MODE1_RAW: Self = Self {
        sector: 2352,
        offset: 16,
        payload: 2048,
    };
    pub const MODE2_RAW: Self = Self {
        sector: 2352,
        offset: 24,
        payload: 2048,
    };
    pub const MODE2_2336: Self = Self {
        sector: 2336,
        offset: 8,
        payload: 2048,
    };
    pub const AUDIO: Self = Self {
        sector: 2352,
        offset: 0,
        payload: 2352,
    };

    /// Guesses the layout from the first bytes of a data track.
    pub fn detect(head: &[u8]) -> Self {
        // A raw sector opens with the 12-byte sync pattern; byte 15 of its header is the mode.
        match (head.get(..12), head.get(15)) {
            (Some(sync), Some(2)) if sync == SYNC => Self::MODE2_RAW,
            (Some(sync), Some(_)) if sync == SYNC => Self::MODE1_RAW,
            _ => Self::COOKED,
        }
    }

    fn contiguous(self) -> bool {
        self.offset == 0 && self.payload == self.sector
    }

    fn file_offset(self, base: u64, logical: u64) -> u64 {
        base + logical / self.payload * self.sector + self.offset + logical % self.payload
    }
}

/// Reads a run of user bytes from an image file, skipping sector headers and error correction.
#[derive(Debug)]
pub(crate) struct SpanReader {
    file: File,
    geometry: Geometry,
    base: u64,
    start: u64,
    len: u64,
    pos: u64,
    file_pos: Option<u64>,
    scratch: Vec<u8>,
}

impl SpanReader {
    /// `base` is the file offset of logical byte 0; the span covers `len` logical bytes from `start`.
    pub fn new(file: File, geometry: Geometry, base: u64, start: u64, len: u64) -> Self {
        Self {
            file,
            geometry,
            base,
            start,
            len,
            pos: 0,
            file_pos: None,
            scratch: Vec::new(),
        }
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    fn seek_file(&mut self, to: u64) -> io::Result<()> {
        if self.file_pos != Some(to) {
            self.file_pos = None;
            self.file.seek(SeekFrom::Start(to))?;
            self.file_pos = Some(to);
        }
        Ok(())
    }

    fn read_raw(&mut self, to: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.seek_file(to)?;
        let n = self.file.read(buf).inspect_err(|_| self.file_pos = None)?;
        self.file_pos = Some(to + n as u64);
        Ok(n)
    }

    fn read_sectors(&mut self, logical: u64, out: &mut [u8]) -> io::Result<usize> {
        let g = self.geometry;
        let sectors = out.len() as u64 / g.payload;
        let raw_len = to_usize((sectors - 1) * g.sector + g.offset + g.payload);
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.resize(raw_len, 0);
        let from = g.file_offset(self.base, logical);
        let result = self
            .seek_file(from)
            .and_then(|()| self.file.read_exact(&mut scratch));
        self.file_pos = result.is_ok().then_some(from + raw_len as u64);
        let payload = to_usize(g.payload);
        let sector = to_usize(g.sector);
        for (dst, src) in out.chunks_exact_mut(payload).zip(scratch.chunks(sector)) {
            dst.copy_from_slice(&src[..payload]);
        }
        self.scratch = scratch;
        result.map(|()| out.len())
    }
}

impl Read for SpanReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let want = buf.len().min(to_usize(self.len.saturating_sub(self.pos)));
        if want == 0 {
            return Ok(0);
        }
        let g = self.geometry;
        let logical = self.start + self.pos;
        let within = logical % g.payload;
        let n = if g.contiguous() {
            self.read_raw(self.base + logical, &mut buf[..want])?
        } else if within == 0 && want as u64 >= g.payload {
            let sectors = (want as u64 / g.payload).min(BULK_SECTORS);
            self.read_sectors(logical, &mut buf[..to_usize(sectors * g.payload)])?
        } else {
            let take = want.min(to_usize(g.payload - within));
            self.read_raw(g.file_offset(self.base, logical), &mut buf[..take])?
        };
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for SpanReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::End(delta) => self.len.checked_add_signed(delta),
            SeekFrom::Current(delta) => self.pos.checked_add_signed(delta),
        };
        self.pos = target.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "seek before the start of the file")
        })?;
        Ok(self.pos)
    }
}

/// Converts a length to `usize`, saturating on 32-bit targets where it cannot fit.
pub(crate) fn to_usize(n: u64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, to_raw_mode1};

    fn patterned(len: usize) -> Vec<u8> {
        (0..len).map(|i| u8::try_from(i * 7 % 251).unwrap()).collect()
    }

    fn reader(dir: &TempDir, bytes: &[u8], geometry: Geometry, start: u64, len: u64) -> SpanReader {
        let path = dir.write("img", bytes);
        SpanReader::new(File::open(path).unwrap(), geometry, 0, start, len)
    }

    #[test]
    fn detects_layouts() {
        let cooked = vec![0; 4096];
        assert_eq!(Geometry::detect(&cooked), Geometry::COOKED);
        let raw = to_raw_mode1(&cooked);
        assert_eq!(Geometry::detect(&raw), Geometry::MODE1_RAW);
        let mut mode2 = raw.clone();
        mode2[15] = 2;
        assert_eq!(Geometry::detect(&mode2), Geometry::MODE2_RAW);
    }

    #[test]
    fn raw_and_cooked_read_the_same_bytes() {
        let dir = TempDir::new();
        let user = patterned(2048 * 40);
        let raw = to_raw_mode1(&user);
        for (start, len) in [(0, user.len()), (100, 5000), (2047, 2), (2048 * 3, 2048 * 35)] {
            let expect = &user[start..start + len];
            for (bytes, g) in [(&user, Geometry::COOKED), (&raw, Geometry::MODE1_RAW)] {
                let mut out = Vec::new();
                reader(&dir, bytes, g, start as u64, len as u64)
                    .read_to_end(&mut out)
                    .unwrap();
                assert_eq!(out, expect, "start {start} len {len} geometry {g:?}");
            }
        }
    }

    #[test]
    fn seeks_within_the_span() {
        let dir = TempDir::new();
        let user = patterned(2048 * 4);
        let mut r = reader(&dir, &to_raw_mode1(&user), Geometry::MODE1_RAW, 10, 6000);
        let mut buf = [0; 16];
        r.seek(SeekFrom::End(-16)).unwrap();
        r.read_exact(&mut buf).unwrap();
        assert_eq!(buf, user[5994..6010]);
        r.seek(SeekFrom::Start(2040)).unwrap();
        r.read_exact(&mut buf).unwrap();
        assert_eq!(buf, user[2050..2066]);
        r.seek(SeekFrom::Current(-32)).unwrap();
        r.read_exact(&mut buf).unwrap();
        assert_eq!(buf, user[2034..2050]);
        assert!(r.seek(SeekFrom::Current(-10_000)).is_err());
        r.seek(SeekFrom::Start(9000)).unwrap();
        assert_eq!(r.read(&mut buf).unwrap(), 0);
    }
}
