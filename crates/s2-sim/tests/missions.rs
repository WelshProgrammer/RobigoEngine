//! The ten Roman campaign chapters start and run from the player's own data, read through
//! `S2_DATA_PLAIN`. The assertions are counts and positions, never the data itself.
//!
//! `ROBIGO_CAMPAIGN=write cargo test -p s2-sim --test missions -- --ignored write` plays the first
//! chapter's opening again and rewrites `tests/replays/chapter1.replay`; bless the replay
//! afterwards.
//!
//! `tests/replays/people.rtx` is Robigo's own script for the first chapter's map, for the two
//! keywords no shipped chapter uses: `MET_PEOPLE_AMOUNT` and `MET_CLEAR_ACT_EVENTS`. S2 played it
//! under DOSBox-X with the computer in player 0's seat, and every reading matched Robigo's.

#[allow(
    dead_code,
    reason = "the replay helpers are shared; this file uses the planner alone"
)]
mod common;

use std::path::PathBuf;

use s2_data::DataSet;
use s2_formats::bob::RomAnim;
use s2_formats::map::Map;
use s2_formats::rtx::Script;
use s2_sim::replay::{MapSource, Replay, content_hash};
use s2_sim::{
    AnimationScript, BugFixes, BuildingKind, Command, Controller, Game, Node, Player, PlayerSlot,
    PlayerSlots, Portraits, Setup, SimTables, map_path, script_path,
};

use common::{Planner, folder, player, run, write_replay};

const CHAPTERS: u16 = 10;

/// The animation script's path in the game's data.
const ROM_ANIM: &str = "DATA/ANIMDAT/ROM_ANIM.DAT";

/// The first chapter's replay, beside the other replays.
const REPLAY: &str = "chapter1.replay";

/// How many steps the first chapter's replay runs.
const END: u32 = 4_000;

/// The people script, beside the replays.
const PEOPLE: &str = "people.rtx";

/// How many steps the people script runs.
const PEOPLE_END: u32 = 40_000;

/// Each event of the people script that fires by [`PEOPLE_END`], with the step it fires on.
const PEOPLE_FIRED: [(u8, u32); 8] = [
    (1, 13),
    (2, 13),
    (4, 253),
    (3, 11_773),
    (7, 25_373),
    (5, 34_833),
    (10, 34_833),
    (11, 34_853),
];

/// The people script's events that never fire: carpenters never reach 4, and events 20 and 21
/// were cleared off the active list before it filled.
const PEOPLE_WAITING: [u8; 3] = [6, 20, 21];

/// The buildings the first chapter's opening asks for.
const ASKED_FOR: [BuildingKind; 3] = [
    BuildingKind::WOODCUTTER,
    BuildingKind::QUARRY,
    BuildingKind::SAWMILL,
];

fn data() -> Option<DataSet> {
    std::env::var_os("S2_DATA_PLAIN").map(|path| DataSet::discover(PathBuf::from(path)).unwrap())
}

fn setup(number: u16) -> Setup {
    Setup {
        mission: Some(number),
        bug_fixes: BugFixes::Off,
        ..Setup::default()
    }
}

fn chapter(data: &DataSet, number: u16) -> Game {
    let map = Map::parse_wld(&data.read(&map_path(number)).unwrap()).unwrap();
    let script = Script::parse(&data.read(&script_path(number)).unwrap()).unwrap();
    Game::start_mission(&map, &setup(number), &script, &SimTables::ORIGINAL).unwrap()
}

fn headquarters(game: &Game, player: Player) -> Vec<Node> {
    game.world()
        .buildings()
        .filter(|(_, building)| building.kind == BuildingKind::HEADQUARTERS && building.owner == player)
        .map(|(_, building)| building.node)
        .collect()
}

#[test]
fn every_chapter_starts_with_its_events_watched_and_runs() {
    let Some(data) = data() else {
        eprintln!("skipped, set S2_DATA_PLAIN to run it");
        return;
    };
    for number in 0..CHAPTERS {
        let mut game = chapter(&data, number);
        let mission = game.mission().unwrap();
        assert_eq!(mission.texts(), Some(number + 1), "chapter {number}");
        assert!(mission.active().flatten().count() > 0, "chapter {number}");
        assert_eq!(headquarters(&game, Player::ALL[0]).len(), 1, "chapter {number}");
        for _ in 0..2_000 {
            game.step();
        }
        assert!(!game.mission().unwrap().won(), "chapter {number}");
    }
}

#[test]
fn the_first_chapter_keeps_the_header_headquarters_over_the_scripts() {
    let Some(data) = data() else {
        eprintln!("skipped, set S2_DATA_PLAIN to run it");
        return;
    };
    let game = chapter(&data, 0);
    assert_eq!(headquarters(&game, Player::ALL[0]), [Node { x: 33, y: 38 }]);
    let in_game = game.world().slots().players().count();
    assert_eq!(in_game, 7, "the header counts seven players");
}

/// The nation each seated player's face gives it, up to the last player in the game, as the
/// campaign start sets them (`0x16f7c`).
fn faces(game: &Game) -> Vec<u8> {
    let slots = game.world().slots();
    let count = slots.players().last().map_or(0, |player| player.index() + 1);
    slots.nations(&Portraits::ORIGINAL)[..count].to_vec()
}

fn distance(a: Node, b: Node) -> u32 {
    u32::from(a.x.abs_diff(b.x)) + u32::from(a.y.abs_diff(b.y))
}

#[test]
#[ignore = "writes the first chapter's replay; run with ROBIGO_CAMPAIGN=write"]
fn write() {
    if std::env::var_os("ROBIGO_CAMPAIGN").is_none() {
        return;
    }
    let data = data().expect("set S2_DATA_PLAIN");
    let map = data.read(&map_path(0)).unwrap();
    let script = data.read(&script_path(0)).unwrap();
    let animations = data.read(ROM_ANIM).unwrap();
    let mut game = chapter(&data, 0);
    game.set_animations(AnimationScript::new(&RomAnim::parse(&animations).unwrap()));
    let mut replay = Replay::new(
        MapSource::Data(map_path(0)),
        content_hash(&map),
        Some(content_hash(&animations)),
        setup(0),
    );
    replay.mission_script = Some(content_hash(&script));
    replay.nations = faces(&game);
    replay.waypoints.clone_from(&replay.nations);
    replay.apply_players(&mut game);
    replay.end = END;
    let mut planner = Planner::new(game, player(0));
    let home = headquarters(&planner.game, player(0))[0];
    let mut candidates = planner.owned();
    candidates.sort_by_key(|&node| distance(node, home));
    for kind in ASKED_FOR {
        planner.place(kind, &candidates).expect("no room");
    }
    let mut letters = 0;
    while planner.step() < END {
        planner.game.step();
        if planner.game.mission().unwrap().letter().is_some() {
            assert!(planner.execute(Command::CloseLetter));
            letters += 1;
        }
    }
    eprintln!("{letters} letters closed by step {END}");
    replay.commands = planner.commands;
    let header = "# The first Roman chapter: player 0 places the woodcutter, quarry and sawmill the \
                  opening asks\n# for and closes the two letters it opens. Once all three stand, \
                  the opening's end event\n# unlocks the forester. Written by tests/missions.rs.\n";
    write_replay(REPLAY, &replay, header);
}

#[test]
fn the_first_chapters_replay_finishes_the_opening() {
    let Some(data) = data() else {
        eprintln!("skipped, set S2_DATA_PLAIN to run it");
        return;
    };
    let text = std::fs::read_to_string(folder().join(REPLAY)).unwrap();
    let replay = Replay::parse(&text).unwrap();
    let mut game = chapter(&data, 0);
    replay.apply_players(&mut game);
    assert!(!game.mission().unwrap().allowed(player(0), BuildingKind::FORESTER));
    let animations = data.read(ROM_ANIM).unwrap();
    game.set_animations(AnimationScript::new(&RomAnim::parse(&animations).unwrap()));
    run(&mut game, &replay, replay.end, |_| {});
    let closes = replay
        .commands
        .iter()
        .filter(|stamped| stamped.command == Command::CloseLetter)
        .count();
    assert_eq!(closes, 2, "the first letter and the opening's end");
    assert!(game.mission().unwrap().allowed(player(0), BuildingKind::FORESTER));
    assert!(!game.mission().unwrap().won());
}

/// The first chapter's map with the people script, seated as the campaign seats it but with the
/// computer in player 0's seat, as the probe played it.
fn people_game(data: &DataSet) -> Game {
    let map = Map::parse_wld(&data.read(&map_path(0)).unwrap()).unwrap();
    let script = Script::parse(&std::fs::read(folder().join(PEOPLE)).unwrap()).unwrap();
    let mut slots = PlayerSlots::campaign(&map, &script);
    let first = player(0);
    let portrait = slots.get(first).unwrap().portrait;
    slots.set(
        first,
        Some(PlayerSlot {
            controller: Controller::Computer,
            portrait,
        }),
    );
    let setup = Setup {
        slots: Some(slots),
        ..setup(0)
    };
    let mut game = Game::start_mission(&map, &setup, &script, &SimTables::ORIGINAL).unwrap();
    let animations = data.read(ROM_ANIM).unwrap();
    game.set_animations(AnimationScript::new(&RomAnim::parse(&animations).unwrap()));
    let seated: Vec<_> = game.world().slots().players().collect();
    for player in seated {
        let Some(slot) = game.world().slots().get(player) else {
            continue;
        };
        let nation = Portraits::ORIGINAL.get(slot.portrait).map_or(0, |row| row.nation);
        game.set_nation(player, nation);
        if let Some(set) = game.world().waypoint_set(usize::from(nation)).cloned() {
            game.set_waypoints(player, set);
        }
    }
    game
}

#[test]
fn the_people_events_fire_as_they_did_in_s2() {
    let Some(data) = data() else {
        eprintln!("skipped, set S2_DATA_PLAIN to run it");
        return;
    };
    let mut game = people_game(&data);
    let ids = |game: &Game| -> Vec<u8> {
        game.mission()
            .unwrap()
            .records()
            .map(|record| record.id)
            .collect()
    };
    let cleared: Vec<u16> = game
        .mission()
        .unwrap()
        .records()
        .filter(|record| PEOPLE_WAITING[1..].contains(&record.id))
        .map(|record| record.record)
        .collect();
    let active: Vec<u16> = game
        .mission()
        .unwrap()
        .active_records()
        .iter()
        .flatten()
        .map(|&record| u16::from(record))
        .collect();
    assert!(
        cleared.iter().all(|record| !active.contains(record)),
        "{active:?}"
    );
    let mut waiting = ids(&game);
    let mut fired = Vec::new();
    while game.clock().step() < PEOPLE_END {
        game.step();
        let now = ids(&game);
        let step = game.clock().step();
        fired.extend(
            waiting
                .iter()
                .filter(|id| !now.contains(id))
                .map(|&id| (id, step)),
        );
        waiting = now;
        if game.mission().unwrap().letter().is_some() {
            game.execute(player(0), Command::CloseLetter).unwrap();
        }
    }
    let mut expected = PEOPLE_FIRED.to_vec();
    expected.sort_unstable_by_key(|&(id, step)| (step, id));
    fired.sort_unstable_by_key(|&(id, step)| (step, id));
    assert_eq!(fired, expected);
    assert_eq!(waiting, PEOPLE_WAITING);
}
