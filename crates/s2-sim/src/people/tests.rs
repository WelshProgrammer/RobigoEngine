use super::{counts_as, named_job, table_entry};
use crate::job::Job;
use crate::settlers::tests::{game, player};
use crate::world::{Player, World};

#[test]
fn a_job_counts_as_itself_unless_the_table_names_another() {
    assert_eq!(counts_as(0, Job::WOODCUTTER), Job::WOODCUTTER);
    assert_eq!(counts_as(0, Job::CARRYING_WOOD), Job::WOODCUTTER);
    assert_eq!(counts_as(0, Job::CARRYING_GRAIN), Job::FARMER);
    assert_eq!(
        counts_as(1, Job::CARRYING_GRAIN),
        Job::PIG_BREEDER,
        "a Viking carrying grain counts as a pig breeder"
    );
    assert_eq!(counts_as(0, Job::SOWER), Job::SOWER, "nothing named");
    assert_eq!(
        counts_as(3, Job::SOWER),
        Job::FARMER,
        "an African out sowing counts as a farmer"
    );
    assert_eq!(counts_as(9, Job::WOODCUTTER), Job::WOODCUTTER, "no such nation");
}

#[test]
fn people_are_counted_in_and_off_with_the_total() {
    let mut world = World::empty(32, 32);
    let one = player();
    world.count_people(one, Job::WOODCUTTER, 3);
    world.count_people(one, Job::HELPER, 5);
    assert_eq!(world.people(one, Job::WOODCUTTER), 3);
    assert_eq!(world.people(one, Job::HELPER), 5);
    assert_eq!(world.people_total(one), 8);
    world.drop_people(one, Job::WOODCUTTER, 1);
    assert_eq!(world.people(one, Job::WOODCUTTER), 2);
    assert_eq!(world.people_total(one), 7);
    world.drop_people(one, Job::WOODCUTTER, 9);
    assert_eq!(world.people(one, Job::WOODCUTTER), 0, "held to what it holds");
    assert_eq!(world.people_total(one), 5);
    assert_eq!(
        world.people(Player::new(1).expect("player 1"), Job::HELPER),
        0,
        "one player at a time"
    );
}

#[test]
fn a_worker_comes_off_the_job_its_look_counts_as() {
    let mut world = World::empty(32, 32);
    let one = player();
    world.count_people(one, Job::WOODCUTTER, 1);
    world.drop_people(one, Job::CARRYING_WOOD, 1);
    assert_eq!(world.people(one, Job::WOODCUTTER), 0);
    assert_eq!(world.people_total(one), 0);
}

#[test]
fn military_strength_weighs_each_rank_by_its_rank() {
    let mut world = World::empty(32, 32);
    let one = player();
    assert_eq!(world.military_strength(one), 0);
    world.count_people(one, Job::PRIVATE, 2);
    world.count_people(one, Job::GENERAL, 1);
    world.count_people(one, Job::WOODCUTTER, 4);
    assert_eq!(world.military_strength(one), 2 + 5);
}

#[test]
fn a_new_headquarters_counts_the_people_it_holds() {
    let game = game();
    let one = player();
    let store = game
        .world()
        .storehouses()
        .next()
        .map(|(_, store)| store.jobs)
        .expect("a headquarters");
    let held: u32 = store.iter().map(|&count| u32::from(count)).sum();
    assert!(held > 0, "a new headquarters holds people");
    assert_eq!(u32::from(game.world().people_total(one)), held);
    assert_eq!(game.world().people(one, Job::HELPER), store[Job::HELPER.index()]);
    assert_eq!(
        game.world().military_strength(one),
        u32::from(store[Job::PRIVATE.index()]),
        "the privates a headquarters starts with"
    );
}

#[test]
fn a_job_past_the_row_reads_on_into_a_later_nation() {
    assert_eq!(table_entry(0, Job(206)), Some((2, 62)));
    assert_eq!(named_job(0, Job(206)), Some(Job::METALWORKER));
    assert_eq!(
        crate::wares::carried(0, Job(206)),
        Some(crate::ware::WareKind::SCYTHE)
    );
    assert_eq!(table_entry(2, Job(206)), None);
}
