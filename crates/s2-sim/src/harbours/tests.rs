use super::Harbour;
use crate::commands::{Command, Refusal};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::nodes::{NodeFlags, SiteSize};
use crate::settlers::tests::{game, player, road_east};
use crate::ware::WareKind;
use crate::world::{BuildingKind, Flag, FlagKind, MapObject, Player};

/// The sea's area: the strip of water from `x` 22 to 27.
pub(crate) const SEA: u8 = 1;

/// The east shore's area.
const EAST_AREA: u8 = 2;

/// Where the two harbours stand.
pub(crate) const WEST: Node = Node { x: 20, y: 16 };
pub(crate) const EAST: Node = Node { x: 29, y: 16 };

/// A node in the strait.
pub(crate) const LAUNCH_NODE: Node = Node { x: 24, y: 24 };

const WATER: u8 = 5;

/// A map split by a strait of water in area [`SEA`], with the east shore the player's too.
pub(crate) fn two_shores() -> Game {
    let mut game = game();
    for y in 0..40 {
        for x in 22..48 {
            let at = Node { x, y };
            let mut data = game.nodes.get(at);
            if x <= 27 {
                data.terrain = [WATER; 2];
                data.area = SEA;
            } else {
                data.area = EAST_AREA;
                data.owner = Some(player());
            }
            game.nodes.set(at, data);
        }
    }
    game.nodes.set_sea(SEA, true);
    crate::sites::refresh_sites(&mut game.nodes, &game.world, Node { x: 30, y: 20 }, 30);
    game
}

/// A finished harbour of the player on `node`, and its building record.
pub(crate) fn harbour(game: &mut Game, node: Node) -> u16 {
    let mut data = game.nodes.get(node);
    data.flags |= NodeFlags::HARBOUR;
    data.sites.size = SiteSize::Large;
    game.nodes.set(node, data);
    game.execute(
        player(),
        Command::PlaceBuilding {
            node,
            kind: BuildingKind::HARBOUR,
        },
    )
    .unwrap();
    let MapObject::Building(record) = game.world().object(node) else {
        panic!("no harbour");
    };
    game.finish(record);
    record
}

/// The harbour record of building `building`.
pub(crate) fn record(game: &Game, building: u16) -> Harbour {
    *game
        .world()
        .harbour(game.world().harbour_of(building).unwrap())
        .unwrap()
}

fn step(node: Node, direction: Direction) -> Node {
    node.neighbour(direction, 48, 40)
}

/// The flag of building `building`.
fn flag_of(game: &Game, building: u16) -> u16 {
    game.world().building(building).unwrap().flag
}

/// A flag two edges east of the east harbour's flag, joined to it by a road.
pub(crate) fn east_flag(game: &mut Game) -> u16 {
    let start = step(EAST, Direction::SouthEast);
    road_east(game, start, 2);
    game.world()
        .flag_at(Node {
            x: start.x + 2,
            ..start
        })
        .unwrap()
}

#[test]
fn a_finished_harbour_notes_the_sea_at_each_landing_node() {
    let mut game = two_shores();
    let west = harbour(&mut game, WEST);
    let east = harbour(&mut game, EAST);
    assert_eq!(
        record(&game, west).seas,
        [None, None, Some(SEA), None, None, None]
    );
    assert_eq!(
        record(&game, east).seas,
        [Some(SEA), None, None, None, Some(SEA), Some(SEA)],
        "north-west, south-west and west"
    );
    assert!(record(&game, west).shares_sea(&record(&game, east)));
}

#[test]
fn harbours_go_only_on_harbour_sites() {
    let mut game = two_shores();
    let mut data = game.nodes.get(WEST);
    data.sites.size = SiteSize::Large;
    game.nodes.set(WEST, data);
    let place = Command::PlaceBuilding {
        node: WEST,
        kind: BuildingKind::HARBOUR,
    };
    assert_eq!(game.execute(player(), place), Err(Refusal::NotOffered));
}

#[test]
fn ships_land_on_the_first_landing_node_on_their_sea_with_no_flag() {
    let mut game = two_shores();
    let east = harbour(&mut game, EAST);
    let harbour = game.world().harbour_of(east).unwrap();
    let landings = [Direction::NorthWest, Direction::SouthWest, Direction::West].map(|d| step(EAST, d));
    assert_eq!(game.world().landing(harbour, SEA), Some(landings[0]));
    assert_eq!(game.world().landing(harbour, SEA + 5), None);
    let mut flags = 0;
    for &node in &landings[..2] {
        let flag = game
            .world
            .add_flag(Flag::new(FlagKind::Normal, player(), node))
            .unwrap();
        game.world.set_object(node, MapObject::Flag(flag));
        flags += 1;
        assert_eq!(game.world().landing(harbour, SEA), Some(landings[flags]));
    }
    let flag = game
        .world
        .add_flag(Flag::new(FlagKind::Normal, player(), landings[2]))
        .unwrap();
    game.world.set_object(landings[2], MapObject::Flag(flag));
    assert_eq!(
        game.world().landing(harbour, SEA),
        Some(landings[2]),
        "every one flagged: the last"
    );
}

#[test]
fn an_order_failing_at_a_harbour_moves_round_the_harbours_on_its_sea() {
    let mut game = two_shores();
    let west = harbour(&mut game, WEST);
    let east = harbour(&mut game, EAST);
    let (west_harbour, east_harbour) = (
        game.world().harbour_of(west).unwrap(),
        game.world().harbour_of(east).unwrap(),
    );
    assert_eq!(
        game.world().next_harbour_flag(west_harbour),
        Some(flag_of(&game, east))
    );
    assert_eq!(
        game.world().next_harbour_flag(east_harbour),
        Some(flag_of(&game, west))
    );
    game.world.harbours.get_mut(east_harbour).unwrap().owner = Player::new(1).unwrap();
    assert_eq!(
        game.world().next_harbour_flag(west_harbour),
        Some(flag_of(&game, west)),
        "another owner's harbour is passed over"
    );
    game.world.harbours.get_mut(east_harbour).unwrap().owner = player();
    let order = game
        .world
        .order_ware(flag_of(&game, east), WareKind::BEER, 0)
        .unwrap();
    game.serve_ware(order);
    let order = game.world().order(order).unwrap();
    assert_eq!((order.flag, order.harbour), (flag_of(&game, west), None));
}

#[test]
fn an_order_no_road_serves_is_linked_to_the_first_harbour_its_search_passed() {
    let mut game = two_shores();
    harbour(&mut game, WEST);
    let east = harbour(&mut game, EAST);
    let flag = east_flag(&mut game);
    let harbour = game.world().harbour_of(east).unwrap();
    let wood = game.world.order_ware(flag, WareKind::WOOD, 0).unwrap();
    game.serve_ware(wood);
    game.serve_ware(wood);
    assert_eq!(game.world().order(wood).unwrap().harbour, Some(harbour));
    assert_eq!(
        record(&game, east).wares_wanted[WareKind::WOOD.index()],
        1,
        "linked once"
    );
    let soldier = game.world.order_person(flag, Job::PRIVATE, 0, 0).unwrap();
    game.serve_person(soldier);
    let wanted = record(&game, east).people_wanted;
    assert_eq!(
        wanted[Job::PRIVATE.index()..=Job::GENERAL.index()],
        [1; 5],
        "every rank"
    );
    game.world.release_link(soldier);
    game.world.release_link(wood);
    let after = record(&game, east);
    assert_eq!(after.wares_wanted, [0; crate::world::WARE_TYPES]);
    assert_eq!(after.people_wanted, [0; Job::PEOPLE]);
    assert_eq!(game.world().order(wood).unwrap().harbour, None);
}

#[test]
fn a_geologist_order_with_a_harbour_to_turn_to_is_kept() {
    let mut game = two_shores();
    harbour(&mut game, WEST);
    harbour(&mut game, EAST);
    let flag = east_flag(&mut game);
    let geologist = game.world.order_person(flag, Job::GEOLOGIST, 0, 0).unwrap();
    game.serve_person(geologist);
    assert!(
        game.world()
            .order(geologist)
            .is_some_and(|order| order.harbour.is_some())
    );
}

#[test]
fn the_supply_pass_orders_what_no_other_harbour_on_the_sea_holds() {
    let mut game = two_shores();
    let west = harbour(&mut game, WEST);
    let east = harbour(&mut game, EAST);
    let flag = east_flag(&mut game);
    for ware in [WareKind::WOOD, WareKind::BEER, WareKind::BEER] {
        let order = game.world.order_ware(flag, ware, 0).unwrap();
        game.serve_ware(order);
    }
    let (store, _) = game.world().storehouse_of(west).unwrap();
    game.world.storehouse_mut(store).unwrap().wares[WareKind::WOOD.index()] = 3;
    let east_flag = flag_of(&game, east);
    let ordered = |game: &Game| {
        game.world()
            .orders()
            .filter(|(_, order)| order.flag == east_flag)
            .filter_map(|(_, order)| order.ware())
            .collect::<Vec<_>>()
    };
    game.harbour_pass();
    assert_eq!(ordered(&game), [WareKind::BEER], "the west harbour holds wood");
    game.harbour_pass();
    assert_eq!(
        ordered(&game),
        [WareKind::BEER, WareKind::BEER],
        "one a pass, two wanted"
    );
    game.harbour_pass();
    assert_eq!(ordered(&game).len(), 2);
    assert_eq!(record(&game, east).waiting, 3);
    assert_eq!(record(&game, east).wares_ordered[WareKind::BEER.index()], 2);
}
