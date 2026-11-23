//! A fight on the generated meadow: player 0 builds a watchtower at the edge of its land facing
//! player 1, fills it with soldiers and attacks the other headquarters. The replay
//! `replays/meadow-battle.replay` records it and the replays test pins its hashes. The map holds
//! no game data, so this runs everywhere.
//!
//! `ROBIGO_BATTLE=write cargo test -p s2-sim --test battle -- --ignored write` plans the watchtower
//! and its road again and rewrites the replay's commands; bless the replay afterwards.

mod common;

use std::collections::BTreeSet;

use common::{Planner, building_at, headquarters, new_game, player, recorded, run, write_replay};
use s2_sim::{BuildingKind, Command, FigureKind, Game, Node, Player, Role, Setup, StartGoods};

/// The map, a file beside the replay.
const MAP: &str = "meadow.wld";

/// The replay this test writes and plays.
const REPLAY: &str = "meadow-battle.replay";

/// Steps the replay runs: the watchtower is manned by about 4,300 and both attackers are dead by
/// about 5,400.
const END: u32 = 6_000;

/// The military settings the planner turns up: the attack share, then the three occupation
/// shares, so that the watchtower fills whatever its border class.
const ATTACK: u8 = 3;
const OCCUPATION: [u8; 3] = [4, 5, 6];

/// Soldiers a watchtower holds.
const GARRISON: u8 = 6;

/// The hit points a private starts a fight with.
const PRIVATE_POINTS: u8 = 3;

fn setup() -> Setup {
    Setup {
        players: None,
        animals: true,
        goods: StartGoods::Normal,
        slots: None,
        starts: s2_sim::StartOrder::InSequence,
        objective: s2_sim::Objective::None,
        bug_fixes: s2_sim::BugFixes::Off,
        mission: None,
    }
}

fn waypoints() -> Vec<u8> {
    vec![0, 1]
}

/// How far apart two nodes are, as the attack window measures it.
fn distance(a: Node, b: Node) -> u32 {
    let dx = u32::from(a.x.abs_diff(b.x));
    let dy = u32::from(a.y.abs_diff(b.y));
    (dx * dx + dy * dy).isqrt()
}

/// How many soldiers stand in `player`'s military buildings.
fn garrison(game: &Game, player: Player) -> u8 {
    game.world()
        .military()
        .filter(|(_, military)| military.owner == player)
        .map(|(_, military)| military.inside)
        .sum()
}

#[test]
#[ignore = "writes the battle replay; run with ROBIGO_BATTLE=write"]
fn write() {
    if std::env::var_os("ROBIGO_BATTLE").is_none() {
        return;
    }
    let (game, mut replay) = new_game(MAP, setup(), waypoints(), END);
    let mut planner = Planner::new(game, player(0));
    let target = headquarters(&planner.game, player(1));
    for setting in std::iter::once(ATTACK).chain(OCCUPATION) {
        assert!(planner.execute(Command::SetMilitary { setting, value: 100 }));
    }
    let mut candidates = planner.owned();
    candidates.sort_by_key(|&node| distance(node, target));
    let tower = planner
        .place(BuildingKind::WATCHTOWER, &candidates)
        .expect("no room for a watchtower");
    let manned = planner.run_until(END, |game| garrison(game, player(0)) >= GARRISON);
    assert!(manned, "the watchtower never filled");
    let soldiers = planner
        .game
        .attack_count(player(0), building_at(&planner.game, target));
    assert!(soldiers > 0, "the window offers no soldiers");
    assert!(planner.execute(Command::Attack {
        node: target,
        soldiers,
        strongest: true,
    }));
    eprintln!(
        "watchtower at {tower:?}, {} from {target:?}, manned at step {}, {soldiers} sent",
        distance(tower, target),
        planner.step(),
    );
    replay.commands = planner.commands;
    let header = "# Player 0 builds a watchtower at the edge of its land on a generated meadow, \
                  fills it with\n# soldiers and attacks player 1's headquarters. Both attackers \
                  walk round the lake and die\n# at its flag. Written by tests/battle.rs.\n";
    write_replay(REPLAY, &replay, header);
}

#[test]
fn the_attackers_reach_the_other_headquarters_and_die_there() {
    let (mut game, replay) = recorded(MAP, REPLAY, setup(), waypoints(), END);
    let target = headquarters(&game, player(1));
    let flag = Node {
        x: target.x,
        y: target.y + 1,
    };
    let mut attackers = BTreeSet::new();
    let mut defenders = BTreeSet::new();
    let mut paired = BTreeSet::new();
    let mut hurt = false;
    let mut at_the_flag = false;
    run(&mut game, &replay, replay.end, |game| {
        for (record, soldier) in game.world().soldier_records() {
            match soldier.role {
                Role::Attacker => attackers.insert(record),
                Role::Defender => defenders.insert(record),
                _ => false,
            };
            if soldier.opponent.is_some() {
                paired.insert(record);
            }
            hurt |= soldier.hit_points < PRIVATE_POINTS;
        }
        at_the_flag |= game
            .world()
            .figures()
            .any(|(_, figure)| matches!(figure.kind, FigureKind::Soldier(_)) && figure.node() == flag);
    });
    assert_eq!(attackers.len(), 2, "two attackers set off");
    assert!(!defenders.is_empty(), "the headquarters sent a defender out");
    assert!(paired.len() >= 2, "the sides paired off: {paired:?}");
    assert!(hurt, "nobody lost a hit point");
    assert!(at_the_flag, "no soldier stood on the target's flag");
    assert_eq!(
        game.world().soldier_records().count(),
        0,
        "every soldier record was freed"
    );
    assert!(
        !game
            .world()
            .figures()
            .any(|(_, figure)| matches!(figure.kind, FigureKind::Soldier(_))),
        "a soldier is still out"
    );
    assert_eq!(
        game.world()
            .building(building_at(&game, target))
            .map(|building| building.owner),
        Some(player(1)),
        "the headquarters held"
    );
    assert_eq!(
        garrison(&game, player(0)),
        GARRISON,
        "the watchtower filled up again"
    );
    let broken = game.world().broken_links();
    assert!(broken.is_empty(), "{broken:#?}");
}
