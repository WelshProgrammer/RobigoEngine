use super::*;
use crate::computer::ComputerProfiles;

fn profile() -> ComputerProfile {
    ComputerProfiles::ORIGINAL.get(0)
}

/// Runs `calls` of task 17 on `sets` for player 0, whose military settings start as `military`
/// and end there.
fn run(sets: &mut MilitarySets, rng: &mut GameRng, military: &mut [u8; MILITARY_SETTINGS], calls: u32) {
    let mut game = crate::settlers::tests::game();
    game.rng = *rng;
    let player = Player::new(0).unwrap();
    game.world.military_settings[player.index()] = *military;
    let grid = Grid::default();
    for _ in 0..calls {
        let mut think = Think {
            player,
            step: 0,
            profile: profile(),
            game: &mut game,
            grid: &grid,
        };
        sets.think(&mut think);
    }
    *rng = game.rng;
    *military = game.world.military_settings[player.index()];
}

#[test]
fn the_special_set_holds_for_thirty_calls_and_then_the_picked_set_comes_back() {
    let mut sets = MilitarySets {
        wait: 3,
        ..MilitarySets::default()
    };
    let mut rng = GameRng::at_start();
    let mut military = [7; MILITARY_SETTINGS];
    run(&mut sets, &mut rng, &mut military, 2);
    assert_eq!(military, [7; MILITARY_SETTINGS], "not yet");
    run(&mut sets, &mut rng, &mut military, 1);
    assert!(sets.special);
    assert_eq!(sets.wait, 30);
    assert_eq!(military, profile().special_set);
    assert_eq!(rng, GameRng::at_start(), "starting the special set draws nothing");
    run(&mut sets, &mut rng, &mut military, 29);
    assert_eq!(military, profile().special_set);
    assert_eq!(sets.calls, 3, "calls outside the special set only");
    run(&mut sets, &mut rng, &mut military, 1);
    assert!(!sets.special);
    assert_eq!(military, profile().military_sets[0]);
    let mut check = GameRng::at_start();
    let (least, most) = profile().special_wait;
    assert_eq!(
        sets.wait,
        least + u32::from(check.up_to(100)) * (most - least) / 100
    );
    assert_eq!(rng, check, "one draw for the new countdown");
}

#[test]
fn every_hundred_and_twentieth_call_picks_a_set_from_a_raw_draws_low_bits() {
    let mut sets = MilitarySets {
        wait: 10_000,
        ..MilitarySets::default()
    };
    let mut rng = GameRng::at_start();
    let mut military = [7; MILITARY_SETTINGS];
    run(&mut sets, &mut rng, &mut military, 119);
    assert_eq!(military, [7; MILITARY_SETTINGS]);
    run(&mut sets, &mut rng, &mut military, 1);
    let mut check = GameRng::at_start();
    let pick = check.step() & 15;
    assert_eq!(u16::from(sets.pick), pick);
    assert_eq!(sets.set, profile().set_picks[usize::from(pick)]);
    assert_eq!(military, profile().military_sets[usize::from(sets.set)]);
    assert_eq!((sets.calls, sets.wait), (0, 10_000 - 120));
    assert_eq!(rng, check);
}
