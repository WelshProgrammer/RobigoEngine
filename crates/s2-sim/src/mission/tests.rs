use s2_formats::map::Map;
use s2_formats::rtx::Script;

use super::*;
use crate::commands::{Command, Refusal};
use crate::grid::Node;
use crate::nodes::SiteSize;
use crate::players::Controller;
use crate::players::PlayerSlots;
use crate::world::MapObject;
use crate::world::tests::wld;
use crate::{Setup, SimTables};

const SIDE: u16 = 64;

/// A `SIDE` × `SIDE` map whose every node takes a large building, with a header counting
/// `count` players and holding `starts`.
fn map(count: u8, starts: &[(u16, u16)]) -> Map {
    let nodes = usize::from(SIDE) * usize::from(SIDE);
    let mut bytes = wld(SIDE, SIDE, &[(8, vec![SiteSize::Large.field(); nodes])], &[]);
    let header = b"WORLD_V1.0".len();
    bytes[header + 25] = count;
    for (index, &(x, y)) in starts.iter().enumerate() {
        bytes[header + 46 + 2 * index..][..2].copy_from_slice(&x.to_le_bytes());
        bytes[header + 60 + 2 * index..][..2].copy_from_slice(&y.to_le_bytes());
    }
    Map::parse_wld(&bytes).unwrap()
}

/// A script of `[GLOBAL_MAP_SETTINGS]` holding `settings` and `[MAPCOMMANDS]` holding
/// `commands`, one per line.
fn script(settings: &[&str], commands: &[&str]) -> Script {
    let mut text = String::from("[GLOBAL_MAP_SETTINGS]\n");
    for line in settings {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str("[MAPCOMMANDS]\n");
    for line in commands {
        text.push_str(line);
        text.push('\n');
    }
    Script::parse(text.as_bytes()).unwrap()
}

/// Mission 0 on a map with player 0 at (16, 16) and player 1 at (48, 48).
fn chapter(settings: &[&str], commands: &[&str]) -> Game {
    let setup = Setup {
        mission: Some(0),
        ..Setup::default()
    };
    Game::start_mission(
        &map(2, &[(16, 16), (48, 48)]),
        &setup,
        &script(settings, commands),
        &SimTables::ORIGINAL,
    )
    .unwrap()
}

fn first() -> Player {
    Player::ALL[0]
}

fn mission(game: &Game) -> &Mission {
    game.mission().unwrap()
}

fn run(game: &mut Game, steps: u32) {
    for _ in 0..steps {
        game.step();
    }
}

/// The first check runs on step 13, when the cycle counter has reached 14.
const FIRST_CHECK: u32 = 13;

#[test]
fn a_campaign_seats_the_header_players_for_the_computer_and_player_0_for_the_person() {
    let script = script(&["!GLOBAL_ADD_COMPUTER_PLAYER 4 9"], &[]);
    let slots = PlayerSlots::campaign(&map(3, &[(16, 16)]), &script);
    let seat = |index: u8| {
        slots
            .get(Player::new(index).unwrap())
            .map(|slot| (slot.controller, slot.portrait.0))
    };
    assert_eq!(seat(0), Some((Controller::First, 0)));
    assert_eq!(
        seat(1),
        Some((Controller::Computer, 1)),
        "in the game with no start position"
    );
    assert_eq!(seat(2), Some((Controller::Computer, 2)));
    assert_eq!(seat(3), None);
    assert_eq!(seat(4), Some((Controller::Computer, 9)));
}

#[test]
fn each_header_start_gets_an_empty_headquarters() {
    let game = chapter(&[], &[]);
    let headquarters: Vec<_> = game
        .world()
        .buildings()
        .map(|(_, building)| (building.owner.index(), building.node))
        .collect();
    assert_eq!(
        headquarters,
        [(0, Node { x: 16, y: 16 }), (1, Node { x: 48, y: 48 })]
    );
    for (_, store) in game.world().storehouses() {
        assert!(store.wares.iter().chain(&store.jobs).all(|&count| count == 0));
    }
}

#[test]
fn a_script_headquarters_inside_another_ones_land_is_not_placed() {
    let game = chapter(
        &[],
        &["!SET_ACT_PLAYER 0", "!SET_HOUSE 24 17 16", "!SET_HOUSE 24 30 30"],
    );
    let nodes: Vec<_> = game
        .world()
        .buildings()
        .map(|(_, building)| building.node)
        .collect();
    assert_eq!(
        nodes,
        [
            Node { x: 16, y: 16 },
            Node { x: 48, y: 48 },
            Node { x: 30, y: 30 }
        ]
    );
}

#[test]
fn wares_and_people_go_into_the_acting_players_headquarters_and_its_counts() {
    let game = chapter(&[], &["!SET_ACT_PLAYER 1", "!ADD_WARE 3 7", "!ADD_PEOPLE 1 5"]);
    let player = Player::new(1).unwrap();
    let (_, store) = game
        .world()
        .storehouses()
        .find(|(_, store)| game.world().building(store.building).unwrap().owner == player)
        .unwrap();
    assert_eq!((store.wares[3], store.jobs[1]), (7, 5));
    assert_eq!(game.world().owned_wares(player, WareKind(3)), 7);
    assert_eq!(game.world().people(player, Job(1)), 5);
    assert_eq!(
        game.world().ware_arrivals(player, WareKind(3)),
        0,
        "not counted as production"
    );
}

#[test]
fn only_allowed_types_can_be_built() {
    let mut game = chapter(&[], &["!ENABLE_HOUSE 0", "!ENABLE_HOUSE 1", "!DISABLE_HOUSE 1"]);
    let player = Player::new(1).unwrap();
    assert!(
        game.may_build(player, BuildingKind::WOODCUTTER),
        "allowed for every player"
    );
    assert!(!game.may_build(first(), BuildingKind::FORESTER));
    let refused = game.execute(
        first(),
        Command::PlaceBuilding {
            node: Node { x: 20, y: 20 },
            kind: BuildingKind::FORESTER,
        },
    );
    assert_eq!(refused, Err(Refusal::NotOffered));
}

#[test]
fn an_animal_line_leaves_the_active_list_alone() {
    let game = chapter(
        &[],
        &[
            "!MET_DIRECT_EVENT 1 255 -1 -1",
            "!MET_ADD_AS_ACT_EVENT 1",
            "!ADD_ANIMAL 2 3 20 20",
        ],
    );
    assert_eq!(mission(&game).active().next(), Some(Some(1)));
    assert_eq!(game.world().figures_at(Node { x: 20, y: 20 }).count(), 3);
}

#[test]
fn the_check_runs_every_20_steps_from_step_13() {
    let mut game = chapter(&[], &["!MET_DIRECT_EVENT 1 255 -1 -1", "!MET_ADD_AS_ACT_EVENT 1"]);
    run(&mut game, FIRST_CHECK - 1);
    assert!(mission(&game).waiting(1));
    run(&mut game, 1);
    assert!(!mission(&game).waiting(1), "fired and freed");
}

#[test]
fn an_event_with_a_letter_waits_while_a_letter_is_open() {
    let mut game = chapter(
        &[],
        &[
            "!MET_DIRECT_EVENT 1 255 10 11",
            "!MET_DIRECT_EVENT 2 255 12 -1",
            "!MET_DIRECT_EVENT 3 255 -1 -1",
            "!MET_ADD_AS_ACT_EVENT 1",
            "!MET_ADD_AS_ACT_EVENT 2",
            "!MET_ADD_AS_ACT_EVENT 3",
        ],
    );
    run(&mut game, FIRST_CHECK);
    let letter = mission(&game).letter();
    assert_eq!(
        letter,
        Some(Letter {
            text: 10,
            page: Some(11),
            closing: false
        })
    );
    assert!(mission(&game).waiting(2), "held back by the open letter");
    assert!(!mission(&game).waiting(3), "no letter, so not held back");
    assert_eq!(
        game.execute(Player::new(1).unwrap(), Command::CloseLetter),
        Err(Refusal::NoLetter)
    );
    assert_eq!(game.execute(first(), Command::CloseLetter), Ok(()));
    run(&mut game, 20);
    assert_eq!(mission(&game).letter().map(|letter| letter.text), Some(12));
}

#[test]
fn the_final_events_letter_holds_other_letters_back_as_well() {
    let mut game = chapter(
        &[],
        &[
            "!MET_DIRECT_EVENT 1 255 10 11",
            "!MET_SET_FINAL_EVENT 1 255 255 255",
            "!MET_DIRECT_EVENT 2 255 12 -1",
            "!MET_ADD_AS_ACT_EVENT 1",
            "!MET_ADD_AS_ACT_EVENT 2",
        ],
    );
    run(&mut game, FIRST_CHECK);
    let letter = mission(&game).letter();
    assert_eq!(
        letter.map(|letter| (letter.text, letter.closing)),
        Some((10, true))
    );
    assert!(
        mission(&game).waiting(2),
        "S2 opens the final letter at the wide size too, which 0x454d0 counts"
    );
}

#[test]
fn an_end_event_counts_its_triggers_then_activates_and_wins() {
    let mut game = chapter(
        &[],
        &[
            "!MET_DIRECT_EVENT 1 99 -1 -1",
            "!MET_DIRECT_EVENT 2 99 -1 -1",
            "!MET_END_EVENT 99 255 2 98 255 255 255 -1 -1",
            "!MET_SET_FINAL_EVENT 99 4 255 40",
            "!MET_SET_MAP_ELEMENT 98 30 31 23",
            "!MET_ADD_AS_ACT_EVENT 99",
            "!MET_ADD_AS_ACT_EVENT 1",
        ],
    );
    run(&mut game, FIRST_CHECK);
    assert!(mission(&game).waiting(99), "one of its two triggers has fired");
    assert!(!mission(&game).won());
    game.execute(first(), Command::CloseLetter).ok();
    let script = script(&[], &["!MET_ADD_AS_ACT_EVENT 2"]);
    game.run_section(script.map_commands().unwrap());
    run(&mut game, 20);
    assert!(!mission(&game).waiting(2));
    assert!(
        mission(&game).waiting(99),
        "its slot comes before the trigger's, so it sees the second count on the next check"
    );
    run(&mut game, 20);
    assert!(mission(&game).won());
    assert_eq!(mission(&game).opened(), [4], "40 is past the chapter flags");
    assert!(
        !mission(&game).waiting(98),
        "activated into a slot after the end event's and placed in the same check"
    );
    assert_eq!(game.world().object(Node { x: 30, y: 31 }), MapObject::element(23));
}

#[test]
fn a_fired_event_allows_its_buildings_for_player_0_with_a_message_each() {
    let mut game = chapter(
        &[],
        &[
            "!MET_DIRECT_EVENT 1 255 -1 -1",
            "!MET_HOUSE_ENABLING 1 2 3 255 255 255 255",
            "!MET_ADD_AS_ACT_EVENT 1",
        ],
    );
    run(&mut game, FIRST_CHECK);
    assert!(game.may_build(first(), BuildingKind::QUARRY));
    assert!(game.may_build(first(), BuildingKind::FISHERY));
    assert!(!game.may_build(Player::new(1).unwrap(), BuildingKind::QUARRY));
    let allowed: Vec<_> = game
        .world()
        .messages(first())
        .list()
        .iter()
        .filter(|message| message.id == MessageId::BUILDING_ALLOWED)
        .map(|message| message.building)
        .collect();
    assert_eq!(allowed, [Some(BuildingKind::QUARRY), Some(BuildingKind::FISHERY)]);
}

#[test]
fn position_events_watch_player_0s_land_and_sight() {
    let mut game = chapter(
        &[],
        &[
            "!MET_POSITION_OCCUPIED 1 255 18 18 -1 -1 0",
            "!MET_POSITION_OCCUPIED 2 255 40 40 -1 -1 0",
            "!MET_POSITION_EXPLORED 3 255 26 16 -1 -1 0",
            "!MET_POSITION_EXPLORED 4 255 40 40 -1 -1 0",
            "!MET_ADD_AS_ACT_EVENT 1",
            "!MET_ADD_AS_ACT_EVENT 2",
            "!MET_ADD_AS_ACT_EVENT 3",
            "!MET_ADD_AS_ACT_EVENT 4",
        ],
    );
    run(&mut game, FIRST_CHECK);
    let waiting: Vec<bool> = (1..=4).map(|id| mission(&game).waiting(id)).collect();
    assert_eq!(waiting, [false, true, false, true]);
}

#[test]
fn contact_comes_from_seeing_another_players_land() {
    let mut game = chapter(
        &[],
        &["!MET_CONTACT_TO_PLAYER 1 255 1 -1 -1", "!MET_ADD_AS_ACT_EVENT 1"],
    );
    let other = Player::new(1).unwrap();
    assert!(
        game.nodes().contact(first()),
        "its own land, seen as its headquarters went down"
    );
    assert!(!game.nodes().contact(other));
    game.nodes.reveal(Node { x: 48, y: 48 }, first(), 4);
    assert!(game.nodes().contact(other));
    run(&mut game, FIRST_CHECK);
    assert!(!mission(&game).waiting(1));
}

#[test]
fn a_claim_spreading_over_seen_land_makes_no_contact() {
    let mut game = chapter(&[], &[]);
    let other = Player::new(1).unwrap();
    let seen = Node { x: 32, y: 32 };
    game.nodes.reveal(seen, first(), 2);
    assert!(!game.nodes().contact(other), "nobody owns the land revealed");
    crate::territory::settle(&mut game.nodes, seen, other, 3);
    assert!(
        !game.nodes().contact(other),
        "a spread writes past the owner setter"
    );
    let mut data = game.nodes().get(seen);
    data.owner = None;
    game.nodes.set(seen, data);
    data.owner = Some(other);
    game.nodes.set(seen, data);
    assert!(game.nodes().contact(other), "the owner setter notes it");
}

#[test]
fn a_claims_protected_nodes_make_contact_even_when_already_owned() {
    let mut game = chapter(&[], &[]);
    let other = Player::new(1).unwrap();
    let centre = Node { x: 32, y: 32 };
    game.nodes.reveal(centre, first(), 3);
    for node in crate::territory::protected_nodes((game.nodes.width(), game.nodes.height()), centre) {
        crate::territory::settle(&mut game.nodes, node, other, 3);
    }
    assert!(!game.nodes().contact(other));
    crate::territory::begin_claim(&mut game.nodes, centre, 4, other);
    assert!(game.nodes().contact(other));
}

#[test]
fn unlimited_play_keeps_no_mission_and_no_contacts() {
    let game = Game::start(&map(2, &[(16, 16), (48, 48)]), &Setup::default()).unwrap();
    assert!(game.mission().is_none());
    assert!(!game.nodes().contact(first()));
    assert!(game.may_build(first(), BuildingKind::CATAPULT));
}

#[test]
fn a_barrier_covers_the_nodes_whose_whole_distance_is_below_its_radius() {
    let game = chapter(
        &[
            "!GLOBAL_SET_COMPUTER_BARRIER 0 10 10",
            "!GLOBAL_SET_COMPUTER_BARRIER 5 30 30",
            "!GLOBAL_SET_COMPUTER_BARRIER 5 1 50",
        ],
        &[],
    );
    assert_eq!(mission(&game).barriers().len(), 2, "a radius of 0 takes no slot");
    let covered = |x, y| game.in_barrier(Node { x, y });
    assert!(covered(34, 30), "4 away");
    assert!(!covered(35, 30), "5 away");
    assert!(covered(34, 32), "√20 rounds down to 4");
    assert!(!covered(33, 34), "3 and 4 make 5");
    assert!(covered(3, 50));
    assert!(
        !covered(62, 50),
        "61 away without wrapping, though 3 round the edge"
    );
}

#[test]
fn only_the_second_chapter_bans_the_computers_catapults() {
    let mut game = chapter(&[], &[]);
    assert!(!game.computers_never_build(BuildingKind::CATAPULT));
    game.mission = Some(Mission::new(1, &MissionTables::ORIGINAL));
    assert!(game.computers_never_build(BuildingKind::CATAPULT));
    assert!(!game.computers_never_build(BuildingKind::FORTRESS));
}

#[test]
fn a_mod_can_ban_other_buildings_in_other_missions() {
    let tables = MissionTables::new(vec![
        ComputerBan {
            mission: 4,
            kind: BuildingKind::FORTRESS,
        },
        ComputerBan {
            mission: 4,
            kind: BuildingKind::WATCHTOWER,
        },
    ]);
    let mut game = chapter(&[], &[]);
    game.mission = Some(Mission::new(4, &tables));
    assert!(game.computers_never_build(BuildingKind::FORTRESS));
    assert!(game.computers_never_build(BuildingKind::WATCHTOWER));
    assert!(!game.computers_never_build(BuildingKind::CATAPULT));
    game.mission = Some(Mission::new(1, &tables));
    assert!(
        !game.computers_never_build(BuildingKind::CATAPULT),
        "the mod dropped the original ban"
    );
}
