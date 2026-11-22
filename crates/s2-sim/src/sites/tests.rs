use super::*;
use crate::nodes::NodeData;
use crate::world::{Building, BuildingKind, BuildingState, Flag, FlagKind, Granite, Player, Tree};

const SIZE: u16 = 16;
const MEADOW: u8 = 8;
const WATER: u8 = 5;
const MOUNTAIN: u8 = 1;
const CENTRE: Node = Node { x: 8, y: 8 };

const TREE: MapObject = MapObject::Tree(Tree {
    kind: 0,
    stage: Tree::GROWN,
    felling: false,
    phase: 0,
});

const GRANITE: MapObject = MapObject::Granite(Granite {
    kind: 0,
    size: 1,
    spare: 0,
});

fn flat(terrain: u8) -> (NodeMap, World) {
    let data = NodeData {
        altitude: 10,
        terrain: [terrain; 2],
        ..NodeData::default()
    };
    (
        NodeMap::filled(SIZE, SIZE, data).unwrap(),
        World::empty(SIZE, SIZE),
    )
}

fn at(direction: Direction, steps: u16) -> Node {
    CENTRE.towards(direction, steps, SIZE, SIZE)
}

fn flag(world: &mut World, node: Node, kind: FlagKind) {
    let owner = Player::new(0).unwrap();
    let record = world.add_flag(Flag::new(kind, owner, node)).unwrap();
    world.set_object(node, MapObject::Flag(record));
}

fn building(world: &mut World, node: Node) {
    let record = world
        .add_building(Building::new(
            BuildingKind::HEADQUARTERS,
            Player::new(0).unwrap(),
            node,
            0,
            BuildingState::NEW_HEADQUARTERS,
        ))
        .unwrap();
    world.set_object(node, MapObject::Building(record));
}

fn size(nodes: &NodeMap, world: &World) -> SiteSize {
    site(nodes, world, CENTRE).size
}

#[test]
fn open_meadow_takes_a_large_building_and_rules_nothing_out() {
    let (nodes, world) = flat(MEADOW);
    assert_eq!(
        site(&nodes, &world, CENTRE),
        Sites {
            size: SiteSize::Large,
            flags: SiteFlags::empty()
        }
    );
}

#[test]
fn water_and_mountain() {
    let (nodes, world) = flat(WATER);
    let closed = SiteFlags::NO_PLANTING | SiteFlags::BLOCKED | SiteFlags::NO_FLAG | SiteFlags::NO_ROAD;
    assert_eq!(site(&nodes, &world, CENTRE).flags, closed);
    let (nodes, world) = flat(MOUNTAIN);
    assert_eq!(size(&nodes, &world), SiteSize::Mine);
}

#[test]
fn neighbours_shrink_the_site() {
    let (nodes, mut world) = flat(MEADOW);
    world.set_object(at(Direction::East, 1), TREE);
    assert_eq!(size(&nodes, &world), SiteSize::Small, "a tree next to it");

    let (nodes, mut world) = flat(MEADOW);
    building(&mut world, at(Direction::East, 2));
    assert_eq!(
        size(&nodes, &world),
        SiteSize::Medium,
        "a building two steps away"
    );

    let (nodes, mut world) = flat(MEADOW);
    flag(&mut world, at(Direction::West, 1), FlagKind::Normal);
    let found = site(&nodes, &world, CENTRE);
    assert_eq!(
        found.size,
        SiteSize::Medium,
        "a flag where the large footprint goes"
    );
    assert!(found.flags.contains(SiteFlags::NO_FLAG));

    let (nodes, mut world) = flat(MEADOW);
    world.set_object(at(Direction::East, 1), GRANITE);
    let found = site(&nodes, &world, CENTRE);
    assert_eq!(found.size, SiteSize::Flag, "granite next to it");
    assert!(found.flags.contains(SiteFlags::NO_PLANTING));
}

#[test]
fn a_water_flag_south_east_rules_out_a_building() {
    let (nodes, mut world) = flat(MEADOW);
    flag(&mut world, at(Direction::SouthEast, 1), FlagKind::Water);
    assert_eq!(size(&nodes, &world), SiteSize::Nothing);
    let (nodes, mut world) = flat(MEADOW);
    flag(&mut world, at(Direction::SouthEast, 1), FlagKind::Normal);
    assert_eq!(
        size(&nodes, &world),
        SiteSize::Large,
        "an ordinary flag can serve it"
    );
}

#[test]
fn slopes() {
    let (mut nodes, world) = flat(MEADOW);
    let mut data = nodes.get(at(Direction::SouthEast, 1));
    data.altitude += FLAG_RISE;
    nodes.set(at(Direction::SouthEast, 1), data);
    assert_eq!(
        size(&nodes, &world),
        SiteSize::Flag,
        "the flag would stand too high"
    );

    let (mut nodes, world) = flat(MEADOW);
    let mut data = nodes.get(at(Direction::NorthEast, 2));
    data.altitude += LARGE_STEP + 1;
    nodes.set(at(Direction::NorthEast, 2), data);
    assert_eq!(
        size(&nodes, &world),
        SiteSize::Small,
        "uneven ground two steps out"
    );
}

#[test]
fn trees_and_flags_on_the_node_stay_walkable() {
    let (nodes, mut world) = flat(MEADOW);
    world.set_object(CENTRE, TREE);
    let found = site(&nodes, &world, CENTRE);
    assert_eq!(found.size, SiteSize::Nothing);
    assert!(!found.flags.contains(SiteFlags::BLOCKED));
    world.set_object(CENTRE, GRANITE);
    assert!(site(&nodes, &world, CENTRE).flags.contains(SiteFlags::BLOCKED));
}

#[test]
fn refreshing_sees_earlier_results_in_the_ring() {
    let (mut nodes, mut world) = flat(MEADOW);
    for node in CENTRE.rings(3, SIZE, SIZE) {
        let mut data = nodes.get(node);
        data.sites = Sites::default();
        nodes.set(node, data);
    }
    flag(&mut world, CENTRE, FlagKind::Normal);
    let north_east = at(Direction::NorthEast, 1);
    refresh_sites(&mut nodes, &world, CENTRE, 2);
    assert_eq!(
        nodes.get(north_east).sites.size,
        SiteSize::Large,
        "its flag would go east of the new flag, which the ring reaches later"
    );
    refresh_sites(&mut nodes, &world, CENTRE, 2);
    assert_eq!(
        nodes.get(north_east).sites.size,
        SiteSize::Nothing,
        "the second pass sees it"
    );
    assert_eq!(
        nodes.get(at(Direction::East, 1)).sites.size,
        SiteSize::Medium,
        "the new flag is in its large footprint"
    );
    assert_eq!(
        nodes.get(at(Direction::NorthEast, 3)).sites,
        Sites::default(),
        "three steps away is left alone"
    );
}
