use super::*;
use crate::nodes::{NodeData, SiteSize};
use crate::sites::site;
use crate::world::Decoration;

const SIZE: u16 = 24;
const MEADOW: u8 = 8;

fn player() -> Player {
    Player::new(0).unwrap()
}

fn node(x: u16, y: u16) -> Node {
    Node { x, y }
}

/// Owned meadow with every node's slot 8 worked out from scratch.
fn meadow() -> (NodeMap, World) {
    let data = NodeData {
        altitude: 10,
        terrain: [MEADOW; 2],
        owner: Some(player()),
        ..NodeData::default()
    };
    let mut nodes = NodeMap::filled(SIZE, SIZE, data).unwrap();
    let world = World::empty(SIZE, SIZE);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut data = nodes.get(node(x, y));
            data.sites = site(&nodes, &world, node(x, y));
            nodes.set(node(x, y), data);
        }
    }
    (nodes, world)
}

fn flag(nodes: &mut NodeMap, world: &mut World, at: Node) -> u16 {
    world
        .place_flag(nodes, at, FlagKind::Normal, true)
        .expect("room for a flag")
}

/// Nodes east of `from`, `count` of them.
fn east(from: Node, count: u16) -> Vec<Node> {
    (1..=count).map(|i| node(from.x + i, from.y)).collect()
}

#[test]
fn a_road_is_written_into_slot_3_and_linked_at_both_flags() {
    let (mut nodes, mut world) = meadow();
    let a = flag(&mut nodes, &mut world, node(4, 6));
    let b = flag(&mut nodes, &mut world, node(8, 6));
    let road = world
        .create_road(&mut nodes, RoadKind::Normal, a, Some(b), east(node(4, 6), 3))
        .unwrap();
    assert_eq!(road, 0);
    for x in 4..8 {
        let roads = nodes.get(node(x, 6)).roads;
        assert_eq!(roads.along(Edges::EAST), Some(RoadKind::Normal), "{x}");
        assert_eq!(roads.marker(), Some(RoadKind::Normal), "{x}");
    }
    assert_eq!(nodes.get(node(8, 6)).roads.marker(), Some(RoadKind::Normal));
    for x in 5..8 {
        let flags = nodes.get(node(x, 6)).sites.flags;
        assert!(flags.contains(SiteFlags::NO_ROAD), "no road may cross at {x}");
        assert!(
            !flags.contains(SiteFlags::BLOCKED),
            "figures walk the road at {x}"
        );
    }
    let link = |flag: u16, way: Direction| world.flag(flag).unwrap().links[way.index()];
    assert_eq!(link(a, Direction::East), Some(FlagLink { road, far: Some(b) }));
    assert_eq!(link(b, Direction::West), Some(FlagLink { road, far: Some(a) }));
}

#[test]
fn a_refused_road_is_cleared_again_with_the_flags_markers() {
    let (mut nodes, mut world) = meadow();
    let a = flag(&mut nodes, &mut world, node(4, 6));
    let b = flag(&mut nodes, &mut world, node(8, 6));
    let c = flag(&mut nodes, &mut world, node(8, 10));
    world
        .create_road(&mut nodes, RoadKind::Normal, a, Some(b), east(node(4, 6), 3))
        .unwrap();
    let path = vec![node(5, 6), node(5, 7), node(6, 8), node(6, 9), node(7, 10)];
    assert_eq!(
        world.create_road(&mut nodes, RoadKind::Normal, a, Some(c), path.clone()),
        Err(RoadError::StartTaken)
    );
    assert_eq!(world.roads().count(), 1);
    assert_eq!(
        nodes.get(node(4, 6)).roads.marker(),
        None,
        "clearing the refused road takes the start flag's marker although a road still leaves it"
    );
    assert_eq!(
        nodes.get(node(4, 6)).roads.along(Edges::EAST),
        None,
        "and the shared first segment"
    );
    assert_eq!(
        world.create_road(&mut nodes, RoadKind::Normal, a, Some(a), path),
        Err(RoadError::SameFlag)
    );
}

#[test]
fn a_flag_on_a_road_splits_it_into_two_with_its_traffic() {
    let (mut nodes, mut world) = meadow();
    let a = flag(&mut nodes, &mut world, node(4, 6));
    let b = flag(&mut nodes, &mut world, node(10, 6));
    let road = world
        .create_road(&mut nodes, RoadKind::Normal, a, Some(b), east(node(4, 6), 5))
        .unwrap();
    world.roads.get_mut(road).unwrap().traffic = 355;
    let middle = flag(&mut nodes, &mut world, node(7, 6));
    let roads: Vec<_> = world
        .roads()
        .map(|(r, road)| (r, road.start, road.end, road.path.len()))
        .collect();
    assert_eq!(roads, [(0, a, Some(middle), 2), (1, middle, Some(b), 2)]);
    assert!(world.roads().all(|(_, road)| road.traffic == 99));
    let links = world.flag(middle).unwrap().links;
    assert_eq!(
        links[Direction::West.index()],
        Some(FlagLink {
            road: 0,
            far: Some(a)
        })
    );
    assert_eq!(
        links[Direction::East.index()],
        Some(FlagLink {
            road: 1,
            far: Some(b)
        })
    );
    assert_eq!(world.road_through(&nodes, node(6, 6)), Some(0));
    assert_eq!(
        world.road_through(&nodes, node(7, 6)),
        None,
        "a flag node is no road's inner node"
    );
}

#[test]
fn a_flag_on_a_donkey_road_is_a_donkey_flag_and_unsplit_roads_stay_whole() {
    let (mut nodes, mut world) = meadow();
    let a = flag(&mut nodes, &mut world, node(4, 6));
    let b = flag(&mut nodes, &mut world, node(10, 6));
    world
        .create_road(&mut nodes, RoadKind::Donkey, a, Some(b), east(node(4, 6), 5))
        .unwrap();
    let middle = world
        .place_flag(&mut nodes, node(7, 6), FlagKind::Normal, false)
        .unwrap();
    assert_eq!(world.flag(middle).unwrap().kind, FlagKind::Donkey);
    assert_eq!(world.roads().count(), 1);
}

#[test]
fn removing_a_flag_removes_its_roads_and_frees_the_lowest_records() {
    let (mut nodes, mut world) = meadow();
    let a = flag(&mut nodes, &mut world, node(4, 6));
    let b = flag(&mut nodes, &mut world, node(8, 6));
    let c = flag(&mut nodes, &mut world, node(12, 6));
    world
        .create_road(&mut nodes, RoadKind::Normal, a, Some(b), east(node(4, 6), 3))
        .unwrap();
    world
        .create_road(&mut nodes, RoadKind::Normal, b, Some(c), east(node(8, 6), 3))
        .unwrap();
    world.remove_flag(&mut nodes, a);
    assert_eq!(world.flag_at(node(4, 6)), None);
    assert_eq!(world.roads().map(|(r, _)| r).collect::<Vec<_>>(), [1]);
    assert!(nodes.get(node(5, 6)).roads.is_empty());
    assert_eq!(world.flag(b).unwrap().links[Direction::West.index()], None);
    assert_eq!(
        flag(&mut nodes, &mut world, node(4, 10)),
        a,
        "the freed record is reused"
    );
}

#[test]
fn a_finished_road_clears_some_decorations_off_its_path() {
    let (mut nodes, mut world) = meadow();
    let flower = MapObject::Decoration(Decoration { set: 0, index: 5 });
    let stone = MapObject::Decoration(Decoration { set: 0, index: 34 });
    world.set_object(node(5, 6), flower);
    world.set_object(node(6, 6), stone);
    let a = flag(&mut nodes, &mut world, node(4, 6));
    let b = flag(&mut nodes, &mut world, node(8, 6));
    world
        .create_road(&mut nodes, RoadKind::Normal, a, Some(b), east(node(4, 6), 3))
        .unwrap();
    assert_eq!(world.object(node(5, 6)), MapObject::Nothing);
    assert_eq!(world.object(node(6, 6)), stone);
}

#[test]
fn a_road_being_drawn_stops_at_its_head_and_clears_nothing() {
    let (mut nodes, mut world) = meadow();
    let flower = MapObject::Decoration(Decoration { set: 0, index: 5 });
    world.set_object(node(5, 6), flower);
    let a = flag(&mut nodes, &mut world, node(4, 6));
    let road = world
        .create_road(&mut nodes, RoadKind::Normal, a, None, east(node(4, 6), 2))
        .unwrap();
    assert_eq!(
        world.flag(a).unwrap().links[Direction::East.index()],
        Some(FlagLink { road, far: None })
    );
    assert_eq!(nodes.get(node(6, 6)).roads.marker(), Some(RoadKind::Normal));
    assert_eq!(nodes.get(node(6, 6)).roads.along(Edges::EAST), None);
    assert_eq!(world.object(node(5, 6)), flower);
    world.remove_road(&mut nodes, road);
    assert_eq!(world.flag(a).unwrap().links, [None; 6]);
    assert!((4..=6).all(|x| nodes.get(node(x, 6)).roads.is_empty()));
}

#[test]
fn a_flag_needs_room_and_an_owner() {
    let (mut nodes, mut world) = meadow();
    flag(&mut nodes, &mut world, node(4, 6));
    assert_eq!(
        world.place_flag(&mut nodes, node(5, 6), FlagKind::Normal, true),
        None
    );
    let mut data = nodes.get(node(4, 12));
    data.owner = None;
    nodes.set(node(4, 12), data);
    assert_eq!(
        world.place_flag(&mut nodes, node(4, 12), FlagKind::Normal, true),
        None
    );
    assert_eq!(
        nodes.get(node(6, 6)).sites.size,
        SiteSize::Large,
        "two steps away keeps its size"
    );
}
