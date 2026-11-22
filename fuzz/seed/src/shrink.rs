//! Cuts real files down to seeds small enough to mutate quickly while staying well formed.

use s2_formats::lst::{Item, ItemType, MAGIC};
use s2_formats::map::{AREA_TABLE_LEN, LayerKind, Map, SIGNATURE};
use s2_formats::save::{AI_CELL_LEN, AiGrid, Part};
use s2_formats::smacker::{HEADER_SIZE, Video};

/// Single-item seeds taken from one archive or index, at most.
const MAX_ITEMS: usize = 64;

/// Nodes across and down a cropped map; a multiple of the 8-node AI cell.
const CROP: u16 = 32;

/// Frame bytes kept per Smacker seed.
const SMACKER_BUDGET: usize = 64 << 10;

/// Bytes ahead of the layer container in a `.WLD`: signature, header and area table.
const WLD_PREFIX: usize = SIGNATURE.len() + 82 + AREA_TABLE_LEN;

/// Offset of the stored width in a `.WLD`, after the signature and 20-byte title.
const WLD_STORED_SIZE: usize = SIGNATURE.len() + 20;

/// Class words of the map container, of each layer and of a layer's byte buffer
/// (`docs/formats/map.md`).
const MAP_CLASS: u16 = 10_001;
const LAYER_CLASS: u16 = 10_000;
const BUFFER_CLASS: u16 = 1;

/// One archive slot as a one-slot archive and as a tagged item.
pub struct Slot {
    pub archive: Vec<u8>,
    pub tagged: Vec<u8>,
}

/// Every non-empty slot of an LST archive, thinned to [`MAX_ITEMS`].
pub fn lst_slots(data: &[u8]) -> Vec<Slot> {
    let mut slots = Vec::new();
    let Some(count) = data
        .get(2..6)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
    else {
        return slots;
    };
    let mut pos = 6;
    for _ in 0..count {
        let start = pos;
        match u16_at(data, pos) {
            Some(1) => {}
            Some(_) => {
                pos += 2;
                continue;
            }
            None => break,
        }
        let Some(kind) = u16_at(data, pos + 2).and_then(ItemType::from_id) else {
            break;
        };
        let Ok((_, used)) = Item::parse(kind, &data[pos + 4..]) else {
            break;
        };
        pos += 4 + used;
        let mut archive = MAGIC.to_le_bytes().to_vec();
        archive.extend(1u32.to_le_bytes());
        archive.extend(&data[start..pos]);
        slots.push(Slot {
            archive,
            tagged: data[start + 2..pos].to_vec(),
        });
    }
    thin(slots)
}

/// One single-entry index per distinct DAT offset, with its item cut from the DAT.
pub fn idx_entries<'a>(idx: &[u8], dat: &'a [u8]) -> Vec<(Vec<u8>, &'a [u8])> {
    const ENTRY: usize = 28;
    let entries: Vec<&[u8]> = idx.get(4..).unwrap_or_default().chunks_exact(ENTRY).collect();
    let offset = |entry: &[u8]| u32_at(entry, 16).map_or(usize::MAX, |o| o as usize);
    let mut starts: Vec<usize> = entries
        .iter()
        .map(|e| offset(e))
        .filter(|&o| o < dat.len())
        .collect();
    starts.sort_unstable();
    starts.dedup();
    let mut seeds = Vec::new();
    for entry in entries {
        let start = offset(entry);
        let Ok(at) = starts.binary_search(&start) else {
            continue;
        };
        starts.remove(at);
        let end = starts.get(at).copied().unwrap_or(dat.len());
        // A count of 1, then the entry with its DAT offset zeroed.
        let mut single = 1u32.to_le_bytes().to_vec();
        single.extend(&entry[..16]);
        single.extend(0u32.to_le_bytes());
        single.extend(&entry[20..]);
        seeds.push((single, &dat[start..end]));
    }
    thin(seeds)
}

/// A `.WLD` cropped to [`CROP`] nodes square, with the layers tagged or packed.
pub fn wld(data: &[u8], map: &Map, compact: bool) -> Vec<u8> {
    let (width, height) = crop_size(map);
    let mut out = data[..WLD_PREFIX].to_vec();
    out[WLD_STORED_SIZE..WLD_STORED_SIZE + 2].copy_from_slice(&width.to_le_bytes());
    out[WLD_STORED_SIZE + 2..WLD_STORED_SIZE + 4].copy_from_slice(&height.to_le_bytes());
    out.extend(if compact { packed(map) } else { container(map) });
    for animal in map.animals() {
        if animal.position.x < width && animal.position.y < height {
            out.push(animal.species);
            out.extend(animal.position.x.to_le_bytes());
            out.extend(animal.position.y.to_le_bytes());
        }
    }
    // The animal list's terminator.
    out.push(u8::MAX);
    out
}

/// The tagged layer container of `map`, cropped, as `WORLDnnn.DAT` and `.WLD` store it.
pub fn container(map: &Map) -> Vec<u8> {
    let (width, height) = crop_size(map);
    let mut out = container_head(width, height, 0);
    let kinds: &[LayerKind] = if map.layer(LayerKind::Runtime13).is_some() {
        &LayerKind::ALL
    } else {
        &LayerKind::STORED
    };
    for &kind in kinds {
        out.extend(LAYER_CLASS.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(width.to_le_bytes());
        out.extend(height.to_le_bytes());
        match cropped(map, kind) {
            Some(data) => {
                out.extend(BUFFER_CLASS.to_le_bytes());
                out.extend(u32::try_from(data.len()).unwrap_or(u32::MAX).to_le_bytes());
                out.extend(data);
            }
            None => out.extend([0; 6]),
        }
    }
    out
}

/// Each cropped layer packed with the map RLE, behind the `u24` length the RLE target reads.
pub fn packed_layers(map: &Map) -> Vec<Vec<u8>> {
    LayerKind::STORED
        .iter()
        .filter_map(|&kind| cropped(map, kind))
        .map(|data| {
            let mut out = u32::try_from(data.len()).unwrap_or(0).to_le_bytes()[..3].to_vec();
            out.extend(rle(&data));
            out
        })
        .collect()
}

/// A save part fitted to the cropped world: the transport header's size patched, and the
/// computer players' grid cut to the cropped map's cells.
pub fn save_part(part: Part, mut bytes: Vec<u8>) -> Vec<u8> {
    match part {
        Part::Transport if bytes.len() >= 6 => {
            bytes[2..4].copy_from_slice(&CROP.to_le_bytes());
            bytes[4..6].copy_from_slice(&CROP.to_le_bytes());
            bytes
        }
        Part::Computer => {
            let Some(cells) = u32_at(&bytes, 8) else {
                return bytes;
            };
            let (columns, rows) = AiGrid::size_for((u32::from(CROP), u32::from(CROP)));
            let kept = (columns * rows) as usize * AI_CELL_LEN;
            let players = 24 + cells as usize * AI_CELL_LEN;
            let (Some(params), Some(grid), Some(rest)) =
                (bytes.get(12..24), bytes.get(24..24 + kept), bytes.get(players..))
            else {
                return bytes;
            };
            let mut out = [columns, rows, columns * rows].map(u32::to_le_bytes).concat();
            out.extend(params);
            out.extend(grid);
            out.extend(rest);
            out
        }
        _ => bytes,
    }
}

/// Short runs of frames from eight points in the video, each a playable file of its own.
pub fn smacker(data: &[u8]) -> Vec<Vec<u8>> {
    let Ok(video) = Video::parse(data) else {
        return Vec::new();
    };
    let frames = video.frames();
    let count = frames.len();
    // The frame tables hold a `u32` size and a type byte per frame.
    let trees_at = HEADER_SIZE + count * 5;
    let trees = &data[trees_at..trees_at + video.header().trees_size as usize];
    let mut seeds = Vec::new();
    for start in (0..8).map(|eighth| count * eighth / 8) {
        let mut kept = 0;
        let mut bytes = 0;
        while let Some(frame) = frames.get(start + kept)
            && (kept == 0 || bytes + frame.size <= SMACKER_BUDGET)
        {
            bytes += frame.size;
            kept += 1;
        }
        let range = start..start + kept;
        let mut out = data[..HEADER_SIZE].to_vec();
        // The frame count at header offset 12; clearing bit 0 of the flags at 20 drops the ring
        // frame.
        out[12..16].copy_from_slice(&u32::try_from(kept).unwrap_or(0).to_le_bytes());
        out[20] &= !1;
        out.extend(&data[HEADER_SIZE + start * 4..HEADER_SIZE + range.end * 4]);
        out.extend(&data[HEADER_SIZE + count * 4 + start..HEADER_SIZE + count * 4 + range.end]);
        out.extend(trees);
        for frame in &frames[range] {
            out.extend(&data[frame.offset..frame.offset + frame.size]);
        }
        seeds.push(out);
    }
    seeds
}

fn packed(map: &Map) -> Vec<u8> {
    let (width, height) = crop_size(map);
    let mut out = container_head(width, height, u32::MAX);
    for kind in LayerKind::STORED {
        let data = cropped(map, kind).unwrap_or_else(|| vec![0; usize::from(width) * usize::from(height)]);
        let body = rle(&data);
        let body = if body.len() < data.len() { body } else { data };
        out.extend(u32::try_from(body.len()).unwrap_or(u32::MAX).to_le_bytes());
        out.extend(body);
    }
    out
}

fn container_head(width: u16, height: u16, marker: u32) -> Vec<u8> {
    let mut out = MAP_CLASS.to_le_bytes().to_vec();
    out.extend(marker.to_le_bytes());
    // A compact container's origin follows its marker; in the tagged layout the zero marker is
    // the origin.
    if marker == u32::MAX {
        out.extend([0; 4]);
    }
    out.extend(width.to_le_bytes());
    out.extend(height.to_le_bytes());
    out
}

fn crop_size(map: &Map) -> (u16, u16) {
    (map.width().min(CROP), map.height().min(CROP))
}

fn cropped(map: &Map, kind: LayerKind) -> Option<Vec<u8>> {
    let data = map.layer(kind)?.data();
    let (width, height) = crop_size(map);
    let stride = usize::from(map.width());
    Some(
        data.chunks(stride)
            .take(usize::from(height))
            .flat_map(|row| &row[..usize::from(width)])
            .copied()
            .collect(),
    )
}

/// The map RLE: the rarest byte escapes runs of four or more as escape, count, value; a
/// literal escape byte is written as escape, 0.
fn rle(src: &[u8]) -> Vec<u8> {
    let mut histogram = [0usize; 256];
    for &b in src {
        histogram[usize::from(b)] += 1;
    }
    let escape = (0..=u8::MAX)
        .min_by_key(|&b| histogram[usize::from(b)])
        .unwrap_or(0);
    let mut out = vec![escape];
    let mut rest = src;
    while let Some(&value) = rest.first() {
        let run = rest.iter().take(255).take_while(|&&b| b == value).count();
        if value == escape {
            (0..run).for_each(|_| out.extend([escape, 0]));
        } else if run >= 4 {
            out.extend([escape, u8::try_from(run).unwrap_or(u8::MAX), value]);
        } else {
            out.extend(&rest[..run]);
        }
        rest = &rest[run..];
    }
    out
}

fn thin<T>(items: Vec<T>) -> Vec<T> {
    let step = items.len().div_ceil(MAX_ITEMS).max(1);
    items.into_iter().step_by(step).collect()
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}
