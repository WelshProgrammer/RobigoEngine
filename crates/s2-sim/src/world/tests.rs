use s2_formats::map::Map;

use super::*;
use crate::territory::mark_borders;
use crate::{Edges, FigureKind, NodeData, NodeFlags, NodeMap, RoadKind, Roads, SiteFlags, SiteSize};

/// A `.WLD` file of `width` × `height` nodes whose layers are all zero except those in
/// `layers` (slot, bytes), with the animal list `animals` as (map species, x, y).
pub(crate) fn wld(width: u16, height: u16, layers: &[(u8, Vec<u8>)], animals: &[(u8, u16, u16)]) -> Vec<u8> {
    let nodes = usize::from(width) * usize::from(height);
    let mut out = b"WORLD_V1.0".to_vec();
    // Offsets from `docs/formats/map.md` less the 10-byte signature: width at 20, height at 22,
    // player count at 25, headquarters x and y at 46 and 60 (`u16::MAX` for none).
    let mut header = vec![0; 82];
    header[..4].copy_from_slice(b"Test");
    header[20..22].copy_from_slice(&width.to_le_bytes());
    header[22..24].copy_from_slice(&height.to_le_bytes());
    header[25] = 1;
    header[46..60].fill(u8::MAX);
    header[60..74].fill(u8::MAX);
    out.extend(header);
    // An empty area table, then the tagged layer container: the 10001 map header, then a
    // 10000 header and byte buffer per layer.
    out.extend(vec![0; 250 * 9]);
    out.extend(10_001_u16.to_le_bytes());
    out.extend([0; 4]);
    out.extend(width.to_le_bytes());
    out.extend(height.to_le_bytes());
    for slot in (0..=12).chain([14]) {
        let data = layers
            .iter()
            .find(|(s, _)| *s == slot)
            .map_or_else(|| vec![0; nodes], |(_, d)| d.clone());
        out.extend(10_000_u16.to_le_bytes());
        out.extend([0; 4]);
        out.extend(width.to_le_bytes());
        out.extend(height.to_le_bytes());
        out.extend(1_u16.to_le_bytes());
        out.extend(u32::try_from(data.len()).unwrap().to_le_bytes());
        out.extend(data);
    }
    for &(species, x, y) in animals {
        out.push(species);
        out.extend(x.to_le_bytes());
        out.extend(y.to_le_bytes());
    }
    out.push(u8::MAX);
    out
}

fn node(x: u16, y: u16) -> Node {
    Node { x, y }
}

fn player(index: u8) -> Player {
    Player::new(index).unwrap()
}

#[test]
fn object_words_split_by_class() {
    let object = |word| MapObject::from(ObjectWord(word));
    assert_eq!(object(0), MapObject::Nothing);
    assert_eq!(object(0x0012), MapObject::Plain(0x0012));
    assert_eq!(object(0x4005), MapObject::Building(5));
    assert_eq!(object(0x8003), MapObject::Flag(3));
    assert_eq!(
        object(0xc471),
        MapObject::Tree(Tree {
            kind: 1,
            stage: 3,
            felling: false,
            phase: 1
        })
    );
    assert_eq!(
        object(ObjectWord::from(MapObject::Building(u16::MAX >> 1)).0),
        MapObject::Building((1 << 14) - 1)
    );
    assert!(Player::new(7).is_none());
}

#[test]
fn the_loader_keeps_objects_drops_start_markers_and_lists_animals() {
    let (width, height) = (8, 6);
    let mut low = vec![0; 48];
    let mut high = vec![0; 48];
    (low[9], high[9]) = (0x31, 0xc4);
    (low[10], high[10]) = (0x00, 0x80);
    let bytes = wld(
        width,
        height,
        &[(4, low), (5, high)],
        &[(1, 1, 1), (1, 2, 1), (2, 1, 1), (3, 9, 1), (11, 0, 0)],
    );
    let map = Map::parse_wld(&bytes).unwrap();
    let mut rng = GameRng::at_start();
    let world = World::from_map(&map, &mut rng);
    assert_eq!(ObjectWord::from(world.object(node(1, 1))), ObjectWord(0xc431));
    assert_eq!(
        world.object(node(2, 1)),
        MapObject::Nothing,
        "start marker cleared"
    );
    let species = |n| {
        world
            .figures_at(n)
            .filter_map(|(_, f)| match f.kind {
                FigureKind::Animal { species, .. } => Some(species),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        species(node(1, 1)),
        [Species::Stag, Species::Fox, Species::GreyRabbit],
        "newest first; the first rabbit draws 0"
    );
    assert_eq!(species(node(2, 1)), [Species::LightRabbit], "the second draws 1");
    assert_eq!(world.figures().count(), 4, "species 11 is past the table");
    assert_eq!(rng, {
        let mut two = GameRng::at_start();
        two.step();
        two.step();
        two
    });
    let fox = world.figure(2).unwrap();
    assert_eq!(
        (fox.direction, fox.remaining, fox.counter),
        (Direction::East, 0, 0)
    );
}

#[test]
fn a_headquarters_puts_its_flag_south_east() {
    let mut world = World::empty(64, 64);
    let mut map = flat(64, 64);
    world.place_headquarters(&mut map, node(3, 3), player(0), StartGoods::Normal);
    world.place_headquarters(&mut map, node(5, 4), player(2), StartGoods::Normal);
    assert_eq!(world.object(node(3, 3)), MapObject::Building(0));
    assert_eq!(
        world.object(node(4, 4)),
        MapObject::Flag(0),
        "odd row: south-east is x + 1"
    );
    assert_eq!(
        world.object(node(5, 5)),
        MapObject::Flag(1),
        "even row: south-east is x"
    );
    let hq = world.building(1).unwrap();
    assert_eq!(
        (hq.kind, hq.owner, hq.state),
        (
            BuildingKind::HEADQUARTERS,
            player(2),
            BuildingState::FINISHED | BuildingState::MANNED | BuildingState::PRESENT
        )
    );
    assert_eq!(world.flag(1).unwrap().kind, FlagKind::Normal);
}

#[test]
fn a_building_is_joined_to_its_flag_by_a_road() {
    let road = Some(RoadKind::Normal);
    let east = Roads::default().with_road(Edges::EAST, road);
    let mut map = flat(8, 8);
    let world = World::empty(8, 8);
    let mut data = map.get(node(4, 4));
    data.roads = east;
    map.set(node(4, 4), data);
    connect_to_flag(&mut map, &world, node(3, 3));
    assert_eq!(
        map.get(node(3, 3)).roads,
        Roads::default()
            .with_road(Edges::SOUTH_EAST, road)
            .with_marker(road)
    );
    assert_eq!(
        map.get(node(4, 4)).roads,
        east.with_marker(road),
        "keeps the road already there"
    );
    connect_to_flag(&mut map, &world, node(7, 7));
    assert_eq!(
        map.get(node(0, 0)).roads,
        Roads::default().with_marker(road),
        "wraps at the map edge"
    );
}

fn flat(width: u16, height: u16) -> NodeMap {
    NodeMap::filled(width, height, NodeData::default()).unwrap()
}

fn owned(map: &NodeMap, owner: Player) -> usize {
    (0..map.height())
        .flat_map(|y| (0..map.width()).map(move |x| node(x, y)))
        .filter(|&n| map.get(n).owner == Some(owner))
        .count()
}

fn edges(map: &NodeMap) -> u32 {
    (0..map.height())
        .flat_map(|y| (0..map.width()).map(move |x| node(x, y)))
        .map(|n| map.get(n).borders.bits().count_ones())
        .sum()
}

#[test]
fn a_claim_holds_one_ring_less_than_its_strength_ringed_by_border_edges() {
    for (centre, strength) in [(node(30, 30), 10), (node(31, 31), 10), (node(10, 9), 3)] {
        let mut map = flat(64, 64);
        claim(&mut map, centre, strength, player(1));
        mark_borders(&mut map);
        let r = usize::from(strength - 1);
        assert_eq!(owned(&map, player(1)), 1 + 3 * r * (r + 1), "{centre:?}");
        assert_eq!(edges(&map), 6 * u32::from(strength - 1), "{centre:?}");
        assert_eq!(map.get(centre).strength, strength);
    }
}

#[test]
fn a_claim_stops_at_the_map_edge_and_keeps_off_land_held_as_strongly() {
    // A building's ten protected nodes become its owner's whatever held them (`0x3e760`).
    let mut map = flat(32, 32);
    claim(&mut map, node(1, 1), 4, player(0));
    assert_eq!(map.get(node(31, 31)).owner, None, "no wrap round the corner");
    assert!(owned(&map, player(0)) < 37);
    let before: Vec<_> = (0..32).map(|x| map.get(node(x, 1))).collect();
    claim(&mut map, node(5, 1), 4, player(1));
    let fenced = crate::territory::protected_nodes((32, 32), node(5, 1));
    for (x, data) in (0..32).zip(before) {
        if data.owner == Some(player(0)) && !fenced.contains(&node(x, 1)) {
            assert_eq!(map.get(node(x, 1)).owner, Some(player(0)), "x {x}");
        }
    }
}

#[test]
fn another_players_node_falls_only_to_a_stronger_unprotected_claim() {
    use crate::territory::{Contest, contest, settle};
    let mut map = flat(16, 16);
    let at = node(5, 5);
    settle(&mut map, at, player(0), 3);
    assert_eq!(contest(&map, at, player(1), 3), Contest::Keep, "equal strength");
    assert_eq!(contest(&map, at, player(1), 4), Contest::Take);
    assert_eq!(contest(&map, at, player(0), 4), Contest::Raise);
    assert_eq!(contest(&map, node(6, 6), player(1), 1), Contest::Gain);
    let mut data = map.get(at);
    data.protected = true;
    map.set(at, data);
    assert_eq!(contest(&map, at, player(1), 9), Contest::Keep, "protected");
}

#[test]
fn a_headquarters_on_open_ground_leaves_its_border_and_sites() {
    let mut world = World::empty(64, 64);
    let mut map = flat(64, 64);
    let hq = node(30, 30);
    world.place_headquarters(&mut map, hq, player(0), StartGoods::Normal);
    assert_eq!(owned(&map, player(0)), 271);
    assert_eq!(edges(&map), 54, "nine edges on each side of the hexagon");
    let border_nodes = (0..64)
        .flat_map(|y| (0..64).map(move |x| node(x, y)))
        .filter(|&n| map.get(n).flags.contains(NodeFlags::BORDER))
        .count();
    assert_eq!(border_nodes, 54);
    assert!(map.get(hq).sites.flags.contains(SiteFlags::BLOCKED));
    for direction in [Direction::West, Direction::NorthWest, Direction::NorthEast] {
        let sites = map.get(hq.neighbour(direction, 64, 64)).sites;
        assert!(sites.flags.contains(SiteFlags::FOOTPRINT), "{direction:?}");
    }
    let flag = map.get(hq.neighbour(Direction::SouthEast, 64, 64));
    assert!(
        !flag.sites.flags.contains(SiteFlags::BLOCKED),
        "figures walk over flags"
    );
    assert_eq!(flag.roads.marker(), Some(RoadKind::Normal));
    let open = map.get(hq.towards(Direction::East, 4, 64, 64)).sites;
    assert_eq!(open.size, SiteSize::Large);
}
