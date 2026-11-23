use super::*;
use crate::computer::ComputerProfiles;
use crate::computer::chain::tests::{computer_game, with_think};
use crate::game::Game;
use crate::settlers::tests::{HQ, player};

/// Runs task 14 `calls` times for player 0.
fn run(game: &mut Game, calls: u16) {
    with_think(game, |ai, think| {
        for _ in 0..calls {
            ai.economy(think);
        }
    });
}

#[test]
fn the_fifth_call_runs_and_the_first_run_writes_the_profiles_settings() {
    let mut game = computer_game();
    game.world.military_settings[0] = [0; 7];
    run(&mut game, 4);
    assert_eq!(game.world.military_settings[0], [0; 7], "four calls do nothing");
    run(&mut game, 1);
    let profile = ComputerProfiles::ORIGINAL.get(0);
    assert_eq!(game.world.military_settings[0], profile.military_sets[0]);
    assert!(game.computers().get(player()).unwrap().tasks().economy().written);
}

#[test]
fn a_player_with_no_sawmill_asks_for_wood_and_stone_at_its_headquarters() {
    let mut game = computer_game();
    run(&mut game, 5);
    let board = game.computers().get(player()).unwrap().tasks().board();
    let at = (HQ.x.cast_signed(), HQ.y.cast_signed());
    let wanted: Vec<_> = board
        .entries()
        .iter()
        .flatten()
        .map(|request| (request.kind, request.at))
        .collect();
    assert_eq!(wanted[0], (WOOD, at));
    assert!(wanted.contains(&(STONE, at)));
    assert_eq!(board.count(WOOD), 1, "one wood request, the later rows see it");
}

#[test]
fn the_storehouse_cursor_moves_on_and_wraps() {
    let game = computer_game();
    let hq = game.world().object(HQ).building().unwrap();
    let next = |cursor| next_storehouse(game.world(), game.nodes(), cursor, player());
    assert_eq!(next(None), Some(hq));
    assert_eq!(next(Some(hq)), Some(hq), "the only one comes round again");
    assert_eq!(next(Some(u16::MAX - 1)), Some(hq));
}
