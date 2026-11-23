use s2_formats::map::{Header, Layer, LayerKind, MAX_PLAYERS, Map, Position, Terrain};

use super::*;

fn faces(slots: &PlayerSlots) -> Vec<Option<u8>> {
    Player::ALL
        .iter()
        .map(|&player| slots.get(player).map(|slot| slot.portrait.0))
        .collect()
}

fn map(count: u8, starts: usize) -> Map {
    let mut headquarters = [None; MAX_PLAYERS];
    for (index, start) in headquarters.iter_mut().enumerate().take(starts) {
        let at = u16::try_from(index).unwrap() * 4;
        *start = Some(Position { x: at, y: at });
    }
    let header = Header::new("slots", "robigo", Terrain::Greenland, count, headquarters).unwrap();
    let layers = LayerKind::ALL
        .into_iter()
        .filter(|&kind| kind != LayerKind::Runtime13)
        .map(|kind| Layer::new(kind, vec![0; 32 * 32]))
        .collect();
    Map::new(header, 32, 32, layers, Vec::new(), Vec::new()).unwrap()
}

#[test]
fn four_players_get_the_faces_the_original_gives_the_first_unlimited_play_map() {
    let slots = PlayerSlots::offered(4, &Portraits::ORIGINAL);
    assert_eq!(
        faces(&slots),
        [Some(0), Some(4), Some(5), Some(6), None, None, None],
        "read out of a running game and out of every four-player save"
    );
    let controllers: Vec<_> = slots
        .players()
        .filter_map(|p| slots.get(p))
        .map(|s| s.controller)
        .collect();
    assert_eq!(
        controllers,
        [
            Controller::First,
            Controller::Computer,
            Controller::Computer,
            Controller::Computer
        ]
    );
    assert_eq!(
        slots.nations(&Portraits::ORIGINAL),
        [0, 1, 1, 2, 0, 0, 0],
        "Romans, two Vikings and the Japanese"
    );
}

#[test]
fn each_later_player_moves_on_past_every_face_already_taken() {
    assert_eq!(
        faces(&PlayerSlots::offered(2, &Portraits::ORIGINAL)),
        [Some(0), Some(2), None, None, None, None, None],
        "the second player passes its own face, which every two-player save shows"
    );
    assert_eq!(
        faces(&PlayerSlots::offered(7, &Portraits::ORIGINAL)),
        [Some(0), Some(7), Some(8), Some(9), Some(10), Some(11), Some(1)],
        "the last one wraps round to the first face nobody has"
    );
    assert_eq!(PlayerSlots::offered(0, &Portraits::ORIGINAL), PlayerSlots::EMPTY);
}

#[test]
fn a_new_game_keeps_the_players_asked_for_that_have_a_start() {
    let slots = PlayerSlots::new_game(&map(4, 4), None, &Portraits::ORIGINAL);
    assert_eq!(slots, PlayerSlots::offered(4, &Portraits::ORIGINAL));
    let first = PlayerSlots::new_game(&map(4, 4), Some(1), &Portraits::ORIGINAL);
    assert_eq!(first.players().count(), 1);
    let two = PlayerSlots::new_game(&map(4, 2), None, &Portraits::ORIGINAL);
    assert_eq!(
        faces(&two),
        [Some(0), Some(4), None, None, None, None, None],
        "the faces are dealt for the header's count before the empty starts drop out"
    );
}

#[test]
fn a_beaten_player_stays_in_the_game() {
    let mut slots = PlayerSlots::offered(2, &Portraits::ORIGINAL);
    let second = Player::new(1).unwrap();
    slots.beat(second);
    assert_eq!(
        slots.get(second).map(|slot| slot.controller),
        Some(Controller::Beaten)
    );
    assert!(slots.in_game(second));
    slots.beat(Player::new(5).unwrap());
    assert!(!slots.in_game(Player::new(5).unwrap()), "nobody to beat");
}

#[test]
fn losing_the_last_finished_building_beats_a_player() {
    use crate::world::{BuildingKind, World};

    let mut world = World::empty(8, 8);
    world.slots = PlayerSlots::offered(2, &Portraits::ORIGINAL);
    let second = Player::new(1).unwrap();
    world.count_building(second, BuildingKind::HEADQUARTERS);
    world.count_building(second, BuildingKind::WOODCUTTER);
    world.uncount_building(second, BuildingKind::WOODCUTTER);
    assert_eq!(
        world.slots().get(second).map(|s| s.controller),
        Some(Controller::Computer)
    );
    world.uncount_building(second, BuildingKind::HEADQUARTERS);
    assert_eq!(
        world.slots().get(second).map(|s| s.controller),
        Some(Controller::Beaten)
    );
    assert_eq!(world.all_finished_buildings(second), 0);
}

#[test]
fn a_game_puts_in_the_players_with_a_headquarters() {
    let map = map(4, 3);
    let game = crate::Game::start(&map, &crate::Setup::default()).unwrap();
    assert_eq!(
        faces(game.world().slots()),
        [Some(0), Some(4), Some(5), None, None, None, None]
    );
    let mut setup = crate::Setup::default();
    let mut chosen = PlayerSlots::EMPTY;
    let second = Player::new(1).unwrap();
    chosen.set(
        second,
        Some(PlayerSlot {
            controller: Controller::First,
            portrait: Portrait(9),
        }),
    );
    chosen.set(
        Player::new(5).unwrap(),
        Some(PlayerSlot {
            controller: Controller::Computer,
            portrait: Portrait(1),
        }),
    );
    setup.slots = Some(chosen);
    let game = crate::Game::start(&map, &setup).unwrap();
    let sixth = Player::new(5).unwrap();
    assert_eq!(
        game.world().slots().players().collect::<Vec<_>>(),
        [second, sixth],
        "the players in the game take the map's positions in turn"
    );
    assert_eq!(game.world().all_finished_buildings(Player::new(0).unwrap()), 0);
    let owner = |at: u16| game.nodes().get(crate::Node { x: at, y: at }).owner;
    assert_eq!(owner(0), Some(second));
    assert_eq!(owner(4), Some(sixth));
}

#[test]
fn the_screen_offers_no_more_players_than_the_map_has_positions_for() {
    assert_eq!(PlayerSlots::room(&map(4, 4)), 4);
    assert_eq!(
        PlayerSlots::room(&map(4, 7)),
        4,
        "the header's count still caps it"
    );
    assert_eq!(
        PlayerSlots::room(&map(7, 1)),
        1,
        "a mission map that counts seven"
    );
}

#[test]
fn a_player_left_without_a_position_is_taken_out() {
    let map = map(4, 2);
    let setup = crate::Setup {
        slots: Some(PlayerSlots::offered(3, &Portraits::ORIGINAL)),
        ..crate::Setup::default()
    };
    let game = crate::Game::start(&map, &setup).unwrap();
    assert_eq!(
        game.world()
            .slots()
            .players()
            .map(Player::index)
            .collect::<Vec<_>>(),
        [0, 1]
    );
}

#[test]
fn the_random_order_swaps_a_hundred_times_with_two_draws_each() {
    let mut rng = GameRng::at_start();
    let starts = [Some(0), None, Some(1), Some(2), Some(3), None, None];
    let shuffled = StartOrder::Random.arrange(&starts, &mut rng);
    let mut expected = vec![0, 1, 2, 3];
    let mut check = GameRng::at_start();
    for _ in 0..100 {
        let first = usize::from(check.step() % 4);
        let second = usize::from(check.step() % 4);
        expected.swap(first, second);
    }
    assert_eq!(shuffled, expected);
    assert_eq!(rng, check, "two draws a swap and nothing else");
    let mut untouched = GameRng::at_start();
    assert_eq!(
        StartOrder::InSequence.arrange(&starts, &mut untouched),
        [0, 1, 2, 3]
    );
    assert_eq!(untouched, GameRng::at_start(), "in sequence draws nothing");
}

#[test]
fn a_portrait_table_needs_a_face_a_player_and_known_nations() {
    let row = PortraitRow {
        nation: 0,
        profile: 0,
    };
    assert_eq!(Portraits::new(vec![row; 6]), Err(PortraitsError::TooFew(6)));
    assert_eq!(Portraits::new(vec![row; 257]), Err(PortraitsError::TooMany(257)));
    let mut rows = vec![row; 8];
    rows[3].nation = NATIONS;
    assert_eq!(
        Portraits::new(rows),
        Err(PortraitsError::Nation {
            portrait: 3,
            nation: NATIONS
        })
    );
    assert_eq!(Portraits::new(vec![row; 7]).map(|table| table.len()), Ok(7));
    assert_eq!(Portraits::ORIGINAL.len(), 12);
}

fn player(index: u8) -> Player {
    Player::new(index).unwrap()
}

fn roster_faces(roster: &Roster) -> Vec<(bool, u8)> {
    Player::ALL
        .iter()
        .map(|&p| (roster.entry(p).in_game, roster.entry(p).portrait.0))
        .collect()
}

#[test]
fn the_roster_keeps_the_players_out_of_the_game_with_their_own_faces() {
    let roster = Roster::offered(4, &Portraits::ORIGINAL);
    assert_eq!(
        roster_faces(&roster),
        [
            (true, 0),
            (true, 4),
            (true, 5),
            (true, 6),
            (false, 4),
            (false, 5),
            (false, 6)
        ],
        "read from S2's setup screen on the first Unlimited Play map"
    );
    assert_eq!(roster.slots(), PlayerSlots::offered(4, &Portraits::ORIGINAL));
}

#[test]
fn a_player_coming_in_moves_past_every_face_taken() {
    let mut roster = Roster::offered(4, &Portraits::ORIGINAL);
    roster.toggle(player(4));
    assert_eq!(roster.entry(player(4)).portrait, Portrait(7));
    roster.toggle(player(3));
    assert!(!roster.entry(player(3)).in_game);
    roster.toggle(player(3));
    assert_eq!(roster.entry(player(3)).portrait, Portrait(8));
    roster.next_face(player(0));
    assert_eq!(roster.entry(player(0)).portrait, Portrait(1));
}

#[test]
fn controllers_step_round_and_a_person_brought_back_goes_to_the_computer() {
    let mut roster = Roster::offered(2, &Portraits::ORIGINAL);
    roster.next_controller(player(1));
    assert_eq!(roster.entry(player(1)).controller, Controller::Second);
    roster.next_controller(player(0));
    assert_eq!(roster.entry(player(0)).controller, Controller::Computer);
    roster.next_controller(player(0));
    assert_eq!(roster.entry(player(0)).controller, Controller::First);
    roster.toggle(player(1));
    assert_eq!(roster.entry(player(1)).controller, Controller::Second);
    roster.toggle(player(1));
    assert_eq!(roster.entry(player(1)).controller, Controller::Computer);
}

#[test]
fn nobody_can_start_without_a_person() {
    let mut roster = Roster::offered(2, &Portraits::ORIGINAL);
    assert!(roster.can_start());
    roster.next_controller(player(0));
    roster.next_controller(player(0));
    assert_eq!(roster.entry(player(0)).controller, Controller::Computer);
    assert!(!roster.can_start());
}

#[test]
fn the_people_move_to_the_first_two_start_positions() {
    let mut roster = Roster::offered(4, &Portraits::ORIGINAL);
    roster.next_controller(player(0));
    roster.next_controller(player(0));
    assert_eq!(roster.entry(player(0)).controller, Controller::Computer);
    roster.next_controller(player(1));
    assert_eq!(roster.entry(player(1)).controller, Controller::First);
    let slots = roster.slots();
    assert_eq!(slots.get(player(0)).unwrap().controller, Controller::First);
    assert_eq!(slots.get(player(0)).unwrap().portrait, Portrait(4));
    assert_eq!(slots.get(player(1)).unwrap().controller, Controller::Computer);
    assert_eq!(slots.get(player(1)).unwrap().portrait, Portrait(0));

    let mut second_only = Roster::offered(2, &Portraits::ORIGINAL);
    second_only.next_controller(player(0));
    second_only.next_controller(player(1));
    assert_eq!(second_only.entry(player(0)).controller, Controller::Second);
    assert_eq!(second_only.entry(player(1)).controller, Controller::First);
    second_only.next_controller(player(1));
    assert_eq!(second_only.entry(player(1)).controller, Controller::Computer);
    let slots = second_only.slots();
    assert_eq!(slots.get(player(0)).unwrap().controller, Controller::First);
}

#[test]
fn a_continent_seats_the_person_first_and_the_header_faces_for_the_computers() {
    let mut headquarters = [None; MAX_PLAYERS];
    for (index, start) in headquarters.iter_mut().enumerate().take(3) {
        let at = u16::try_from(index).unwrap() * 4;
        *start = Some(Position { x: at, y: at });
    }
    let header = Header::new("continent", "robigo", Terrain::Greenland, 3, headquarters)
        .unwrap()
        .with_leaders([0, 9, 10, 11, 0, 0, 0]);
    let layers = LayerKind::ALL
        .into_iter()
        .filter(|&kind| kind != LayerKind::Runtime13)
        .map(|kind| Layer::new(kind, vec![0; 32 * 32]))
        .collect();
    let map = Map::new(header, 32, 32, layers, Vec::new(), Vec::new()).unwrap();
    let slots = PlayerSlots::world_campaign(&map);
    assert_eq!(
        faces(&slots),
        [Some(0), Some(9), Some(10), None, None, None, None]
    );
    let controllers: Vec<_> = slots
        .players()
        .filter_map(|p| slots.get(p))
        .map(|s| s.controller)
        .collect();
    assert_eq!(
        controllers,
        [Controller::First, Controller::Computer, Controller::Computer]
    );
}
