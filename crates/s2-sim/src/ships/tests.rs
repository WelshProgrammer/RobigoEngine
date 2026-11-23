use super::{MAX_SHIPS, Ship, ShipMode};
use crate::figures::FigureKind;
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::settlers::tests::{game, player};
use crate::world::Player;

const SITE: Node = Node { x: 30, y: 20 };

fn area(game: &mut Game, node: Node, area: u8) {
    let mut data = game.nodes.get(node);
    data.area = area;
    game.nodes.set(node, data);
}

#[test]
fn a_ship_floats_on_the_first_sea_round_its_site_from_the_north_west() {
    let mut game = game();
    area(&mut game, SITE.neighbour(Direction::NorthEast, 48, 40), 3);
    area(&mut game, SITE.neighbour(Direction::East, 48, 40), 4);
    area(&mut game, SITE.neighbour(Direction::West, 48, 40), 5);
    game.nodes.set_sea(4, true);
    game.nodes.set_sea(5, true);
    let figure = game.launch_ship(SITE, player()).unwrap();
    let FigureKind::Ship { owner, record } = game.world().figure(figure).unwrap().kind else {
        panic!("not a ship");
    };
    assert_eq!(owner, player());
    let ship = *game.world().ship(record).unwrap();
    assert_eq!(game.world().figure(figure).unwrap().node(), SITE);
    assert_eq!(
        ship,
        Ship {
            figure,
            number: 1,
            ..Ship::new(player(), 4)
        }
    );
    assert_eq!(ship.mode, ShipMode::Transport { harbour: None });
}

#[test]
fn with_no_sea_round_it_a_ship_takes_its_west_neighbours_area_and_each_player_counts_their_own() {
    let mut game = game();
    area(&mut game, SITE.neighbour(Direction::West, 48, 40), 7);
    game.launch_ship(SITE, player()).unwrap();
    let other = Player::new(2).unwrap();
    game.launch_ship(SITE, other).unwrap();
    game.launch_ship(SITE, player()).unwrap();
    let ships: Vec<_> = game
        .world()
        .ships()
        .map(|(_, ship)| (ship.owner, ship.number, ship.sea))
        .collect();
    assert_eq!(ships, [(player(), 1, 7), (other, 1, 7), (player(), 2, 7)]);
}

#[test]
fn a_full_ship_pool_loses_the_ship() {
    let mut game = game();
    for _ in 0..MAX_SHIPS {
        game.launch_ship(SITE, player()).unwrap();
    }
    let figures = game.world().figures().count();
    assert_eq!(game.launch_ship(SITE, player()), None);
    assert_eq!(game.world().figures().count(), figures);
}

mod voyages {
    use crate::figures::FigureKind;
    use crate::game::Game;
    use crate::grid::Node;
    use crate::harbours::tests::{EAST, SEA, WEST, east_flag, harbour, record, two_shores};
    use crate::settlers::tests::player;
    use crate::ships::{Ship, ShipMode};
    use crate::ware::WareKind;

    /// Where the ship floats from, in the strait.
    const LAUNCH: Node = Node { x: 24, y: 24 };

    fn ship(game: &Game) -> (u16, Ship) {
        game.world()
            .ships()
            .next()
            .map(|(record, ship)| (record, *ship))
            .unwrap()
    }

    fn ship_node(game: &Game) -> Node {
        game.world().figure(ship(game).1.figure).unwrap().node()
    }

    /// Steps until `done`, checking the links on the way.
    fn run(game: &mut Game, limit: u32, done: impl Fn(&Game) -> bool) {
        for _ in 0..limit {
            game.step();
            let broken = game.world().broken_links();
            assert!(broken.is_empty(), "{broken:?}");
            if done(game) {
                return;
            }
        }
        panic!("not done in {limit} steps");
    }

    #[test]
    fn a_new_ship_finds_the_first_harbour_on_its_sea_and_anchors_off_it_with_no_work() {
        let mut game = two_shores();
        let west = harbour(&mut game, WEST);
        harbour(&mut game, EAST);
        game.launch_ship(LAUNCH, player()).unwrap();
        let home = game.world().harbour_of(west).unwrap();
        run(&mut game, 120, |game| ship(game).1.sailing);
        assert_eq!(game.clock().step(), 120, "the first check");
        assert_eq!(ship(&game).1.mode, ShipMode::Transport { harbour: Some(home) });
        let landing = game.world().landing(home, SEA).unwrap();
        run(&mut game, 2000, |game| {
            matches!(ship(game).1.mode, ShipMode::Anchoring { .. })
        });
        assert_eq!(
            ship_node(&game),
            landing,
            "it arrives, finds no work and sets off at once"
        );
        assert!(matches!(ship(&game).1.mode, ShipMode::Anchoring { harbour: Some(h) } if h == home));
        run(&mut game, 2000, |game| !ship(game).1.sailing);
        let anchorage = ship_node(&game);
        assert_eq!(game.nodes().get(anchorage).area, SEA);
        assert_eq!(
            ship(&game).1.mode,
            ShipMode::Transport { harbour: Some(home) },
            "back to transport on arrival"
        );
        for _ in 0..300 {
            game.step();
            assert_eq!(ship_node(&game), anchorage, "no work, so it stays");
        }
    }

    #[test]
    fn a_sailing_ship_reveals_three_rings_round_the_node_ahead() {
        let mut game = two_shores();
        harbour(&mut game, WEST);
        game.launch_ship(LAUNCH, player()).unwrap();
        for y in 0..game.nodes().height() {
            for x in 0..game.nodes().width() {
                let at = Node { x, y };
                let mut data = game.nodes.get(at);
                data.seen = crate::sight::Seen::empty();
                game.nodes.set(at, data);
            }
        }
        run(&mut game, 120, |game| ship(game).1.sailing);
        run(&mut game, 64, |game| ship_node(game) != LAUNCH);
        let at = ship_node(&game);
        assert!(game.nodes().fully_seen(at, player()));
        let width = game.nodes().width();
        let far = Node {
            x: (at.x + 5) % width,
            y: at.y,
        };
        assert!(!game.nodes().fully_seen(far, player()), "3 rings and no further");
    }

    #[test]
    fn an_edge_takes_sixteen_steps_whatever_the_water() {
        let mut game = two_shores();
        harbour(&mut game, WEST);
        game.launch_ship(LAUNCH, player()).unwrap();
        run(&mut game, 120, |game| ship(game).1.sailing);
        let mut moves = Vec::new();
        let mut last = ship_node(&game);
        for _ in 0..64 {
            game.step();
            if ship_node(&game) != last {
                last = ship_node(&game);
                moves.push(game.clock().step());
            }
        }
        let gaps: Vec<u32> = moves.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert!(gaps.iter().all(|&gap| gap == 16), "{gaps:?}");
    }

    #[test]
    fn a_ship_takes_wood_wanted_across_the_water_and_the_order_is_served_from_the_far_harbour() {
        let mut game = two_shores();
        let west = harbour(&mut game, WEST);
        let east = harbour(&mut game, EAST);
        let flag = east_flag(&mut game);
        let (store, _) = game.world().storehouse_of(west).unwrap();
        game.world.storehouse_mut(store).unwrap().wares[WareKind::WOOD.index()] = 5;
        let order = game.world.order_ware(flag, WareKind::WOOD, 0).unwrap();
        game.launch_ship(LAUNCH, player()).unwrap();
        let wood = |game: &Game, building| {
            game.world().storehouse_of(building).unwrap().1.wares[WareKind::WOOD.index()]
        };
        run(&mut game, 5000, |game| {
            ship(game).1.wares[WareKind::WOOD.index()] > 0
        });
        assert_eq!(
            ship(&game).1.wares[WareKind::WOOD.index()],
            5,
            "its share of all the demand"
        );
        assert_eq!(wood(&game, west), 0);
        assert_eq!(record(&game, west).waiting, 0);
        assert_eq!(record(&game, east).waiting, 0);
        let away = game.world().harbour_of(east).unwrap();
        assert_eq!(ship(&game).1.mode, ShipMode::Transport { harbour: Some(away) });
        run(&mut game, 5000, |game| {
            game.world().order(order).is_none_or(|order| order.delivering)
        });
        assert_eq!(ship(&game).1.wares, [0; crate::world::WARE_TYPES]);
        assert_eq!(wood(&game, east), 4, "five landed, one sent out");
        assert_eq!(
            record(&game, east).wares_wanted[WareKind::WOOD.index()],
            0,
            "released"
        );
    }

    #[test]
    fn a_ship_with_nowhere_to_anchor_holds_no_path_records() {
        let mut game = two_shores();
        let west = harbour(&mut game, WEST);
        harbour(&mut game, EAST);
        let home = game.world().harbour_of(west).unwrap();
        let landing = game.world().landing(home, SEA).unwrap();
        for y in 0..40 {
            for x in 22..28 {
                let at = Node { x, y };
                let mut data = game.nodes.get(at);
                data.area = SEA + 10;
                game.nodes.set(at, data);
            }
        }
        let figure = game.launch_ship(landing, player()).unwrap();
        let crate::figures::FigureKind::Ship { record, .. } = game.world().figure(figure).unwrap().kind
        else {
            panic!();
        };
        let ship = game.world.ships.get_mut(record).unwrap();
        ship.mode = ShipMode::Transport { harbour: Some(home) };
        ship.sea = SEA;
        let (store, _) = game.world().storehouse_of(west).unwrap();
        game.world.storehouse_mut(store).unwrap().wares[WareKind::WOOD.index()] = 5;
        let far = game.world().object(EAST).building().unwrap();
        let away = game.world().harbour_of(far).unwrap();
        game.world.harbours.get_mut(away).unwrap().wares_wanted[WareKind::WOOD.index()] = 1;
        let paths = game.world().paths.len();
        game.anchor(record, landing);
        assert_eq!(game.world().paths.len(), paths, "no anchorage, no record kept");
        for _ in 0..300 {
            game.step();
        }
        assert_eq!(ship_node(&game), landing, "no route over the moved sea");
        assert_eq!(
            game.world().paths.len(),
            paths,
            "planning from the landing node keeps none"
        );
    }

    #[test]
    fn a_ship_whose_harbour_goes_stops_after_its_edge_and_keeps_its_cargo() {
        let mut game = two_shores();
        let west = harbour(&mut game, WEST);
        harbour(&mut game, EAST);
        game.launch_ship(LAUNCH, player()).unwrap();
        run(&mut game, 200, |game| ship(game).1.sailing);
        let (record, _) = ship(&game);
        game.world.ships.get_mut(record).unwrap().wares[WareKind::BEER.index()] = 3;
        game.demolish(west);
        let (_, after) = ship(&game);
        assert_eq!(after.mode, ShipMode::Transport { harbour: None });
        assert!(!after.sailing);
        assert_eq!(after.wares[WareKind::BEER.index()], 3);
        assert!(matches!(
            game.world().figure(after.figure).unwrap().kind,
            FigureKind::Ship { .. }
        ));
        let broken = game.world().broken_links();
        assert!(broken.is_empty(), "{broken:?}");
        run(&mut game, 400, |game| ship(game).1.sailing);
        let east = game
            .world()
            .harbour_of(game.world().object(EAST).building().unwrap())
            .unwrap();
        assert_eq!(
            ship(&game).1.mode,
            ShipMode::Transport { harbour: Some(east) },
            "the next on the sea"
        );
    }
}

mod loading {
    use crate::harbours::tests::{EAST, LAUNCH_NODE, WEST, harbour, two_shores};
    use crate::job::Job;
    use crate::settlers::tests::player;
    use crate::ware::WareKind;

    #[test]
    fn a_load_takes_shares_of_the_demand_keeps_the_home_demand_and_stops_at_forty() {
        let mut game = two_shores();
        let west = harbour(&mut game, WEST);
        let east = harbour(&mut game, EAST);
        let (from, to) = (
            game.world().harbour_of(west).unwrap(),
            game.world().harbour_of(east).unwrap(),
        );
        let mut other = *game.world().harbour(to).unwrap();
        other.wares_wanted[WareKind::WOOD.index()] = 2;
        game.world.harbours.insert(other, 10);
        let away = game.world.harbours.get_mut(to).unwrap();
        away.wares_wanted[WareKind::WOOD.index()] = 2;
        away.wares_wanted[WareKind::BOARDS.index()] = 2;
        away.wares_wanted[WareKind::BEER.index()] = 2;
        away.people_wanted[Job::WOODCUTTER.index()] = 3;
        game.world.harbours.get_mut(from).unwrap().wares_wanted[WareKind::BOARDS.index()] = 10;
        let (far, _) = game.world().storehouse_of(east).unwrap();
        game.world.storehouse_mut(far).unwrap().ware_settings[WareKind::BOARDS.index()] =
            crate::world::StockSetting::SEND;
        let (store, _) = game.world().storehouse_of(west).unwrap();
        let stock = game.world.storehouse_mut(store).unwrap();
        stock.wares = [0; crate::world::WARE_TYPES];
        stock.jobs = [0; Job::PEOPLE];
        stock.wares[WareKind::WOOD.index()] = 30;
        stock.wares[WareKind::BOARDS.index()] = 30;
        stock.wares[WareKind::BEER.index()] = 10;
        stock.wares[WareKind::AXE.index()] = 2;
        stock.jobs[Job::HELPER.index()] = 2;
        let figure = game.launch_ship(LAUNCH_NODE, player()).unwrap();
        let crate::figures::FigureKind::Ship { record, .. } = game.world().figure(figure).unwrap().kind
        else {
            panic!();
        };
        game.load(record, from, to);
        let ship = game.world().ship(record).unwrap();
        assert_eq!(ship.wares[WareKind::WOOD.index()], 15, "2 of the 4 wanted, of 30");
        assert_eq!(
            ship.wares[WareKind::BOARDS.index()],
            20,
            "all of them for a harbour sending boards out, less 10 kept back for the west"
        );
        assert_eq!(ship.wares[WareKind::BEER.index()], 5, "cut at 40");
        assert_eq!(ship.people, [0; Job::PEOPLE], "people are never split");
        let stock = game.world().storehouse_of(west).unwrap().1;
        assert_eq!(
            (
                stock.jobs[Job::WOODCUTTER.index()],
                stock.jobs[Job::HELPER.index()]
            ),
            (1, 1),
            "looking for a woodcutter made one"
        );
        assert_eq!(stock.wares[WareKind::AXE.index()], 1);
    }
}
