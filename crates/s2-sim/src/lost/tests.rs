use crate::commands::Command;
use crate::figures::{CarrierState, FigureKind};
use crate::game::{BugFixes, Game};
use crate::grid::Node;
use crate::job::Job;
use crate::settlers::tests::{FIRST_PASS, HQ_FLAG, game, player, road_east, run_to};
use crate::transport::OrderKind;

const MIDDLE: Node = Node { x: 18, y: 17 };

/// A game where a 4-edge road east of the headquarters' flag has its carrier waiting in the
/// middle, and the carrier's record.
fn served() -> (Game, u16, u16) {
    let mut game = game();
    let road = road_east(&mut game, HQ_FLAG, 4);
    run_to(&mut game, 200);
    let carrier = game
        .world()
        .figures()
        .find(|(_, figure)| matches!(figure.kind, FigureKind::Carrier(_)))
        .map(|(record, _)| record)
        .unwrap();
    (game, road, carrier)
}

#[test]
fn a_removed_road_strands_its_carrier_with_one_draw() {
    let (mut game, road, carrier) = served();
    let mut expected = game.rng;
    let countdown = 30 + u8::try_from(expected.up_to(100)).unwrap();
    game.execute(player(), Command::RemoveRoad { node: MIDDLE })
        .unwrap();
    assert!(game.world().road(road).is_none());
    assert_eq!(game.rng, expected);
    let kind = game.world().figure(carrier).unwrap().kind;
    assert_eq!(
        kind,
        FigureKind::Lost {
            owner: player(),
            job: Job::HELPER,
            countdown
        }
    );
}

#[test]
fn a_removed_road_spares_a_worker_walking_cross_country_over_it() {
    let (mut game, _, carrier) = served();
    let ahead = Node { x: 19, y: 17 };
    let path = game
        .find_path(ahead, HQ_FLAG, crate::homeward::Crossing::Walkable, false)
        .unwrap();
    let trip = crate::figures::Trip::Walking { path };
    let worker = game.world.add_figure(
        ahead,
        crate::figures::Figure::new(FigureKind::Gatherer(crate::figures::Gatherer {
            owner: player(),
            job: Job::WOODCUTTER,
            building: 0,
            trip,
            prey: None,
            searches: 0,
        })),
    );
    let mut expected = game.rng;
    let _ = expected.up_to(100);
    game.execute(player(), Command::RemoveRoad { node: MIDDLE })
        .unwrap();
    assert!(matches!(
        game.world().figure(carrier).unwrap().kind,
        FigureKind::Lost { .. }
    ));
    assert!(matches!(
        game.world().figure(worker).unwrap().kind,
        FigureKind::Gatherer(gatherer) if gatherer.trip == trip
    ));
    assert_eq!(game.rng, expected, "one draw, the carrier's");
}

#[test]
fn a_lost_settler_that_runs_out_of_time_leaves_a_skeleton() {
    let mut game = game();
    let at = Node { x: 30, y: 20 };
    let rival = crate::world::Player::new(1).unwrap();
    let record = game.world.add_figure(
        at,
        crate::figures::Figure::new(FigureKind::Lost {
            owner: rival,
            job: Job::HELPER,
            countdown: 1,
        }),
    );
    game.step();
    assert!(game.world().figure(record).is_none());
    assert_eq!(
        game.world().object(at),
        crate::world::MapObject::Remnant(crate::world::Remnant::SKELETON)
    );
}

#[test]
fn removing_a_road_strands_the_helper_on_its_way_and_frees_the_order() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    run_to(&mut game, FIRST_PASS + 3);
    let (helper, _) = game.world().figures().next().unwrap();
    game.execute(player(), Command::RemoveRoad { node: MIDDLE })
        .unwrap();
    assert!(matches!(
        game.world().figure(helper).unwrap().kind,
        FigureKind::Lost { job: Job::HELPER, .. }
    ));
    assert_eq!(game.world().orders().count(), 0);
}

#[test]
fn a_split_road_keeps_its_carrier_on_the_half_it_stands_on() {
    let (mut game, _, carrier) = served();
    game.execute(
        player(),
        Command::PlaceFlag {
            node: MIDDLE,
            water: false,
        },
    )
    .unwrap();
    let FigureKind::Carrier(on) = game.world().figure(carrier).unwrap().kind else {
        panic!("no longer a carrier");
    };
    assert_eq!((on.on.position, on.state), (0, CarrierState::Idle));
    let half = game.world().road(on.on.road).unwrap();
    assert_eq!(game.world().flag(half.start).unwrap().node, MIDDLE);
    assert_eq!((half.carriers, half.served, half.order), (1, true, None));
    let orders: Vec<_> = game.world().orders().map(|(_, order)| order.flag).collect();
    assert_eq!(
        orders,
        [game.world().flag_at(HQ_FLAG).unwrap()],
        "the other half wants a helper"
    );
    run_to(&mut game, 600);
    let carriers = game
        .world()
        .figures()
        .filter(|(_, figure)| matches!(figure.kind, FigureKind::Carrier(_)))
        .count();
    assert_eq!((carriers, game.world().orders().count()), (2, 0));
}

#[test]
fn a_helper_on_its_way_keeps_its_order_when_the_road_is_split() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    run_to(&mut game, FIRST_PASS + 3);
    let (order, _) = game.world().orders().next().unwrap();
    game.execute(
        player(),
        Command::PlaceFlag {
            node: MIDDLE,
            water: false,
        },
    )
    .unwrap();
    let kinds: Vec<_> = game
        .world()
        .orders()
        .map(|(record, order)| (record, order.kind()))
        .collect();
    assert_eq!(kinds.len(), 2);
    assert!(kinds.contains(&(order, OrderKind::Delivering)));
    run_to(&mut game, 600);
    let carriers = game
        .world()
        .roads()
        .filter(|(_, road)| road.carriers == 1)
        .count();
    assert_eq!((carriers, game.world().orders().count()), (2, 0));
}

#[test]
fn a_worker_lost_with_its_load_takes_its_job_back_as_its_nation_says() {
    use crate::figures::{Figure, Gatherer, Trip};
    let cases = [
        (0, Job::CARRYING_WOOD, Job::WOODCUTTER),
        (0, Job::CARRYING_MEAT, Job::HUNTER),
        (0, Job::CARRYING_GRAIN, Job::FARMER),
        (1, Job::CARRYING_GRAIN, Job::PIG_BREEDER),
        (3, Job::SOWER, Job::FARMER),
        (0, Job::SOWER, Job::SOWER),
    ];
    for (nation, job, lost) in cases {
        let mut game = game();
        game.world.nations[player().index()] = nation;
        let record = game.world.add_figure(
            Node { x: 30, y: 20 },
            Figure::fresh(FigureKind::Gatherer(Gatherer {
                owner: player(),
                job,
                building: 0,
                trip: Trip::Waiting,
                prey: None,
                searches: 0,
            })),
        );
        game.make_lost(record);
        let Some(FigureKind::Lost { job: now, .. }) = game.world().figure(record).map(|f| f.kind) else {
            panic!("not lost");
        };
        assert_eq!(now, lost, "nation {nation}, job {job:?}");
    }
}

#[test]
fn a_carrier_held_in_a_queue_walks_on_once_lost() {
    let (mut game, _, carrier) = served();
    let figure = game.world().figure(carrier).copied().unwrap();
    let (width, height) = (game.nodes.width(), game.nodes.height());
    let behind = figure
        .node()
        .neighbour(figure.direction.opposite(), width, height);
    let mut data = game.nodes.get(behind);
    data.flags |= crate::nodes::NodeFlags::HELD;
    game.nodes.set(behind, data);
    if let Some(figure) = game.world.figure_mut(carrier) {
        figure.held = true;
        figure.holds_behind = true;
    }
    game.execute(player(), Command::RemoveRoad { node: MIDDLE })
        .unwrap();
    let lost = game.world().figure(carrier).copied().unwrap();
    assert!(matches!(lost.kind, FigureKind::Lost { .. }));
    assert!(!lost.held && !lost.holds_behind);
    assert!(
        !game
            .nodes
            .get(behind)
            .flags
            .contains(crate::nodes::NodeFlags::HELD),
        "the node behind it is given back"
    );
}

/// The job a pack donkey delivering into a building is lost as, with `fixes`.
fn lost_delivering_donkey(fixes: BugFixes) -> Job {
    use crate::figures::{Delivery, Figure, Transport};
    let mut game = game();
    game.world.bug_fixes = fixes;
    let record = game.world.add_figure(
        Node { x: 30, y: 20 },
        Figure::fresh(FigureKind::Delivering(Delivery {
            owner: player(),
            transport: Transport::Donkey,
            road: None,
            ware: None,
            shows: None,
            inside: true,
        })),
    );
    game.make_lost(record);
    let Some(FigureKind::Lost { job, .. }) = game.world().figure(record).map(|f| f.kind) else {
        panic!("not lost");
    };
    job
}

#[test]
fn a_pack_donkey_lost_while_delivering_comes_home_as_a_helper() {
    assert_eq!(lost_delivering_donkey(BugFixes::Off), Job::HELPER);
}

#[test]
fn with_the_bug_fixes_on_a_pack_donkey_lost_while_delivering_stays_a_pack_donkey() {
    assert_eq!(lost_delivering_donkey(BugFixes::On), Job::PACK_DONKEY);
}

#[test]
fn a_lost_settler_with_nowhere_to_wander_comes_off_the_people_count() {
    let mut game = game();
    let at = Node { x: 30, y: 20 };
    let rival = crate::world::Player::new(1).unwrap();
    let (width, height) = (game.nodes.width(), game.nodes.height());
    for direction in crate::grid::Direction::ALL {
        let near = at.neighbour(direction, width, height);
        let mut data = game.nodes.get(near);
        data.flags |= crate::nodes::NodeFlags::HELD;
        game.nodes.set(near, data);
    }
    game.world.count_people(rival, Job::STONEMASON, 1);
    let record = game.world.add_figure(
        at,
        crate::figures::Figure::new(FigureKind::Lost {
            owner: rival,
            job: Job::STONEMASON,
            countdown: 50,
        }),
    );
    game.step();
    assert!(game.world().figure(record).is_none());
    assert_eq!(game.world().people(rival, Job::STONEMASON), 0);
}

/// A game whose road east of the headquarters names a helper being delivered whose record has
/// been freed, with a lost settler above it to keep the record count up, and that record.
fn freed_helper(fixes: BugFixes) -> (Game, u16) {
    let mut game = game();
    game.world.bug_fixes = fixes;
    road_east(&mut game, HQ_FLAG, 4);
    run_to(&mut game, FIRST_PASS + 3);
    let (helper, _) = game.world().figures().next().unwrap();
    game.world.add_figure(
        Node { x: 30, y: 20 },
        crate::figures::Figure::new(FigureKind::Lost {
            owner: player(),
            job: Job::HELPER,
            countdown: 50,
        }),
    );
    game.world.remove_figure(helper);
    (game, helper)
}

#[test]
fn with_the_bug_fixes_on_a_freed_helper_record_stays_free() {
    let (mut game, helper) = freed_helper(BugFixes::On);
    let first = crate::world::Player::ALL[0];
    let scythes = game.world().owned_wares(first, crate::ware::WareKind::SCYTHE);
    let draws = game.rng;
    game.execute(player(), Command::RemoveRoad { node: MIDDLE })
        .unwrap();
    assert_eq!(game.rng, draws, "no countdown is drawn");
    assert!(game.world().figure(helper).is_none());
    assert_eq!(
        game.world().owned_wares(first, crate::ware::WareKind::SCYTHE),
        scythes
    );
}

#[test]
fn a_freed_helper_record_named_by_a_removed_road_becomes_a_lost_metalworker_of_player_0() {
    let (mut game, helper) = freed_helper(BugFixes::Off);
    let first = crate::world::Player::ALL[0];
    let scythes = game.world().owned_wares(first, crate::ware::WareKind::SCYTHE);
    let mut expected = game.rng;
    let countdown = 30 + u8::try_from(expected.up_to(100)).unwrap();
    game.execute(player(), Command::RemoveRoad { node: MIDDLE })
        .unwrap();
    assert_eq!(game.rng, expected);
    let figure = *game.world().figure(helper).unwrap();
    assert_eq!(
        figure.kind,
        FigureKind::Lost {
            owner: first,
            job: Job::METALWORKER,
            countdown
        }
    );
    let (width, height) = (game.nodes.width(), game.nodes.height());
    assert_eq!(figure.node(), Node::wrapped(-12594, -12594, width, height));
    assert!(
        game.world()
            .figures_at(figure.node())
            .all(|(record, _)| record != helper)
    );
    assert_eq!(
        game.world().owned_wares(first, crate::ware::WareKind::SCYTHE),
        scythes.saturating_sub(1)
    );
}
