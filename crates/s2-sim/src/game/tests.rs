use super::*;
use crate::world::tests::wld;

fn game() -> Game {
    let map = Map::parse_wld(&wld(32, 32, &[], &[(1, 4, 4), (2, 9, 9)])).unwrap();
    Game::start(&map, &crate::Setup::default()).unwrap()
}

#[test]
fn the_cycle_counter_starts_at_one_and_wraps() {
    let mut clock = Clock::START;
    assert!(clock.every(8, 7), "the spawner's slot comes at step 8, cycle 9");
    for _ in 0..8 {
        clock.advance();
    }
    assert_eq!((clock.step(), clock.cycle()), (8, 9));
    assert!(clock.every(8, 7));
    for _ in 8..u32::from(Clock::CYCLE) - 1 {
        clock.advance();
    }
    assert_eq!(clock.cycle(), 0);
    clock.advance();
    assert_eq!((clock.step(), clock.cycle()), (u32::from(Clock::CYCLE), 1));
}

#[test]
fn equal_games_hash_equal_and_steps_change_the_hash() {
    let mut a = game();
    let b = game();
    assert_eq!(a.state_hash(), b.state_hash());
    a.step();
    assert_ne!(a.state_hash(), b.state_hash());
}

/// Pins the state after a run on a synthetic map. The same number on every OS and word size is
/// the M3 exit gate. Update it when a step gains behaviour, never to paper over a difference
/// between machines.
#[test]
fn a_run_hashes_the_same_everywhere() {
    let mut game = game();
    for _ in 0..1000 {
        game.step();
    }
    assert_eq!(game.clock().step(), 1000);
    assert_eq!(game.state_hash(), 7_041_716_206_854_226_730);
}
