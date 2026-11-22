use super::*;
use crate::players::{PlayerSlot, Portrait};
use crate::settlers::tests::game;

fn player(index: u8) -> Player {
    Player::new(index).unwrap()
}

fn slots(seats: &[(u8, Controller, u8)]) -> PlayerSlots {
    let mut slots = PlayerSlots::EMPTY;
    for &(index, controller, face) in seats {
        slots.set(
            player(index),
            Some(PlayerSlot {
                controller,
                portrait: Portrait(face),
            }),
        );
    }
    slots
}

fn one_player() -> ComputerPlayer {
    ComputerPlayer::new(
        player(0),
        0,
        &ComputerProfiles::ORIGINAL,
        &mut GameRng::at_start(),
    )
}

#[test]
fn every_player_in_the_game_gets_an_ai_and_one_draw_in_player_order() {
    let seats = slots(&[
        (0, Controller::First, 0),
        (2, Controller::Computer, 4),
        (5, Controller::Computer, 9),
    ]);
    let mut rng = GameRng::at_start();
    let world = World::empty(16, 16);
    let nodes = NodeMap::filled(16, 16, crate::nodes::NodeData::default()).unwrap();
    let computers = Computers::build(
        (&nodes, &world),
        &seats,
        (&Portraits::ORIGINAL, &ComputerProfiles::ORIGINAL),
        &mut rng,
    );
    let mut check = GameRng::at_start();
    for (index, profile) in [(0, 0), (2, 3), (5, 3)] {
        let ai = computers.get(player(index)).expect("an AI, a person's too");
        assert_eq!(ai.profile(), profile, "the face's profile");
        let (least, most) = ComputerProfiles::ORIGINAL.get(profile).special_wait;
        let share = u32::from(check.up_to(100));
        assert_eq!(ai.tasks().special_wait(), least + share * (most - least) / 100);
    }
    assert_eq!(rng, check, "one draw a player and no more");
    assert!(computers.get(player(1)).is_none());
}

#[test]
fn the_first_task_made_that_claims_a_kind_handles_it() {
    let ai = one_player();
    assert_eq!(
        ai.handler(JobKind::Reserve),
        Some(TaskKind::Reserve),
        "task 0 before task 5"
    );
    assert_eq!(ai.handler(JobKind::BuildOrder), Some(TaskKind::Orders));
    assert_eq!(ai.handler(JobKind::Project), Some(TaskKind::Projects));
    assert_eq!(ai.handler(JobKind::Unused9), None);
    let economy = TaskKind::MADE.iter().position(|&kind| kind == TaskKind::Economy);
    let attacks = TaskKind::MADE.iter().position(|&kind| kind == TaskKind::Attacks);
    assert!(economy < attacks, "14 is made before 13");
}

#[test]
fn a_job_takes_the_lowest_free_index_and_joins_the_head_of_its_parents_children() {
    let mut ai = one_player();
    let root = ai.add(Job::new(JobKind::BuildOrder, 1), None, None).unwrap();
    let first = ai
        .add(Job::new(JobKind::Place, 1), Some(root), Some(TaskKind::Orders))
        .unwrap();
    let second = ai.add(Job::new(JobKind::Site, 1), Some(root), None).unwrap();
    assert_eq!((root, first, second), (0, 1, 2));
    assert_eq!(
        ai.job(root).unwrap().child,
        Some(second),
        "the newest child first"
    );
    assert_eq!(ai.job(second).unwrap().next, Some(first));
    assert_eq!(ai.fresh(), [0, 1, 2]);
    ai.delete(first);
    assert_eq!(ai.job(second).unwrap().next, None);
    assert_eq!(
        ai.add(Job::new(JobKind::Reserve, 2), None, None),
        Some(1),
        "the freed index"
    );
    ai.delete(root);
    assert_eq!(ai.jobs().map(|(index, _)| index).collect::<Vec<_>>(), [1]);
    assert_eq!(ai.fresh(), [1], "children go with their parent");
}

#[test]
fn a_full_pool_loses_the_job_and_a_full_list_drops_the_add() {
    let mut ai = one_player();
    for _ in 0..JOBS {
        assert!(ai.add(Job::new(JobKind::Reserve, 0), None, None).is_some());
    }
    assert_eq!(ai.add(Job::new(JobKind::Reserve, 0), None, None), None);
    assert_eq!(ai.fresh().len(), 50, "the new-job queue holds 50");
    let mut list = Capped::<2>::default();
    assert!(list.add(7) && list.add(8));
    assert!(!list.add(9));
    list.remove(7);
    assert_eq!(list.as_slice(), [8], "the gap closes");
}

#[test]
fn a_computer_thinks_on_its_own_tenth_step_and_a_person_never() {
    let mut game = game();
    let seats = slots(&[(0, Controller::First, 0), (3, Controller::Computer, 3)]);
    game.world.set_slots(seats);
    game.computers = Computers::build(
        (&game.nodes, &game.world),
        &seats,
        (&Portraits::ORIGINAL, &ComputerProfiles::ORIGINAL),
        &mut GameRng::at_start(),
    );
    for index in [0, 3] {
        let ai = game.computers.get_mut(player(index)).unwrap();
        ai.add(Job::new(JobKind::Unused9, 0), None, None);
    }
    let waiting = |game: &Game, index| game.computers().get(player(index)).unwrap().jobs().count();
    for _ in 0..2 {
        game.step();
    }
    assert_eq!(waiting(&game, 3), 1, "not its step yet");
    game.step();
    assert_eq!(game.clock().step(), 3);
    assert_eq!(
        waiting(&game, 3),
        0,
        "a job no task handles is deleted when it starts"
    );
    for _ in 0..40 {
        game.step();
    }
    assert_eq!(waiting(&game, 0), 1, "a person's AI never thinks");
}
