//! Roads (slot 3) and territory border edges (slot 10) of a node.

use bitflags::bitflags;

use crate::grid::Direction;
use std::hash::{Hash, Hasher};

use crate::packed::Bits;

bitflags! {
    /// A set of the three edges a node owns: to its east, south-east and south-west neighbours.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
    pub struct Edges: u8 {
        /// To the east neighbour.
        const EAST = 1;
        /// To the south-east neighbour.
        const SOUTH_EAST = 2;
        /// To the south-west neighbour.
        const SOUTH_WEST = 4;
    }
}

impl Edges {
    /// Each edge with the direction it leaves the node in.
    pub const DIRECTIONS: [(Direction, Self); 3] = [
        (Direction::East, Self::EAST),
        (Direction::SouthEast, Self::SOUTH_EAST),
        (Direction::SouthWest, Self::SOUTH_WEST),
    ];

    /// The edge leaving in `direction`, if the node owns it: east, south-east or south-west.
    #[must_use]
    pub const fn leaving(direction: Direction) -> Option<Self> {
        match direction {
            Direction::East => Some(Self::EAST),
            Direction::SouthEast => Some(Self::SOUTH_EAST),
            Direction::SouthWest => Some(Self::SOUTH_WEST),
            Direction::West | Direction::NorthWest | Direction::NorthEast => None,
        }
    }

    /// The border edges of a slot 10 byte (bits 4–6).
    #[must_use]
    pub const fn from_slot10(byte: u8) -> Self {
        Self::from_bits_truncate(Bits::new(4, 3).get(byte))
    }

    /// The position of a single edge in east, south-east, south-west order.
    const fn position(self) -> Option<usize> {
        match self.bits() {
            bits if bits == Self::EAST.bits() => Some(0),
            bits if bits == Self::SOUTH_EAST.bits() => Some(1),
            bits if bits == Self::SOUTH_WEST.bits() => Some(2),
            _ => None,
        }
    }
}

/// A road type, the value of one two-bit field of slot 3 and of a road record's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum RoadKind {
    /// 1: an ordinary road, and the link from a building to its flag.
    Normal,
    /// 2: a road busy enough for a donkey.
    Donkey,
    /// 3: a water road, for boats.
    Water,
}

impl RoadKind {
    /// The type a two-bit field of slot 3 holds, if any.
    const fn from_field(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Normal),
            2 => Some(Self::Donkey),
            3 => Some(Self::Water),
            _ => None,
        }
    }

    /// The value of the type's two-bit field, 0 for no road.
    const fn field(kind: Option<Self>) -> u8 {
        match kind {
            None => 0,
            Some(Self::Normal) => 1,
            Some(Self::Donkey) => 2,
            Some(Self::Water) => 3,
        }
    }
}

/// The roads leaving a node east, south-east and south-west, and the marker drawn on the node
/// (slot 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Roads {
    leaving: [Option<RoadKind>; 3],
    marker: Option<RoadKind>,
}

/// Slot 3: two bits per road in east, south-east, south-west order from bit 0, then the marker.
const ROAD_FIELDS: [Bits; 3] = [Bits::new(0, 2), Bits::new(2, 2), Bits::new(4, 2)];
const MARKER_FIELD: Bits = Bits::new(6, 2);

impl Roads {
    /// The roads a slot 3 byte holds.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Self {
        Self {
            leaving: [
                RoadKind::from_field(ROAD_FIELDS[0].get(byte)),
                RoadKind::from_field(ROAD_FIELDS[1].get(byte)),
                RoadKind::from_field(ROAD_FIELDS[2].get(byte)),
            ],
            marker: RoadKind::from_field(MARKER_FIELD.get(byte)),
        }
    }

    /// The slot 3 byte.
    #[must_use]
    pub const fn to_byte(self) -> u8 {
        ROAD_FIELDS[0].set(RoadKind::field(self.leaving[0]))
            | ROAD_FIELDS[1].set(RoadKind::field(self.leaving[1]))
            | ROAD_FIELDS[2].set(RoadKind::field(self.leaving[2]))
            | MARKER_FIELD.set(RoadKind::field(self.marker))
    }

    /// The road leaving in `edge`'s direction, if any.
    #[must_use]
    pub const fn along(self, edge: Edges) -> Option<RoadKind> {
        match edge.position() {
            Some(at) => self.leaving[at],
            None => None,
        }
    }

    /// The marker drawn on the node.
    #[must_use]
    pub const fn marker(self) -> Option<RoadKind> {
        self.marker
    }

    /// These roads with the one leaving in `edge`'s direction set to `kind`, or cleared.
    #[must_use]
    pub const fn with_road(mut self, edge: Edges, kind: Option<RoadKind>) -> Self {
        if let Some(at) = edge.position() {
            self.leaving[at] = kind;
        }
        self
    }

    /// These roads with the node's marker set to `kind`, or cleared.
    #[must_use]
    pub const fn with_marker(mut self, kind: Option<RoadKind>) -> Self {
        self.marker = kind;
        self
    }

    /// Whether the node has neither roads nor a marker.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        matches!(
            self,
            Self {
                leaving: [None, None, None],
                marker: None
            }
        )
    }
}

impl Hash for Roads {
    /// Hashes the slot 3 byte, so state hashes match those of the packed form.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_byte().hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slot_3_byte_reads_back_unchanged() {
        for byte in 0..=u8::MAX {
            assert_eq!(Roads::from_byte(byte).to_byte(), byte);
        }
    }

    #[test]
    fn slot_bytes_decode() {
        let roads = Roads::from_byte(0b11_10_00_01);
        assert_eq!(roads.along(Edges::EAST), Some(RoadKind::Normal));
        assert_eq!(roads.along(Edges::SOUTH_EAST), None);
        assert_eq!(roads.along(Edges::SOUTH_WEST), Some(RoadKind::Donkey));
        assert_eq!(roads.marker(), Some(RoadKind::Water));
        assert_eq!(Edges::from_slot10(0b0101_0001), Edges::EAST | Edges::SOUTH_WEST);
        let built = Roads::default()
            .with_road(Edges::SOUTH_EAST, Some(RoadKind::Normal))
            .with_marker(Some(RoadKind::Normal));
        assert_eq!(built.to_byte(), 0b01_00_01_00);
        assert!(
            built
                .with_road(Edges::SOUTH_EAST, None)
                .with_marker(None)
                .is_empty()
        );
    }
}
