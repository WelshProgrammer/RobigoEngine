use crate::ware::WareKind;
use std::collections::BTreeMap;

use crate::commands::Command;
use crate::figures::{CarrierState, FigureKind, Transport};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::marks::RoadKind;
use crate::nodes::{NodeData, NodeMap};
use crate::sites::site;
use crate::transport::{MAX_ORDERS, Order, OrderKind, WARE_MODE, Wanted, Ware, WareState};
use crate::world::{BuildingState, MapObject, Player, StartGoods, World};

const WIDTH: u16 = 48;
const HEIGHT: u16 = 40;
pub(crate) const HQ: Node = Node { x: 16, y: 16 };
pub(crate) const HQ_FLAG: Node = Node { x: 16, y: 17 };

/// The step whose order pass first runs: the cycle counter starts at 1.
pub(crate) const FIRST_PASS: u32 = 15;

/// The first step after [`FIRST_PASS`] that is a multiple of 7, when a new figure starts.
const RELEASE: u32 = 21;

pub(crate) fn player() -> Player {
    Player::new(0).unwrap()
}

pub(crate) fn game() -> Game {
    let data = NodeData {
        altitude: 10,
        terrain: [8; 2],
        ..NodeData::default()
    };
    let mut nodes = NodeMap::filled(WIDTH, HEIGHT, data).unwrap();
    let mut world = World::empty(WIDTH, HEIGHT);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let at = Node { x, y };
            let mut data = nodes.get(at);
            data.sites = site(&nodes, &world, at);
            nodes.set(at, data);
        }
    }
    world.place_headquarters(&mut nodes, HQ, player(), StartGoods::Normal);
    Game::from_parts(nodes, world)
}

/// Builds a road from the flag on `from` going east `edges` times, ending in a new flag.
pub(crate) fn road_east(game: &mut Game, from: Node, edges: u16) -> u16 {
    game.execute(
        player(),
        Command::StartRoad {
            node: from,
            water: false,
        },
    )
    .unwrap();
    for _ in 0..edges {
        game.execute(
            player(),
            Command::ExtendRoad {
                direction: Direction::East,
            },
        )
        .unwrap();
    }
    let end = Node {
        x: from.x + edges,
        y: from.y,
    };
    game.execute(
        player(),
        Command::PlaceFlag {
            node: end,
            water: false,
        },
    )
    .unwrap();
    let flag = game.world().flag_at(end).unwrap();
    game.world()
        .roads()
        .find(|(_, road)| road.end == Some(flag))
        .map(|(record, _)| record)
        .unwrap()
}

/// A ware of `kind` waiting with a delivery order for flag `to`, not yet put down.
pub(crate) fn ware_for(world: &mut World, kind: WareKind, to: u16) -> u16 {
    let ware = world.add_ware(Ware::new(kind)).unwrap();
    let owner = world.flag(to).unwrap().owner;
    let order = world
        .orders
        .insert(
            Order {
                flag: to,
                urgency: 50,
                wanted: Wanted::Ware(kind),
                delivering: true,
                mode: WARE_MODE,
                on_way: Some(ware),
                harbour: None,
                tag: 0,
                owner,
            },
            MAX_ORDERS,
        )
        .unwrap();
    let record = world.wares.get_mut(ware).unwrap();
    record.order = Some(order);
    record.state = WareState::Waiting;
    ware
}

/// Sets or clears the held bit on `node`, as a fight at a flag does.
pub(crate) fn hold(game: &mut Game, node: Node, on: bool) {
    let mut data = game.nodes.get(node);
    data.flags.set(crate::nodes::NodeFlags::HELD, on);
    game.nodes.set(node, data);
}

pub(crate) fn run_to(game: &mut Game, step: u32) {
    while game.clock().step() < step {
        game.step();
    }
}

pub(crate) fn headquarters(game: &Game) -> (BuildingState, u16) {
    let MapObject::Building(building) = game.world().object(HQ) else {
        panic!("no headquarters");
    };
    let state = game.world().building(building).unwrap().state;
    let helpers = game.world().storehouse_of(building).unwrap().1.jobs[0];
    (state, helpers)
}

#[test]
fn the_headquarters_sends_a_helper_who_becomes_the_carrier_at_its_flag() {
    let mut game = game();
    let road = road_east(&mut game, HQ_FLAG, 4);
    let order = game.world().road(road).unwrap().order.unwrap();
    assert_eq!(headquarters(&game).1, 52);

    run_to(&mut game, FIRST_PASS - 1);
    assert_eq!(game.world().figures().count(), 0);
    game.step();
    let order_record = *game.world().order(order).unwrap();
    assert_eq!(
        (order_record.kind(), order_record.urgency),
        (OrderKind::Delivering, 40)
    );
    let (figure, leaving) = game.world().figures().next().map(|(r, f)| (r, *f)).unwrap();
    assert_eq!(order_record.on_way, Some(figure));
    assert!(matches!(
        leaving.kind,
        FigureKind::Leaving { job: Job::HELPER, .. }
    ));
    assert_eq!(
        (leaving.node(), leaving.direction, leaving.remaining),
        (HQ_FLAG, Direction::SouthEast, 242)
    );
    let (state, helpers) = headquarters(&game);
    assert_eq!(helpers, 51);
    assert!(state.contains(BuildingState::DOOR) && !state.contains(BuildingState::PRESENT));

    assert!(
        game.world().figure(figure).unwrap().held,
        "new records start held"
    );
    run_to(&mut game, RELEASE - 1);
    assert_eq!(
        game.world().figure(figure).unwrap().remaining,
        242,
        "held until step 21"
    );
    game.step();
    let released = game.world().figure(figure).unwrap();
    assert_eq!((released.held, released.remaining), (false, 228));
    run_to(&mut game, RELEASE + 17);
    assert!(
        matches!(
            game.world().figure(figure).unwrap().kind,
            FigureKind::Leaving { .. }
        ),
        "still walking out"
    );
    let (state, _) = headquarters(&game);
    assert!(!state.contains(BuildingState::DOOR), "the door closed behind it");
    game.step();
    let FigureKind::Carrier(carrier) = game.world().figure(figure).unwrap().kind else {
        panic!("not a carrier on step {}", game.clock().step());
    };
    assert_eq!(
        (carrier.on.road, carrier.on.position, carrier.state),
        (road, 0, CarrierState::Idle)
    );
    let road_record = game.world().road(road).unwrap();
    assert_eq!(
        (road_record.order, road_record.served, road_record.carriers),
        (None, true, 1)
    );
    assert!(game.world().order(order).is_none());
    assert!(headquarters(&game).0.contains(BuildingState::PRESENT));

    run_to(&mut game, 200);
    let figure = game.world().figure(figure).unwrap();
    let FigureKind::Carrier(carrier) = figure.kind else {
        panic!("no longer a carrier");
    };
    assert_eq!(carrier.on.position, 2, "waits in the middle");
    assert!(matches!(
        figure.direction,
        Direction::SouthEast | Direction::SouthWest
    ));
    assert_eq!(carrier.state, CarrierState::Waiting);
}

#[test]
fn a_figure_walking_onto_a_held_node_stops_and_holds_the_node_behind_it() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    run_to(&mut game, RELEASE);
    let figure = game.world().figures().next().map(|(record, _)| record).unwrap();
    assert_eq!(game.world().figure(figure).unwrap().remaining, 228);
    hold(&mut game, HQ_FLAG, true);
    run_to(&mut game, 23);
    assert_eq!(
        game.world().figure(figure).unwrap().remaining,
        200,
        "it walks on between the checks"
    );
    game.step();
    let stopped = game.world().figure(figure).copied().unwrap();
    assert_eq!((stopped.held, stopped.holds_behind), (true, true));
    assert_eq!(stopped.remaining, 200, "it stops part way along the edge");
    assert!(
        game.nodes.get(HQ).flags.contains(crate::nodes::NodeFlags::HELD),
        "the node it came from is held too, so the next figure stops behind it"
    );
    run_to(&mut game, 29);
    assert_eq!(
        game.world().figure(figure).unwrap().remaining,
        200,
        "it waits while its own node is held"
    );
    hold(&mut game, HQ_FLAG, false);
    run_to(&mut game, 35);
    let walking = game.world().figure(figure).copied().unwrap();
    assert_eq!((walking.held, walking.holds_behind), (false, false));
    assert_eq!(walking.remaining, 186, "released and walking again");
    assert!(
        !game.nodes.get(HQ).flags.contains(crate::nodes::NodeFlags::HELD),
        "the bit it took is given back"
    );
}

/// A two-edge road east of the headquarters' flag turned into a water road, with its order for
/// a water-road carrier.
fn water_road(game: &mut Game) -> (u16, u16) {
    let road = road_east(game, HQ_FLAG, 2);
    let record = game.world.road_mut(road).unwrap();
    record.kind = RoadKind::Water;
    let order = record.order.unwrap();
    game.world.orders.get_mut(order).unwrap().wanted = Wanted::Person(Job::WATER_ROAD);
    (road, order)
}

fn boats(game: &Game) -> u16 {
    game.world().storehouses().next().unwrap().1.wares[WareKind::BOAT.index()]
}

#[test]
fn a_water_road_gets_a_helper_with_a_boat_from_stock() {
    let mut game = game();
    let (road, _) = water_road(&mut game);
    let before = boats(&game);
    run_to(&mut game, FIRST_PASS);
    let (figure, leaving) = game.world().figures().next().map(|(r, f)| (r, *f)).unwrap();
    assert!(matches!(
        leaving.kind,
        FigureKind::Leaving {
            job: Job::WATER_ROAD,
            ..
        }
    ));
    assert_eq!((headquarters(&game).1, boats(&game)), (51, before - 1));
    run_to(&mut game, RELEASE + 18);
    let FigureKind::Carrier(carrier) = game.world().figure(figure).unwrap().kind else {
        panic!("not a carrier");
    };
    assert_eq!((carrier.on.road, carrier.transport), (road, Transport::Boat));
}

#[test]
fn an_unserved_water_road_order_turns_to_the_other_shore_each_pass() {
    let mut game = game();
    let (road, order) = water_road(&mut game);
    let store = game.world().storehouses().next().unwrap().0;
    game.world.storehouse_mut(store).unwrap().wares[WareKind::BOAT.index()] = 0;
    let start = game.world().flag_at(HQ_FLAG).unwrap();
    let end = game.world().road(road).unwrap().end.unwrap();
    game.serve_person(order);
    let after = *game.world().order(order).unwrap();
    assert_eq!((after.flag, after.urgency), (end, 0));
    game.serve_person(order);
    assert_eq!(game.world().order(order).unwrap().flag, start);
    assert_eq!(game.world().figures().count(), 0);
}

#[test]
fn a_helper_walks_the_network_and_takes_over_at_either_end() {
    let mut game = game();
    let first = road_east(&mut game, HQ_FLAG, 4);
    let second = road_east(&mut game, Node { x: 20, y: 17 }, 3);
    run_to(&mut game, FIRST_PASS);
    assert_eq!(game.world().figures().count(), 1, "one person leaves at a time");
    let waiting = game.world().road(second).unwrap().order.unwrap();
    assert_eq!(
        game.world().order(waiting).unwrap().urgency,
        45,
        "the storehouse was busy"
    );
    run_to(&mut game, 400);
    for road in [first, second] {
        assert_eq!(game.world().road(road).unwrap().carriers, 1, "road {road}");
    }
    let positions: Vec<_> = game
        .world()
        .figures()
        .filter_map(|(_, f)| match f.kind {
            FigureKind::Carrier(carrier) => Some((carrier.on.road, carrier.on.position)),
            _ => None,
        })
        .collect();
    assert_eq!(positions, [(first, 2), (second, 1)]);
    assert_eq!(game.world().orders().count(), 0);
}

#[test]
fn a_settler_with_no_route_from_its_storehouse_flag_is_lost_there() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    let second = road_east(&mut game, Node { x: 20, y: 17 }, 3);
    let waiting = game.world().road(second).unwrap().order.unwrap();
    let mut leaving = None;
    for _ in 0..2000 {
        game.step();
        leaving = game
            .world()
            .figures()
            .find(|(_, figure)| matches!(figure.kind, FigureKind::Leaving { order: Some(order), .. } if order == waiting))
            .map(|(record, _)| record);
        if leaving.is_some() {
            break;
        }
    }
    let helper = leaving.expect("nobody set out for the second road");
    game.execute(
        player(),
        Command::RemoveRoad {
            node: Node { x: 18, y: 17 },
        },
    )
    .unwrap();
    while matches!(
        game.world().figure(helper).unwrap().kind,
        FigureKind::Leaving { .. }
    ) {
        game.step();
    }
    assert!(
        matches!(game.world().figure(helper).unwrap().kind, FigureKind::Lost { .. }),
        "{:?}",
        game.world().figure(helper).unwrap().kind
    );
}

/// Reads the shipped `ROM_ANIM.DAT` from the folder `S2_DATA_PLAIN` names.
pub(crate) fn shipped_script() -> Option<crate::carriers::AnimationScript> {
    let root = std::env::var_os("S2_DATA_PLAIN")?;
    let path = std::path::Path::new(&root).join("DATA/ANIMDAT/ROM_ANIM.DAT");
    let file = std::fs::read(path).ok()?;
    let rom_anim = s2_formats::bob::RomAnim::parse(&file).ok()?;
    Some(crate::carriers::AnimationScript::new(&rom_anim))
}

#[test]
fn idle_animations_from_the_shipped_script_run_and_end() {
    let Some(script) = shipped_script() else {
        eprintln!("S2_DATA_PLAIN not set; skipped");
        return;
    };
    let mut game = game();
    game.set_animations(script);
    road_east(&mut game, HQ_FLAG, 4);
    road_east(&mut game, Node { x: 20, y: 17 }, 3);
    let mut started = BTreeMap::new();
    let mut lengths = Vec::new();
    while game.clock().step() < 20_000 {
        game.step();
        for (record, figure) in game.world().figures() {
            let FigureKind::Carrier(carrier) = figure.kind else {
                continue;
            };
            let step = game.clock().step();
            let animating = carrier.state == CarrierState::Animating;
            match started.get(&record) {
                None if animating => {
                    started.insert(record, step);
                }
                Some(&from) if !animating => {
                    lengths.push(step - from);
                    started.remove(&record);
                }
                _ => {}
            }
        }
        assert!(game.world().animations.len() <= 2);
    }
    assert!(lengths.len() > 20, "{} animations", lengths.len());
    assert!(lengths.iter().all(|n| (10..=36).contains(n)), "{lengths:?}");
}

#[test]
fn ordinary_roads_lose_one_count_every_500_steps() {
    let mut game = game();
    let road = road_east(&mut game, HQ_FLAG, 4);
    game.world.road_mut(road).unwrap().traffic = 3;
    run_to(&mut game, 497);
    assert_eq!(game.world().road(road).unwrap().traffic, 3);
    game.step();
    assert_eq!(
        game.world().road(road).unwrap().traffic,
        2,
        "on step 498, cycle 499"
    );
    run_to(&mut game, 998);
    assert_eq!(game.world().road(road).unwrap().traffic, 1);
}

#[test]
fn a_walker_sent_home_to_the_storehouse_at_its_flag_turns_in_a_step_later() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 2);
    let flag = game.world().flag_at(HQ_FLAG).unwrap();
    let record = game.world.add_figure(
        HQ_FLAG,
        crate::figures::Figure::fresh(FigureKind::Walker(crate::figures::Walker {
            owner: player(),
            job: Job::HELPER,
            order: 0,
            mode: crate::settlers::HOME_MODE,
            on: crate::figures::OnRoad {
                road: 0,
                position: 0,
                to_start: false,
            },
        })),
    );
    assert!(game.send_home(record, flag));
    assert!(matches!(
        game.world().figure(record).unwrap().kind,
        FigureKind::Walker(_)
    ));
    game.step_settler(record);
    assert!(matches!(
        game.world().figure(record).unwrap().kind,
        FigureKind::Entering { .. }
    ));
}

#[test]
fn a_walker_whose_order_was_freed_goes_home_from_the_next_flag() {
    let mut game = game();
    let road = road_east(&mut game, HQ_FLAG, 2);
    let far = Node {
        x: HQ_FLAG.x + 2,
        y: HQ_FLAG.y,
    };
    let freed = u16::try_from(MAX_ORDERS - 1).unwrap();
    assert!(game.world().order(freed).is_none());
    let record = game.world.add_figure(
        far,
        crate::figures::Figure::fresh(FigureKind::Walker(crate::figures::Walker {
            owner: player(),
            job: Job::PACK_DONKEY,
            order: freed,
            mode: crate::workplace::WORKER_MODE,
            on: crate::figures::OnRoad {
                road,
                position: 2,
                to_start: false,
            },
        })),
    );
    game.step_walker(record);
    let FigureKind::Walker(walker) = game.world().figure(record).unwrap().kind else {
        panic!("not a walker");
    };
    assert_eq!(walker.mode, crate::settlers::HOME_MODE);
    assert_eq!(walker.order, game.world().flag_at(HQ_FLAG).unwrap());
}
