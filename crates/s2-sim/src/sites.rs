//! Building sites (slot 8): what each node can take and whether figures can cross it. `S2.EXE`
//! never recomputes the whole map. It trusts the map file and recomputes the nodes around each
//! change, as these functions do.

use crate::grid::{Direction, Node};
use crate::ground::Property;
use crate::nodes::{NodeFlags, NodeMap, SiteFlags, SiteSize, Sites};
use crate::world::{FlagKind, MapObject, World};

/// Ground nothing can be built on or walk to: lava or polar ground anywhere around the node, or
/// water or class 5 all round.
const UNUSABLE: Property = Property::ANY_LAVA
    .union(Property::ALL_WATER)
    .union(Property::ANY_POLAR)
    .union(Property::ALL_CLASS_5);

/// Water and class 5 ground meeting with nothing else around the node, also unusable.
const SHORE: Property = Property::ANY_WATER.union(Property::ANY_CLASS_5);

/// A building's flag must stand less than this much higher than the building.
const FLAG_RISE: u8 = 2;

/// A mine's flag must stand less than this much higher than the mine.
const MINE_FLAG_RISE: u8 = 4;

/// The largest height step from a building's node to any neighbour.
const NEIGHBOUR_STEP: u8 = 3;

/// The largest height difference from a large building's node to any node two steps away.
const LARGE_STEP: u8 = 2;

/// What the object on a node means for the node itself.
enum Occupant {
    /// Nothing that matters: the node may take a flag or building.
    Clear,
    /// A tree or a flag: nothing more can go here, but figures can walk on.
    Passable,
    /// Something figures cannot walk onto either.
    Solid,
}

fn occupant(object: MapObject) -> Occupant {
    match object {
        MapObject::Flag(_) | MapObject::Tree(_) => Occupant::Passable,
        MapObject::Decoration(decoration) if decoration.blocks() => Occupant::Solid,
        // `S2.EXE` tests slot 5 `0xcc`–`0xd3` here, which takes in fields along with granite.
        MapObject::Building(_)
        | MapObject::Granite(_)
        | MapObject::Field(_)
        | MapObject::Fire(_)
        | MapObject::ShipSite(_) => Occupant::Solid,
        _ => Occupant::Clear,
    }
}

/// What the six neighbours of a node hold that bears on the node's own site.
#[derive(Default)]
struct Neighbours {
    /// A building, granite, a field or a fire next to the node.
    obstacle: bool,
    /// A tree next to it.
    tree: bool,
    /// A flag next to it.
    flag: bool,
}

impl Neighbours {
    fn of(world: &World, node: Node) -> Self {
        let mut found = Self::default();
        for direction in Direction::ALL {
            let object = world.object(node.neighbour(direction, world.width(), world.height()));
            match object {
                MapObject::Building(_) | MapObject::Granite(_) | MapObject::Field(_) | MapObject::Fire(_) => {
                    found.obstacle = true;
                }
                MapObject::Flag(_) => found.flag = true,
                MapObject::Tree(_) => found.tree = true,
                _ => {}
            }
        }
        found
    }
}

/// The site `S2.EXE` computes for `node` from the ground, objects, roads and heights around it
/// (`0x192b7`). Of the node's old value only [`SiteFlags::FOOTPRINT`] survives, which only
/// placing and removing large buildings changes.
#[must_use]
pub fn site(nodes: &NodeMap, world: &World, node: Node) -> Sites {
    let (width, height) = (nodes.width(), nodes.height());
    let here = nodes.get(node);
    let footprint = here.sites.flags & SiteFlags::FOOTPRINT;
    let mut flags =
        footprint | SiteFlags::NO_PLANTING | SiteFlags::BLOCKED | SiteFlags::NO_FLAG | SiteFlags::NO_ROAD;
    let mut size = SiteSize::Nothing;
    let closed = Sites { size, flags };
    let walkable = Sites {
        size,
        flags: flags - SiteFlags::BLOCKED,
    };

    let property = nodes.property(node);
    if property.intersects(UNUSABLE) || property == SHORE || property.contains(Property::ANY_CLASS_13) {
        return closed;
    }
    match occupant(world.object(node)) {
        Occupant::Passable => return walkable,
        Occupant::Solid => return closed,
        Occupant::Clear if !footprint.is_empty() => return closed,
        Occupant::Clear => {}
    }
    if here.flags.contains(NodeFlags::BORDER) {
        return walkable;
    }

    let around = Neighbours::of(world, node);
    if !around.flag {
        flags -= SiteFlags::NO_FLAG;
        size = SiteSize::Flag;
    }
    flags -= SiteFlags::BLOCKED;
    if !here.roads.is_empty() {
        return Sites { size, flags };
    }
    flags -= SiteFlags::NO_ROAD;
    if property.contains(Property::ALL_PLANTABLE) && !around.obstacle {
        flags -= SiteFlags::NO_PLANTING;
    }
    let stop = |size| Sites { size, flags };

    let flag_node = node.neighbour(Direction::SouthEast, width, height);
    let flag_data = nodes.get(flag_node);
    let flag_blocked = match world.object(flag_node) {
        MapObject::Flag(record) => world.flag(record).is_some_and(|f| f.kind == FlagKind::Water),
        _ => flag_data.sites.flags.contains(SiteFlags::NO_FLAG),
    };
    if flag_data.flags.contains(NodeFlags::BORDER) || flag_blocked || around.obstacle {
        return stop(size);
    }

    let altitude = here.altitude;
    let flag_altitude = flag_data.altitude;
    if property.contains(Property::ALL_MOUNTAIN) {
        if flag_altitude < altitude + MINE_FLAG_RISE {
            size = SiteSize::Mine;
        }
        return stop(size);
    }
    let steepest = Direction::ALL
        .into_iter()
        .map(|d| {
            nodes
                .get(node.neighbour(d, width, height))
                .altitude
                .abs_diff(altitude)
        })
        .max()
        .unwrap_or_default();
    if !property.contains(Property::ALL_BUILDABLE)
        || flag_altitude >= altitude + FLAG_RISE
        || steepest > NEIGHBOUR_STEP
    {
        return stop(size);
    }
    if around.tree {
        return stop(SiteSize::Small);
    }

    size = SiteSize::Large;
    for far in node.ring(2, width, height) {
        if matches!(world.object(far), MapObject::Building(_)) {
            size = SiteSize::Medium;
        }
        if nodes.get(far).altitude.abs_diff(altitude) > LARGE_STEP {
            return stop(SiteSize::Small);
        }
    }
    let footprint_taken = [Direction::West, Direction::NorthWest, Direction::NorthEast]
        .into_iter()
        .map(|d| node.neighbour(d, width, height))
        .any(|n| !nodes.get(n).roads.is_empty() || world.object(n).is_flag());
    if size == SiteSize::Large && footprint_taken {
        size = SiteSize::Medium;
    }
    stop(size)
}

/// Recomputes the site of `centre` and of every node within `reach` steps, ring by ring
/// outwards: `0x193aa` for a reach of 1, `0x19442` for 2. Each node sees the new values of
/// those before it.
pub fn refresh_sites(nodes: &mut NodeMap, world: &World, centre: Node, reach: u16) {
    for node in centre.rings(reach, nodes.width(), nodes.height()) {
        refresh_site(nodes, world, node);
    }
}

/// Recomputes the site of one node.
pub(crate) fn refresh_site(nodes: &mut NodeMap, world: &World, node: Node) {
    let mut data = nodes.get(node);
    data.sites = site(nodes, world, node);
    nodes.set(node, data);
}

#[cfg(test)]
mod tests;
