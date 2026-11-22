use super::*;
use crate::combat::Alliances;
use crate::game::Game;
use crate::grid::Node;
use crate::players::{Controller, PlayerSlot, Portrait};
use crate::settlers::tests::{game, player};
use crate::world::StartGoods;

/// The step the first objective pass runs on: the cycle counter is one ahead of the step, and
/// the pass runs when it is one short of a multiple of 160.
const FIRST_PASS: u32 = 158;

fn rival() -> Player {
    Player::new(1).unwrap()
}

fn seat(game: &mut Game, player: Player) {
    let portrait = Portrait(u8::try_from(player.index()).unwrap());
    game.world.slots.set(
        player,
        Some(PlayerSlot {
            controller: Controller::First,
            portrait,
        }),
    );
}

/// Player 0's headquarters, and player 1 in the game with a headquarters of its own when
/// `housed`.
fn two(objective: Objective, housed: bool) -> Game {
    let mut game = game();
    seat(&mut game, player());
    seat(&mut game, rival());
    if housed {
        let (world, nodes) = (&mut game.world, &mut game.nodes);
        world.place_headquarters(nodes, Node { x: 36, y: 16 }, rival(), StartGoods::Normal);
    }
    game.goal.objective = objective;
    game
}

fn run(game: &mut Game, steps: u32) {
    for _ in 0..steps {
        game.step();
    }
}

#[test]
fn the_pass_first_runs_on_its_own_step() {
    let mut game = two(Objective::Domination, false);
    run(&mut game, FIRST_PASS - 1);
    assert_eq!(game.winner(), None);
    run(&mut game, 1);
    assert_eq!(game.winner(), Some(player()), "player 1 has no building left");
}

#[test]
fn nothing_wins_a_game_without_an_objective() {
    let mut game = two(Objective::None, false);
    run(&mut game, FIRST_PASS * 2);
    assert_eq!(game.winner(), None);
}

#[test]
fn an_enemy_building_keeps_the_game_open_and_an_ally_does_not() {
    let mut game = two(Objective::Domination, true);
    run(&mut game, FIRST_PASS);
    assert_eq!(game.winner(), None);
    let mut allied = two(Objective::Domination, true);
    allied.set_alliances(Alliances::Controllers);
    run(&mut allied, FIRST_PASS);
    assert_eq!(allied.winner(), Some(player()), "the lowest of the side wins");
}

#[test]
fn three_quarters_of_the_land_wins_with_allies_counted() {
    let mut alone = two(Objective::Land, false);
    run(&mut alone, FIRST_PASS);
    assert_eq!(alone.winner(), Some(player()));
    let mut halves = two(Objective::Land, true);
    assert_eq!(halves.nodes.owned(player()), halves.nodes.owned(rival()));
    run(&mut halves, FIRST_PASS);
    assert_eq!(halves.winner(), None, "half the land is short of 75 per cent");
    let mut allied = two(Objective::Land, true);
    allied.set_alliances(Alliances::Controllers);
    run(&mut allied, FIRST_PASS);
    assert_eq!(allied.winner(), Some(player()));
}

#[test]
fn a_decided_game_stays_decided() {
    let mut game = two(Objective::Domination, false);
    run(&mut game, FIRST_PASS);
    let (world, nodes) = (&mut game.world, &mut game.nodes);
    world.place_headquarters(nodes, Node { x: 36, y: 16 }, rival(), StartGoods::Normal);
    run(&mut game, 160);
    assert_eq!(game.winner(), Some(player()));
}

#[test]
fn the_last_headquarters_standing_wins_the_continent() {
    let mut alone = two(Objective::Headquarters, false);
    run(&mut alone, FIRST_PASS);
    assert_eq!(alone.winner(), Some(player()));
    let mut rivals = two(Objective::Headquarters, true);
    run(&mut rivals, FIRST_PASS * 2);
    assert_eq!(rivals.winner(), None);
}

#[test]
fn an_allys_headquarters_keeps_the_continent_open() {
    let mut allied = two(Objective::Headquarters, true);
    allied.set_alliances(Alliances::Controllers);
    run(&mut allied, FIRST_PASS);
    assert_eq!(allied.winner(), None);
}

#[test]
fn a_computer_never_wins_the_continent() {
    let mut game = two(Objective::Headquarters, false);
    let computer = PlayerSlot {
        controller: Controller::Computer,
        portrait: Portrait(0),
    };
    game.world.slots.set(player(), Some(computer));
    run(&mut game, FIRST_PASS * 2);
    assert_eq!(game.winner(), None, "S2 names player 0, a computer, here");
}
