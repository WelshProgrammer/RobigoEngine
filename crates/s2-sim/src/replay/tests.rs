use super::*;
use crate::objectives::Objective;
use crate::players::{Controller, PlayerSlot, PlayerSlots, Portrait, StartOrder};
use crate::settlers::tests::{HQ_FLAG, game, player};

fn sample() -> Replay {
    let mut replay = Replay::new(
        MapSource::File("meadow.wld".into()),
        81_985_529_216_486_895,
        None,
        Setup {
            players: Some(1),
            animals: false,
            goods: StartGoods::High,
            slots: None,
            starts: StartOrder::InSequence,
            objective: Objective::None,
            bug_fixes: BugFixes::On,
            mission: None,
        },
    );
    replay.waypoints = vec![0, 2];
    replay.end = 400;
    let node = HQ_FLAG;
    replay.commands = [
        Command::StartRoad { node, water: false },
        Command::ExtendRoad {
            direction: Direction::East,
        },
        Command::ExtendRoad {
            direction: Direction::East,
        },
        Command::PlaceFlag {
            node: Node {
                x: node.x + 2,
                y: node.y,
            },
            water: false,
        },
    ]
    .into_iter()
    .enumerate()
    .map(|(i, command)| Stamped {
        step: 10 + u32::try_from(i).unwrap(),
        player: player(),
        command,
    })
    .collect();
    replay
}

#[test]
fn a_written_replay_reads_back_the_same() {
    let mut replay = sample();
    replay.checks = vec![(100, 1), (400, u64::MAX)];
    let node = Node { x: 12, y: 40 };
    let every = [
        Command::PlaceFlag { node, water: true },
        Command::RemoveFlag { node },
        Command::StartRoad { node, water: true },
        Command::CancelRoad,
        Command::RemoveRoad { node },
        Command::PlaceBuilding {
            node,
            kind: BuildingKind::FORTRESS,
        },
        Command::Demolish { node },
        Command::SetStock {
            node,
            item: StockItem::Ware(WareKind::COINS),
            setting: StockSetting::SEND,
            on: true,
        },
        Command::SetStock {
            node,
            item: StockItem::Job(Job::MINER),
            setting: StockSetting::STOP,
            on: false,
        },
        Command::SetAllStock {
            node,
            people: true,
            setting: StockSetting::STOP,
            on: true,
        },
        Command::SetAllStock {
            node,
            people: false,
            setting: StockSetting::SEND,
            on: false,
        },
        Command::StopProduction { node, stop: true },
        Command::StopProduction { node, stop: false },
        Command::ShipyardMode { node, ships: true },
        Command::ShipyardMode { node, ships: false },
        Command::SetDistribution {
            group: 5,
            consumer: 3,
            weight: 90,
        },
        Command::ResetDistribution { group: 2 },
        Command::SetToolWeight {
            tool: WareKind::SCYTHE,
            weight: 60,
        },
        Command::ResetToolWeights,
        Command::MoveTransport { group: 11, rank: 4 },
        Command::ResetTransport,
        Command::SendGeologist { node },
        Command::SendScout { node },
        Command::SetMilitary {
            setting: 4,
            value: 60,
        },
        Command::ResetMilitary,
        Command::Attack {
            node,
            soldiers: 3,
            strongest: false,
        },
        Command::PrepareExpedition { node },
        Command::SailExpedition { ship: 4, sector: 5 },
        Command::FoundHarbour { ship: 4 },
        Command::CancelExpedition { ship: 4 },
        Command::RouteRoad { node },
    ]
    .into_iter()
    .chain(Direction::ALL.map(|direction| Command::ExtendRoad { direction }));
    replay.commands.extend(every.map(|command| Stamped {
        step: 300,
        player: Player::new(2).unwrap(),
        command,
    }));
    assert_eq!(Replay::parse(&replay.to_text()).unwrap(), replay);
    replay.map = MapSource::Data("DATA/MAPS3/OMAP00.WLD".into());
    replay.script = Some(7);
    replay.setup = Setup::default();
    replay.waypoints.clear();
    assert_eq!(Replay::parse(&replay.to_text()).unwrap(), replay);
}

#[test]
fn bad_lines_name_their_line() {
    let head = "map-file a.wld\nmap-hash 1\n";
    let error = Replay::parse(&format!("{head}# fine\n\n5 0 flag 1 2\n6 0 flag 1\n")).unwrap_err();
    assert_eq!(error.line, 6);
    assert!(
        Replay::parse(&format!("{head}1 9 cancel")).is_err(),
        "no player 9"
    );
    assert!(Replay::parse(&format!("{head}1 0 step up")).is_err());
    assert!(Replay::parse(&format!("{head}1 0 dance")).is_err());
    assert!(Replay::parse(&format!("{head}goods plenty")).is_err());
    assert!(Replay::parse(&format!("{head}weather rain")).is_err());
    assert_eq!(Replay::parse("map-hash 1\n").unwrap_err().line, 0, "no map");
    assert_eq!(Replay::parse("map-file a.wld\n").unwrap_err().line, 0, "no hash");
}

#[test]
fn the_end_defaults_to_the_last_command() {
    let replay = Replay::parse("map-file a.wld\nmap-hash 1\n20 0 cancel\n5 0 cancel\n").unwrap();
    assert_eq!(replay.end, 20);
    assert_eq!(replay.commands[0].step, 5, "sorted");
}

#[test]
fn recorded_checks_pass_and_a_changed_one_fails() {
    let mut replay = sample();
    replay.record_checks(&mut game(), 100);
    assert_eq!(
        replay.checks.iter().map(|&(step, _)| step).collect::<Vec<_>>(),
        [100, 200, 300, 400]
    );
    assert_eq!(replay.play(&mut game()), Ok(()));

    let mut changed = replay.clone();
    changed.checks[2].1 ^= 1;
    let error = changed.play(&mut game()).unwrap_err();
    assert_eq!(error.step, 300);
    assert_eq!(error.expected, replay.checks[2].1 ^ 1);
}

#[test]
fn the_commands_change_the_hashes() {
    let mut replay = sample();
    replay.record_checks(&mut game(), 100);
    let mut idle = replay.clone();
    idle.commands.clear();
    idle.record_checks(&mut game(), 100);
    assert_ne!(replay.checks, idle.checks);
}

#[test]
fn the_mods_line_reads_back_and_is_optional() {
    let mut replay = sample();
    assert_eq!(Replay::parse(&replay.to_text()).unwrap().mods, None);
    replay.mods = Some(vec!["base".into(), "gold".into()]);
    let text = replay.to_text();
    assert!(text.contains("\nmods base gold\n"), "{text}");
    assert_eq!(Replay::parse(&text).unwrap().mods, replay.mods);
    replay.mods = Some(Vec::new());
    assert_eq!(Replay::parse(&replay.to_text()).unwrap().mods, Some(Vec::new()));
}

#[test]
fn the_tables_line_reads_back_and_is_optional() {
    let mut replay = sample();
    assert_eq!(Replay::parse(&replay.to_text()).unwrap().tables, None);
    replay.tables = Some(1_234_567_890);
    let text = replay.to_text();
    assert!(text.contains("\ntables 00000000499602d2\n"), "{text}");
    assert_eq!(Replay::parse(&text).unwrap().tables, replay.tables);
}

#[test]
fn a_game_takes_its_waypoint_sets_from_its_tables() {
    let mut tables = crate::SimTables::ORIGINAL;
    tables.waypoints.swap(0, 3);
    let mut game = game();
    game.world.set_tables(&tables);
    let mut players = sample();
    players.waypoints = vec![0];
    players.apply_players(&mut game);
    assert_eq!(
        game.world().waypoints(crate::world::Player::new(0).unwrap()),
        &crate::WaypointSet::ORIGINAL[3]
    );
}

#[test]
fn the_bug_fixes_read_back_and_a_replay_without_them_plays_with_them_off() {
    let mut replay = sample();
    let text = replay.to_text();
    assert!(text.contains("\nbug-fixes on\n"), "{text}");
    assert_eq!(Replay::parse(&text).unwrap().setup.bug_fixes, BugFixes::On);
    replay.setup.bug_fixes = BugFixes::Off;
    let text = replay.to_text();
    assert_eq!(Replay::parse(&text).unwrap().setup.bug_fixes, BugFixes::Off);
    let without = text.replace("bug-fixes off\n", "");
    assert_eq!(Replay::parse(&without).unwrap().setup.bug_fixes, BugFixes::Off);
    assert!(Replay::parse(&text.replace("bug-fixes off", "bug-fixes maybe")).is_err());
}

#[test]
fn a_campaign_replay_keeps_its_mission_and_its_closed_letters() {
    let mut replay = sample();
    assert!(!replay.to_text().contains("mission"));
    replay.setup.mission = Some(3);
    replay.commands.push(Stamped {
        step: 20,
        player: player(),
        command: Command::CloseLetter,
    });
    let text = replay.to_text();
    assert!(text.contains("\nmission 3\n"), "{text}");
    assert!(text.contains("\n20 0 letter-close\n"), "{text}");
    let parsed = Replay::parse(&text).unwrap();
    assert_eq!(parsed.setup.mission, Some(3));
    assert_eq!(parsed.mission_script, None, "no hash: any script");
    assert_eq!(parsed.commands, replay.commands);
    replay.mission_script = Some(1_234_567_890);
    let text = replay.to_text();
    assert!(text.contains("\nmission 3 00000000499602d2\n"), "{text}");
    assert_eq!(Replay::parse(&text).unwrap(), replay);
    let bad = text.replace("mission 3 00000000499602d2", "mission 3 script");
    assert!(Replay::parse(&bad).is_err());
}

fn seat(controller: Controller, face: u8) -> PlayerSlot {
    PlayerSlot {
        controller,
        portrait: Portrait(face),
    }
}

#[test]
fn the_slots_and_the_alliances_read_back_and_are_optional() {
    let mut replay = sample();
    let parsed = Replay::parse(&replay.to_text()).unwrap();
    assert_eq!(
        parsed.setup.slots, None,
        "no slot lines: what the setup screen offers"
    );
    assert_eq!(parsed.alliances, Alliances::None);
    let mut slots = PlayerSlots::EMPTY;
    slots.set(player(), Some(seat(Controller::First, 0)));
    slots.set(Player::new(2).unwrap(), Some(seat(Controller::Computer, 11)));
    replay.setup.slots = Some(slots);
    replay.alliances = Alliances::Controllers;
    let text = replay.to_text();
    assert!(
        text.contains("\nslot 0 first 0\nslot 2 computer 11\nalliances controllers\n"),
        "{text}"
    );
    let parsed = Replay::parse(&text).unwrap();
    assert_eq!(parsed.setup.slots, Some(slots));
    assert_eq!(parsed.alliances, Alliances::Controllers);
    for bad in [
        "slot 7 first 0",
        "slot 0 nobody 0",
        "slot 0 first 256",
        "alliances all",
    ] {
        let text = format!("map-file a.wld\nmap-hash 1\n{bad}\n");
        assert!(Replay::parse(&text).is_err(), "{bad}");
    }
}

#[test]
fn the_alliances_are_set_once_the_nations_are() {
    let mut game = game();
    let second = Player::new(1).unwrap();
    game.world
        .slots
        .set(player(), Some(seat(Controller::Computer, 0)));
    game.world.slots.set(second, Some(seat(Controller::Computer, 4)));
    let mut players = sample();
    players.nations = vec![1, 1];
    players.alliances = Alliances::Nations;
    players.apply_players(&mut game);
    assert!(game.world().allied(player(), second));
    assert!(game.world().allied(second, player()));
}

#[test]
fn the_start_order_the_objective_and_exploration_read_back_and_are_optional() {
    let mut replay = sample();
    let text = replay.to_text();
    assert!(!text.contains("starts") && !text.contains("objective"), "{text}");
    let parsed = Replay::parse(&text).unwrap();
    assert_eq!(parsed.setup.starts, StartOrder::InSequence);
    assert_eq!(parsed.setup.objective, Objective::None);
    assert!(parsed.exploration);
    replay.setup.starts = StartOrder::Random;
    replay.setup.objective = Objective::Domination;
    replay.exploration = false;
    let text = replay.to_text();
    assert!(text.contains("\nstarts random\nobjective domination\n"), "{text}");
    assert!(text.contains("\nexploration off\n"), "{text}");
    let parsed = Replay::parse(&text).unwrap();
    assert_eq!(parsed.setup, replay.setup);
    assert!(!parsed.exploration);
    for bad in ["starts shuffled", "objective all", "exploration no"] {
        let text = format!("map-file a.wld\nmap-hash 1\n{bad}\n");
        assert!(Replay::parse(&text).is_err(), "{bad}");
    }
}
