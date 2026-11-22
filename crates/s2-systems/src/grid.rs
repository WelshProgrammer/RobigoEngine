//! The node grid: positions, the six directions and wrapping at the map's edges.

/// A node in the map grid, wrapped into the map.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub struct Node {
    /// Column, `0..width`.
    pub x: u16,
    /// Row, `0..height`.
    pub y: u16,
}

impl Node {
    /// The node at (`x`, `y`) wrapped into a `width` × `height` map, which must not be empty.
    #[must_use]
    pub fn wrapped(x: i32, y: i32, width: u16, height: u16) -> Self {
        let wrap = |value: i32, size: u16| {
            u16::try_from(value.rem_euclid(i32::from(size.max(1)))).unwrap_or_default()
        };
        Self {
            x: wrap(x, width),
            y: wrap(y, height),
        }
    }

    /// Whether the node's row is shifted half a node to the right of the even rows.
    #[must_use]
    pub const fn odd_row(self) -> bool {
        !self.y.is_multiple_of(2)
    }

    /// The neighbouring node in `direction`, wrapped into a `width` × `height` map.
    #[must_use]
    pub fn neighbour(self, direction: Direction, width: u16, height: u16) -> Self {
        let (x, y) = (i32::from(self.x), i32::from(self.y));
        let odd = i32::from(self.odd_row());
        let (nx, ny) = match direction {
            Direction::East => (x + 1, y),
            Direction::SouthEast => (x + odd, y + 1),
            Direction::SouthWest => (x + odd - 1, y + 1),
            Direction::West => (x - 1, y),
            Direction::NorthWest => (x + odd - 1, y - 1),
            Direction::NorthEast => (x + odd, y - 1),
        };
        Self::wrapped(nx, ny, width, height)
    }

    /// The direction from this node to its neighbour `other`; `None` when they are not
    /// neighbours.
    #[must_use]
    pub fn direction_to(self, other: Self, width: u16, height: u16) -> Option<Direction> {
        Direction::ALL
            .into_iter()
            .find(|&direction| self.neighbour(direction, width, height) == other)
    }

    /// The node `steps` steps away in `direction`.
    #[must_use]
    pub fn towards(self, direction: Direction, steps: u16, width: u16, height: u16) -> Self {
        (0..steps).fold(self, |node, _| node.neighbour(direction, width, height))
    }

    /// The nodes exactly `radius` steps away, in the order `S2.EXE` walks a ring. The walk starts
    /// `radius` steps north-west and goes `radius` steps in each direction from east round to
    /// north-east, yielding each node before the step that leaves it. Radius 0 is the node
    /// itself. Several of `S2.EXE`'s passes read nodes they have already rewritten, and their
    /// result depends on this order.
    pub fn ring(self, radius: u16, width: u16, height: u16) -> impl Iterator<Item = Self> {
        let mut at = self.towards(Direction::NorthWest, radius, width, height);
        let centre = (radius == 0).then_some(self);
        let sides = Direction::ALL
            .into_iter()
            .flat_map(move |direction| std::iter::repeat_n(direction, usize::from(radius)));
        centre.into_iter().chain(sides.map(move |direction| {
            let here = at;
            at = at.neighbour(direction, width, height);
            here
        }))
    }

    /// The nodes exactly `radius` steps away in the order of `S2.EXE`'s ring cursor (`0x22498`),
    /// which yields each node after the step that reaches it: [`Self::ring`] with the north-west
    /// corner moved to the end. Radius 0 is the node itself.
    pub fn stepped_ring(self, radius: u16, width: u16, height: u16) -> impl Iterator<Item = Self> {
        let corner = (radius > 0).then(|| self.towards(Direction::NorthWest, radius, width, height));
        self.ring(radius, width, height)
            .skip(usize::from(radius > 0))
            .chain(corner)
    }

    /// The node and every node within `reach` steps, ring by ring outwards.
    pub fn rings(self, reach: u16, width: u16, height: u16) -> impl Iterator<Item = Self> {
        (0..=reach).flat_map(move |radius| self.ring(radius, width, height))
    }
}

/// The six directions from a node, as `S2.EXE` numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Direction {
    /// Same row, one to the right.
    East,
    /// Next row, to the right.
    SouthEast,
    /// Next row, to the left.
    SouthWest,
    /// Same row, one to the left.
    West,
    /// Previous row, to the left.
    NorthWest,
    /// Previous row, to the right.
    NorthEast,
}

impl Direction {
    /// Every direction in `S2.EXE`'s order: east is 0, north-east 5.
    pub const ALL: [Self; 6] = [
        Self::East,
        Self::SouthEast,
        Self::SouthWest,
        Self::West,
        Self::NorthWest,
        Self::NorthEast,
    ];

    /// The direction's number, 0 (east) to 5 (north-east).
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The direction numbered `index` modulo 6.
    #[must_use]
    pub const fn from_index(index: usize) -> Self {
        Self::ALL[index % 6]
    }

    /// The opposite direction: where a figure walking this way came from.
    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::East => Self::West,
            Self::SouthEast => Self::NorthWest,
            Self::SouthWest => Self::NorthEast,
            Self::West => Self::East,
            Self::NorthWest => Self::SouthEast,
            Self::NorthEast => Self::SouthWest,
        }
    }
}

/// A rectangle of nodes, which may run past the map's edges and wraps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct NodeRect {
    /// First column.
    pub x: i32,
    /// First row.
    pub y: i32,
    /// Columns.
    pub width: u32,
    /// Rows.
    pub height: u32,
}

/// A node position that does not wrap round the map, as the reveal and the territory claim walk
/// it: a step past an edge leaves the map.
#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    /// Column, which may be negative or past the map's width.
    pub x: i32,
    /// Row, which may be negative or past the map's height.
    pub y: i32,
}

impl Cursor {
    /// A cursor on `node`.
    #[must_use]
    pub fn at(node: Node) -> Self {
        Self {
            x: i32::from(node.x),
            y: i32::from(node.y),
        }
    }

    /// The position one step away in `direction`, which may leave the map.
    #[must_use]
    pub const fn step(self, direction: Direction) -> Self {
        let odd = self.y & 1;
        let (x, y) = (self.x, self.y);
        let (x, y) = match direction {
            Direction::East => (x + 1, y),
            Direction::SouthEast => (x + odd, y + 1),
            Direction::SouthWest => (x + odd - 1, y + 1),
            Direction::West => (x - 1, y),
            Direction::NorthWest => (x + odd - 1, y - 1),
            Direction::NorthEast => (x + odd, y - 1),
        };
        Self { x, y }
    }
}

impl Cursor {
    /// The node it stands on, or `None` past an edge of a `width` × `height` map.
    #[must_use]
    pub fn node(self, width: u16, height: u16) -> Option<Node> {
        let x = u16::try_from(self.x).ok().filter(|&x| x < width)?;
        let y = u16::try_from(self.y).ok().filter(|&y| y < height)?;
        Some(Node { x, y })
    }
}

/// Walks the rings of a reveal of `radius` round `centre` on a `width` × `height` map as
/// `S2.EXE` does (`0x3fe8c`), calling `mark` with each node and which of its two triangles to
/// mark. Away from the edges that is every triangle whose three corners lie within `radius`
/// steps of the centre. The walk does not wrap: a ring position past a map edge marks nothing,
/// and neither does a mark whose own node lies past one. Returns the nodes whose drawing may
/// have changed.
pub fn reveal_walk(
    (width, height): (u16, u16),
    centre: Node,
    radius: u16,
    mut mark: impl FnMut(Node, [bool; 2]),
) -> NodeRect {
    use Direction::{East, NorthEast, NorthWest, SouthEast, SouthWest, West};
    let inside = |at: Cursor| (0..i32::from(width)).contains(&at.x) && (0..i32::from(height)).contains(&at.y);
    let mut mark_at = |at: Cursor, triangles: [bool; 2]| {
        if inside(at)
            && let (Ok(x), Ok(y)) = (u16::try_from(at.x), u16::try_from(at.y))
        {
            mark(Node { x, y }, triangles);
        }
    };
    let mut at = Cursor::at(centre);
    for ring in 1..=radius {
        at = at.step(NorthWest);
        for (side, direction) in [East, SouthEast, SouthWest, West, NorthWest, NorthEast]
            .into_iter()
            .enumerate()
        {
            for _ in 0..ring {
                if inside(at) {
                    match side {
                        0 => {
                            mark_at(at, [false, true]);
                            mark_at(at.step(East), [true, false]);
                        }
                        1 => {
                            mark_at(at, [true, false]);
                            mark_at(at.step(SouthWest), [false, true]);
                        }
                        2 => mark_at(at.step(West), [true, true]),
                        3 => {
                            let north_west = at.step(NorthWest);
                            mark_at(north_west, [true, false]);
                            mark_at(north_west.step(West), [false, true]);
                        }
                        4 => {
                            let north_west = at.step(NorthWest);
                            mark_at(north_west, [false, true]);
                            mark_at(north_west.step(NorthEast), [true, false]);
                        }
                        _ => mark_at(at.step(NorthEast), [true, true]),
                    }
                }
                at = at.step(direction);
            }
        }
    }
    let reach = i32::from(radius) + 1;
    let span = u32::from(radius) * 2 + 3;
    NodeRect {
        x: i32::from(centre.x) - reach,
        y: i32::from(centre.y) - reach,
        width: span,
        height: span,
    }
}

/// The ring positions [`reveal_walk`] marks from, in its order: every position of rings 1 to
/// `radius` round `centre` that lies on the map. In a campaign chapter `S2.EXE` reads the owner of
/// each one as it reveals land for player 0 (`0x3ff7b`).
pub fn reveal_ring((width, height): (u16, u16), centre: Node, radius: u16, mut visit: impl FnMut(Node)) {
    use Direction::{East, NorthEast, NorthWest, SouthEast, SouthWest, West};
    let mut at = Cursor::at(centre);
    for ring in 1..=radius {
        at = at.step(NorthWest);
        for direction in [East, SouthEast, SouthWest, West, NorthWest, NorthEast] {
            for _ in 0..ring {
                if let (Ok(x), Ok(y)) = (u16::try_from(at.x), u16::try_from(at.y))
                    && x < width
                    && y < height
                {
                    visit(Node { x, y });
                }
                at = at.step(direction);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbours_follow_row_parity_and_wrap() {
        let even = Node { x: 3, y: 2 };
        let odd = Node { x: 3, y: 3 };
        let at = |n: Node, d| {
            let m = n.neighbour(d, 10, 8);
            (m.x, m.y)
        };
        assert_eq!(at(even, Direction::SouthEast), (3, 3));
        assert_eq!(at(even, Direction::SouthWest), (2, 3));
        assert_eq!(at(odd, Direction::SouthEast), (4, 4));
        assert_eq!(at(odd, Direction::SouthWest), (3, 4));
        assert_eq!(at(even, Direction::NorthEast), (3, 1));
        assert_eq!(at(odd, Direction::NorthEast), (4, 2));
        assert_eq!(at(Node { x: 0, y: 0 }, Direction::NorthWest), (9, 7));
        assert_eq!(at(Node { x: 9, y: 7 }, Direction::SouthEast), (0, 0));
    }

    #[test]
    fn rings_start_north_west_and_go_round_clockwise() {
        let centre = Node { x: 5, y: 4 };
        let ring: Vec<_> = centre.ring(1, 16, 16).map(|n| (n.x, n.y)).collect();
        assert_eq!(ring, [(4, 3), (5, 3), (6, 4), (5, 5), (4, 5), (4, 4)]);
        assert_eq!(centre.ring(0, 16, 16).collect::<Vec<_>>(), [centre]);
        for radius in 1..5 {
            let mut nodes: Vec<_> = centre.ring(radius, 16, 16).collect();
            assert_eq!(nodes.len(), 6 * usize::from(radius));
            nodes.sort_unstable();
            nodes.dedup();
            assert_eq!(
                nodes.len(),
                6 * usize::from(radius),
                "radius {radius} repeats a node"
            );
        }
        assert_eq!(centre.rings(2, 16, 16).count(), 19);
    }

    #[test]
    fn a_stepped_ring_ends_on_its_north_west_corner() {
        let centre = Node { x: 5, y: 4 };
        let ring: Vec<_> = centre.stepped_ring(1, 16, 16).map(|n| (n.x, n.y)).collect();
        assert_eq!(ring, [(5, 3), (6, 4), (5, 5), (4, 5), (4, 4), (4, 3)]);
        assert_eq!(centre.stepped_ring(0, 16, 16).collect::<Vec<_>>(), [centre]);
        let corner = Node { x: 0, y: 0 };
        let wrapped: Vec<_> = corner.stepped_ring(1, 16, 16).collect();
        assert_eq!(wrapped.last(), Some(&Node { x: 15, y: 15 }));
    }
}
