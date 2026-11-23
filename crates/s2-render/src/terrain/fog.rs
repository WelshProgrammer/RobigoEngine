//! The fog of war as the terrain pass sees it: one seen-flag per triangle for the viewing
//! player, and the dark fringe around what has been seen.

use s2_sim::{Direction, Edges, Node, NodeMap, NodeRect, Player, Seen, reveal_walk};

/// How much of a triangle the player sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Sight {
    /// Not drawn, and neither are its borders: the background shows.
    Hidden,
    /// Drawn with every shading row shifted right by two.
    Dimmed,
    /// Drawn normally.
    #[default]
    Visible,
}

impl Sight {
    pub(super) const fn shift(self) -> u32 {
        match self {
            Self::Dimmed => 2,
            Self::Hidden | Self::Visible => 0,
        }
    }
}

const FIRST: u8 = 1;
const SECOND: u8 = 2;

/// Which of a map's triangles one player has seen.
///
/// In `S2.EXE` these are bits 7 and 6 of slot 9 for player 0, bits 5 and 4 for player 1;
/// players above 1 see everything and need no fog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fog {
    width: u16,
    height: u16,
    seen: Vec<u8>,
    player: Option<Player>,
}

impl Fog {
    /// Fog over a `width` × `height` map with nothing seen yet.
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            seen: vec![0; usize::from(width) * usize::from(height)],
            player: None,
        }
    }

    fn bits(&self, node: Node) -> u8 {
        self.seen.get(self.index(node)).copied().unwrap_or_default()
    }

    fn index(&self, node: Node) -> usize {
        let node = Node::wrapped(node.x.into(), node.y.into(), self.width, self.height);
        usize::from(node.y) * usize::from(self.width) + usize::from(node.x)
    }

    fn step(&self, node: Node, direction: Direction) -> Node {
        node.neighbour(direction, self.width, self.height)
    }

    /// Whether `node`'s two triangles (node–SE–SW, node–E–SE) have been seen.
    #[must_use]
    pub fn seen(&self, node: Node) -> [bool; 2] {
        let bits = self.bits(node);
        [bits & FIRST != 0, bits & SECOND != 0]
    }

    /// Marks `node`'s triangles as seen where `seen` says so; never unmarks.
    pub fn mark(&mut self, node: Node, seen: [bool; 2]) {
        let at = self.index(node);
        if let Some(bits) = self.seen.get_mut(at) {
            *bits |= (u8::from(seen[0]) * FIRST) | (u8::from(seen[1]) * SECOND);
        }
    }

    /// Marks every triangle within `radius` rings of `centre` as seen, walking the rings as
    /// `S2.EXE` does ([`s2_sim::reveal_walk`]), and returns the nodes whose drawing may have
    /// changed.
    pub fn reveal(&mut self, centre: Node, radius: u16) -> NodeRect {
        reveal_walk((self.width, self.height), centre, radius, |node, seen| {
            self.mark(node, seen);
        })
    }

    /// The fog `player` sees in a game on `nodes`, following its seen bits through
    /// [`Self::follow`]; `None` for a player above 1, who sees everything.
    #[must_use]
    pub fn of(nodes: &NodeMap, player: Player) -> Option<Self> {
        Seen::of(player)?;
        let mut fog = Self::new(nodes.width(), nodes.height());
        fog.player = Some(player);
        fog.follow(nodes);
        Some(fog)
    }

    /// Takes the followed player's seen bits from `nodes`; nothing for fog that follows no one.
    pub fn follow(&mut self, nodes: &NodeMap) {
        let Some(player) = self.player else {
            return;
        };
        if (nodes.width(), nodes.height()) != (self.width, self.height) {
            return;
        }
        for y in 0..self.height {
            for x in 0..self.width {
                let node = Node { x, y };
                self.mark(node, nodes.seen(node, player));
            }
        }
    }

    /// The node's edges that can be seen (`0x3f8a8` bits 2–4): an edge is visible when either
    /// triangle along it has been seen. Roads and border stones are drawn only along these.
    #[must_use]
    pub fn edges(&self, node: Node) -> Edges {
        let [first, second] = self.seen(node);
        let north_east = self.seen(self.step(node, Direction::NorthEast))[0];
        let west = self.seen(self.step(node, Direction::West))[1];
        let mut edges = Edges::empty();
        edges.set(Edges::EAST, second || north_east);
        edges.set(Edges::SOUTH_EAST, first || second);
        edges.set(Edges::SOUTH_WEST, first || west);
        edges
    }

    /// Whether anything on `node` is drawn (`0x3f8a8` bit 5): its objects, flags, buildings and
    /// figures need one of its triangles or edges seen, and are then drawn in full.
    #[must_use]
    pub fn node_visible(&self, node: Node) -> bool {
        self.seen(node).contains(&true) || !self.edges(node).is_empty()
    }

    /// How the terrain pass draws `node`'s two triangles (`0x3fb08`): a seen triangle in full,
    /// an unseen one sharing a corner with a seen triangle dimmed, anything else not at all.
    #[must_use]
    pub fn sight(&self, node: Node) -> [Sight; 2] {
        use Direction::{East, NorthEast, NorthWest, SouthEast, SouthWest, West};
        let [first, second] = self.seen(node);
        if first && second {
            return [Sight::Visible; 2];
        }
        let any = |n: Node| self.bits(n) != 0;
        let has = |n: Node, bit: u8| self.bits(n) & bit != 0;
        let step = |n, d| self.step(n, d);
        let shared = any(step(node, NorthWest)) || any(step(node, SouthEast));
        let near_first = shared
            || second
            || any(step(node, West))
            || any(step(node, SouthWest))
            || has(step(node, NorthEast), FIRST)
            || has(step(node, East), FIRST)
            || has(step(step(node, SouthWest), West), SECOND);
        let near_second = shared
            || first
            || any(step(node, NorthEast))
            || any(step(node, East))
            || has(step(step(node, NorthEast), East), FIRST)
            || has(step(node, West), SECOND)
            || has(step(node, SouthWest), SECOND);
        let sight = |seen, near| match (seen, near) {
            (true, _) => Sight::Visible,
            (false, true) => Sight::Dimmed,
            (false, false) => Sight::Hidden,
        };
        [sight(first, near_first), sight(second, near_second)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(x: u16, y: u16) -> Node {
        Node { x, y }
    }

    #[test]
    fn seen_triangles_are_visible_and_their_neighbours_dimmed() {
        let mut fog = Fog::new(16, 16);
        fog.mark(node(5, 6), [true, false]);
        assert_eq!(fog.sight(node(5, 6)), [Sight::Visible, Sight::Dimmed]);
        assert_eq!(fog.sight(node(4, 6)), [Sight::Dimmed, Sight::Dimmed], "west");
        assert_eq!(fog.sight(node(6, 6)), [Sight::Dimmed, Sight::Hidden], "east");
        assert_eq!(fog.sight(node(5, 8)), [Sight::Hidden, Sight::Hidden]);
        assert_eq!(fog.sight(node(10, 10)), [Sight::Hidden; 2]);
    }

    #[test]
    fn a_hexagon_of_radius_r_holds_6_r_squared_triangles() {
        for radius in [1, 2, 5, 12] {
            let mut fog = Fog::new(64, 64);
            fog.reveal(node(30, 30), radius);
            let seen: u32 = fog.seen.iter().map(|b| b.count_ones()).sum();
            assert_eq!(seen, 6 * u32::from(radius).pow(2), "radius {radius}");
        }
    }

    #[test]
    fn revealing_stops_at_the_map_edges_and_reports_the_area() {
        let mut fog = Fog::new(32, 32);
        let area = fog.reveal(node(0, 0), 3);
        assert_eq!(fog.seen(node(0, 0)), [true, true]);
        assert_eq!(
            fog.seen(node(31, 31)),
            [false, false],
            "no wrap to the far corner"
        );
        assert_eq!((area.x, area.y, area.width), (-4, -4, 9));

        let mut fog = Fog::new(64, 64);
        fog.reveal(node(3, 32), 12);
        let count = |columns: std::ops::Range<u16>| {
            columns
                .flat_map(|x| (0..64).map(move |y| node(x, y)))
                .map(|n| fog.seen(n).into_iter().filter(|&s| s).count())
                .sum::<usize>()
        };
        assert_eq!(count(0..32), 565, "a few triangles along the edge stay unmarked");
        assert_eq!(count(32..64), 0);
    }
}
