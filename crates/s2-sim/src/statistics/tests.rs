use super::{FOLD, GRAPH_SLOTS, SAMPLES, SERIES, Sample, Statistic};
use crate::grid::Node;
use crate::nodes::NodeMap;
use crate::settlers::tests::game;
use crate::ware::WareKind;
use crate::world::{BuildingKind, Player, World};

fn nodes() -> NodeMap {
    NodeMap::filled(32, 32, crate::nodes::NodeData::default()).expect("a map")
}

fn player() -> Player {
    Player::new(0).expect("player 0")
}

#[test]
fn the_shortest_series_takes_the_ware_counters_and_clears_them() {
    let mut world = World::empty(32, 32);
    let player = player();
    world.count_ware(player, WareKind::WOOD, 2);
    world.count_ware(player, WareKind::HAM, 1);
    world.count_ware(player, WareKind::BREAD, 3);
    world.count_ware(player, WareKind::SAW, 1);
    world.count_ware(player, WareKind::SHOVEL, 2);
    world.take_samples(0, &nodes());

    let sample = world.sample(player, 0, 0);
    assert_eq!(sample.counters[0], 2, "wood has a slot of its own");
    assert_eq!(sample.counters[3], 4, "the foods share slot 3");
    assert_eq!(sample.counters[11], 3, "the tools share slot 11");
    assert_eq!(world.ware_arrivals(player, WareKind::WOOD), 0);
    assert_eq!(world.ware_arrivals(player, WareKind::SAW), 0);

    world.take_samples(0, &nodes());
    let since = world.sample(player, 0, 0);
    assert_eq!(since.counters, Sample::EMPTY.counters, "nothing counted in since");
    assert_eq!(
        since.value(Statistic::Wares),
        9,
        "the wares the player owns are not a counter and stay"
    );
    assert_eq!(
        world.sample(player, 0, 1).counters[3],
        4,
        "the first sample has moved back one"
    );
}

#[test]
fn a_longer_series_adds_up_the_four_newest_samples_below_it() {
    let mut world = World::empty(32, 32);
    let player = player();
    for count in 1..=5 {
        world.count_ware(player, WareKind::BOARDS, count);
        world.take_samples(0, &nodes());
    }
    world.take_samples(1, &nodes());
    assert_eq!(
        world.sample(player, 1, 0).counters[1],
        5 + 4 + 3 + 2,
        "the four newest, not the oldest"
    );
    world.take_samples(2, &nodes());
    assert_eq!(
        world.sample(player, 2, 0).counters[1],
        5 + 4 + 3 + 2,
        "one filled sample of series 1 and three empty ones"
    );
}

#[test]
fn the_fold_holds_a_counter_at_its_top() {
    let mut world = World::empty(32, 32);
    let player = player();
    for _ in 0..FOLD {
        world.count_ware(player, WareKind::STONES, u16::MAX);
        world.take_samples(0, &nodes());
    }
    world.take_samples(1, &nodes());
    assert_eq!(world.sample(player, 1, 0).counters[2], u16::MAX);
}

#[test]
fn every_ware_has_a_slot_and_every_slot_is_used() {
    let mut used = [false; GRAPH_SLOTS];
    for &slot in &super::GRAPH_SLOT {
        used[usize::from(slot)] = true;
    }
    assert!(used.iter().all(|&used| used));
}

#[test]
fn a_series_keeps_forty_samples_and_four_series_are_kept() {
    let mut world = World::empty(32, 32);
    let player = player();
    world.count_ware(player, WareKind::WOOD, 1);
    world.take_samples(0, &nodes());
    for _ in 0..SAMPLES {
        world.take_samples(0, &nodes());
    }
    assert_eq!(
        world.sample(player, 0, SAMPLES - 1).counters,
        Sample::EMPTY.counters
    );
    world.take_samples(SERIES, &nodes());
    assert_eq!(
        world.sample(player, SERIES, 0),
        Sample::EMPTY,
        "no series past the four"
    );
}

#[test]
fn the_shortest_series_samples_every_hundred_and_eighty_steps() {
    let mut game = game();
    let player = player();
    game.world.count_ware(player, WareKind::WOOD, 3);
    while game.world().sample(player, 0, 0).counters[0] == 0 {
        game.step();
        assert!(game.clock().step() < 400, "no sample");
    }
    assert_eq!(game.clock().cycle(), 180);
    assert_eq!(game.world().sample(player, 0, 0).counters[0], 3);
}

#[test]
fn the_values_read_what_the_world_and_the_map_hold() {
    let mut world = World::empty(32, 32);
    let mut nodes = nodes();
    let player = player();
    for x in 0..3 {
        let node = Node { x, y: 0 };
        let mut data = nodes.get(node);
        data.owner = Some(player);
        nodes.set(node, data);
    }
    world.count_ware(player, WareKind::COINS, 4);
    world.count_fight_won(player);
    world.take_samples(0, &nodes);

    let sample = world.sample(player, 0, 0);
    assert_eq!(sample.value(Statistic::Land), 3);
    assert_eq!(sample.value(Statistic::Gold), 4);
    assert_eq!(sample.value(Statistic::FightsWon), 1);
    assert_eq!(sample.value(Statistic::Buildings), 0);
    assert_eq!(sample.value(Statistic::Productivity), 0, "no rated buildings");
    assert_eq!(
        sample.value(Statistic::People),
        0,
        "the job counts are not kept yet"
    );
}

#[test]
fn the_average_productivity_counts_only_rated_types_the_player_holds() {
    let mut world = World::empty(32, 32);
    let player = player();
    for (kind, average) in [
        (BuildingKind::WOODCUTTER, 60),
        (BuildingKind::QUARRY, 80),
        (BuildingKind::HEADQUARTERS, 100),
    ] {
        world.count_building(player, kind);
        world.averages[player.index()][kind.index()] = average;
    }
    world.take_samples(0, &nodes());
    assert_eq!(
        world.sample(player, 0, 0).value(Statistic::Productivity),
        70,
        "the headquarters is not one of the twenty"
    );
}
