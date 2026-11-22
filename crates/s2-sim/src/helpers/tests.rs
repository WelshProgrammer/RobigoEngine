use crate::job::Job;
use crate::settlers::tests::{HQ, game};
use crate::ware::WareKind;
use crate::world::{MapObject, Storehouse};

fn stock(game: &crate::game::Game) -> Storehouse {
    let MapObject::Building(record) = game.world().object(HQ) else {
        panic!("no headquarters");
    };
    game.world().storehouse_of(record).unwrap().1.clone()
}

#[test]
fn the_pass_runs_every_240_steps_from_step_238() {
    let mut game = game();
    while game.clock().step() < 237 {
        game.step();
    }
    let before = stock(&game);
    game.step();
    let after = stock(&game);
    assert_ne!(before.jobs, after.jobs, "the pass ran on step 238");
    let recruits = after.jobs[Job::PRIVATE.index()] - before.jobs[Job::PRIVATE.index()];
    assert_eq!(
        after.jobs[Job::HELPER.index()] + recruits,
        before.jobs[Job::HELPER.index()] + 1,
        "one helper for a lone headquarters' land"
    );
    for _ in 0..239 {
        game.step();
        assert_eq!(stock(&game).jobs, after.jobs, "nothing until step 478");
    }
}

#[test]
fn recruits_take_one_beer_sword_and_shield_each() {
    let mut store = Storehouse::new(0);
    store.jobs[Job::HELPER.index()] = 40;
    store.wares[WareKind::BEER.index()] = 3;
    store.wares[WareKind::SWORD.index()] = 9;
    store.wares[WareKind::SHIELD.index()] = 5;
    store.recruit_and_add(1, 100);
    assert_eq!(store.jobs[Job::PRIVATE.index()], 3);
    assert_eq!(store.jobs[Job::HELPER.index()], 40 - 3 + 1);
    assert_eq!(store.wares[WareKind::BEER.index()], 0);
    assert_eq!(store.wares[WareKind::SWORD.index()], 6);
}

#[test]
fn helpers_stop_at_a_hundred() {
    let mut store = Storehouse::new(0);
    store.jobs[Job::HELPER.index()] = 99;
    store.recruit_and_add(5, 100);
    assert_eq!(store.jobs[Job::HELPER.index()], 100);
    store.jobs[Job::HELPER.index()] = 140;
    store.recruit_and_add(1, 100);
    assert_eq!(
        store.jobs[Job::HELPER.index()],
        100,
        "cut back as the 16-bit sum wraps"
    );
}
