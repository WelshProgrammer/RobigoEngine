//! The serialised layer container (class 10001) shared by `.WLD` and `WORLDnnn.DAT`.

use super::{Error, Layer, LayerKind, Quirk, rle};
use crate::bytes::{Reader, to_usize};

pub(super) const MAP_CLASS: u16 = 10_001;
pub(super) const LAYER_CLASS: u16 = 10_000;
pub(super) const BUFFER_CLASS: u16 = 1;
pub(super) const NULL_CLASS: u16 = 0;
const COMPACT: u32 = u32::MAX;

/// Which game version wrote the container. It decides how many tagged layers follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Version {
    /// Exactly 14 layers, slot 13 left out.
    V151,
    /// 15 layers when the input holds them, else 14.
    V102,
}

/// The container's size and layers.
pub(super) struct Grid {
    pub width: u16,
    pub height: u16,
    pub layers: Vec<Layer>,
    pub quirks: Vec<Quirk>,
}

/// Reads a container at the cursor, leaving the cursor just past it. `displaced` holds the two
/// bytes found ahead of the container tag in a file with the extra-word fault, which may belong
/// at the end of the first layer.
pub(super) fn read(r: &mut Reader<'_>, version: Version, displaced: Option<[u8; 2]>) -> Result<Grid, Error> {
    expect_class(r, MAP_CLASS)?;
    // A tagged container's first u32 is its origin. `u32::MAX` marks the compact form, whose
    // origin follows it (loader 0x91f85, docs/formats/map.md).
    let compact = r.u32()? == COMPACT;
    if compact {
        r.skip(4)?;
    }
    let width = r.u16()?;
    let height = r.u16()?;
    if width == 0 || height == 0 {
        return Err(Error::Empty { width, height });
    }
    let grid = Grid {
        width,
        height,
        layers: Vec::new(),
        quirks: Vec::new(),
    };
    if compact {
        read_compact(r, grid)
    } else {
        read_tagged(r, grid, version, displaced)
    }
}

fn read_compact(r: &mut Reader<'_>, mut grid: Grid) -> Result<Grid, Error> {
    let size = grid.node_count();
    for (layer, kind) in LayerKind::STORED.into_iter().enumerate() {
        let len = to_usize(r.u32()?);
        let packed = r.bytes(len)?;
        let data = if len == size {
            packed.to_vec()
        } else {
            rle::decode(packed, size).map_err(|source| Error::Rle { layer, source })?
        };
        grid.layers.push(Layer { kind, data });
    }
    Ok(grid)
}

fn read_tagged(
    r: &mut Reader<'_>,
    mut grid: Grid,
    version: Version,
    displaced: Option<[u8; 2]>,
) -> Result<Grid, Error> {
    let stored = LayerKind::STORED.len();
    let mut data = Vec::with_capacity(LayerKind::ALL.len());
    while data.len() < stored
        || (version == Version::V102 && data.len() < LayerKind::ALL.len() && !r.is_empty())
    {
        let displaced = displaced.filter(|_| data.is_empty());
        data.push(read_layer(r, &mut grid, data.len(), displaced)?);
    }
    let kinds: &[LayerKind] = if data.len() == LayerKind::ALL.len() {
        &LayerKind::ALL
    } else {
        &LayerKind::STORED
    };
    for (&kind, read) in kinds.iter().zip(data) {
        match read {
            Read::Data { data, short } => {
                if short {
                    grid.quirks.push(Quirk::ShortLayer {
                        layer: kind,
                        missing: 2,
                    });
                }
                grid.layers.push(Layer { kind, data });
            }
            Read::Absent { offset } => grid.quirks.push(Quirk::AbsentLayer { layer: kind, offset }),
        }
    }
    Ok(grid)
}

/// One tagged layer as read.
enum Read {
    /// A byte buffer; `short` when it was completed from the displaced bytes.
    Data { data: Vec<u8>, short: bool },
    /// A layer whose buffer is the null object, at this offset.
    Absent { offset: usize },
}

fn read_layer(
    r: &mut Reader<'_>,
    grid: &mut Grid,
    layer: usize,
    displaced: Option<[u8; 2]>,
) -> Result<Read, Error> {
    let offset = r.position();
    expect_class(r, LAYER_CLASS)?;
    // The layer's origin, i16 x0 and y0.
    r.skip(4)?;
    let width = r.u16()?;
    let height = r.u16()?;
    let buffer_offset = r.position();
    if r.u16()? == NULL_CLASS {
        let len = to_usize(r.u32()?);
        if len != 0 {
            return Err(Error::LayerLength {
                layer,
                len,
                expected: 0,
            });
        }
        return Ok(Read::Absent { offset });
    }
    r.seek(buffer_offset)?;
    if (width, height) != (grid.width, grid.height) {
        return Err(Error::LayerSize {
            layer,
            width,
            height,
            map_width: grid.width,
            map_height: grid.height,
        });
    }
    expect_class(r, BUFFER_CLASS)?;
    let len = to_usize(r.u32()?);
    let expected = grid.node_count();
    if len != expected {
        return Err(Error::LayerLength { layer, len, expected });
    }
    if let Some(tail) = displaced.filter(|_| ends_two_bytes_early(r, len)) {
        let mut data = r.bytes(len - 2)?.to_vec();
        data.extend(tail);
        return Ok(Read::Data { data, short: true });
    }
    Ok(Read::Data {
        data: r.bytes(len)?.to_vec(),
        short: false,
    })
}

/// Whether a `len`-byte layer at the cursor ends two bytes early. That holds when the next
/// layer's tag sits two bytes before the declared end and not at the end itself.
fn ends_two_bytes_early(r: &Reader<'_>, len: usize) -> bool {
    let tag_at = |offset: Option<usize>| {
        offset
            .and_then(|at| r.data().get(at..at.checked_add(2)?))
            .is_some_and(|b| b == LAYER_CLASS.to_le_bytes())
    };
    let end = r.position().checked_add(len);
    len >= 2 && tag_at(end.and_then(|e| e.checked_sub(2))) && !tag_at(end)
}

fn expect_class(r: &mut Reader<'_>, expected: u16) -> Result<(), Error> {
    let offset = r.position();
    let found = r.u16()?;
    if found == expected {
        Ok(())
    } else {
        Err(Error::UnexpectedClass {
            offset,
            expected,
            found,
        })
    }
}

impl Grid {
    fn node_count(&self) -> usize {
        to_usize(u32::from(self.width) * u32::from(self.height))
    }
}
