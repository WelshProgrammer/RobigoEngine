//! What the ground around a node is made of: terrain classes, the node property `S2.EXE` derives
//! from the six triangles around a node (`0x18b6e`), and which edges figures can walk or swim
//! along.

use bitflags::bitflags;

use crate::grid::{Direction, Node};
use crate::nodes::NodeMap;

/// A terrain class, 0 to 13: what a terrain type counts as for walking, building and borders
/// (the first word of each entry of the terrain table at `0x1f9074`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct TerrainClass(u8);

/// The class of each terrain type 0–22.
const CLASS_OF_TYPE: [u8; 23] = [
    1, 2, 4, 5, 6, 7, 8, 9, 0, 0, 0, 3, 2, 3, 10, 0, 11, 12, 8, 13, 11, 11, 11,
];

/// How many terrain types a terrain byte can name: the type is the byte's low six bits, and
/// bits 6 and 7 are flags.
const TERRAIN_TYPES: u8 = 64;

/// A class's row of the class table (`0x1f7b8b`, 14 bytes per class).
#[derive(Debug, Clone, Copy)]
struct ClassRow {
    /// Set in the property when any of the six triangles has the class.
    any: Property,
    /// Kept in the property only when all six have it.
    all: Property,
    /// Figures walk along its edges (`+8`).
    walkable: bool,
    /// Water (`+10`).
    water: bool,
}

const fn row(any: Property, all: Property, walkable: bool, water: bool) -> ClassRow {
    ClassRow {
        any,
        all,
        walkable,
        water,
    }
}

const BUILDABLE_ALL: Property = Property::ALL_BUILDABLE.union(Property::ALL_PLANTABLE);

const CLASS_ROWS: [ClassRow; 14] = [
    row(Property::ANY_BUILDABLE, BUILDABLE_ALL, true, false),
    row(Property::ANY_BUILDABLE, BUILDABLE_ALL, true, false),
    row(Property::ANY_MOUNTAIN, Property::ALL_MOUNTAIN, true, false),
    row(Property::ANY_MOUNTAIN, Property::ALL_MOUNTAIN, true, false),
    row(Property::ANY_POLAR, Property::ALL_POLAR, false, false),
    row(Property::ANY_CLASS_5, Property::ALL_CLASS_5, false, false),
    row(Property::ANY_CLASS_6, Property::ALL_CLASS_6, true, false),
    row(Property::ANY_WATER, Property::ALL_WATER, false, true),
    row(Property::ANY_BUILDABLE, Property::ALL_BUILDABLE, true, false),
    row(Property::ANY_CLASS_9, Property::ALL_CLASS_9, true, false),
    row(Property::ANY_BUILDABLE, BUILDABLE_ALL, true, false),
    row(Property::ANY_LAVA, Property::ALL_LAVA, false, false),
    row(Property::empty(), Property::empty(), false, false),
    row(Property::ANY_CLASS_13, Property::empty(), false, false),
];

impl TerrainClass {
    /// The class with no properties, which Robigo gives to types past 22. `S2.EXE` does not
    /// range-check the type and reads past its table. No shipped map has such a type.
    pub const NONE: Self = Self(12);

    /// The class of a terrain byte (slot 1 or 2), from its low six bits.
    #[must_use]
    pub const fn of(terrain: u8) -> Self {
        let kind = (terrain % TERRAIN_TYPES) as usize;
        if kind < CLASS_OF_TYPE.len() {
            Self(CLASS_OF_TYPE[kind])
        } else {
            Self::NONE
        }
    }

    /// The class number.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }

    const fn row(self) -> ClassRow {
        CLASS_ROWS[self.0 as usize]
    }

    /// Whether figures can walk along an edge of a triangle of this class.
    #[must_use]
    pub const fn walkable(self) -> bool {
        self.row().walkable
    }

    /// Whether this is water.
    #[must_use]
    pub const fn water(self) -> bool {
        self.row().water
    }
}

bitflags! {
    /// The property of a node: for each class, one flag set when any of the six triangles
    /// around the node has it and one kept only when all six do (`0x18b6e`). Classes without
    /// a known role are named by number.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
    pub struct Property: u32 {
        /// All six triangles are class 5 (type 3).
        const ALL_CLASS_5 = 1 << 4;
        /// Any triangle is class 5.
        const ANY_CLASS_5 = 1 << 5;
        /// All six are water (class 7).
        const ALL_WATER = 1 << 6;
        /// Any triangle is water.
        const ANY_WATER = 1 << 7;
        /// All six are mountain (classes 2 and 3), the ground mines need.
        const ALL_MOUNTAIN = 1 << 8;
        /// Any triangle is mountain.
        const ANY_MOUNTAIN = 1 << 9;
        /// All six are class 4 (type 2).
        const ALL_POLAR = 1 << 10;
        /// Any triangle is class 4, the only ground polar bears walk to.
        const ANY_POLAR = 1 << 11;
        /// All six are class 6 (type 4).
        const ALL_CLASS_6 = 1 << 12;
        /// Any triangle is class 6.
        const ANY_CLASS_6 = 1 << 13;
        /// All six are classes 0, 1, 8 or 10: the ground buildings need.
        const ALL_BUILDABLE = 1 << 14;
        /// Any triangle is class 0, 1, 8 or 10.
        const ANY_BUILDABLE = 1 << 15;
        /// All six are class 9 (type 7).
        const ALL_CLASS_9 = 1 << 16;
        /// Any triangle is class 9.
        const ANY_CLASS_9 = 1 << 17;
        /// All six are lava (class 11).
        const ALL_LAVA = 1 << 18;
        /// Any triangle is lava.
        const ANY_LAVA = 1 << 19;
        /// All six are classes 0, 1 or 10: the ground trees can be planted on.
        const ALL_PLANTABLE = 1 << 20;
        /// Any triangle is class 13 (type 19).
        const ANY_CLASS_13 = 1 << 21;
    }
}

impl NodeMap {
    /// The terrain bytes of the six triangles around `node` (`0x1737e`). Triangle `i` lies
    /// between the edges in directions `i − 1` and `i`, counting directions from 0 (east), so
    /// triangle 0 lies between north-east and east.
    #[must_use]
    pub fn triangles_around(&self, node: Node) -> [u8; 6] {
        let (width, height) = (self.width(), self.height());
        let west = node.neighbour(Direction::West, width, height);
        let north_west = node.neighbour(Direction::NorthWest, width, height);
        let north_east = node.neighbour(Direction::NorthEast, width, height);
        let here = self.get(node).terrain;
        [
            self.get(north_east).terrain[0],
            here[1],
            here[0],
            self.get(west).terrain[1],
            self.get(north_west).terrain[0],
            self.get(north_west).terrain[1],
        ]
    }

    /// The node's property (`0x18b6e`).
    #[must_use]
    pub fn property(&self, node: Node) -> Property {
        let classes = self.triangles_around(node).map(TerrainClass::of);
        let any = classes
            .iter()
            .fold(Property::empty(), |found, class| found | class.row().any);
        let all = classes
            .iter()
            .fold(Property::all(), |found, class| found & class.row().all);
        any | all
    }

    /// The classes of the two triangles along the edge leaving `node` in `direction`.
    fn along(&self, node: Node, direction: Direction) -> [TerrainClass; 2] {
        let triangles = self.triangles_around(node);
        let d = direction.index();
        [
            TerrainClass::of(triangles[d]),
            TerrainClass::of(triangles[(d + 1) % 6]),
        ]
    }

    /// Whether a figure can walk along the edge leaving `node` in `direction`: at least one of
    /// the two triangles along it is walkable (`0x18bbb`).
    #[must_use]
    pub fn edge_walkable(&self, node: Node, direction: Direction) -> bool {
        self.along(node, direction).iter().any(|class| class.walkable())
    }

    /// Whether the edge leaving `node` in `direction` runs through water on both sides
    /// (`0x18c75`).
    #[must_use]
    pub fn edge_water(&self, node: Node, direction: Direction) -> bool {
        self.along(node, direction).iter().all(|class| class.water())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::NodeData;

    const MEADOW: u8 = 8;
    const WATER: u8 = 5;

    fn map(terrain: [u8; 2]) -> NodeMap {
        NodeMap::filled(
            8,
            8,
            NodeData {
                terrain,
                ..NodeData::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn every_type_has_a_class() {
        assert_eq!(TerrainClass::of(8).index(), 0);
        assert_eq!(
            TerrainClass::of(WATER + 64).index(),
            7,
            "bit 6 is not part of the type"
        );
        assert_eq!(TerrainClass::of(22).index(), 11);
        assert_eq!(TerrainClass::of(23), TerrainClass::NONE);
    }

    #[test]
    fn uniform_ground_has_its_any_and_all_bits() {
        let meadow = map([MEADOW, MEADOW]).property(Node { x: 3, y: 3 });
        assert!(meadow.contains(Property::ALL_BUILDABLE | Property::ALL_PLANTABLE));
        let water = map([WATER, WATER]);
        let node = Node { x: 3, y: 3 };
        assert_eq!(water.property(node), Property::ANY_WATER | Property::ALL_WATER);
        assert!(water.edge_water(node, Direction::East));
        assert!(!water.edge_walkable(node, Direction::East));
    }

    #[test]
    fn each_triangle_borders_two_edges() {
        let mut nodes = map([WATER, WATER]);
        let node = Node { x: 4, y: 4 };
        let mut data = nodes.get(node);
        data.terrain[1] = MEADOW;
        nodes.set(node, data);
        assert_eq!(nodes.triangles_around(node)[1], MEADOW);
        let walkable: Vec<bool> = Direction::ALL
            .iter()
            .map(|&d| nodes.edge_walkable(node, d))
            .collect();
        assert_eq!(walkable, [true, true, false, false, false, false]);
        let property = nodes.property(node);
        assert!(property.contains(Property::ANY_BUILDABLE | Property::ANY_WATER));
        assert!(!property.intersects(Property::ALL_WATER | Property::ALL_BUILDABLE));
    }
}
