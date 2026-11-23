//! Territory: who owns each node (slot 9 bits 0–2) and where the border stones go (slot 10 bits
//! 4–6).
//!
//! Both rules were checked against savegames written by the Gold Edition's executable. In a fresh game
//! every player owns the 271 nodes within 9 steps of its headquarters. In every save examined,
//! including later ones with several military buildings, the stored border edges are exactly
//! those given by [`mark_borders`].

use crate::grid::{Cursor, Direction, Node, NodeRect};
use crate::marks::Edges;
use crate::nodes::{NodeFlags, NodeMap};
use crate::sites::refresh_site;
use crate::world::{Player, World};

/// How far a headquarters' territory reaches, in steps: one less than its strength.
pub const HQ_RADIUS: u16 = 9;

/// The order a claim spreads to a node's neighbours (`0x3e358`).
pub(crate) const SPREAD_ORDER: [Direction; 6] = [
    Direction::NorthWest,
    Direction::NorthEast,
    Direction::East,
    Direction::SouthEast,
    Direction::SouthWest,
    Direction::West,
];

/// The order a claim clears the neighbours of a node it takes from another player (`0x3e358`).
pub(crate) const CLEAR_ORDER: [Direction; 6] = [
    Direction::NorthEast,
    Direction::East,
    Direction::SouthEast,
    Direction::SouthWest,
    Direction::West,
    Direction::NorthWest,
];

/// What a claim does to one neighbour of a node it spreads from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Contest {
    /// Nobody's: the claimant takes it.
    Gain,
    /// The claimant's already, held more weakly: its strength rises.
    Raise,
    /// Another player's, unprotected and weaker: the claimant takes it, after clearing what
    /// stands round it.
    Take,
    /// Left as it is.
    Keep,
}

/// The neighbours of `node` in `order` that lie on a `width` × `height` map: the claim does not
/// wrap round its edges.
pub(crate) fn neighbours_on_map(
    (width, height): (u16, u16),
    node: Node,
    order: [Direction; 6],
) -> impl Iterator<Item = Node> {
    let at = Cursor::at(node);
    order
        .into_iter()
        .filter_map(move |direction| at.step(direction).node(width, height))
}

/// The nodes a claim of strength `strength` round `centre` spreads from, in order (`0x3e0fc`):
/// the centre, then rings 1 to `strength`, each from `k` steps north-west round east to
/// north-east, a node before the step that leaves it. Nodes past a map edge are left out.
pub(crate) fn claim_order((width, height): (u16, u16), centre: Node, strength: u8) -> Vec<Node> {
    let mut nodes = vec![centre];
    for ring in 1..=i32::from(strength) {
        let mut at = Cursor::at(centre);
        for _ in 0..ring {
            at = at.step(Direction::NorthWest);
        }
        for direction in [
            Direction::East,
            Direction::SouthEast,
            Direction::SouthWest,
            Direction::West,
            Direction::NorthWest,
            Direction::NorthEast,
        ] {
            for _ in 0..ring {
                nodes.extend(at.node(width, height));
                at = at.step(direction);
            }
        }
    }
    nodes
}

/// The strength a claim by `owner` spreads from `node`, or `None` when the node is not
/// `owner`'s or holds less than 2 (`0x3e358`).
pub(crate) fn spreads_from(map: &NodeMap, node: Node, owner: Player) -> Option<u8> {
    let data = map.get(node);
    (data.owner == Some(owner) && data.strength >= 2).then_some(data.strength)
}

/// What a claim by `owner` spreading `value` does to `node` (`0x3e358`).
pub(crate) fn contest(map: &NodeMap, node: Node, owner: Player, value: u8) -> Contest {
    let data = map.get(node);
    match data.owner {
        None => Contest::Gain,
        Some(held) if held == owner => {
            if data.strength < value {
                Contest::Raise
            } else {
                Contest::Keep
            }
        }
        Some(_) if data.protected || data.strength >= value => Contest::Keep,
        Some(_) => Contest::Take,
    }
}

/// Gives `node` to `owner` at strength `value`, as a claim's spread does (`0x3f184`, `0x3f1b4`):
/// past the owner setter, so a campaign notes no contact.
pub(crate) fn settle(map: &mut NodeMap, node: Node, owner: Player, value: u8) {
    let mut data = map.get(node);
    data.owner = Some(owner);
    data.strength = value;
    map.set_spreading(node, data);
}

/// The ten nodes a building on `node` protects (`0x3e7d0`): its own, its six neighbours and the
/// east, south-east and south-west neighbours of its flag.
pub(crate) fn protected_nodes((width, height): (u16, u16), node: Node) -> [Node; 10] {
    let flag = node.neighbour(Direction::SouthEast, width, height);
    let mut nodes = [node; 10];
    for (slot, direction) in nodes[1..7].iter_mut().zip(Direction::ALL) {
        *slot = node.neighbour(direction, width, height);
    }
    for (slot, direction) in
        nodes[7..]
            .iter_mut()
            .zip([Direction::East, Direction::SouthEast, Direction::SouthWest])
    {
        *slot = flag.neighbour(direction, width, height);
    }
    nodes
}

/// Starts a claim by `owner` of strength `strength` from a building on `centre` (`0x3e0fc`
/// steps 4 to 6): the building's node takes the strength, and its ten protected nodes are
/// protected and given to `owner` through the owner setter (`0x3e760`), which in a campaign notes
/// a contact for each one player 0 can see, whoever owned it before.
pub(crate) fn begin_claim(map: &mut NodeMap, centre: Node, strength: u8, owner: Player) {
    let mut data = map.get(centre);
    data.strength = strength;
    map.set(centre, data);
    for node in protected_nodes((map.width(), map.height()), centre) {
        let mut data = map.get(node);
        data.protected = true;
        data.owner = Some(owner);
        map.set_owner(node, data);
    }
}

/// Claims land for `owner` round a building of strength `strength` on `centre` (`0x3e0fc`) where
/// no other player's land lies in reach, as for a headquarters at the start of a game. A lone
/// claim gives ring `k` strength `strength − k` and owns rings 0 to `strength − 1`. Returns the
/// nodes whose border marks may change.
///
/// Taking another player's land clears what stands round it, which needs the whole game; a
/// claim here leaves that land alone.
pub fn claim(map: &mut NodeMap, centre: Node, strength: u8, owner: Player) -> NodeRect {
    let size = (map.width(), map.height());
    begin_claim(map, centre, strength, owner);
    for from in claim_order(size, centre, strength) {
        let Some(value) = spreads_from(map, from, owner) else {
            continue;
        };
        for node in neighbours_on_map(size, from, SPREAD_ORDER) {
            if matches!(
                contest(map, node, owner, value - 1),
                Contest::Gain | Contest::Raise
            ) {
                settle(map, node, owner, value - 1);
            }
        }
    }
    let reach = i32::from(strength) + 1;
    let span = u32::from(strength) * 2 + 3;
    NodeRect {
        x: i32::from(centre.x) - reach,
        y: i32::from(centre.y) - reach,
        width: span,
        height: span,
    }
}

/// The two corners of a triangle that share an owner while the third has another (or none),
/// as indices into `corners`; `None` when all three match or no two owned corners do.
fn shared_pair(corners: [Option<Player>; 3]) -> Option<(usize, usize)> {
    let mut matching = [(0, 1), (0, 2), (1, 2)]
        .into_iter()
        .filter(|&(a, b)| corners[a].is_some() && corners[a] == corners[b]);
    match (matching.next(), matching.next()) {
        (Some(pair), None) => Some(pair),
        _ => None,
    }
}

/// Adds the border marks of the two triangles below `node`, node–E–SE and node–SE–SW, as
/// `0x3f28c` does. In a triangle whose corners belong to exactly two owners, the edge between the
/// two that match is a border edge. Each edge is stored on the node it leaves east, south-east
/// or south-west. The border-node flag goes on `node` when it gains an edge of its own, on E and
/// SE for the E–SE edge, and on SW for either edge ending there. The far end of a node's east or
/// south-east edge is not flagged here.
fn mark(map: &mut NodeMap, node: Node) {
    let (width, height) = (map.width(), map.height());
    let [east, south_east, south_west] = [Direction::East, Direction::SouthEast, Direction::SouthWest]
        .map(|d| node.neighbour(d, width, height));
    let owner = |n: Node| map.get(n).owner;
    let upper = shared_pair([owner(node), owner(east), owner(south_east)]);
    let lower = shared_pair([owner(node), owner(south_east), owner(south_west)]);
    let add = |map: &mut NodeMap, at: Node, edges: Edges| {
        let mut data = map.get(at);
        data.flags |= NodeFlags::BORDER;
        data.borders |= edges;
        map.set(at, data);
    };
    let mut own = Edges::empty();
    match upper {
        Some((0, 1)) => own |= Edges::EAST,
        Some((0, 2)) => own |= Edges::SOUTH_EAST,
        Some(_) => {
            add(map, east, Edges::SOUTH_WEST);
            add(map, south_east, Edges::empty());
        }
        None => {}
    }
    match lower {
        Some((0, 1)) => own |= Edges::SOUTH_EAST,
        Some((0, 2)) => {
            own |= Edges::SOUTH_WEST;
            add(map, south_west, Edges::empty());
        }
        Some(_) => add(map, south_west, Edges::EAST),
        None => {}
    }
    if !own.is_empty() {
        add(map, node, own);
    }
}

/// Clears every node's border marks and marks the whole map again, node by node, as the border
/// pass marks the nodes it covers. The game only redraws around a change ([`border_pass`]);
/// this function is for maps built by hand.
pub fn mark_borders(map: &mut NodeMap) {
    let (width, height) = (map.width(), map.height());
    let nodes = || (0..height).flat_map(move |y| (0..width).map(move |x| Node { x, y }));
    for node in nodes() {
        let mut data = map.get(node);
        data.flags -= NodeFlags::BORDER;
        data.borders = Edges::empty();
        map.set(node, data);
    }
    for node in nodes() {
        mark(map, node);
    }
}

/// Redraws the border and the building sites it affects after the territory of radius `radius`
/// around `centre` changed (`0x3e550`). It clears the marks on rings 1 to `radius + 1` and marks
/// rings 1 to `radius + 3` again. Then it recomputes the site of each node on rings 1 to
/// `radius + 2`, its south-east neighbour first. Marks outside the cleared rings are only ever
/// added to, so stale ones there stay.
pub fn border_pass(map: &mut NodeMap, world: &World, centre: Node, radius: u16) {
    let (width, height) = (map.width(), map.height());
    let rings = |last: u16| (1..=last).flat_map(move |r| centre.ring(r, width, height));
    for node in rings(radius + 1) {
        let mut data = map.get(node);
        data.flags -= NodeFlags::BORDER;
        data.borders = Edges::empty();
        map.set(node, data);
    }
    for node in rings(radius + 3) {
        mark(map, node);
    }
    for node in rings(radius + 2) {
        refresh_site(map, world, node.neighbour(Direction::SouthEast, width, height));
        refresh_site(map, world, node);
    }
}
