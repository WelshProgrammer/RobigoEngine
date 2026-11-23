use crate::commands::Command;
use crate::construction::PLANER_MODE;
use crate::figures::{Figure, FigureKind, OnRoad, Planer, PlanerWork, Walker};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::homeward::HELD_TRIES;
use crate::job::Job;
use crate::settlers::HOME_MODE;
use crate::settlers::tests::{HQ_FLAG, game, hold, player, road_east, shipped_script};
use crate::world::BuildingKind;
use crate::world::{BuildingState, MapObject};

const END: Node = Node { x: 20, y: 17 };

/// Raised neighbours of the site on ground at altitude 10: the target is
/// `(4 · 10 + 16 + 15 + 14) / 7`, rounded down.
const RAISED: [(Direction, u8); 3] = [
    (Direction::NorthEast, 16),
    (Direction::West, 15),
    (Direction::NorthWest, 14),
];
const TARGET: u8 = 12;

/// A game with a 4-edge road east of the headquarters and a sawmill site on uneven ground at
/// its far flag. Returns the game, the site's record and its node.
fn uneven_site() -> (Game, u16, Node) {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    let (width, height) = (game.nodes.width(), game.nodes.height());
    let node = END.neighbour(Direction::NorthWest, width, height);
    for (direction, altitude) in RAISED {
        game.nodes
            .set_altitude(node.neighbour(direction, width, height), altitude);
    }
    game.execute(
        player(),
        Command::PlaceBuilding {
            node,
            kind: BuildingKind::SAWMILL,
        },
    )
    .unwrap();
    let MapObject::Building(site) = game.world().object(node) else {
        panic!("no site");
    };
    (game, site, node)
}

/// Puts a planer walker on the site's flag and lets it arrive. Returns the figure.
fn arrive(game: &mut Game) -> u16 {
    let flag = game.world().flag_at(END).unwrap();
    let (order, _) = game
        .world()
        .orders()
        .find(|(_, order)| order.job() == Some(Job::PLANER) && order.flag == flag)
        .unwrap();
    let walker = game.world.add_figure(
        END,
        Figure::new(FigureKind::Walker(Walker {
            owner: player(),
            job: Job::PLANER,
            order,
            mode: PLANER_MODE,
            on: OnRoad {
                road: 0,
                position: 0,
                to_start: false,
            },
        })),
    );
    game.planer_arrives(walker, order);
    walker
}

fn planer(game: &Game, record: u16) -> Option<Planer> {
    match game.world().figure(record)?.kind {
        FigureKind::Planer(planer) => Some(planer),
        _ => None,
    }
}

/// The site, its six neighbours and its flag: the nodes a planer levels.
fn footprint(game: &Game, site: Node) -> Vec<Node> {
    let (width, height) = (game.nodes.width(), game.nodes.height());
    std::iter::once(site)
        .chain(Direction::ALL.map(|direction| site.neighbour(direction, width, height)))
        .collect()
}

/// Runs planer `record` alone, one step at a time, until it is a planer no more. Returns the
/// steps at which each node was levelled and the step it left.
fn work(game: &mut Game, record: u16) -> (Vec<u32>, u32) {
    let mut levelled = Vec::new();
    for step in 1..5000 {
        let before = planer(game, record).map(|planer| planer.levelled);
        game.step_settler(record);
        match planer(game, record) {
            Some(after) if Some(after.levelled) != before => levelled.push(step),
            Some(_) => {}
            None => return (levelled, step),
        }
    }
    panic!("still levelling");
}

#[test]
fn a_site_on_uneven_ground_orders_a_planer_and_no_builder() {
    let (game, site, _) = uneven_site();
    let jobs: Vec<Job> = game
        .world()
        .orders()
        .filter_map(|(_, order)| order.job())
        .filter(|&job| job != Job::HELPER)
        .collect();
    assert_eq!(jobs, [Job::PLANER], "the road's helper aside");
    assert!(
        game.world()
            .building(site)
            .unwrap()
            .state
            .contains(BuildingState::LEVELLING)
    );
}

#[test]
fn an_arriving_planer_takes_the_mean_altitude_and_heads_west_of_the_flag() {
    let (mut game, _, _) = uneven_site();
    let mut expected = game.rng;
    let record = arrive(&mut game);
    let planer = planer(&game, record).unwrap();
    assert_eq!((planer.target, planer.levelled), (TARGET, 0));
    assert_eq!(planer.flag, game.world().flag_at(END).unwrap());
    let PlanerWork::Walking { path } = planer.work else {
        panic!("not walking");
    };
    let (width, height) = (game.nodes.width(), game.nodes.height());
    assert_eq!(
        game.world.paths.get(path).unwrap().to,
        END.neighbour(Direction::West, width, height)
    );
    assert!(
        game.world()
            .orders()
            .all(|(_, order)| order.job() != Some(Job::PLANER)),
        "the order is freed"
    );
    expected.up_to(0);
    assert_eq!(game.rng, expected, "one draw for the one-step path");
}

#[test]
fn the_planer_levels_seven_nodes_starts_construction_and_goes_home() {
    let (mut game, site, node) = uneven_site();
    let (width, height) = (game.nodes.width(), game.nodes.height());
    let untouched = node.neighbour(Direction::NorthEast, width, height);
    let untouched = untouched.neighbour(Direction::NorthEast, width, height);
    let before = game.nodes.get(untouched).altitude;
    let mut expected = game.rng;
    let record = arrive(&mut game);
    let (levelled, left) = work(&mut game, record);
    assert_eq!(
        levelled,
        [21, 42, 63, 84, 105],
        "one step of digging with no script"
    );
    assert_eq!(left, 125);
    for at in footprint(&game, node) {
        assert_eq!(game.nodes.get(at).altitude, TARGET, "{at:?}");
    }
    assert_eq!(game.nodes.get(untouched).altitude, before);
    let building = game.world().building(site).unwrap();
    assert!(building.state.contains(BuildingState::BUILDING));
    assert!(!building.state.contains(BuildingState::LEVELLING));
    assert_eq!(building.requested, 1);
    assert!(
        game.world()
            .orders()
            .any(|(_, order)| order.job() == Some(Job::BUILDER))
    );
    let Some(FigureKind::Walker(walker)) = game.world().figure(record).map(|figure| figure.kind) else {
        panic!("not walking home");
    };
    assert_eq!((walker.job, walker.mode), (Job::PLANER, HOME_MODE));
    assert_eq!(walker.order, game.world().flag_at(HQ_FLAG).unwrap());
    assert_eq!(game.world().figure(record).unwrap().node(), END);
    for _ in 0..6 {
        expected.up_to(0);
    }
    assert_eq!(game.rng, expected, "one draw per one-step path, six in all");
    assert_eq!(game.world.paths.len(), 0);
}

#[test]
fn a_planer_with_the_shipped_script_takes_85_steps_a_node() {
    let Some(script) = shipped_script() else {
        eprintln!("S2_DATA_PLAIN not set; skipped");
        return;
    };
    let (mut game, _, _) = uneven_site();
    game.set_animations(script);
    let record = arrive(&mut game);
    let (levelled, left) = work(&mut game, record);
    assert_eq!(levelled, [85, 170, 255, 340, 425]);
    assert_eq!(left, 445);
}

#[test]
fn a_site_on_uneven_ground_is_levelled_built_and_its_people_go_home() {
    let (mut game, site, node) = uneven_site();
    let (_, store) = game.world().storehouses().next().unwrap();
    let stock = (store.jobs[Job::PLANER.index()], store.jobs[Job::BUILDER.index()]);
    crate::settlers::tests::run_to(&mut game, 6000);
    for at in footprint(&game, node) {
        assert_eq!(game.nodes.get(at).altitude, TARGET, "{at:?}");
    }
    let building = game.world().building(site).unwrap();
    assert!(building.state.contains(BuildingState::FINISHED), "{building:?}");
    let (_, store) = game.world().storehouses().next().unwrap();
    assert_eq!(
        (store.jobs[Job::PLANER.index()], store.jobs[Job::BUILDER.index()]),
        stock,
        "both back in stock"
    );
}

#[test]
fn a_held_node_stops_the_planer_and_the_twenty_first_step_searches() {
    let (mut game, _, _) = uneven_site();
    let record = arrive(&mut game);
    let (width, height) = (game.nodes.width(), game.nodes.height());
    let next = END.neighbour(Direction::West, width, height);
    hold(&mut game, next, true);
    for waits in 1..=HELD_TRIES {
        let drawn = game.rng;
        game.step_settler(record);
        let figure = game.world().figure(record).copied().unwrap();
        assert_eq!(figure.node(), END, "it stays on the flag");
        assert_eq!(figure.waits, waits);
        assert_eq!(game.rng, drawn, "no search while it waits");
    }
    let drawn = game.rng;
    game.step_settler(record);
    assert_ne!(game.rng, drawn, "the twenty-first step searches");
    assert_eq!(
        game.world().figure(record).unwrap().waits,
        0,
        "the count starts again, so a new route held too waits a full count"
    );
    hold(&mut game, next, false);
    game.step_settler(record);
    assert_eq!(
        game.world().figure(record).unwrap().node(),
        next,
        "on its way again"
    );
}
