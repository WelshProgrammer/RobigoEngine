use crate::commands::{Command, Refusal};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::nodes::{SiteFlags, SiteSize};
use crate::settlers::tests::{game, player};
use crate::transport::{OrderKind, Wanted};
use crate::world::BuildingKind;
use crate::world::{BuildingState, MapObject, Player};

use super::*;

const SITE: Node = Node { x: 20, y: 13 };

fn place(game: &mut Game, kind: BuildingKind) -> Result<(), Refusal> {
    game.execute(player(), Command::PlaceBuilding { node: SITE, kind })
}

fn site(game: &Game) -> crate::world::Building {
    let MapObject::Building(record) = game.world().object(SITE) else {
        panic!("no building");
    };
    *game.world().building(record).unwrap()
}

fn flag_node(game: &Game) -> Node {
    SITE.neighbour(Direction::SouthEast, game.nodes.width(), game.nodes.height())
}

#[test]
fn a_small_site_gets_a_flag_and_orders_its_builder() {
    let mut game = game();
    let orders = game.world().orders().count();
    place(&mut game, BuildingKind::WOODCUTTER).unwrap();
    let flag = game.world().flag_at(flag_node(&game)).unwrap();
    let building = site(&game);
    assert_eq!(
        (building.flag, building.kind, building.owner),
        (flag, BuildingKind::WOODCUTTER, player())
    );
    assert_eq!(building.state, BuildingState::BUILDING);
    assert_eq!(building.requested, 1);
    let (_, order) = game.world().orders().last().unwrap();
    assert_eq!(game.world().orders().count(), orders + 1);
    assert_eq!(
        (order.flag, order.wanted, order.mode, order.urgency, order.kind()),
        (
            flag,
            Wanted::Person(Job::BUILDER),
            BUILDER_MODE,
            50,
            OrderKind::Person
        )
    );
    assert_eq!(game.nodes.get(SITE).sites.size, SiteSize::Nothing);
}

#[test]
fn a_medium_site_on_uneven_ground_orders_a_planer_first() {
    let mut game = game();
    let east = SITE.neighbour(Direction::East, game.nodes.width(), game.nodes.height());
    let mut data = game.nodes.get(east);
    data.altitude += 1;
    game.nodes.set(east, data);
    crate::sites::refresh_sites(&mut game.nodes, &game.world, SITE, 2);
    place(&mut game, BuildingKind::SAWMILL).unwrap();
    let building = site(&game);
    assert_eq!(
        (building.state, building.requested),
        (BuildingState::LEVELLING, 0)
    );
    let (_, order) = game.world().orders().last().unwrap();
    assert_eq!((order.job(), order.mode), (Some(Job::PLANER), PLANER_MODE));
}

#[test]
fn a_medium_site_on_flat_ground_starts_at_once() {
    let mut game = game();
    place(&mut game, BuildingKind::SAWMILL).unwrap();
    assert_eq!(site(&game).state, BuildingState::BUILDING);
}

#[test]
fn a_large_site_covers_three_neighbours() {
    let mut game = game();
    place(&mut game, BuildingKind::FORTRESS).unwrap();
    for direction in [Direction::West, Direction::NorthWest, Direction::NorthEast] {
        let near = SITE.neighbour(direction, game.nodes.width(), game.nodes.height());
        assert!(game.nodes.get(near).sites.flags.contains(SiteFlags::FOOTPRINT));
    }
}

#[test]
fn sites_are_refused_where_the_interface_offers_none() {
    let mut game = game();
    assert_eq!(place(&mut game, BuildingKind::GOLD_MINE), Err(Refusal::NoRoom));
    assert_eq!(
        place(&mut game, BuildingKind::HEADQUARTERS),
        Err(Refusal::NotOffered)
    );
    assert_eq!(place(&mut game, BuildingKind(32)), Err(Refusal::NotOffered));
    let rival = Player::new(1).unwrap();
    assert_eq!(
        game.execute(
            rival,
            Command::PlaceBuilding {
                node: SITE,
                kind: BuildingKind::WOODCUTTER
            }
        ),
        Err(Refusal::NotYours)
    );
    place(&mut game, BuildingKind::WOODCUTTER).unwrap();
    assert_eq!(place(&mut game, BuildingKind::WOODCUTTER), Err(Refusal::NoRoom));
}

#[test]
fn sizes_fit_as_the_slot_8_tests_say() {
    use crate::world::Size;
    assert!(fits(Size::Small, SiteSize::Large));
    assert!(!fits(Size::Small, SiteSize::Flag));
    assert!(fits(Size::Medium, SiteSize::Medium));
    assert!(!fits(Size::Medium, SiteSize::Small));
    assert!(!fits(Size::Large, SiteSize::Medium));
    assert!(fits(Size::Mine, SiteSize::Mine));
    assert!(!fits(Size::Small, SiteSize::Mine));
}

#[test]
fn a_custom_type_table_decides_what_is_levelled() {
    use crate::world::{BuildingType, BuildingTypes};
    let mut game = game();
    let east = SITE.neighbour(Direction::East, game.nodes.width(), game.nodes.height());
    let mut data = game.nodes.get(east);
    data.altitude += 1;
    game.nodes.set(east, data);
    crate::sites::refresh_sites(&mut game.nodes, &game.world, SITE, 2);
    let mut rows: Vec<BuildingType> = BuildingTypes::ORIGINAL.types().to_vec();
    rows[BuildingKind::SAWMILL.index()].levelled = false;
    game.world.set_building_types(BuildingTypes::new(rows).unwrap());
    place(&mut game, BuildingKind::SAWMILL).unwrap();
    assert_eq!(site(&game).state, BuildingState::BUILDING);
}
