use crate::grid::Node;
use crate::settlers::tests::{HQ_FLAG, game, road_east, ware_for};
use crate::ware::WareKind;
use crate::world::FLAG_CAPACITY;

use super::*;

#[test]
fn priorities_follow_the_default_list_with_player_0s_wood_last() {
    let values = crate::world::World::empty(4, 4).priorities;
    assert_eq!(values[1][WareKind::COINS.index()], 75);
    assert_eq!(values[1][WareKind::BOARDS.index()], 40);
    assert_eq!(values[1][WareKind::WOOD.index()], 30);
    assert_eq!(values[0][WareKind::WOOD.index()], 10);
    assert_eq!(values[0][WareKind::BOAT.index()], 10);
}

#[test]
fn a_ware_put_down_waits_for_the_next_flag_on_its_way() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    let world = &mut game.world;
    let start = world.flag_at(HQ_FLAG).unwrap();
    let end = world.flag_at(Node { x: 20, y: 17 }).unwrap();
    let ware = ware_for(world, WareKind::BOARDS, end);
    assert!(world.put_down(ware, start));
    let record = *world.ware(ware).unwrap();
    assert_eq!(
        (record.state, record.flag, record.slot),
        (WareState::Waiting, Some(end), 0)
    );
    assert_eq!(world.flag(start).unwrap().waiting, 1);
    assert_eq!(world.best_ware(start, end), Some(ware));
    assert_eq!(world.best_ware(end, start), None);
    assert!(world.take_ware(start, ware));
    assert_eq!(world.flag(start).unwrap().waiting, 0);
    assert!(!world.take_ware(start, ware));
}

#[test]
fn a_flag_holds_eight_and_carriers_take_the_best_oldest_first() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    let world = &mut game.world;
    let start = world.flag_at(HQ_FLAG).unwrap();
    let end = world.flag_at(Node { x: 20, y: 17 }).unwrap();
    let kinds = [
        WareKind::WOOD,
        WareKind::BOARDS,
        WareKind::COINS,
        WareKind::BOARDS,
        WareKind::COINS,
        WareKind::WOOD,
        WareKind::WOOD,
        WareKind::BOARDS,
    ];
    let wares: Vec<u16> = kinds
        .iter()
        .map(|&kind| {
            let ware = ware_for(world, kind, end);
            assert!(world.put_down(ware, start));
            ware
        })
        .collect();
    let slots: Vec<u8> = world.wares_at(start).map(|(_, ware)| ware.slot).collect();
    assert_eq!(slots, [7, 6, 5, 4, 3, 2, 1, 0], "newest first");
    let extra = ware_for(world, WareKind::COINS, end);
    world.wares.get_mut(extra).unwrap().state = WareState::Carried;
    assert!(!world.put_down(extra, start), "full at {FLAG_CAPACITY}");
    assert_eq!(
        world.ware(extra).unwrap().state,
        WareState::Carried,
        "unchanged when full"
    );
    assert_eq!(world.best_ware(start, end), Some(wares[2]), "the first coins");
    assert!(world.take_ware(start, wares[2]));
    assert_eq!(world.best_ware(start, end), Some(wares[4]));
    assert!(world.put_down(extra, start));
    assert_eq!(world.ware(extra).unwrap().slot, 2, "the lowest free place");
}

#[test]
fn a_ware_with_no_route_is_stranded_and_loses_its_order() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    let world = &mut game.world;
    let start = world.flag_at(HQ_FLAG).unwrap();
    let end = world.flag_at(Node { x: 20, y: 17 }).unwrap();
    let ware = ware_for(world, WareKind::BOARDS, end);
    let order = world.ware(ware).unwrap().order.unwrap();
    let road = world.roads().next().unwrap().0;
    world.remove_road(&mut game.nodes, road);
    let world = &mut game.world;
    assert!(world.put_down(ware, start));
    let record = *world.ware(ware).unwrap();
    assert_eq!(
        (record.state, record.flag, record.order),
        (WareState::Stranded, Some(start), Some(order)),
        "the order field goes stale"
    );
    assert!(world.order(order).is_none());
    assert_eq!(world.destination(ware), None);
    assert_eq!(world.best_ware(start, end), None);
}

#[test]
fn a_ware_put_down_at_its_destination_goes_into_the_storehouse() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    let world = &mut game.world;
    let start = world.flag_at(HQ_FLAG).unwrap();
    let store = world.storehouses().next().unwrap().0;
    let before = world.storehouses().next().unwrap().1.wares[WareKind::BOARDS.index()];
    let ware = ware_for(world, WareKind::BOARDS, start);
    assert!(world.put_down(ware, start));
    assert!(world.ware(ware).is_none());
    assert_eq!(world.orders().count(), 1, "only the road's helper order is left");
    let after = world.storehouses().find(|&(r, _)| r == store).unwrap().1.wares[WareKind::BOARDS.index()];
    assert_eq!(after, before + 1);
}

#[test]
fn removing_a_road_strands_the_wares_waiting_to_cross_it() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    let world = &mut game.world;
    let start = world.flag_at(HQ_FLAG).unwrap();
    let end = world.flag_at(Node { x: 20, y: 17 }).unwrap();
    let out = ware_for(world, WareKind::BOARDS, end);
    let back = ware_for(world, WareKind::COINS, start);
    assert!(world.put_down(out, start));
    assert!(world.put_down(back, end));
    world.wares.get_mut(back).unwrap().state = WareState::Taken;
    let road = world.roads().next().unwrap().0;
    world.remove_road(&mut game.nodes, road);
    for (ware, at) in [(out, start), (back, end)] {
        let record = *game.world().ware(ware).unwrap();
        assert_eq!((record.state, record.flag), (WareState::Stranded, Some(at)));
        assert!(game.world().order(record.order.unwrap()).is_none());
    }
}

#[test]
fn splitting_a_road_sends_the_waiting_wares_to_the_new_flag() {
    use crate::commands::Command;
    use crate::settlers::tests::player;
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    let start = game.world.flag_at(HQ_FLAG).unwrap();
    let end = game.world.flag_at(Node { x: 20, y: 17 }).unwrap();
    let ware = ware_for(&mut game.world, WareKind::BOARDS, end);
    assert!(game.world.put_down(ware, start));
    game.world.wares.get_mut(ware).unwrap().state = WareState::Taken;
    let middle = Node { x: 18, y: 17 };
    game.execute(
        player(),
        Command::PlaceFlag {
            node: middle,
            water: false,
        },
    )
    .unwrap();
    let record = *game.world().ware(ware).unwrap();
    assert_eq!(
        (record.state, record.flag),
        (WareState::Waiting, game.world().flag_at(middle))
    );
}
