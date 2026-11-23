//! Plays a game against a trace of the computer players' state and stops at the first step where
//! the two part. A probe read the trace out of the original's memory while it ran under DOSBox-X.
//! The trace is the original's data and stays outside the repository: the test runs only when
//! `S2_AI_TRACE` names one and `S2_DATA_PLAIN` the installed game it was made from.
//! `S2_AI_FIELDS`, a comma-separated list of top-level fields such as `grid`, limits the
//! comparison to those, to follow one part past a step where another has already parted.
//!
//! A trace of a Roman chapter names its mission in the header, and the script it ran when the
//! probe wrote one over the chapter's own. The game then starts as the campaign seats it, with the
//! computer in player 0's seat, and player 0 closes each letter after the step the probe closed
//! it, which the lines record (`closed`). Older traces, taken at up to 19 steps a frame, record
//! none, and each letter closes after the step that opens it. A line that records geologist or
//! scout orders gives player 0 the same orders once it has been compared.

use std::fmt::Write as _;
use std::path::Path;

use s2_formats::bob::RomAnim;
use s2_formats::map::Map;
use s2_formats::rtx::Script;
use s2_sim::computer::ComputerPlayer;
use s2_sim::{
    Alliances, AnimationScript, BUILDING_TYPES, BugFixes, BuildingKind, Command, Controller, Game, Mission,
    Node, Objective, Player, PlayerSlot, PlayerSlots, Portraits, Setup, SimTables, StartGoods, StartOrder,
    map_path, script_path,
};
use serde_json::{Value, json};

/// Fields added after the first traces were made, left out where a trace lacks them.
const OPTIONAL: [&str; 2] = ["tools", "allies"];

/// Index fields hold this in the original for "none".
fn index(value: Option<u16>) -> Value {
    value.map_or(Value::Null, |index| json!(index))
}

fn player(game: &Game, ai: &ComputerPlayer) -> Value {
    let jobs: serde_json::Map<String, Value> = ai
        .jobs()
        .map(|(at, job)| {
            (
                at.to_string(),
                json!({
                    "flags": job.flags.bits(),
                    "kind": job.kind.index(),
                    "made": job.made,
                    "parent": index(job.parent),
                    "next": index(job.next),
                    "child": index(job.child),
                    "owner": job.owner.map_or(Value::Null, |task| json!(task.index())),
                    "handler": job.handler.map_or(Value::Null, |task| json!(task.index())),
                }),
            )
        })
        .collect();
    let sets = ai.tasks().military_sets();
    let handlers: Vec<Value> = (0..s2_sim::computer::JobKind::COUNT)
        .map(|kind| {
            let kind = s2_sim::computer::JobKind::ALL[kind];
            ai.handler(kind).map_or(Value::Null, |task| json!(task.index()))
        })
        .collect();
    json!({
        "roots": ai.roots(),
        "fresh": ai.fresh(),
        "running": ai.running(),
        "doomed": ai.doomed(),
        "cursor": ai.cursor(),
        "handlers": handlers,
        "jobs": jobs,
        "military_sets": {
            "special": sets.special,
            "wait": sets.wait,
            "pick": sets.pick,
            "set": sets.set,
            "calls": sets.calls,
        },
        "military": game.world().military_settings(ai.player()),
        "tools": game.world().distribution(ai.player()).tools(),
    })
}

fn snapshot(game: &Game) -> Value {
    let (a, b, carry) = game.generator().words();
    let players: serde_json::Map<String, Value> = Player::ALL
        .into_iter()
        .filter_map(|p| Some((p.index().to_string(), player(game, game.computers().get(p)?))))
        .collect();
    let grid = game.computers().grid();
    let mut cells = String::new();
    for byte in grid.cells().iter().flat_map(s2_sim::computer::Cell::to_bytes) {
        let _ = write!(cells, "{byte:02x}");
    }
    let (column, row) = grid.cursor();
    let world = game.world();
    let flags: serde_json::Map<String, Value> = world
        .flags()
        .map(|(at, flag)| {
            (
                at.to_string(),
                json!({ "node": [flag.node.x, flag.node.y], "owner": flag.owner.index() }),
            )
        })
        .collect();
    let roads: serde_json::Map<String, Value> = world
        .roads()
        .map(|(at, road)| {
            let path: Vec<Value> = road.path.iter().map(|node| json!([node.x, node.y])).collect();
            (
                at.to_string(),
                json!({
                    "carriers": road.carriers,
                    "served": road.served,
                    "order": index(road.order),
                    "start": road.start,
                    "end": index(road.end),
                    "path": path,
                }),
            )
        })
        .collect();
    let buildings: serde_json::Map<String, Value> = world
        .buildings()
        .filter(|(_, building)| !building.state.is_empty())
        .map(|(at, building)| {
            (
                at.to_string(),
                json!({
                    "type": building.kind.index(),
                    "flag": building.flag,
                    "owner": building.owner.index(),
                    "state": building.state.bits(),
                }),
            )
        })
        .collect();
    json!({
        "step": game.clock().step(),
        "generator": [a, b, u8::from(carry)],
        "grid": { "cursor": [column, row], "cells": cells },
        "world": { "flags": flags, "roads": roads, "buildings": buildings },
        "players": players,
    })
}

/// The slots of the original's alliance table, one more than the players: the eighth row and
/// column are never set.
const SLOTS: u8 = 8;

/// The alliance table as the original keeps it, a row for each slot naming the slots it counts
/// as allies.
fn allies(game: &Game) -> Value {
    let row = |player: u8| -> Vec<bool> {
        (0..SLOTS)
            .map(|other| match (Player::new(player), Player::new(other)) {
                (Some(player), Some(other)) => game.world().allied(player, other),
                _ => false,
            })
            .collect()
    };
    json!((0..SLOTS).map(row).collect::<Vec<_>>())
}

fn mission(mission: &Mission) -> Value {
    let records: serde_json::Map<String, Value> = mission
        .records()
        .map(|record| {
            (
                record.record.to_string(),
                json!({
                    "id": record.id,
                    "kind": record.kind,
                    "text": record.text,
                    "counter": record.counter,
                }),
            )
        })
        .collect();
    let first = Player::ALL[0];
    let allowed: Vec<bool> = (0..BUILDING_TYPES)
        .map(|kind| mission.allowed(first, BuildingKind(u8::try_from(kind).unwrap())))
        .collect();
    json!({
        "records": records,
        "active": mission.active_records(),
        "allowed": allowed,
        "won": mission.won(),
    })
}

/// The first place `ours` and `theirs` differ, as a path into the trace's fields.
fn first_difference(ours: &Value, theirs: &Value, path: &str) -> Option<String> {
    match (ours, theirs) {
        (Value::Object(a), Value::Object(b)) => {
            let keys: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            keys.into_iter().find_map(|key| {
                let here = format!("{path}.{key}");
                match (a.get(key), b.get(key)) {
                    (Some(x), Some(y)) => first_difference(x, y, &here),
                    (Some(_), None) if OPTIONAL.contains(&key.as_str()) => None,
                    (x, y) => Some(format!("{here}: ours {x:?}, the original's {y:?}")),
                }
            })
        }
        _ if ours == theirs => None,
        _ => Some(format!("{path}: ours {ours}, the original's {theirs}")),
    }
}

/// The map at `row` of Unlimited play's list, which sorts `DATA/MAPS3` by name.
fn map_at(data: &Path, row: usize) -> Map {
    let folder = data.join("DATA").join("MAPS3");
    let mut names: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("wld"))
        })
        .collect();
    names.sort_by_key(|path| path.file_name().map(std::ffi::OsStr::to_ascii_uppercase));
    Map::parse_wld(&std::fs::read(&names[row]).unwrap()).unwrap()
}

/// Campaign mission `number` as the probe starts it: seated as the campaign seats it, with the
/// computer in player 0's seat, and with the script at `script` in place of the chapter's own.
fn chapter(data: &Path, number: u16, script: Option<&Path>) -> Game {
    let map = Map::parse_wld(&std::fs::read(data.join(map_path(number))).unwrap()).unwrap();
    let script = script.map_or_else(|| data.join(script_path(number)), Path::to_path_buf);
    let script = Script::parse(&std::fs::read(script).unwrap()).unwrap();
    let mut slots = PlayerSlots::campaign(&map, &script);
    let first = Player::ALL[0];
    let portrait = slots.get(first).unwrap().portrait;
    slots.set(
        first,
        Some(PlayerSlot {
            controller: Controller::Computer,
            portrait,
        }),
    );
    let setup = Setup {
        mission: Some(number),
        slots: Some(slots),
        bug_fixes: BugFixes::Off,
        ..Setup::default()
    };
    Game::start_mission(&map, &setup, &script, &SimTables::ORIGINAL).unwrap()
}

#[test]
fn the_computer_players_follow_the_original_step_for_step() {
    let Some(trace) = std::env::var_os("S2_AI_TRACE") else {
        return;
    };
    let text = std::fs::read_to_string(trace).unwrap();
    let mut lines = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap());
    let header = lines.next().unwrap();
    let data = if header["continent"].is_string() {
        "S2_DATA_GOLD"
    } else {
        "S2_DATA_PLAIN"
    };
    let Some(data) = std::env::var_os(data) else {
        return;
    };
    let recorded = header["closed_letters"].as_bool().unwrap_or(false);
    let mut game = start(&header, Path::new(&data));
    let fields: Option<Vec<String>> = std::env::var("S2_AI_FIELDS")
        .ok()
        .map(|list| list.split(',').map(str::to_owned).collect());
    let only = |value: Value| match (&fields, value) {
        (Some(fields), Value::Object(all)) => {
            Value::Object(all.into_iter().filter(|(key, _)| fields.contains(key)).collect())
        }
        (_, value) => value,
    };
    for mut theirs in lines {
        let orders = theirs.as_object_mut().and_then(|line| line.remove("orders"));
        let closed: Vec<u32> = theirs
            .as_object_mut()
            .and_then(|line| line.remove("closed"))
            .and_then(|steps| serde_json::from_value(steps).ok())
            .unwrap_or_default();
        let step = u32::try_from(theirs["step"].as_u64().unwrap()).unwrap();
        loop {
            let now = game.clock().step();
            if closed.contains(&now) {
                game.execute(Player::ALL[0], Command::CloseLetter).unwrap();
            }
            if now >= step {
                break;
            }
            game.step();
            if !recorded && game.mission().and_then(Mission::letter).is_some() {
                game.execute(Player::ALL[0], Command::CloseLetter).unwrap();
            }
        }
        let mut ours = snapshot(&game);
        if let Some(mission) = game.mission() {
            ours["mission"] = self::mission(mission);
            ours["mission"]["allies"] = allies(&game);
        }
        if let Some(theirs) = theirs.get_mut("continent") {
            ours["continent"] = continent(&game);
            in_game(&game, &mut theirs["allies"]);
        }
        if let Some(difference) = first_difference(&only(ours), &only(theirs), "") {
            panic!("step {step}: {difference}");
        }
        for order in orders.as_ref().and_then(Value::as_array).into_iter().flatten() {
            give(&mut game, order, step);
        }
    }
}

/// Gives player 0 an order the probe gave the original after `step`, which found the flag
/// `order["flag"]` or none.
fn give(game: &mut Game, order: &Value, step: u32) {
    let (name, at) = if order["scout"].is_null() {
        ("geologist", &order["geologist"])
    } else {
        ("scout", &order["scout"])
    };
    let coordinate = |index: usize| u16::try_from(at[index].as_u64().unwrap()).unwrap();
    let node = Node {
        x: coordinate(0),
        y: coordinate(1),
    };
    let command = if name == "scout" {
        Command::SendScout { node }
    } else {
        Command::SendGeologist { node }
    };
    let given = game.execute(Player::ALL[0], command);
    assert_eq!(
        given.is_ok(),
        !order["flag"].is_null(),
        "step {step}: the {name} to {node:?}: ours {given:?}, the original's flag {}",
        order["flag"]
    );
}

/// The game the trace's header describes, set up as the probe started it in the original.
fn start(header: &Value, data: &Path) -> Game {
    let campaign = header["mission"]
        .as_u64()
        .map(|number| u16::try_from(number).unwrap());
    let script = header["script"].as_str().map(Path::new);
    let mut game = if let Some(number) = campaign {
        chapter(data, number, script)
    } else if let Some(map) = header["continent"].as_str() {
        world_campaign(data, map)
    } else {
        let row = usize::try_from(header["map_row"].as_u64().unwrap()).unwrap();
        Game::start(
            &map_at(data, row),
            &Setup {
                bug_fixes: BugFixes::Off,
                ..Setup::default()
            },
        )
        .unwrap()
    };
    let script = std::fs::read(data.join("DATA/ANIMDAT/ROM_ANIM.DAT")).unwrap();
    game.set_animations(AnimationScript::new(&RomAnim::parse(&script).unwrap()));
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

/// The World Campaign continent on the map at `map` in the Gold Edition's data, as the probe
/// starts it: seated and allied as the World Campaign seats it, with the computer in player 0's
/// seat.
fn world_campaign(data: &Path, map: &str) -> Game {
    let map = Map::parse_wld(&std::fs::read(data.join(map)).unwrap()).unwrap();
    let mut slots = PlayerSlots::world_campaign(&map);
    let first = Player::ALL[0];
    let portrait = slots.get(first).unwrap().portrait;
    slots.set(
        first,
        Some(PlayerSlot {
            controller: Controller::Computer,
            portrait,
        }),
    );
    let setup = Setup {
        players: None,
        animals: true,
        goods: StartGoods::Normal,
        slots: Some(slots),
        starts: StartOrder::InSequence,
        objective: Objective::Headquarters,
        bug_fixes: BugFixes::Off,
        mission: None,
    };
    let mut game = Game::start(&map, &setup).unwrap();
    game.set_alliances(Alliances::AgainstFirst);
    game
}

/// A continent's state as the probe reads it: whether a player has won, the game decided, and
/// the alliances between the players in the game.
fn continent(game: &Game) -> Value {
    let won = game.winner().is_some();
    let mut allies = allies(game);
    in_game(game, &mut allies);
    json!({ "won": won, "decided": won, "allies": allies })
}

/// Keeps the rows and columns of an alliance table for the players in the game. The original
/// writes all eight rows as a continent starts and Robigo keeps only those, which nothing reads
/// past.
fn in_game(game: &Game, table: &mut Value) {
    let playing: Vec<usize> = game.world().slots().players().map(Player::index).collect();
    let rows = table.as_array().cloned().unwrap_or_default();
    *table = json!(
        playing
            .iter()
            .map(|&row| playing
                .iter()
                .map(|&column| rows[row][column].clone())
                .collect::<Vec<_>>())
            .collect::<Vec<_>>()
    );
}
