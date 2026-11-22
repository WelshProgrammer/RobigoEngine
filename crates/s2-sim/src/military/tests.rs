use crate::commands::{Command, Refusal};
use crate::game::Game;
use crate::grid::Node;
use crate::job::Job;
use crate::settlers::tests::{HQ, HQ_FLAG, game, player, road_east};
use crate::world::{BuildingKind, BuildingState, MapObject};

use super::{MOST_MILITARY, OCCUPATION, RANK};

const BARRACKS: Node = Node { x: 22, y: 16 };

/// A finished barracks east of the headquarters, joined to it by road, that has just asked
/// for its first soldier.
fn barracks(game: &mut Game) -> u16 {
    road_east(game, HQ_FLAG, 6);
    game.execute(
        player(),
        Command::PlaceBuilding {
            node: BARRACKS,
            kind: BuildingKind::BARRACKS,
        },
    )
    .unwrap();
    let MapObject::Building(record) = game.world().object(BARRACKS) else {
        panic!("no barracks");
    };
    let building = game.world.building_mut(record).unwrap();
    building.state = BuildingState::FINISHED | BuildingState::DOOR;
    building.requested = 0;
    game.start_military(record);
    record
}

fn occupied(game: &Game, record: u16) -> bool {
    game.world()
        .building(record)
        .unwrap()
        .state
        .contains(BuildingState::MANNED)
}

fn run_until(game: &mut Game, steps: u32, done: impl Fn(&Game) -> bool) {
    for _ in 0..steps {
        if done(game) {
            return;
        }
        game.step();
    }
    assert!(done(game), "gave up at step {}", game.clock().step());
}

#[test]
fn a_finished_barracks_asks_for_the_strongest_soldier_and_claims_land_when_it_arrives() {
    let mut game = game();
    let record = barracks(&mut game);
    let (military, inside) = game.world().military_of(record).map(|(m, r)| (m, *r)).unwrap();
    assert_eq!((inside.requested, inside.inside, inside.capacity), (1, 0, 2));
    let asked = game
        .world()
        .orders()
        .find(|(_, order)| order.job().is_some_and(Job::is_soldier))
        .map(|(_, order)| order.job());
    assert_eq!(
        asked,
        Some(Some(Job::GENERAL)),
        "rank setting 50 asks for the strongest"
    );
    let east = Node { x: 30, y: 16 };
    assert_eq!(
        game.nodes().get(east).owner,
        None,
        "beyond the headquarters' land"
    );
    let (seen, beyond) = (Node { x: 32, y: 16 }, Node { x: 34, y: 16 });
    assert!(
        !game.nodes().fully_seen(seen, player()),
        "past the headquarters' 12"
    );
    run_until(&mut game, 3000, |game| occupied(game, record));
    assert!(
        game.nodes().fully_seen(seen, player()),
        "the first soldier in reveals the barracks' 11"
    );
    assert!(!game.nodes().fully_seen(beyond, player()));
    let held = *game.world().military.get(military).unwrap();
    assert_eq!((held.requested, held.inside), (0, 1));
    assert_eq!(game.nodes().get(BARRACKS).strength, 9);
    assert!(game.nodes().get(BARRACKS).protected);
    assert_eq!(
        game.nodes().get(east).owner,
        Some(player()),
        "eight rings from the barracks"
    );
    let hq = game.world().object(HQ).building().unwrap();
    assert_eq!(game.world().building(hq).unwrap().occupied, 0);
    assert_eq!(game.world().building(record).unwrap().occupied, 1);
}

#[test]
fn the_occupation_setting_fills_the_barracks_and_lowering_it_sends_soldiers_home() {
    let mut game = game();
    let record = barracks(&mut game);
    run_until(&mut game, 3000, |game| occupied(game, record));
    let interior = u8::try_from(OCCUPATION).unwrap();
    let full = Command::SetMilitary {
        setting: interior,
        value: MOST_MILITARY,
    };
    game.execute(player(), full).unwrap();
    let inside = |game: &Game| game.world().military_of(record).unwrap().1.inside;
    run_until(&mut game, 3000, |game| inside(game) == 2);
    game.execute(
        player(),
        Command::SetMilitary {
            setting: interior,
            value: 0,
        },
    )
    .unwrap();
    run_until(&mut game, 200, |game| inside(game) == 1);
    let walking_home = game.world().figures().any(|(_, figure)| {
        matches!(figure.kind, crate::figures::FigureKind::Walker(walker) if walker.job.is_soldier())
            || matches!(figure.kind, crate::figures::FigureKind::Porter(porter) if porter.job.is_soldier())
    });
    assert!(walking_home, "the extra soldier walks out and home");
}

#[test]
fn a_soldier_sent_home_walks_out_onto_the_flag_and_closes_the_door_on_the_way() {
    let mut game = game();
    let record = barracks(&mut game);
    run_until(&mut game, 3000, |game| occupied(game, record));
    let interior = u8::try_from(OCCUPATION).unwrap();
    let set = |value| Command::SetMilitary {
        setting: interior,
        value,
    };
    game.execute(player(), set(MOST_MILITARY)).unwrap();
    let inside = |game: &Game| game.world().military_of(record).unwrap().1.inside;
    run_until(&mut game, 3000, |game| inside(game) == 2);
    game.execute(player(), set(0)).unwrap();
    run_until(&mut game, 200, |game| inside(game) == 1);
    let flag = game.world().building(record).unwrap().flag;
    let door = game.world().flag(flag).unwrap().node;
    let porter = |game: &Game| {
        game.world().figures().find_map(|(at, figure)| {
            matches!(figure.kind, crate::figures::FigureKind::Porter(porter) if porter.job.is_soldier())
                .then(|| (at, *figure))
        })
    };
    let (soldier, figure) = porter(&game).unwrap();
    assert_eq!(figure.node(), door, "it walks onto the flag's node");
    let mut closed_on_the_way = false;
    while porter(&game).is_some_and(|(at, figure)| at == soldier && figure.walking()) {
        game.step();
        let open = game
            .world()
            .building(record)
            .unwrap()
            .state
            .contains(BuildingState::DOOR);
        closed_on_the_way |= !open && porter(&game).is_some_and(|(_, figure)| figure.walking());
    }
    assert!(closed_on_the_way, "the walk step closes the door before the flag");
}

#[test]
fn military_settings_take_values_up_to_a_hundred_and_reset() {
    let mut game = game();
    let rank = u8::try_from(RANK).unwrap();
    let set = |setting, value| Command::SetMilitary { setting, value };
    assert_eq!(game.execute(player(), set(rank, 101)), Err(Refusal::OutOfRange));
    assert_eq!(game.execute(player(), set(7, 10)), Err(Refusal::OutOfRange));
    game.execute(player(), set(rank, 25)).unwrap();
    assert_eq!(game.world().military_settings(player())[RANK], 25);
    game.execute(player(), Command::ResetMilitary).unwrap();
    assert_eq!(game.world().military_settings(player())[RANK], 50);
}

#[test]
fn demolishing_an_occupied_barracks_releases_its_land_and_loses_its_soldier() {
    let mut game = game();
    let record = barracks(&mut game);
    run_until(&mut game, 3000, |game| occupied(game, record));
    let east = Node { x: 30, y: 16 };
    let shared = Node { x: 20, y: 16 };
    assert_eq!(game.nodes().get(east).owner, Some(player()));
    game.execute(player(), Command::Demolish { node: BARRACKS })
        .unwrap();
    assert_eq!(game.nodes().get(east).owner, None, "only the barracks held it");
    assert_eq!(
        game.nodes().get(shared).owner,
        Some(player()),
        "the headquarters holds it again"
    );
    assert!(game.world().military().next().is_none(), "the record is freed");
    let lost_soldiers = game
        .world()
        .figures()
        .filter(|(_, figure)| {
            matches!(figure.kind, crate::figures::FigureKind::Lost { job, .. } if job.is_soldier())
        })
        .count();
    assert_eq!(lost_soldiers, 1);
    assert!(
        game.world().broken_links().is_empty(),
        "{:?}",
        game.world().broken_links()
    );
}

/// An occupied barracks holding one private: the rank setting asks for the weakest.
fn barracks_with_a_private(game: &mut Game) -> u16 {
    let rank = u8::try_from(RANK).unwrap();
    game.execute(
        player(),
        Command::SetMilitary {
            setting: rank,
            value: 0,
        },
    )
    .unwrap();
    let record = barracks(game);
    run_until(game, 3000, |game| occupied(game, record));
    let (_, military) = game.world().military_of(record).unwrap();
    assert_eq!(military.ranks, [1, 0, 0, 0, 0]);
    record
}

fn coin_orders(game: &Game, record: u16) -> usize {
    let flag = game.world().building(record).unwrap().flag;
    game.world()
        .orders()
        .filter(|(_, order)| order.flag == flag && order.ware() == Some(crate::ware::WareKind::COINS))
        .count()
}

#[test]
fn a_barracks_orders_a_coin_and_spends_it_to_promote_after_its_wait() {
    let mut game = game();
    let record = barracks_with_a_private(&mut game);
    run_until(&mut game, 300, |game| coin_orders(game, record) == 1);
    let site = game.world.building_mut(record).unwrap();
    site.stock[0] = 1;
    site.productivity = 21;
    run_until(&mut game, 40, |game| {
        game.world().military_of(record).unwrap().1.ranks[1] == 1
    });
    let site = game.world().building(record).unwrap();
    assert_eq!(
        (site.stock[0], site.productivity),
        (0, 0),
        "a coin spent, the timer reset"
    );
}

#[test]
fn stopping_coins_frees_the_open_coin_order() {
    let mut game = game();
    let record = barracks_with_a_private(&mut game);
    run_until(&mut game, 300, |game| coin_orders(game, record) == 1);
    game.execute(
        player(),
        Command::StopProduction {
            node: BARRACKS,
            stop: true,
        },
    )
    .unwrap();
    assert_eq!(coin_orders(&game, record), 0);
    let site = game.world().building(record).unwrap();
    assert_eq!((site.on_way[0], site.requested), (0, 0));
}

/// The headquarters record, its storehouse record and its private count.
fn store(game: &Game) -> (u16, u16, u16) {
    let MapObject::Building(record) = game.world().object(HQ) else {
        panic!("no headquarters");
    };
    let (store, stock) = game.world().storehouse_of(record).unwrap();
    (record, store, stock.jobs[Job::PRIVATE.index()])
}

fn reserve(game: &Game, building: u16) -> Vec<(u8, u8)> {
    game.world()
        .building(building)
        .unwrap()
        .reserve
        .iter()
        .map(|rank| (rank.wanted, rank.held))
        .collect()
}

#[test]
fn a_new_headquarters_keeps_one_of_each_rank_back() {
    let game = game();
    let (record, _, _) = store(&game);
    assert_eq!(reserve(&game, record), vec![(1, 0); Job::RANKS.len()]);
}

#[test]
fn the_building_tick_moves_one_private_into_the_reserve() {
    let mut game = game();
    let (record, _, before) = store(&game);
    while !game.clock().every(48, 2) {
        game.step();
    }
    game.step();
    let (_, _, after) = store(&game);
    assert_eq!(after, before - 1, "one private left the stock");
    assert_eq!(reserve(&game, record)[0], (1, 1));
    assert_eq!(
        reserve(&game, record)[1..],
        [(1, 0); Job::RANKS.len() - 1],
        "the ranks with none in stock hold none"
    );
}

#[test]
fn raising_the_reserve_takes_a_private_per_press_and_lowering_puts_them_back() {
    let mut game = game();
    let (record, _, before) = store(&game);
    let set = |wanted| Command::SetReserve {
        node: HQ,
        rank: Job::PRIVATE,
        wanted,
    };
    game.execute(player(), set(3)).unwrap();
    assert_eq!(
        reserve(&game, record)[0],
        (3, 2),
        "two presses from one, one private each"
    );
    let (_, _, held) = store(&game);
    assert_eq!(held, before - 2);
    game.execute(player(), set(0)).unwrap();
    assert_eq!(reserve(&game, record)[0], (0, 0));
    let (_, _, after) = store(&game);
    assert_eq!(after, before, "both back in stock");
}

#[test]
fn a_reserve_of_fifteen_is_the_most_and_only_the_owner_sets_it() {
    let mut game = game();
    let (record, _, _) = store(&game);
    let set = |wanted| Command::SetReserve {
        node: HQ,
        rank: Job::PRIVATE,
        wanted,
    };
    assert_eq!(game.execute(player(), set(16)), Err(Refusal::OutOfRange));
    game.execute(player(), set(15)).unwrap();
    assert_eq!(reserve(&game, record)[0].0, 15);
    let rival = crate::world::Player::new(1).unwrap();
    assert_eq!(game.execute(rival, set(2)), Err(Refusal::NotYours));
    assert_eq!(
        game.execute(
            player(),
            Command::SetReserve {
                node: HQ,
                rank: Job::HELPER,
                wanted: 1,
            }
        ),
        Err(Refusal::OutOfRange),
        "only the five ranks"
    );
}

#[test]
fn a_pass_that_reaches_the_last_military_record_starts_the_next_from_the_first() {
    let mut game = game();
    let empty = super::Military {
        building: u16::MAX,
        border: 0,
        owner: player(),
        requested: 0,
        inside: 0,
        ranks: [0; 5],
        capacity: 0,
    };
    for _ in 0..80 {
        game.world.military.insert(empty, super::MAX_MILITARY);
    }
    game.military_pass();
    assert_eq!(game.world.military_cursor, 40);
    game.military_pass();
    assert_eq!(game.world.military_cursor, 0, "record 79 is the last");
}
