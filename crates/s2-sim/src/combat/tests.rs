use crate::commands::{Command, Refusal};
use crate::figures::{FigureKind, Soldier, SoldierState};
use crate::game::{BugFixes, Game};
use crate::gather::tests::manned;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::marks::{Edges, RoadKind};
use crate::military::ATTACK;
use crate::players::{Controller, PlayerSlot, Portrait};
use crate::rng::GameRng;
use crate::settlers::tests::{HQ_FLAG, game, player, road_east, run_to};
use crate::world::{BuildingKind, BuildingState, MapObject, Player};

use super::{Alliances, FightStep};

const OWN: Node = Node { x: 12, y: 12 };
const TARGET: Node = Node { x: 21, y: 20 };

fn rival() -> Player {
    Player::new(1).unwrap()
}

/// Puts `player` in the game, played by `controller`.
fn seat(game: &mut Game, player: Player, controller: Controller) {
    let portrait = Portrait(u8::try_from(player.index()).unwrap());
    game.world
        .slots
        .set(player, Some(PlayerSlot { controller, portrait }));
}

/// The game with players 0 to 2 in it, all played by people.
fn three() -> Game {
    let mut game = game();
    for index in 0..3 {
        seat(&mut game, Player::new(index).unwrap(), Controller::First);
    }
    game
}

/// A finished, occupied barracks of `owner` on `node` with `soldiers` privates inside.
fn barracks(game: &mut Game, node: Node, owner: Player, soldiers: u8) -> u16 {
    let record = manned(game, BuildingKind::BARRACKS, node);
    let flag = game.world().building(record).unwrap().flag;
    game.world.building_mut(record).unwrap().owner = owner;
    game.world.flag_mut(flag).unwrap().owner = owner;
    game.start_military(record);
    game.world.orders.iter_mut().for_each(|(_, order)| order.tag = 1);
    let (military, _) = game.world().military_of(record).unwrap();
    let held = game.world.military.get_mut(military).unwrap();
    held.owner = owner;
    held.requested = 0;
    held.inside = soldiers;
    held.ranks = [soldiers, 0, 0, 0, 0];
    record
}

fn soldiers(game: &Game) -> Vec<(u16, Soldier)> {
    game.world()
        .figures()
        .filter_map(|(record, figure)| match figure.kind {
            FigureKind::Soldier(soldier) => Some((record, soldier)),
            _ => None,
        })
        .collect()
}

fn attack(soldiers: u8) -> Command {
    Command::Attack {
        node: TARGET,
        soldiers,
        strongest: true,
    }
}

#[test]
fn the_window_counts_all_but_one_soldier_times_the_attack_share() {
    let mut game = game();
    barracks(&mut game, OWN, player(), 5);
    let target = barracks(&mut game, TARGET, rival(), 1);
    assert_eq!(game.attack_count(player(), target), 2, "4 · 50 · 42 / 42 / 100");
    game.world.military_settings[player().index()][ATTACK] = 100;
    assert_eq!(game.attack_count(player(), target), 4);
}

#[test]
fn nothing_can_attack_a_building_with_no_border_near_it() {
    let mut game = game();
    barracks(&mut game, OWN, player(), 5);
    let target = barracks(&mut game, TARGET, rival(), 1);
    for y in 0..game.nodes().height() {
        for x in 0..game.nodes().width() {
            let node = Node { x, y };
            let mut data = game.nodes().get(node);
            data.borders = Edges::empty();
            game.nodes.set(node, data);
        }
    }
    assert_eq!(game.attack_count(player(), target), 0);
    assert_eq!(game.execute(player(), attack(1)), Err(Refusal::NoSoldiers));
}

#[test]
fn an_attack_on_your_own_building_is_refused() {
    let mut game = game();
    barracks(&mut game, OWN, player(), 5);
    barracks(&mut game, TARGET, player(), 1);
    assert_eq!(game.execute(player(), attack(1)), Err(Refusal::Own));
}

#[test]
fn attackers_queue_in_the_door_and_leave_one_at_a_time() {
    let mut game = game();
    let home = barracks(&mut game, OWN, player(), 4);
    barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(3)).unwrap();
    let out = soldiers(&game);
    assert_eq!(out.len(), 3);
    let waiting = out
        .iter()
        .filter(|(_, soldier)| matches!(soldier.state, SoldierState::Leaving { waiting: true, .. }))
        .count();
    assert_eq!(waiting, 2, "the first walks out, the others wait");
    let (_, held) = game.world().military_of(home).unwrap();
    assert_eq!((held.inside, held.requested), (1, 3), "counted as coming back");
}

/// The door queue empties oldest first, then newest first, 19 steps apart, or 18 when the
/// released figure still takes its own step in the pass that let it go (watched in S2).
#[test]
fn the_door_queue_sends_the_oldest_attacker_then_the_newest() {
    let mut game = game();
    barracks(&mut game, OWN, player(), 4);
    barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(3)).unwrap();
    let mut made: Vec<u16> = soldiers(&game).iter().map(|(record, _)| *record).collect();
    made.sort_unstable();
    assert_eq!(made.len(), 3);
    let mut out: Vec<(u16, u32)> = Vec::new();
    let waiting = |game: &Game, record: u16| {
        soldiers(game)
            .into_iter()
            .find(|(other, _)| *other == record)
            .is_some_and(|(_, soldier)| matches!(soldier.state, SoldierState::Leaving { .. }))
    };
    for _ in 0..200 {
        game.step();
        for &record in &made {
            if !waiting(&game, record) && !out.iter().any(|(other, _)| *other == record) {
                out.push((record, game.clock().step()));
            }
        }
    }
    let order: Vec<u16> = out.iter().map(|(record, _)| *record).collect();
    assert_eq!(
        order,
        vec![made[0], made[2], made[1]],
        "oldest, then newest first"
    );
    let gaps: Vec<u32> = out.windows(2).map(|pair| pair[1].1 - pair[0].1).collect();
    assert_eq!(
        gaps,
        vec![18, 19],
        "18 when the released figure steps in the same pass"
    );
}

/// Attackers that lose their target while they are still in their door turn round at their own
/// flag and disappear, and the building keeps asking for them (watched in S2).
/// Two attackers from a barracks of three lose their target while still in their door, with
/// `fixes`: the barracks once neither is out any more.
fn attackers_lose_their_target_in_the_door(fixes: BugFixes) -> (Game, u16) {
    let mut game = game();
    game.world.bug_fixes = fixes;
    let home = barracks(&mut game, OWN, player(), 3);
    let target = barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(2)).unwrap();
    assert_eq!(soldiers(&game).len(), 2);
    game.demolish(target);
    assert!(
        soldiers(&game).iter().all(|(record, _)| game
            .world()
            .soldier_of(*record)
            .unwrap()
            .1
            .target
            .is_none()),
        "both attackers lost their target"
    );
    run(&mut game, 200, |game| soldiers(game).is_empty());
    (game, home)
}

#[test]
fn attackers_that_lose_their_target_in_the_door_vanish_and_leave_the_request_raised() {
    let (game, home) = attackers_lose_their_target_in_the_door(BugFixes::Off);
    let (_, held) = game.world().military_of(home).unwrap();
    assert_eq!((held.inside, held.requested), (1, 2), "the two are still wanted");
    assert_eq!(held.ranks, [1, 0, 0, 0, 0]);
    let site = game.world().building(home).unwrap();
    assert!(
        !site.state.contains(BuildingState::DOOR),
        "the door is free again"
    );
    assert!(site.state.contains(BuildingState::PRESENT));
}

#[test]
fn with_the_bug_fixes_on_attackers_that_lose_their_target_in_the_door_join_their_home_again() {
    let (game, home) = attackers_lose_their_target_in_the_door(BugFixes::On);
    let (_, held) = game.world().military_of(home).unwrap();
    assert_eq!((held.inside, held.requested), (3, 0));
    assert_eq!(held.ranks, [3, 0, 0, 0, 0]);
    let site = game.world().building(home).unwrap();
    assert!(
        !site.state.contains(BuildingState::DOOR),
        "the door is free again"
    );
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

/// How far `player`'s land count is from the nodes it holds on the map.
fn land_count_off(game: &Game, player: Player) -> i64 {
    let (width, height) = (game.nodes().width(), game.nodes().height());
    let held = (0..height)
        .flat_map(|y| (0..width).map(move |x| Node { x, y }))
        .filter(|&node| game.nodes().get(node).owner == Some(player))
        .count();
    i64::from(game.nodes().owned(player)) - i64::try_from(held).unwrap()
}

/// How far each side's land count is from the map after an attacker takes an empty barracks,
/// with `fixes`.
fn capture_land_counts(fixes: BugFixes) -> (i64, i64) {
    let mut game = game();
    game.world.bug_fixes = fixes;
    barracks(&mut game, OWN, player(), 2);
    let target = barracks(&mut game, TARGET, rival(), 0);
    let (width, height) = (game.nodes().width(), game.nodes().height());
    for node in crate::territory::protected_nodes((width, height), TARGET) {
        let mut data = game.nodes().get(node);
        data.owner = Some(rival());
        game.nodes.set(node, data);
    }
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(1)).unwrap();
    run(&mut game, 2000, |game| {
        game.world().building_owner(target) == Some(player())
    });
    (land_count_off(&game, player()), land_count_off(&game, rival()))
}

#[test]
fn a_capture_books_nine_of_its_ten_protected_nodes() {
    assert_eq!(capture_land_counts(BugFixes::Off), (-1, 1));
}

#[test]
fn with_the_bug_fixes_on_a_capture_books_the_nodes_that_change_hands() {
    assert_eq!(capture_land_counts(BugFixes::On), (0, 0));
}

#[test]
fn an_attacker_walks_into_an_empty_building_and_takes_it() {
    let mut game = game();
    barracks(&mut game, OWN, player(), 2);
    let target = barracks(&mut game, TARGET, rival(), 0);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(1)).unwrap();
    let past = Node {
        x: TARGET.x + 10,
        y: TARGET.y,
    };
    assert!(!game.nodes().fully_seen(past, player()));
    run(&mut game, 2000, |game| {
        game.world().building_owner(target) == Some(player())
    });
    assert!(
        game.nodes().fully_seen(past, player()),
        "the capture reveals the barracks' 11 for its new owner"
    );
    let (_, held) = game.world().military_of(target).unwrap();
    assert_eq!((held.owner, held.inside, held.ranks[0]), (player(), 1, 1));
    let flag = game.world().building(target).unwrap().flag;
    assert_eq!(game.world().flag(flag).unwrap().owner, player());
    assert_eq!(
        game.world()
            .object(TARGET.neighbour(Direction::SouthEast, 48, 40)),
        MapObject::Flag(flag)
    );
    assert_eq!(game.world().soldier_records().count(), 0);
    let taken = game.world().messages(player()).list();
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].id, crate::messages::MessageId::BUILDING_TAKEN);
    assert_eq!(taken[0].node, Some(TARGET));
    let lost = game.world().messages(rival()).list();
    assert_eq!(lost.len(), 1, "an empty building sends no attack warning");
    assert_eq!(lost[0].id, crate::messages::MessageId::BUILDING_LOST);
}

#[test]
fn a_defender_comes_out_and_one_of_the_two_dies() {
    let mut game = game();
    barracks(&mut game, OWN, player(), 2);
    let target = barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(1)).unwrap();
    let fought = |game: &Game| {
        game.world()
            .soldier_records()
            .any(|(_, held)| held.step != FightStep::None)
    };
    run(&mut game, 2000, fought);
    run(&mut game, 2000, |game| soldiers(game).len() < 2);
    let skeletons = (0..40)
        .flat_map(|y| (0..48).map(move |x| Node { x, y }))
        .filter(|&node| matches!(game.world().object(node), MapObject::Remnant(_)))
        .count();
    let flag = TARGET.neighbour(Direction::SouthEast, 48, 40);
    assert_eq!(skeletons, 0, "a fight on a flag leaves no skeleton");
    assert!(
        !game
            .nodes()
            .get(flag)
            .flags
            .contains(crate::nodes::NodeFlags::HELD)
    );
    run(&mut game, 3000, |game| {
        game.world().soldier_records().count() == 0
    });
    let owner = game.world().building_owner(target);
    let (_, held) = game.world().military_of(target).unwrap();
    assert_eq!(
        held.inside, 1,
        "the defender back inside, or the attacker in the captured building"
    );
    assert!(owner == Some(player()) || owner == Some(rival()));
    let won = game.world().fights_won(player()) + game.world().fights_won(rival());
    assert_eq!(won, 1, "the survivor of the one fight counted it");
    let warned = game.world().messages(rival()).list();
    assert_eq!(warned[0].id, crate::messages::MessageId::UNDER_ATTACK);
    assert_eq!(warned[0].node, Some(TARGET));
}

#[test]
fn the_same_attack_plays_out_the_same_way_twice() {
    let play = || {
        let mut game = game();
        barracks(&mut game, OWN, player(), 5);
        barracks(&mut game, TARGET, rival(), 3);
        game.execute(player(), attack(2)).unwrap();
        for _ in 0..3000 {
            game.step();
        }
        game.state_hash()
    };
    assert_eq!(play(), play());
}

/// Empties the headquarters' soldier stock and keeps `held` privates in its reserve. Returns the
/// headquarters record.
fn hq_with_reserve(game: &mut Game, held: u8) -> u16 {
    let MapObject::Building(record) = game.world().object(crate::settlers::tests::HQ) else {
        panic!("no headquarters");
    };
    let (store, _) = game.world().storehouse_of(record).unwrap();
    if let Some(stock) = game.world.storehouse_mut(store) {
        for rank in Job::RANKS {
            stock.jobs[rank.index()] = 0;
        }
    }
    if let Some(building) = game.world.building_mut(record) {
        building.reserve[0].held = held;
    }
    record
}

#[test]
fn a_headquarters_with_only_its_reserve_left_sends_it_to_defend() {
    let mut game = game();
    let record = hq_with_reserve(&mut game, 1);
    let job = game.choose_defender(record, player(), false).unwrap();
    assert_eq!(job, Job::PRIVATE);
    assert_eq!(
        game.world().reserve_rank(record),
        None,
        "the soldier left the reserve"
    );
    assert_eq!(game.choose_defender(record, player(), false), None);
}

#[test]
fn a_defence_draw_never_reaches_the_reserve() {
    let mut game = game();
    let record = hq_with_reserve(&mut game, 1);
    game.world.military_settings[player().index()][crate::military::DEFENCE] = 100;
    assert_eq!(game.choose_defender(record, player(), true), None);
    assert_eq!(game.world().reserve_rank(record), Some(Job::PRIVATE));
}

/// Steps until attacker `record` stands on its cross-country path with an edge left, and
/// returns the node ahead of it.
fn ahead_of(game: &mut Game, record: u16) -> Node {
    for _ in 0..2000 {
        game.step();
        let Some(figure) = game.world().figure(record).copied() else {
            continue;
        };
        let FigureKind::Soldier(Soldier {
            state: SoldierState::Walking { path: Some(path) },
            ..
        }) = figure.kind
        else {
            continue;
        };
        if figure.walking() {
            continue;
        }
        let mut ahead = game.world.paths.get(path).copied().unwrap();
        if let Some(direction) = ahead.next() {
            return figure
                .node()
                .neighbour(direction, game.nodes.width(), game.nodes.height());
        }
    }
    panic!("no attacker on its way");
}

#[test]
fn a_soldier_held_by_another_waits_twenty_one_steps_and_keeps_its_path() {
    let mut game = game();
    barracks(&mut game, OWN, player(), 3);
    barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(2)).unwrap();
    let walking = soldiers(&game)[0].0;
    let next = ahead_of(&mut game, walking);
    let blocker = soldiers(&game)
        .into_iter()
        .map(|(record, _)| record)
        .find(|&record| record != walking)
        .unwrap();
    game.world.move_figure(blocker, next);
    let standing = game.world().figure(walking).unwrap().node();
    let (width, height) = (game.nodes.width(), game.nodes.height());
    for direction in Direction::ALL {
        let around = standing.neighbour(direction, width, height);
        game.world.add_figure(
            around,
            crate::figures::Figure::animal(crate::figures::Species::Fox),
        );
    }
    let path = match game.world().figure(walking).unwrap().kind {
        FigureKind::Soldier(Soldier {
            state: SoldierState::Walking { path: Some(path) },
            ..
        }) => path,
        other => panic!("{other:?}"),
    };
    let mut walked = None;
    for waits in 1..=crate::homeward::HELD_TRIES {
        game.step_settler(walking);
        walked = walked.or_else(|| game.world.paths.get(path).copied());
        assert_eq!(
            game.world().figure(walking).unwrap().waits,
            waits,
            "it waits for twenty blocked steps"
        );
    }
    let walked = walked.unwrap();
    game.step_settler(walking);
    let figure = game.world().figure(walking).copied().unwrap();
    assert_eq!(figure.node(), standing, "it stays where it stands");
    assert_eq!(
        figure.waits, 1,
        "the twenty-first step searched and found nothing"
    );
    for _ in 1..crate::homeward::HELD_TRIES {
        game.step_settler(walking);
    }
    assert_eq!(
        game.world().figure(walking).unwrap().waits,
        crate::homeward::HELD_TRIES
    );
    game.step_settler(walking);
    assert_eq!(
        game.world().figure(walking).unwrap().waits,
        1,
        "the next search comes twenty blocked steps after the last"
    );
    assert_eq!(
        game.world.paths.get(path).copied(),
        Some(walked),
        "its path is left as it was"
    );
    let (_, held) = game.world().soldier_of(walking).unwrap();
    assert_eq!(held.role, super::Role::Attacker, "not sent home");
}

#[test]
fn a_held_soldier_sent_home_keeps_the_edge_it_was_held_before() {
    let mut game = game();
    barracks(&mut game, OWN, player(), 3);
    barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(2)).unwrap();
    let walking = soldiers(&game)[0].0;
    let home_flag = game.world().figure(walking).unwrap().node();
    let mut next = ahead_of(&mut game, walking);
    while game.world().figure(walking).unwrap().node() == home_flag {
        next = ahead_of(&mut game, walking);
    }
    let blocker = soldiers(&game)
        .into_iter()
        .map(|(record, _)| record)
        .find(|&record| record != walking)
        .unwrap();
    game.world.move_figure(blocker, next);
    game.step_settler(walking);
    let held = game.world().figure(walking).copied().unwrap();
    assert!(matches!(
        held.kind,
        FigureKind::Soldier(Soldier { held: true, .. })
    ));
    game.go_home(walking);
    let home = game.world().figure(walking).copied().unwrap();
    assert!(
        matches!(
            home.kind,
            FigureKind::Soldier(Soldier {
                state: SoldierState::Walking { path: Some(_) },
                held: true,
                ..
            })
        ),
        "still held, on its new path: {:?}",
        home.kind
    );
    assert_eq!(home.direction, held.direction, "the stored edge stays");
}

#[test]
fn a_held_soldier_whose_search_finds_a_route_walks_it_even_when_its_path_held_it_already() {
    let mut game = game();
    barracks(&mut game, OWN, player(), 3);
    barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(2)).unwrap();
    let walking = soldiers(&game)[0].0;
    let next = ahead_of(&mut game, walking);
    let blocker = soldiers(&game)
        .into_iter()
        .map(|(record, _)| record)
        .find(|&record| record != walking)
        .unwrap();
    game.world.move_figure(blocker, next);
    let path = match game.world().figure(walking).unwrap().kind {
        FigureKind::Soldier(Soldier {
            state: SoldierState::Walking { path: Some(path) },
            ..
        }) => path,
        other => panic!("{other:?}"),
    };
    for _ in 0..crate::homeward::HELD_TRIES {
        game.step_settler(walking);
    }
    let mut ahead = game.clone();
    ahead.step_settler(walking);
    let route = ahead.world.paths.get(path).copied().unwrap();
    if let Some(held) = game.world.paths.get_mut(path) {
        *held = route;
    }
    game.step_settler(walking);
    assert_eq!(
        game.world.paths.get(path).copied(),
        Some(route),
        "the search found the route its path already held"
    );
    let figure = game.world().figure(walking).copied().unwrap();
    assert!(
        matches!(figure.kind, FigureKind::Soldier(Soldier { held: false, .. })),
        "{:?}",
        figure.kind
    );
    let mut unread = route;
    let first = unread.next().unwrap();
    let standing = figure.node();
    game.step_settler(walking);
    let figure = game.world().figure(walking).copied().unwrap();
    assert_eq!(figure.direction, first, "it takes its route's first edge");
    assert_eq!(
        figure.node(),
        standing.neighbour(first, game.nodes.width(), game.nodes.height())
    );
}

#[test]
fn a_soldier_whose_path_runs_out_short_searches_again_and_walks_on_in_the_same_step() {
    let mut game = game();
    barracks(&mut game, OWN, player(), 3);
    barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(1)).unwrap();
    let walking = soldiers(&game)[0].0;
    ahead_of(&mut game, walking);
    let FigureKind::Soldier(Soldier {
        state: SoldierState::Walking { path: Some(path) },
        ..
    }) = game.world().figure(walking).unwrap().kind
    else {
        panic!("not walking its path");
    };
    let record = game.world.paths.get_mut(path).unwrap();
    while record.next().is_some() {}
    let (from, records) = (
        game.world().figure(walking).unwrap().node(),
        game.world.paths.len(),
    );
    game.step_settler(walking);
    let figure = game.world().figure(walking).copied().unwrap();
    assert!(figure.walking(), "it starts the new route's first edge at once");
    assert_ne!(figure.node(), from);
    assert!(
        matches!(
            figure.kind,
            FigureKind::Soldier(Soldier {
                state: SoldierState::Walking { path: Some(again) },
                ..
            }) if again == path
        ),
        "the new route goes into the same record"
    );
    assert_eq!(game.world.paths.len(), records);
}

#[test]
fn a_chased_off_carrier_leaves_its_road_ordinary_and_waiting_for_a_helper() {
    let mut game = game();
    let road = road_east(&mut game, HQ_FLAG, 4);
    run_to(&mut game, 200);
    let (carrier, node) = game
        .world()
        .figures()
        .find(|(_, figure)| matches!(figure.kind, FigureKind::Carrier(_)))
        .map(|(record, figure)| (record, figure.node()))
        .unwrap();
    game.world.road_mut(road).unwrap().kind = RoadKind::Donkey;
    let carriers = game.world().road(road).unwrap().carriers;
    assert_eq!(game.world().road(road).unwrap().order, None);
    game.chase_off_carriers(node, player(), None, false);
    assert!(
        matches!(game.world().figure(carrier).unwrap().kind, FigureKind::Carrier(_)),
        "an own carrier stays"
    );
    game.chase_off_carriers(node, rival(), None, false);
    let after = game.world().road(road).unwrap();
    assert!(!matches!(
        game.world().figure(carrier).unwrap().kind,
        FigureKind::Carrier(_)
    ));
    assert_eq!(after.kind, RoadKind::Normal, "a donkey road is made ordinary");
    assert_eq!(after.carriers, carriers - 1);
    assert!(after.order.is_some(), "a helper is ordered for it");
}

#[test]
fn a_chased_off_carrier_has_its_ordinary_road_painted_again() {
    let mut game = game();
    let road = road_east(&mut game, HQ_FLAG, 4);
    run_to(&mut game, 200);
    let node = game
        .world()
        .figures()
        .find(|(_, figure)| matches!(figure.kind, FigureKind::Carrier(_)))
        .map(|(_, figure)| figure.node())
        .unwrap();
    assert_eq!(game.world().road(road).unwrap().kind, RoadKind::Normal);
    let inner = game.world().road(road).unwrap().path[1];
    let right = game.nodes.get(inner).sites;
    let mut data = game.nodes.get(inner);
    data.sites.size = crate::nodes::SiteSize::Large;
    game.nodes.set(inner, data);
    game.chase_off_carriers(node, rival(), None, false);
    assert_eq!(
        game.nodes.get(inner).sites,
        right,
        "the road's sites are worked out again"
    );
}

#[test]
fn the_same_nation_setting_allies_players_of_one_nation() {
    let mut game = three();
    let third = Player::new(2).unwrap();
    game.set_nation(player(), 1);
    game.set_nation(rival(), 1);
    game.set_nation(third, 2);
    game.set_alliances(Alliances::Nations);
    assert!(game.world().allied(player(), rival()));
    assert!(game.world().allied(rival(), player()));
    assert!(!game.world().allied(player(), third));
    assert!(
        game.world().allied(third, third),
        "every setting but None marks a player its own ally"
    );
    game.set_alliances(Alliances::None);
    assert!(!game.world().allied(player(), rival()));
    assert!(!game.world().allied(third, third));
}

#[test]
fn the_controller_setting_allies_the_people_and_the_computers_apart() {
    let mut game = three();
    let third = Player::new(2).unwrap();
    let absent = Player::new(4).unwrap();
    seat(&mut game, rival(), Controller::Second);
    seat(&mut game, third, Controller::Computer);
    game.set_alliances(Alliances::Controllers);
    assert!(game.world().allied(player(), rival()), "two people");
    assert!(!game.world().allied(player(), third));
    assert!(!game.world().allied(third, player()));
    assert!(
        game.world().allied(third, absent),
        "a player not in the game counts as the computer's"
    );
    assert!(!game.world().allied(absent, third), "and its own row is cleared");
}

#[test]
fn a_beaten_player_is_allied_with_nobody() {
    let mut game = three();
    seat(&mut game, rival(), Controller::Beaten);
    game.set_alliances(Alliances::Controllers);
    assert!(!game.world().allied(player(), rival()));
    assert!(!game.world().allied(rival(), rival()));
    assert!(game.world().allied(player(), Player::new(2).unwrap()));
}

#[test]
fn an_attack_breaks_the_alliance_with_the_owner_and_with_its_allies() {
    let mut game = three();
    let third = Player::new(2).unwrap();
    barracks(&mut game, OWN, player(), 2);
    barracks(&mut game, TARGET, rival(), 1);
    game.set_alliances(Alliances::Nations);
    assert!(game.world().allied(player(), third), "one nation to start with");
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(1)).unwrap();
    assert!(!game.world().allied(player(), rival()));
    assert!(!game.world().allied(rival(), player()));
    assert!(
        !game.world().allied(player(), third),
        "the target's allies go with it"
    );
    assert!(
        game.world().allied(rival(), third),
        "the target keeps its own allies"
    );
}

#[test]
fn an_allys_soldier_is_no_opponent() {
    let mut game = three();
    barracks(&mut game, OWN, player(), 2);
    barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(1)).unwrap();
    let attacker = soldiers(&game)[0].0;
    let node = game.world().figure(attacker).unwrap().node();
    let beside = node.neighbour(Direction::East, 48, 40);
    assert_eq!(
        game.find_opponent(beside, rival(), 1),
        Some(attacker),
        "an enemy within a ring"
    );
    game.set_alliances(Alliances::Nations);
    assert_eq!(
        game.find_opponent(beside, rival(), 1),
        None,
        "an ally is passed over"
    );
}

/// A fight with the original's `ROM_ANIM.DAT`: every round lasts 7 or 8 steps (watched in S2).
/// Runs only where the game's data is.
#[test]
fn with_the_original_animations_a_round_lasts_seven_or_eight_steps() {
    let Some(path) = std::env::var_os("S2_DATA_PLAIN") else {
        return;
    };
    let data = s2_data::DataSet::discover(std::path::PathBuf::from(path)).unwrap();
    let script = data.read("DATA/ANIMDAT/ROM_ANIM.DAT").unwrap();
    let mut game = game();
    game.set_animations(crate::AnimationScript::new(
        &s2_formats::bob::RomAnim::parse(&script).unwrap(),
    ));
    barracks(&mut game, OWN, player(), 2);
    barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(1)).unwrap();
    let mut seen: std::collections::BTreeMap<u16, (u8, u32)> = std::collections::BTreeMap::new();
    let mut gaps: std::collections::BTreeMap<u16, Vec<u32>> = std::collections::BTreeMap::new();
    for _ in 0..4000 {
        game.step();
        let step = game.clock().step();
        for (record, held) in game.world().soldier_records() {
            match seen.get(&record) {
                Some(&(rounds, _)) if rounds == held.rounds => continue,
                Some(&(rounds, at)) if rounds > 0 && rounds + 1 == held.rounds => {
                    gaps.entry(record).or_default().push(step - at);
                }
                _ => {}
            }
            seen.insert(record, (held.rounds, step));
        }
    }
    assert!(
        gaps.values().any(|runs| runs.len() > 4),
        "too few rounds: {gaps:?}"
    );
    for runs in gaps.values() {
        assert!(runs.iter().all(|&gap| (7..=8).contains(&gap)), "{gaps:?}");
    }
}

/// A soldier paired and waiting keeps waiting when its opponent is made lost on the way: nothing
/// clears the link, in S2.EXE or here.
#[test]
fn a_waiting_soldier_whose_opponent_is_lost_waits_on() {
    let mut game = game();
    barracks(&mut game, OWN, player(), 3);
    barracks(&mut game, TARGET, rival(), 2);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(1)).unwrap();
    let mut paired = None;
    for _ in 0..3000 {
        game.step();
        paired = soldiers(&game).into_iter().find_map(|(record, soldier)| {
            matches!(soldier.state, SoldierState::Waiting)
                .then(|| {
                    game.world()
                        .soldier_of(record)
                        .and_then(|(_, held)| held.opponent)
                })
                .flatten()
                .map(|opponent| (record, opponent))
        });
        if paired.is_some() {
            break;
        }
    }
    let (waiter, opponent) = paired.expect("no pair came about");
    game.make_lost(opponent);
    for _ in 0..400 {
        game.step();
    }
    let held = game
        .world()
        .soldier_of(waiter)
        .expect("the waiter kept its record")
        .1;
    assert_eq!(held.step, FightStep::Waiting);
    assert_eq!(
        held.opponent,
        Some(opponent),
        "the link to the lost soldier stays"
    );
    let state = soldiers(&game)
        .into_iter()
        .find(|(record, _)| *record == waiter)
        .map(|(_, soldier)| soldier.state);
    assert!(matches!(state, Some(SoldierState::Waiting)), "{state:?}");
}

/// A soldier that walks in through the door of another player's headquarters pulls it down and
/// wanders off as a lost soldier with a countdown of 254 (watched in S2). The attacker is made
/// the rival's by hand: the fixture holds one headquarters, and it is the player's own.
#[test]
fn the_conqueror_of_a_headquarters_pulls_it_down_and_is_lost() {
    let mut game = game();
    let hq = hq_with_reserve(&mut game, 0);
    barracks(&mut game, OWN, player(), 3);
    barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(1)).unwrap();
    let attacker = soldiers(&game)
        .first()
        .map(|(record, _)| *record)
        .expect("no attacker");
    if let Some(FigureKind::Soldier(soldier)) = game.figure_kind_mut(attacker) {
        soldier.owner = rival();
    }
    game.walk_in(attacker, hq, super::Entry::Home);
    let mut gone = false;
    for _ in 0..200 {
        game.step();
        if game.world().building(hq).is_none() {
            gone = true;
            break;
        }
    }
    assert!(gone, "the headquarters still stands");
    assert!(
        game.world().soldier_of(attacker).is_none(),
        "the soldier record is freed"
    );
    let kind = game.world().figure(attacker).map(|figure| figure.kind);
    assert!(
        matches!(
            kind,
            Some(FigureKind::Lost {
                countdown: 254,
                job: Job::PRIVATE,
                ..
            })
        ),
        "{kind:?}"
    );
}

/// Steps a game whose figure pool is full until a figure's turn grows it, with `fixes`, and
/// gives back how far the last record's counter fell on that step and on the next.
fn grow_the_figure_pool(fixes: BugFixes) -> (u16, u16) {
    let mut game = game();
    game.world.bug_fixes = fixes;
    let hq = hq_with_reserve(&mut game, 0);
    barracks(&mut game, OWN, player(), 3);
    barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(1)).unwrap();
    let attacker = soldiers(&game)[0].0;
    if let Some(FigureKind::Soldier(soldier)) = game.figure_kind_mut(attacker) {
        soldier.owner = rival();
    }
    game.walk_in(attacker, hq, super::Entry::Home);
    let mut sheep = crate::figures::Figure::new(FigureKind::Animal {
        species: crate::figures::Species::Sheep,
        state: crate::figures::AnimalState::Resting,
        hunter: None,
    });
    sheep.counter = u16::MAX;
    let pasture = Node { x: 2, y: 2 };
    let mut last = 0;
    while game.world().figures().count() < 2000 {
        last = game.world.add_figure(pasture, sheep);
    }
    assert_eq!(last, 1999);
    let rest = |game: &Game| game.world().figure(last).unwrap().counter;
    loop {
        let (before, records) = (rest(&game), game.world().figure_records());
        game.step();
        if game.world().figure_records() > records && records == 2000 {
            let grown = before - rest(&game);
            let before = rest(&game);
            game.step();
            return (grown, before - rest(&game));
        }
        assert_eq!(rest(&game), before - 1);
        assert!(game.clock().step() < 200, "the pool never grew");
    }
}

#[test]
fn figures_after_the_one_that_grows_the_figure_pool_wait_until_the_next_step() {
    assert_eq!(
        grow_the_figure_pool(BugFixes::Off),
        (0, 1),
        "the last sheep rests on, then runs again on the step after"
    );
}

#[test]
fn with_the_bug_fixes_on_every_figure_takes_its_turn_on_the_step_the_pool_grows() {
    assert_eq!(grow_the_figure_pool(BugFixes::On), (1, 1));
}

#[test]
fn an_attacker_in_a_doorway_sent_home_only_forgets_its_target() {
    let mut game = game();
    barracks(&mut game, OWN, player(), 3);
    let target = barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(1)).unwrap();
    let attacker = soldiers(&game)[0].0;
    game.walk_in(attacker, target, super::Entry::Home);
    let draws = game.rng;
    game.go_home(attacker);
    assert!(matches!(
        game.world().figure(attacker).unwrap().kind,
        FigureKind::Soldier(Soldier {
            state: SoldierState::Entering { .. },
            ..
        })
    ));
    let (_, held) = game.world().soldier_of(attacker).unwrap();
    assert_eq!((held.role, held.target), (super::Role::Attacker, None));
    assert_eq!(game.rng, draws, "no path is searched");
}

/// A soldier going home whose fight ends on its home's flag, with `fixes`: the game after
/// the end of its animation, the figure, and the generator before the animation ended.
fn fight_ends_on_own_flag(fixes: BugFixes) -> (Game, u16, GameRng) {
    let mut game = game();
    game.world.bug_fixes = fixes;
    let home = barracks(&mut game, OWN, player(), 3);
    barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(1)).unwrap();
    let soldier = soldiers(&game)[0].0;
    ahead_of(&mut game, soldier);
    game.drop_path(soldier);
    let flag = game.home_flag_node(home).unwrap();
    game.world.move_figure(soldier, flag);
    game.update_record(soldier, |held| {
        held.role = super::Role::Home;
        held.step = FightStep::None;
    });
    let draws = game.rng;
    game.animation_ended(soldier);
    (game, soldier, draws)
}

#[test]
fn a_soldier_whose_fight_ends_on_its_own_flag_is_lost() {
    let (game, soldier, draws) = fight_ends_on_own_flag(BugFixes::Off);
    assert!(matches!(
        game.world().figure(soldier).unwrap().kind,
        FigureKind::Lost { countdown: 254, .. }
    ));
    assert_eq!(game.rng, draws, "a lost soldier draws nothing");
}

#[test]
fn with_the_bug_fixes_on_a_soldier_whose_fight_ends_on_its_own_flag_walks_in() {
    let (game, soldier, _) = fight_ends_on_own_flag(BugFixes::On);
    assert!(matches!(
        game.world().figure(soldier).unwrap().kind,
        FigureKind::Soldier(Soldier {
            state: SoldierState::Entering { .. },
            ..
        })
    ));
}

/// The target an attacker keeps when a figure with its target building's number dies, with
/// `fixes`, and that building.
fn target_after_a_namesake_dies(fixes: BugFixes) -> (Option<u16>, u16) {
    let mut game = game();
    game.world.bug_fixes = fixes;
    barracks(&mut game, OWN, player(), 3);
    let target = barracks(&mut game, TARGET, rival(), 1);
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(1)).unwrap();
    let attacker = soldiers(&game)[0].0;
    game.forget_target(target);
    (game.world().soldier_of(attacker).unwrap().1.target, target)
}

#[test]
fn an_attacker_forgets_its_target_when_a_figure_of_the_same_number_dies() {
    assert_eq!(target_after_a_namesake_dies(BugFixes::Off).0, None);
}

#[test]
fn with_the_bug_fixes_on_only_defenders_forget_the_figure_that_died() {
    let (kept, target) = target_after_a_namesake_dies(BugFixes::On);
    assert_eq!(kept, Some(target));
}

#[test]
fn a_soldier_walking_cross_country_reveals_round_itself() {
    let mut game = game();
    barracks(&mut game, OWN, player(), 2);
    barracks(&mut game, TARGET, rival(), 0);
    for y in 0..40 {
        for x in 0..48 {
            let at = Node { x, y };
            let mut data = game.nodes.get(at);
            data.seen = crate::sight::Seen::empty();
            game.nodes.set(at, data);
        }
    }
    game.world.military_settings[player().index()][ATTACK] = 100;
    game.execute(player(), attack(1)).unwrap();
    let flag = OWN.neighbour(Direction::SouthEast, 48, 40);
    let away = |game: &Game| {
        soldiers(game).iter().find_map(|&(record, _)| {
            let at = game.world().figure(record)?.node();
            (at != OWN && at != flag).then_some(at)
        })
    };
    run(&mut game, 2000, |game| away(game).is_some());
    let at = away(&game).unwrap();
    assert!(game.nodes().fully_seen(at, player()));
    let far = Node { x: at.x + 6, y: at.y };
    assert!(!game.nodes().fully_seen(far, player()), "4 rings and no further");
}
