use super::*;
use crate::computer::ComputerProfiles;
use crate::computer::chain::tests::{computer_game, with_think};

#[test]
fn defenders_take_the_profile_s_signed_word_for_their_type() {
    let numbers = ComputerProfiles::ORIGINAL.get(3).attacks;
    assert_eq!(defence(BuildingKind::BARRACKS, &numbers), -1);
    assert_eq!(defence(BuildingKind::FORTRESS, &numbers), -3);
    assert_eq!(defence(BuildingKind::HEADQUARTERS, &numbers), 0);
}

#[test]
fn a_site_off_the_player_s_land_is_worth_half_its_type_s_value() {
    let mut game = computer_game();
    let site = Node { x: 20, y: 22 };
    game.world
        .place_site(&mut game.nodes, site, BuildingKind::MINT, Player::new(0).unwrap())
        .unwrap();
    let mut data = game.nodes.get(site);
    data.owner = None;
    game.nodes.set(site, data);
    with_think(&mut game, |_, think| {
        let mint = think.profile.attacks.values.mint;
        assert_eq!(think.target_value(Node { x: 22, y: 22 }), mint / 2);
        assert_eq!(think.target_value(Node { x: 30, y: 22 }), 0, "too far");
    });
}

#[test]
fn the_headquarters_tries_go_round_the_players_in_turn() {
    let mut game = computer_game();
    with_think(&mut game, |ai, think| {
        let every = think.profile.attacks.headquarters_every;
        for _ in 0..every {
            ai.attack_headquarters(think);
        }
        assert_eq!(ai.tasks.attacks.target, 1);
        for _ in 0..every * 7 {
            ai.attack_headquarters(think);
        }
        assert_eq!(ai.tasks.attacks.target, 0, "8 wraps to 0");
    });
}

#[test]
fn with_no_enemy_military_the_cursor_is_cleared() {
    let mut game = computer_game();
    with_think(&mut game, |ai, think| {
        for _ in 0..think.profile.attacks.every {
            ai.attack_targets(think);
        }
        assert_eq!(ai.tasks.attacks.cursor, None);
    });
}
