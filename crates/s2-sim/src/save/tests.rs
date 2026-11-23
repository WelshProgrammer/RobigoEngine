use super::{FORMAT, Restored, Save, SaveError};
use crate::commands::{Command, Stamped};
use crate::game::{Game, Setup};
use crate::grid::{Direction, Node};
use crate::replay::{MapSource, Playback, Replay};
use crate::settlers::tests::{HQ_FLAG, game, player};
use crate::world::BuildingKind;

/// A replay that builds a road east of the headquarters and a woodcutter at its end, with
/// one more flag placed at step 300.
fn replay(map: MapSource) -> Replay {
    let mut replay = Replay::new(map, 0, None, Setup::default());
    let at = |step, command| Stamped {
        step,
        player: player(),
        command,
    };
    let end = Node {
        x: HQ_FLAG.x + 4,
        y: HQ_FLAG.y,
    };
    replay.commands.push(at(
        0,
        Command::StartRoad {
            node: HQ_FLAG,
            water: false,
        },
    ));
    for _ in 0..4 {
        replay.commands.push(at(
            0,
            Command::ExtendRoad {
                direction: Direction::East,
            },
        ));
    }
    replay.commands.push(at(
        0,
        Command::PlaceBuilding {
            node: Node {
                x: end.x - 1,
                y: end.y - 1,
            },
            kind: BuildingKind::WOODCUTTER,
        },
    ));
    replay.commands.push(at(
        300,
        Command::PlaceFlag {
            node: Node {
                x: end.x + 4,
                y: end.y,
            },
            water: false,
        },
    ));
    replay.end = 600;
    replay
}

/// `replay` played on a fresh game to `steps`, with the commands stamped at that step given.
fn played(replay: &Replay, steps: u32) -> Game {
    let mut game = game();
    let mut playback = Playback::new(replay);
    while game.clock().step() < steps {
        playback.catch_up(&mut game);
        game.step();
    }
    playback.catch_up(&mut game);
    game
}

#[test]
fn a_save_reads_back_as_written() {
    let replay = replay(MapSource::File("meadow.wld".into()));
    let game = played(&replay, 450);
    let save = Save::new(&game, &replay, Some(b"map bytes")).unwrap();
    assert_eq!(save.replay.end, 450);
    assert_eq!(save.replay.checks.last(), Some(&(450, game.state_hash())));
    let bytes = save.to_bytes().unwrap();
    assert_eq!(Save::from_bytes(&bytes).unwrap(), save);
}

#[test]
fn the_building_types_read_back_from_a_save_of_this_format_only() {
    let replay = replay(MapSource::Data("DATA/MAPS/MAP00.WLD".into()));
    let game = played(&replay, 10);
    let mut save = Save::from_bytes(&Save::new(&game, &replay, None).unwrap().to_bytes().unwrap()).unwrap();
    assert_eq!(
        save.building_types().as_ref(),
        Some(game.world().building_types())
    );
    save.format = FORMAT + 1;
    assert_eq!(save.building_types(), None);
    save.format = FORMAT;
    save.building_types = None;
    assert_eq!(save.building_types(), None, "a save without them");
}

#[test]
fn the_desktop_entry_reads_back_untouched() {
    let replay = replay(MapSource::Data("DATA/MAPS/MAP00.WLD".into()));
    let mut save = Save::new(&played(&replay, 10), &replay, None).unwrap();
    assert_eq!(Save::from_bytes(&save.to_bytes().unwrap()).unwrap().desktop, None);
    save.desktop = Some(b"{\"task\":3}".to_vec());
    assert_eq!(Save::from_bytes(&save.to_bytes().unwrap()).unwrap(), save);
}

#[test]
fn a_map_from_the_data_is_not_kept() {
    let replay = replay(MapSource::Data("DATA/MAPS/MAP00.WLD".into()));
    let save = Save::new(&played(&replay, 10), &replay, Some(b"map bytes")).unwrap();
    assert_eq!(save.map, None);
}

#[test]
fn the_state_restores_and_plays_on_as_the_original() {
    let replay = replay(MapSource::File("meadow.wld".into()));
    let mut game = played(&replay, 200);
    let save = Save::from_bytes(&Save::new(&game, &replay, None).unwrap().to_bytes().unwrap()).unwrap();
    let (mut restored, how) = save.restore(self::game());
    assert_eq!(how, Restored::State);
    assert_eq!(restored, game);
    let mut playback = Playback::new(&replay);
    playback.catch_up(&mut restored);
    let mut original = Playback::new(&replay);
    original.catch_up(&mut game);
    for _ in 200..600 {
        playback.catch_up(&mut restored);
        original.catch_up(&mut game);
        restored.step();
        game.step();
    }
    assert_eq!(restored.state_hash(), game.state_hash());
}

#[test]
fn another_format_is_replayed_to_the_same_state() {
    let replay = replay(MapSource::File("meadow.wld".into()));
    let game = played(&replay, 300);
    let mut save = Save::new(&game, &replay, None).unwrap();
    save.format = FORMAT + 1;
    let (restored, how) = save.restore(self::game());
    assert_eq!(how, Restored::Replayed);
    assert_eq!(restored.state_hash(), game.state_hash());
}

#[test]
fn a_damaged_state_is_replayed() {
    let replay = replay(MapSource::File("meadow.wld".into()));
    let game = played(&replay, 120);
    let mut save = Save::new(&game, &replay, None).unwrap();
    let middle = save.state.len() / 2;
    save.state[middle] ^= 1;
    let (restored, how) = save.restore(self::game());
    assert_eq!(how, Restored::Replayed);
    assert_eq!(restored.state_hash(), game.state_hash());
}

#[test]
fn other_bytes_are_refused() {
    assert!(matches!(Save::from_bytes(b"not a save"), Err(SaveError::Io(_))));
}
