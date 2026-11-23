//! Runs every map in a player's copy of the game for a few minutes of game time, and checks the
//! building sites each file holds against the game's rules. Runs when `S2_DATA_GOLD` or
//! `S2_DATA_PLAIN` names the game's folder. Only counts and hashes are asserted, so no game data
//! lives here.

use std::path::PathBuf;

use s2_data::DataSet;
use s2_formats::map::Map;
use s2_sim::{Game, GameRng, Node, NodeFlags, NodeMap, Setup, World, site};

/// About four and a half minutes at normal speed.
const STEPS: u32 = 2_000;

fn maps(var: &str) -> Option<(DataSet, Vec<String>)> {
    let Some(path) = std::env::var_os(var).map(PathBuf::from) else {
        eprintln!("skipped: set {var} to run this test");
        return None;
    };
    let data = DataSet::discover(path).unwrap();
    let mut paths = Vec::new();
    for folder in data.list_dir("DATA").unwrap() {
        if !folder.name.to_ascii_uppercase().starts_with("MAPS") {
            continue;
        }
        for file in data.list_dir(&format!("DATA/{}", folder.name)).unwrap() {
            let map = std::path::Path::new(&file.name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("wld") || e.eq_ignore_ascii_case("swd"));
            if map {
                paths.push(format!("DATA/{}/{}", folder.name, file.name));
            }
        }
    }
    Some((data, paths))
}

fn run_every_map(var: &str) {
    let Some((data, paths)) = maps(var) else { return };
    assert!(!paths.is_empty(), "no paths found");
    for path in &paths {
        let map = Map::parse_wld(&data.read(path).unwrap()).unwrap();
        let mut game = Game::start(&map, &Setup::default()).unwrap();
        let before = game.world().figures().count();
        let mut again = game.clone();
        for _ in 0..STEPS {
            game.step();
        }
        for _ in 0..STEPS {
            again.step();
        }
        assert_eq!(game.state_hash(), again.state_hash(), "{path}: two runs differ");
        eprintln!(
            "{path}: {before} animals at the start, {} after {STEPS} steps",
            game.world().figures().count()
        );
    }
}

#[test]
fn gold_maps_run() {
    run_every_map("S2_DATA_GOLD");
}

#[test]
fn plain_maps_run() {
    run_every_map("S2_DATA_PLAIN");
}

/// Every shipped map holds slot 8 as the game's rules give it, with objects as the loader leaves
/// them (start markers cleared). The exception is nine nodes of `OMAP02` that carry the
/// border-node bit, which no other map has. There the file holds what the rules give without
/// that bit.
fn sites_match_the_files(var: &str) {
    let Some((data, paths)) = maps(var) else { return };
    let mut stale = 0;
    for path in &paths {
        let map = Map::parse_wld(&data.read(path).unwrap()).unwrap();
        let mut nodes = NodeMap::from_map(&map).unwrap();
        let world = World::from_map(&map, &mut GameRng::at_start());
        for y in 0..map.height() {
            for x in 0..map.width() {
                let node = Node { x, y };
                let stored = nodes.get(node);
                if site(&nodes, &world, node) == stored.sites {
                    continue;
                }
                stale += 1;
                assert!(path.ends_with("OMAP02.WLD"), "{path} {node:?}");
                assert!(stored.flags.contains(NodeFlags::BORDER), "{path} {node:?}");
                let mut cleared = stored;
                cleared.flags -= NodeFlags::BORDER;
                nodes.set(node, cleared);
                assert_eq!(site(&nodes, &world, node), stored.sites, "{path} {node:?}");
                nodes.set(node, stored);
            }
        }
    }
    assert_eq!(stale, 9);
}

#[test]
fn gold_sites_match_the_files() {
    sites_match_the_files("S2_DATA_GOLD");
}

#[test]
fn plain_sites_match_the_files() {
    sites_match_the_files("S2_DATA_PLAIN");
}
