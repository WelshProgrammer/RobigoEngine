//! Plays every replay in `tests/replays/` and checks the state hashes each recorded. A replay on
//! a map in the folder with no animation script runs everywhere, CI included. One that needs the
//! game's data (a `map-data` map, a `script` hash or a campaign mission's script) runs only when
//! `S2_DATA_PLAIN` names it.
//!
//! After a deliberate change to the simulation, `ROBIGO_BLESS=1 cargo test -p s2-sim --test
//! replays` rewrites each replay's checks from the new behaviour, a check every 250 steps.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use s2_data::DataSet;
use s2_formats::bob::RomAnim;
use s2_formats::map::Map;
use s2_formats::rtx::Script;
use s2_sim::replay::{MapSource, Replay, content_hash};
use s2_sim::save::{FORMAT, Restored, Save};
use s2_sim::{AnimationScript, Game, SimTables, script_path};

/// The animation script's path in the game's data.
const ROM_ANIM: &str = "DATA/ANIMDAT/ROM_ANIM.DAT";

/// Steps between the checks a blessed replay records.
const CHECK_EVERY: u32 = 250;

fn folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/replays")
}

/// The replays in the folder, by file name.
fn replays() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(folder())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "replay"))
        .collect();
    paths.sort();
    paths
}

fn data() -> Option<DataSet> {
    std::env::var_os("S2_DATA_PLAIN").map(|path| DataSet::discover(PathBuf::from(path)).unwrap())
}

/// The game `replay` starts with, or `None` when it needs data that is not here.
fn start(path: &Path, replay: &Replay) -> Option<Game> {
    let data = data();
    let bytes = match &replay.map {
        MapSource::File(name) => std::fs::read(folder().join(name)).unwrap(),
        MapSource::Data(name) => data.as_ref()?.read(name).unwrap(),
    };
    assert_eq!(
        content_hash(&bytes),
        replay.map_hash,
        "{}: the map has changed",
        path.display()
    );
    let map = Map::parse_wld(&bytes).unwrap();
    let mission = match replay.setup.mission {
        Some(number) => {
            let bytes = data.as_ref()?.read(&script_path(number)).unwrap();
            if let Some(hash) = replay.mission_script {
                assert_eq!(
                    content_hash(&bytes),
                    hash,
                    "{}: a different mission script",
                    path.display()
                );
            }
            Script::parse(&bytes).unwrap()
        }
        None => Script::default(),
    };
    let mut game = Game::start_mission(&map, &replay.setup, &mission, &SimTables::ORIGINAL).unwrap();
    if let Some(hash) = replay.script {
        let script = data.as_ref()?.read(ROM_ANIM).unwrap();
        assert_eq!(
            content_hash(&script),
            hash,
            "{}: a different ROM_ANIM.DAT",
            path.display()
        );
        game.set_animations(AnimationScript::new(&RomAnim::parse(&script).unwrap()));
    }
    replay.apply_players(&mut game);
    Some(game)
}

#[test]
fn replays_reach_their_recorded_states() {
    let bless = std::env::var_os("ROBIGO_BLESS").is_some();
    let paths = replays();
    assert!(!paths.is_empty(), "no replays in {}", folder().display());
    std::thread::scope(|scope| {
        for path in &paths {
            scope.spawn(move || reaches_its_states(path, bless));
        }
    });
}

/// Plays the replay at `path` against its checks, or records them when `bless` is set.
fn reaches_its_states(path: &Path, bless: bool) {
    let text = std::fs::read_to_string(path).unwrap();
    let mut replay = Replay::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let Some(mut game) = start(path, &replay) else {
        eprintln!("{}: skipped, set S2_DATA_PLAIN to run it", path.display());
        return;
    };
    if bless {
        replay.record_checks(&mut game, CHECK_EVERY);
        std::fs::write(path, bless_text(&text, &replay)).unwrap();
        eprintln!("{}: blessed {} checks", path.display(), replay.checks.len());
        return;
    }
    assert!(
        !replay.checks.is_empty(),
        "{}: no checks; bless it",
        path.display()
    );
    if let Err(mismatch) = replay.play(&mut game) {
        panic!("{}: {mismatch}", path.display());
    }
    let broken = game.world().broken_links();
    assert!(broken.is_empty(), "{}: {broken:#?}", path.display());
    saves_restore(path, &replay, &game);
}

/// Saves `game`, at the end of `replay`, and checks that the save comes back to the same state
/// from its state and from its replay.
fn saves_restore(path: &Path, replay: &Replay, game: &Game) {
    let bytes = Save::new(game, replay, None).unwrap().to_bytes().unwrap();
    eprintln!("{}: a save of {} bytes", path.display(), bytes.len());
    let bytes = &bytes;
    std::thread::scope(|scope| {
        for (format, expected) in [(FORMAT, Restored::State), (FORMAT + 1, Restored::Replayed)] {
            scope.spawn(move || {
                let mut save = Save::from_bytes(bytes).unwrap();
                save.format = format;
                let (restored, how) = save.restore(start(path, replay).unwrap());
                assert_eq!(how, expected, "{}", path.display());
                assert_eq!(
                    restored.state_hash(),
                    game.state_hash(),
                    "{}: {how:?}",
                    path.display()
                );
            });
        }
    });
}

/// `text` with its check lines replaced by `replay`'s, after the `end` line, keeping the
/// comments and command lines as written.
fn bless_text(text: &str, replay: &Replay) -> String {
    let mut out = String::new();
    for line in text.lines() {
        if line.trim_start().starts_with("check ") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
        if line.trim_start().starts_with("end ") {
            for (step, hash) in &replay.checks {
                let _ = writeln!(out, "check {step} {hash:016x}");
            }
        }
    }
    out
}
