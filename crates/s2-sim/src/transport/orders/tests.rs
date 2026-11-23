use super::*;
use crate::grid::Node;
use crate::job::Job;
use crate::marks::RoadKind;
use crate::nodes::{NodeData, NodeMap};
use crate::sites::site;
use crate::transport::Wanted;
use crate::world::FlagKind;

const SIZE: u16 = 24;

fn meadow() -> (NodeMap, World) {
    let data = NodeData {
        altitude: 10,
        terrain: [8; 2],
        owner: Player::new(3),
        ..NodeData::default()
    };
    let mut nodes = NodeMap::filled(SIZE, SIZE, data).unwrap();
    let world = World::empty(SIZE, SIZE);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let at = Node { x, y };
            let mut data = nodes.get(at);
            data.sites = site(&nodes, &world, at);
            nodes.set(at, data);
        }
    }
    (nodes, world)
}

fn road(kind: RoadKind) -> (World, u16, u16) {
    let (mut nodes, mut world) = meadow();
    let mut flag = |x| {
        world
            .place_flag(&mut nodes, Node { x, y: 6 }, FlagKind::Normal, true)
            .unwrap()
    };
    let (a, b) = (flag(4), flag(8));
    let path = (5..8).map(|x| Node { x, y: 6 }).collect();
    let record = world.create_road(&mut nodes, kind, a, Some(b), path).unwrap();
    (world, record, a)
}

#[test]
fn a_new_road_orders_a_helper_to_its_start_flag() {
    let (world, record, start) = road(RoadKind::Normal);
    let order = world.road(record).unwrap().order.unwrap();
    let order = world.order(order).unwrap();
    assert_eq!(
        (
            order.flag,
            order.kind(),
            order.wanted,
            order.urgency,
            order.mode,
            order.tag
        ),
        (
            start,
            OrderKind::Person,
            Wanted::Person(Job::HELPER),
            70,
            ROAD_MODE,
            0
        )
    );
    assert_eq!(order.owner, Player::new(3).unwrap());
    assert_eq!((order.on_way, order.harbour), (None, None));
}

#[test]
fn a_water_road_orders_something_else_at_a_lower_urgency() {
    let (world, record, _) = road(RoadKind::Water);
    let order = world.order(world.road(record).unwrap().order.unwrap()).unwrap();
    assert_eq!((order.job(), order.urgency), (Some(Job::WATER_ROAD), 50));
}

#[test]
fn the_most_urgent_twenty_are_chosen_first_come_among_equals() {
    let (mut nodes, mut world) = meadow();
    let flag = world
        .place_flag(&mut nodes, Node { x: 4, y: 6 }, FlagKind::Normal, true)
        .unwrap();
    let urgencies = [
        10, 60, 99, 30, 30, 40, 5, 80, 100, 20, 20, 70, 50, 15, 25, 35, 45, 55, 65, 75, 30, 90, 30, 0, 95,
    ];
    for &urgency in &urgencies {
        let record = world.order_person(flag, Job::WOODCUTTER, 0, 0).unwrap();
        world.orders.get_mut(record).unwrap().urgency = urgency;
    }
    world.orders.get_mut(3).unwrap().delivering = true;
    let chosen = world.choose_orders();
    assert_eq!(world.order(8).unwrap().urgency, 100, "capped");
    assert_eq!(world.order(2).unwrap().urgency, 100);
    assert_eq!(world.order(3).unwrap().urgency, 30, "being delivered: not aged");
    assert_eq!(world.order(0).unwrap().urgency, 11);
    // Records 0..=20 but 3 fill the list (urgencies after ageing 11, 61, 100, 31, 41, 6, 81,
    // 100, 21, 21, 71, 51, 16, 26, 36, 46, 56, 66, 76, 31). 21 (91) replaces the least, 6 at
    // record 6. 22 (31) replaces the next least, 11 at record 0. 23 (1) changes nothing, and 24
    // (96) replaces 16 at record 13.
    assert_eq!(
        chosen,
        [
            22, 1, 2, 4, 5, 21, 7, 8, 9, 10, 11, 12, 24, 14, 15, 16, 17, 18, 19, 20
        ]
    );
}
