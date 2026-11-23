//! Synthetic fixtures for unit tests: temporary folders, ISO9660 images and raw sectors.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A uniquely named folder under the system temp directory, removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("s2-data-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn write(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct Dir {
    dirs: BTreeMap<String, Dir>,
    files: BTreeMap<String, Vec<u8>>,
}

enum Item<'a> {
    Dir(&'a Dir),
    File(&'a [u8]),
}

/// Builds a 2048-byte-sector ISO9660 image holding `files` (paths use `/`; parents are implied).
pub fn iso(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut root = Dir::default();
    for (path, bytes) in files {
        let mut parts: Vec<&str> = path.split('/').collect();
        let name = parts.pop().unwrap();
        let dir = parts
            .into_iter()
            .fold(&mut root, |d, p| d.dirs.entry(p.to_owned()).or_default());
        dir.files.insert(name.to_owned(), bytes.to_vec());
    }

    let mut items = vec![(String::new(), 0, Item::Dir(&root))];
    let mut i = 0;
    while i < items.len() {
        if let Item::Dir(d) = items[i].2 {
            items.extend(d.dirs.iter().map(|(n, c)| (n.clone(), i, Item::Dir(c))));
            items.extend(
                d.files
                    .iter()
                    .map(|(n, b)| (format!("{n};1"), i, Item::File(b.as_slice()))),
            );
        }
        i += 1;
    }
    let mut kids = vec![Vec::new(); items.len()];
    for (c, item) in items.iter().enumerate().skip(1) {
        kids[item.1].push(c);
    }
    let children = |dir: usize| kids[dir].iter().copied();
    let dir_sectors = |dir: usize| {
        let sizes = [34, 34]
            .into_iter()
            .chain(children(dir).map(|c| record_len(items[c].0.len())));
        let (mut sectors, mut used) = (1, 0);
        for s in sizes {
            if used + s > 2048 {
                sectors += 1;
                used = 0;
            }
            used += s;
        }
        sectors
    };

    let mut extents = Vec::with_capacity(items.len());
    // Blocks 0–15 are the system area and 16–17 the descriptors, so extents start at 18.
    let mut next = 18;
    for n in (0..items.len()).filter(|&n| matches!(items[n].2, Item::Dir(_))) {
        extents.push((n, next, dir_sectors(n) * 2048));
        next += dir_sectors(n);
    }
    for (n, (_, _, item)) in items.iter().enumerate() {
        if let Item::File(b) = item {
            extents.push((n, next, b.len()));
            next += b.len().div_ceil(2048);
        }
    }
    extents.sort_unstable();

    let mut img = vec![0; next * 2048];
    let rec = |n: usize, name: &[u8]| {
        let (_, lba, size) = extents[n];
        record(name, lba, size, matches!(items[n].2, Item::Dir(_)))
    };
    // Volume size at 80, volume set size at 120, block size at 128, root directory record at 156.
    let pvd = &mut img[16 * 2048..17 * 2048];
    pvd[..7].copy_from_slice(b"\x01CD001\x01");
    both_endian32(&mut pvd[80..88], next);
    both_endian16(&mut pvd[120..124], 1);
    both_endian16(&mut pvd[128..132], 2048);
    pvd[156..190].copy_from_slice(&rec(0, &[0]));
    img[17 * 2048..17 * 2048 + 7].copy_from_slice(b"\xFFCD001\x01");

    for (n, (_, parent, item)) in items.iter().enumerate() {
        let (_, lba, size) = extents[n];
        match item {
            Item::File(b) => img[lba * 2048..lba * 2048 + b.len()].copy_from_slice(b),
            Item::Dir(_) => {
                let recs = [rec(n, &[0]), rec(*parent, &[1])]
                    .into_iter()
                    .chain(children(n).map(|c| rec(c, items[c].0.as_bytes())));
                let mut at = lba * 2048;
                for r in recs {
                    if (at % 2048) + r.len() > 2048 {
                        at = at.next_multiple_of(2048);
                    }
                    img[at..at + r.len()].copy_from_slice(&r);
                    at += r.len();
                }
                assert!(at <= lba * 2048 + size);
            }
        }
    }
    img
}

/// A directory record's length: 33 header bytes and the name, padded to an even length.
fn record_len(name_len: usize) -> usize {
    33 + name_len + usize::from(name_len.is_multiple_of(2))
}

fn record(name: &[u8], lba: usize, size: usize, dir: bool) -> Vec<u8> {
    let mut r = vec![0; record_len(name.len())];
    r[0] = u8::try_from(r.len()).unwrap();
    both_endian32(&mut r[2..10], lba);
    both_endian32(&mut r[10..18], size);
    r[25] = if dir { 2 } else { 0 };
    both_endian16(&mut r[28..32], 1);
    r[32] = u8::try_from(name.len()).unwrap();
    r[33..33 + name.len()].copy_from_slice(name);
    r
}

fn both_endian32(out: &mut [u8], v: usize) {
    let v = u32::try_from(v).unwrap();
    out[..4].copy_from_slice(&v.to_le_bytes());
    out[4..8].copy_from_slice(&v.to_be_bytes());
}

fn both_endian16(out: &mut [u8], v: u16) {
    out[..2].copy_from_slice(&v.to_le_bytes());
    out[2..4].copy_from_slice(&v.to_be_bytes());
}

/// Wraps 2048-byte user sectors in MODE1/2352 framing (sync, address, mode; zeroed EDC/ECC).
pub fn to_raw_mode1(cooked: &[u8]) -> Vec<u8> {
    let bcd = |v: usize| u8::try_from(v / 10 * 16 + v % 10).unwrap();
    let mut out = Vec::with_capacity(cooked.len() / 2048 * 2352);
    for (i, chunk) in cooked.chunks(2048).enumerate() {
        // Sector addresses count from 00:02:00, after track 1's 150-sector pregap.
        let frame = i + 150;
        out.push(0);
        out.extend([0xFF; 10]);
        out.push(0);
        out.extend([bcd(frame / 4500), bcd(frame / 75 % 60), bcd(frame % 75), 1]);
        out.extend_from_slice(chunk);
        out.resize(out.len() + 2048 - chunk.len() + 288, 0);
    }
    out
}

/// `count` sectors of recognisable audio: every byte is `track`.
pub fn audio_sectors(track: u8, count: usize) -> Vec<u8> {
    vec![track; count * 2352]
}
