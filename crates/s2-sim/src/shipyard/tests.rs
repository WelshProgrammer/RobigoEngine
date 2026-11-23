use crate::commands::{Command, Refusal};
use crate::figures::{FigureKind, Gatherer};
use crate::gather::tests::{lake, manned};
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::settlers::tests::{game, player};
use crate::world::{BuildingKind, MapObject, Player, ShipSite};

const YARD: Node = Node { x: 12, y: 14 };

/// The lake's area, marked as a sea.
const SEA: u8 = 1;

/// The lake from [`lake`] in area [`SEA`], and the shore node east of its middle row.
fn shore(game: &mut crate::game::Game) -> Node {
    let water = lake(game);
    for y in 10..=16 {
        for x in 4..=9 {
            let at = Node { x, y };
            if game
                .nodes
                .property(at)
                .contains(crate::ground::Property::ALL_WATER)
            {
                let mut data = game.nodes.get(at);
                data.area = SEA;
                game.nodes.set(at, data);
            }
        }
    }
    game.nodes.set_sea(SEA, true);
    water.neighbour(Direction::East, 48, 40)
}

fn shipyard(game: &mut crate::game::Game) -> u16 {
    let record = manned(game, BuildingKind::SHIPYARD, YARD);
    game.execute(
        player(),
        Command::ShipyardMode {
            node: YARD,
            ships: true,
        },
    )
    .unwrap();
    record
}

#[test]
fn a_ship_goes_on_land_beside_open_water_with_no_flag() {
    let mut game = game();
    let shore = shore(&mut game);
    let water = shore.neighbour(Direction::West, 48, 40);
    assert!(game.ship_site_fits(shore));
    assert!(!game.ship_site_fits(water), "all water");
    assert!(
        !game.ship_site_fits(
            shore
                .neighbour(Direction::East, 48, 40)
                .neighbour(Direction::East, 48, 40)
        ),
        "no water next to it"
    );
    game.execute(
        player(),
        Command::PlaceFlag {
            node: shore,
            water: false,
        },
    )
    .unwrap();
    assert!(!game.ship_site_fits(shore), "a flag");
}

#[test]
fn the_mode_button_is_offered_only_on_a_finished_shipyard_of_the_player() {
    let mut game = game();
    let fishery = Node { x: 22, y: 14 };
    manned(&mut game, BuildingKind::FISHERY, fishery);
    let set = |node| Command::ShipyardMode { node, ships: true };
    assert_eq!(game.execute(player(), set(fishery)), Err(Refusal::NotOffered));
    assert_eq!(game.execute(player(), set(YARD)), Err(Refusal::NoBuilding));
    let record = manned(&mut game, BuildingKind::SHIPYARD, YARD);
    assert_eq!(
        game.execute(Player::new(1).unwrap(), set(YARD)),
        Err(Refusal::NotYours)
    );
    game.execute(player(), set(YARD)).unwrap();
    assert!(game.world().building(record).unwrap().ships);
}

#[test]
fn with_no_shore_in_reach_the_shipyard_goes_back_to_boats_and_uses_a_board() {
    let mut game = game();
    let record = shipyard(&mut game);
    game.world.building_mut(record).unwrap().stock[0] = 2;
    assert!(game.start_shipyard(record));
    let yard = game.world().building(record).unwrap();
    assert!(!yard.ships);
    assert_eq!((yard.ship_site, yard.stock[0]), (None, 1));
}

#[test]
fn thirteen_trips_with_a_board_each_float_a_ship() {
    let mut game = game();
    let shore = shore(&mut game);
    let record = shipyard(&mut game);
    let mut parts = Vec::new();
    let mut used = 0;
    while game.world().ships().count() == 0 {
        let yard = game.world.building_mut(record).unwrap();
        if yard.stock[0] == 0 {
            yard.stock[0] = 1;
            used += 1;
        }
        game.step();
        let site = game.world().building(record).unwrap().ship_site;
        if let Some(site) = site
            && let MapObject::ShipSite(part) = game.world().object(site)
            && parts.last() != Some(&part)
        {
            parts.push(part);
        }
        let broken = game.world().broken_links();
        assert!(broken.is_empty(), "{broken:?}");
        assert!(game.clock().step() < 15_000, "no ship");
    }
    assert_eq!(parts.len(), 12, "twelve parts, the thirteenth trip floats it");
    assert_eq!(parts[0], ShipSite::NEW);
    assert_eq!(
        parts[11],
        ShipSite {
            set: 0,
            part: 3,
            stage: 2
        }
    );
    let left = game.world().building(record).unwrap().stock[0];
    assert_eq!(
        used - u32::from(left),
        14,
        "a board for the cycle that found the site, one per trip"
    );
    let (_, ship) = game.world().ships().next().unwrap();
    let figure = game.world().figure(ship.figure).unwrap();
    let site = figure.node();
    assert!(game.ship_site_fits(site) || site == shore);
    assert_eq!(game.world().object(site), MapObject::Nothing);
    assert!(matches!(figure.kind, FigureKind::Ship { owner, .. } if owner == player()));
    assert_eq!((ship.sea, ship.number, ship.sailing), (SEA, 1, false));
    let yard = game.world().building(record).unwrap();
    assert!(yard.ships, "still in ship mode");
    assert_eq!(yard.ship_site, None);
    let out = game.world().figures().any(|(_, figure)| {
        matches!(
            figure.kind,
            FigureKind::Gatherer(Gatherer {
                job: Job::SHIPWRIGHT | Job::SHIPWRIGHT_OUT,
                ..
            })
        )
    });
    assert!(out, "the shipwright still walking home");
}
