//! A read-only ISO9660 directory index, built once when an image is opened.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};

use crate::path::cmp_ci;

const BLOCK_LEN: usize = 2048;
const BLOCK: u64 = BLOCK_LEN as u64;
const FIRST_DESCRIPTOR: u64 = 16;
const MAX_DESCRIPTORS: u64 = 32;
const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 1 << 18;
const FLAG_DIR: u8 = 0x02;
const FLAG_MORE_EXTENTS: u8 = 0x80;

/// The whole directory tree of an ISO9660 volume (primary descriptor only; Joliet is ignored).
#[derive(Debug, Clone)]
pub(crate) struct IsoTree {
    nodes: Vec<IsoNode>,
}

#[derive(Debug, Clone)]
pub(crate) struct IsoNode {
    /// Name with the `;1` version and any trailing `.` removed.
    pub name: String,
    pub kind: IsoKind,
}

#[derive(Debug, Clone)]
pub(crate) enum IsoKind {
    /// Child node indices, sorted case-insensitively by name.
    Dir(Vec<usize>),
    File {
        lba: u32,
        len: u32,
    },
}

impl IsoTree {
    pub const ROOT: usize = 0;

    /// Indexes a volume of `size` bytes whose logical 2048-byte blocks `r` exposes.
    pub fn read<R: Read + Seek>(r: &mut R, size: u64) -> Result<Self, String> {
        let root = find_root_record(r)?;
        let mut tree = Self {
            nodes: vec![IsoNode {
                name: String::new(),
                kind: IsoKind::Dir(Vec::new()),
            }],
        };
        let mut visited = HashSet::new();
        let mut pending = vec![(Self::ROOT, root.lba, root.len, String::new(), 0)];
        while let Some((node, lba, len, path, depth)) = pending.pop() {
            if depth > MAX_DEPTH || !visited.insert(lba) {
                return Err(format!(
                    "directory /{path} is nested too deeply or loops back on itself"
                ));
            }
            let data = read_extent(r, size, lba, len).ok_or_else(|| past_end(&path))?;
            let mut children = Vec::new();
            for rec in records(&data) {
                let rec = rec.map_err(|e| format!("directory /{path}: {e}"))?;
                if rec.name.is_empty() {
                    continue;
                }
                let child_path = if path.is_empty() {
                    rec.name.clone()
                } else {
                    format!("{path}/{}", rec.name)
                };
                if rec.flags & FLAG_MORE_EXTENTS != 0 {
                    return Err(format!(
                        "{child_path} is split into several extents, which is not supported"
                    ));
                }
                if u64::from(rec.lba) * BLOCK + u64::from(rec.len) > size {
                    return Err(past_end(&child_path));
                }
                if tree.nodes.len() >= MAX_NODES {
                    return Err("the directory tree is implausibly large".to_owned());
                }
                let index = tree.nodes.len();
                let kind = if rec.flags & FLAG_DIR == 0 {
                    IsoKind::File {
                        lba: rec.lba,
                        len: rec.len,
                    }
                } else {
                    pending.push((index, rec.lba, rec.len, child_path, depth + 1));
                    IsoKind::Dir(Vec::new())
                };
                tree.nodes.push(IsoNode { name: rec.name, kind });
                children.push(index);
            }
            children.sort_by(|&a, &b| cmp_ci(&tree.nodes[a].name, &tree.nodes[b].name));
            children.dedup_by(|a, b| tree.nodes[*a].name.eq_ignore_ascii_case(&tree.nodes[*b].name));
            tree.nodes[node].kind = IsoKind::Dir(children);
        }
        Ok(tree)
    }

    pub fn node(&self, index: usize) -> &IsoNode {
        &self.nodes[index]
    }

    /// Children of a directory node; empty for files.
    pub fn children(&self, dir: usize) -> &[usize] {
        match &self.nodes[dir].kind {
            IsoKind::Dir(children) => children,
            IsoKind::File { .. } => &[],
        }
    }

    /// Follows `parts` from the root, ignoring ASCII case.
    pub fn walk<S: AsRef<str>>(&self, parts: &[S]) -> Option<usize> {
        parts.iter().try_fold(Self::ROOT, |dir, part| {
            let children = self.children(dir);
            children
                .binary_search_by(|&c| cmp_ci(&self.nodes[c].name, part.as_ref()))
                .ok()
                .map(|i| children[i])
        })
    }
}

fn past_end(path: &str) -> String {
    format!("/{path} lies past the end of the image, so the image is probably truncated")
}

struct Record {
    name: String,
    lba: u32,
    len: u32,
    flags: u8,
}

fn find_root_record<R: Read + Seek>(r: &mut R) -> Result<Record, String> {
    let mut block = [0; BLOCK_LEN];
    for n in FIRST_DESCRIPTOR..FIRST_DESCRIPTOR + MAX_DESCRIPTORS {
        r.seek(SeekFrom::Start(n * BLOCK)).map_err(|e| e.to_string())?;
        r.read_exact(&mut block)
            .map_err(|_| "no ISO9660 volume descriptor found".to_owned())?;
        if &block[1..6] != b"CD001" {
            break;
        }
        // Descriptor type 1 is the primary volume descriptor; 255 ends the set.
        match block[0] {
            1 => {
                let block_size = u16::from_le_bytes([block[128], block[129]]);
                if u64::from(block_size) != BLOCK {
                    return Err(format!("logical block size {block_size} is not supported"));
                }
                return parse_record(&block[156..190]).map_err(|e| format!("root directory: {e}"));
            }
            255 => break,
            _ => {}
        }
    }
    Err("no ISO9660 primary volume descriptor found".to_owned())
}

fn read_extent<R: Read + Seek>(r: &mut R, size: u64, lba: u32, len: u32) -> Option<Vec<u8>> {
    let start = u64::from(lba) * BLOCK;
    if start + u64::from(len) > size {
        return None;
    }
    let mut data = vec![0; usize::try_from(len).ok()?];
    r.seek(SeekFrom::Start(start)).ok()?;
    r.read_exact(&mut data).ok()?;
    Some(data)
}

fn records(data: &[u8]) -> impl Iterator<Item = Result<Record, &'static str>> + '_ {
    let mut at = 0;
    std::iter::from_fn(move || {
        loop {
            let len = usize::from(*data.get(at)?);
            // Records never cross a block: a zero length byte pads out the rest of it.
            if len == 0 {
                at = (at / BLOCK_LEN + 1) * BLOCK_LEN;
                continue;
            }
            let rec = data.get(at..at + len);
            at += len;
            return Some(
                rec.ok_or("record runs past the end of the directory")
                    .and_then(parse_record),
            );
        }
    })
}

fn parse_record(rec: &[u8]) -> Result<Record, &'static str> {
    if rec.len() < 34 {
        return Err("directory record is too short");
    }
    let le32 = |at: usize| u32::from_le_bytes([rec[at], rec[at + 1], rec[at + 2], rec[at + 3]]);
    let raw_name = rec
        .get(33..33 + usize::from(rec[32]))
        .ok_or("file name runs past its record")?;
    // The file's data starts after its extended attribute record, `rec[1]` blocks long.
    let lba = le32(2)
        .checked_add(u32::from(rec[1]))
        .ok_or("extent address overflows")?;
    Ok(Record {
        name: clean_name(raw_name),
        lba,
        len: le32(10),
        flags: rec[25],
    })
}

/// `README.TXT;1` becomes `README.TXT`, `NEWS.;1` becomes `NEWS`; `.` and `..` become empty.
fn clean_name(raw: &[u8]) -> String {
    if matches!(raw, [0 | 1]) {
        return String::new();
    }
    let name = String::from_utf8_lossy(raw);
    let name = name.split(';').next().unwrap_or_default();
    name.strip_suffix('.').unwrap_or(name).to_owned()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::testutil::iso;

    fn tree(files: &[(&str, &[u8])]) -> (IsoTree, Vec<u8>) {
        let img = iso(files);
        (
            IsoTree::read(&mut Cursor::new(&img), img.len() as u64).unwrap(),
            img,
        )
    }

    fn contents(tree: &IsoTree, img: &[u8], path: &[&str]) -> Vec<u8> {
        let IsoKind::File { lba, len } = tree.node(tree.walk(path).unwrap()).kind else {
            panic!("not a file")
        };
        let start = lba as usize * 2048;
        img[start..start + len as usize].to_vec()
    }

    #[test]
    fn indexes_nested_directories() {
        let (t, img) = tree(&[
            ("S2/DATA/TXT/NAMES.ENG", b"names"),
            ("S2/DATA/RESOURCE.IDX", b"idx"),
            ("S2/ROOT/S2.EXE", b"MZ"),
            ("README.", b"read me"),
            ("EMPTY.DAT", b""),
        ]);
        assert_eq!(contents(&t, &img, &["S2", "DATA", "TXT", "NAMES.ENG"]), b"names");
        assert_eq!(contents(&t, &img, &["s2", "root", "s2.exe"]), b"MZ");
        assert_eq!(contents(&t, &img, &["README"]), b"read me");
        assert_eq!(contents(&t, &img, &["empty.dat"]), b"");
        assert!(t.walk(&["S2", "MISSING"]).is_none());
        assert!(t.walk(&["README", "X"]).is_none());
        let names: Vec<_> = t
            .children(IsoTree::ROOT)
            .iter()
            .map(|&c| t.node(c).name.as_str())
            .collect();
        assert_eq!(names, ["EMPTY.DAT", "README", "S2"]);
    }

    #[test]
    fn spans_directories_over_several_sectors() {
        let names: Vec<String> = (0..200).map(|i| format!("DATA/FILE{i:04}.DAT")).collect();
        let files: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), n.as_bytes())).collect();
        let (t, img) = tree(&files);
        assert_eq!(t.children(t.walk(&["DATA"]).unwrap()).len(), 200);
        assert_eq!(
            contents(&t, &img, &["DATA", "FILE0199.DAT"]),
            b"DATA/FILE0199.DAT"
        );
    }

    #[test]
    fn rejects_truncated_and_foreign_images() {
        let img = iso(&[("BIG.DAT", &[7; 10_000])]);
        let short = &img[..img.len() - 4096];
        let err = IsoTree::read(&mut Cursor::new(short), short.len() as u64).unwrap_err();
        assert!(err.contains("truncated"), "{err}");
        let blank = vec![0; 40 * 2048];
        assert!(IsoTree::read(&mut Cursor::new(&blank), blank.len() as u64).is_err());
    }
}
