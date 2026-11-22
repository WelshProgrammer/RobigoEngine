use super::*;
use crate::grid::{Direction, Node};
use crate::nodes::{NodeData, NodeMap};
use crate::sites::site;
use crate::world::{FlagKind, Player};

use Direction::{NorthEast, SouthEast};

const SIZE: u16 = 24;

/// What a ware routes with.
const ALL: SearchOptions = SearchOptions::BUSY
    .union(SearchOptions::LOAD)
    .union(SearchOptions::WATER);
const MEADOW: u8 = 8;

/// Owned meadow with every node's slot 8 worked out from scratch.
fn meadow() -> (NodeMap, World) {
    let data = NodeData {
        altitude: 10,
        terrain: [MEADOW; 2],
        owner: Player::new(0),
        ..NodeData::default()
    };
    let mut nodes = NodeMap::filled(SIZE, SIZE, data).unwrap();
    let world = World::empty(SIZE, SIZE);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let at = Node { x, y };
            let mut data = nodes.get(at);
            data.sites = site(&nodes, &world, at);
            nodes.set(at, data);
        }
    }
    (nodes, world)
}

/// `count` nodes from `from` going `direction`, `from` left out.
fn walk(from: Node, direction: Direction, count: usize) -> Vec<Node> {
    std::iter::successors(Some(from), |node| Some(node.neighbour(direction, SIZE, SIZE)))
        .skip(1)
        .take(count)
        .collect()
}

/// A diamond of four flags three edges apart: A on the west, B north-east of it, C south-east of
/// it, and D east, reached from both B and C. Returns the world and the flags A, B, C, D and the
/// roads A–B, A–C, B–D and C–D, all served with nothing on them.
struct Diamond {
    world: World,
    flags: [u16; 4],
    roads: [u16; 4],
}

fn diamond(kind: RoadKind) -> Diamond {
    let (mut nodes, mut world) = meadow();
    let a_node = Node { x: 6, y: 10 };
    let b_node = walk(a_node, NorthEast, 3)[2];
    let c_node = walk(a_node, SouthEast, 3)[2];
    let d_node = walk(b_node, SouthEast, 3)[2];
    assert_eq!(d_node, walk(c_node, NorthEast, 3)[2]);
    let mut flag = |at| world.place_flag(&mut nodes, at, FlagKind::Normal, true).unwrap();
    let flags = [flag(a_node), flag(b_node), flag(c_node), flag(d_node)];
    let [a, b, c, d] = flags;
    let mut road = |start, from, direction, end| {
        let path = walk(from, direction, 2);
        world
            .create_road(&mut nodes, kind, start, Some(end), path)
            .unwrap()
    };
    let roads = [
        road(a, a_node, NorthEast, b),
        road(a, a_node, SouthEast, c),
        road(b, b_node, SouthEast, d),
        road(c, c_node, NorthEast, d),
    ];
    for &record in &roads {
        world.road_mut(record).unwrap().served = true;
    }
    Diamond { world, flags, roads }
}

#[test]
fn equal_routes_go_to_the_flag_the_destination_reaches_first() {
    let Diamond { world, flags, .. } = diamond(RoadKind::Normal);
    let [a, b, c, d] = flags;
    assert_eq!(
        next_flag(&world, a, d, 250, ALL),
        Some(c),
        "D reaches C (south-west) before B (north-west)"
    );
    assert_eq!(
        next_flag(&world, d, a, 250, ALL),
        Some(c),
        "A reaches C (south-east, direction 1) before B (north-east, direction 5)"
    );
    assert_eq!(next_flag(&world, a, b, 250, ALL), Some(b));
    assert_eq!(next_flag(&world, a, a, 250, ALL), None);
}

#[test]
fn busy_roads_and_crowded_flags_cost_extra() {
    let Diamond {
        mut world,
        flags,
        roads,
    } = diamond(RoadKind::Normal);
    let [a, b, c, d] = flags;
    world.road_mut(roads[1]).unwrap().busy = 1;
    assert_eq!(next_flag(&world, a, d, 250, ALL), Some(b), "A–C is busier");
    world.road_mut(roads[1]).unwrap().busy = 0;
    world.road_mut(roads[1]).unwrap().served = false;
    assert_eq!(
        next_flag(&world, a, d, 250, ALL),
        Some(b),
        "A–C has never had a carrier"
    );
    world.road_mut(roads[1]).unwrap().served = true;
    world.flag_mut(c).unwrap().waiting = 4;
    assert_eq!(
        next_flag(&world, a, d, 250, ALL),
        Some(c),
        "four wares cost nothing"
    );
    world.flag_mut(c).unwrap().waiting = 5;
    assert_eq!(next_flag(&world, a, d, 250, ALL), Some(b), "five wares cost 2");
}

#[test]
fn costs_add_up_and_stop_at_the_limit() {
    let Diamond { mut world, flags, .. } = diamond(RoadKind::Normal);
    let [a, _, _, d] = flags;
    let found = search(&world, d, 250, SearchOptions::empty(), |r| r.flag == a).unwrap();
    assert_eq!(found.cost, 6);
    assert!(search(&world, d, 5, SearchOptions::empty(), |r| r.flag == a).is_none());
    world.flag_mut(a).unwrap().waiting = 8;
    let options = SearchOptions::LOAD;
    let found = search(&world, d, 250, options, |r| r.flag == a).unwrap();
    assert_eq!(found.cost, 11, "eight wares at A cost 5");
}

#[test]
fn water_roads_need_the_water_option() {
    let Diamond { world, flags, .. } = diamond(RoadKind::Water);
    let [a, _, _, d] = flags;
    assert!(search(&world, d, 250, SearchOptions::empty(), |r| r.flag == a).is_none());
    assert!(search(&world, d, 250, SearchOptions::WATER, |r| r.flag == a).is_some());
}

#[test]
fn skipping_the_start_finds_the_nearest_other_flag() {
    let Diamond { world, flags, .. } = diamond(RoadKind::Normal);
    let [a, _, c, _] = flags;
    let any = |_: &Reached| true;
    assert_eq!(
        search(&world, a, 250, SearchOptions::empty(), any).unwrap().flag,
        a
    );
    let found = search(&world, a, 250, SearchOptions::SKIP_START, any).unwrap();
    assert_eq!((found.flag, found.from, found.cost), (c, Some(a), 3));
}
