//! What the map view draws, layer by layer, and which layers wrote each pixel.

use std::fmt;
use std::str::FromStr;

use bitflags::bitflags;

bitflags! {
    /// The layers of the map view, for debugging. [`Layers::ALL`] draws the original's output.
    ///
    /// Switching a layer off leaves each pixel as the other layers would have left it, so only
    /// pixels the removed layer wrote ([`Provenance`]) change. [`Layers::FOG`] is the exception:
    /// without it every triangle draws as seen, the same as a map with no fog.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Layers: u16 {
        /// Terrain triangles.
        const TRIANGLES = 1 << 0;
        /// Border strips between terrain types.
        const BORDERS = 1 << 1;
        /// Roads.
        const ROADS = 1 << 2;
        /// Border stones along territory edges.
        const STONES = 1 << 3;
        /// Trees, granite, decorations and other static map objects.
        const OBJECTS = 1 << 4;
        /// Shadows of objects, buildings, flags and figures.
        const SHADOWS = 1 << 5;
        /// Buildings.
        const BUILDINGS = 1 << 6;
        /// Flags.
        const FLAGS = 1 << 7;
        /// Figures: animals now, settlers later.
        const FIGURES = 1 << 8;
        /// The fog of war.
        const FOG = 1 << 9;
    }
}

impl Layers {
    /// Every layer: the original's output.
    pub const ALL: Self = Self::all();

    /// The layers the terrain pass draws, which cached terrain tiles depend on.
    pub const TERRAIN: Self = Self::TRIANGLES
        .union(Self::BORDERS)
        .union(Self::ROADS)
        .union(Self::STONES)
        .union(Self::FOG);

    /// Every single layer in order, with its command-line name.
    pub fn named() -> impl Iterator<Item = (&'static str, Self)> {
        NAMES.into_iter()
    }
}

/// The command-line names, in flag order.
const NAMES: [(&str, Layers); 10] = [
    ("triangles", Layers::TRIANGLES),
    ("borders", Layers::BORDERS),
    ("roads", Layers::ROADS),
    ("stones", Layers::STONES),
    ("objects", Layers::OBJECTS),
    ("shadows", Layers::SHADOWS),
    ("buildings", Layers::BUILDINGS),
    ("flags", Layers::FLAGS),
    ("figures", Layers::FIGURES),
    ("fog", Layers::FOG),
];

impl Default for Layers {
    fn default() -> Self {
        Self::ALL
    }
}

impl fmt::Display for Layers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = Self::named()
            .filter(|&(_, layer)| self.contains(layer))
            .map(|(name, _)| name)
            .collect();
        if names.is_empty() {
            f.write_str("none")
        } else {
            f.write_str(&names.join(","))
        }
    }
}

/// The text names no layer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown layer '{0}': expected all, none or a comma-separated list of {list}", list = list())]
pub struct UnknownLayer(String);

fn list() -> String {
    Layers::named()
        .map(|(name, _)| name)
        .collect::<Vec<_>>()
        .join(", ")
}

impl FromStr for Layers {
    type Err = UnknownLayer;

    /// A comma-separated list of layer names, or `all` or `none`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .try_fold(Self::empty(), |set, name| {
                if name.eq_ignore_ascii_case("all") {
                    return Ok(Self::ALL);
                }
                if name.eq_ignore_ascii_case("none") {
                    return Ok(set);
                }
                NAMES
                    .iter()
                    .find(|(known, _)| known.eq_ignore_ascii_case(name))
                    .map(|&(_, layer)| set | layer)
                    .ok_or_else(|| UnknownLayer(name.to_owned()))
            })
    }
}

/// For each pixel of a frame, the layers that wrote to it: [`Layers`] bits, one `u16` per pixel.
///
/// A pixel keeps every layer that touched it, even when a later layer covered it, so the set is
/// a superset of what its final value depends on. A shadow marks both [`Layers::SHADOWS`] and the
/// layer of what cast it.
#[derive(Clone, PartialEq, Eq)]
pub struct Provenance {
    width: u32,
    height: u32,
    marks: Vec<u16>,
}

impl Provenance {
    /// A `width` × `height` record with nothing written.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            marks: vec![0; width as usize * height as usize],
        }
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The layers that wrote pixel (`x`, `y`); empty outside the frame.
    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> Layers {
        if x >= self.width || y >= self.height {
            return Layers::empty();
        }
        Layers::from_bits_retain(self.marks[y as usize * self.width as usize + x as usize])
    }

    /// Per pixel, row by row, whether any of `layers` wrote it. With [`Layers::FIGURES`] this is
    /// the figure mask: every pixel an animal's body or shadow touched.
    #[must_use]
    pub fn mask(&self, layers: Layers) -> Vec<bool> {
        self.marks
            .iter()
            .map(|&m| Layers::from_bits_retain(m).intersects(layers))
            .collect()
    }

    pub(crate) fn marks_mut(&mut self) -> &mut [u16] {
        &mut self.marks
    }

    /// Copies `other` in with its top-left corner at (`x`, `y`), cutting what falls outside.
    pub fn paste(&mut self, other: &Self, x: u32, y: u32) {
        let width = other.width.min(self.width.saturating_sub(x)) as usize;
        for row in 0..other.height.min(self.height.saturating_sub(y)) {
            let from = row as usize * other.width as usize;
            let to = (y + row) as usize * self.width as usize + x as usize;
            self.marks[to..to + width].copy_from_slice(&other.marks[from..from + width]);
        }
    }
}

impl fmt::Debug for Provenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Provenance")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_parse_and_print() {
        assert_eq!("roads,fog".parse(), Ok(Layers::ROADS | Layers::FOG));
        assert_eq!("all".parse(), Ok(Layers::ALL));
        assert_eq!("none".parse(), Ok(Layers::empty()));
        assert_eq!(" Figures ".parse(), Ok(Layers::FIGURES));
        assert!("trees".parse::<Layers>().is_err());
        assert_eq!((Layers::STONES | Layers::FLAGS).to_string(), "stones,flags");
        assert_eq!(Layers::empty().to_string(), "none");
        assert_eq!(Layers::named().count(), 10);
        assert_eq!(Layers::default(), Layers::all());
    }
}
