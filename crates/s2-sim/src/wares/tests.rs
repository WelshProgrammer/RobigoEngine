use super::*;
use crate::world::World;

fn world() -> World {
    World::empty(8, 8)
}

fn player() -> Player {
    Player::new(0).expect("player 0")
}

#[test]
fn wares_are_counted_in_and_off_with_the_total() {
    let mut world = world();
    let player = player();
    world.count_ware(player, WareKind::BOARDS, 3);
    world.count_ware(player, WareKind::WOOD, 2);
    assert_eq!(world.owned_wares(player, WareKind::BOARDS), 3);
    assert_eq!(world.owned_wares(player, WareKind::WOOD), 2);
    assert_eq!(world.owned_wares_total(player), 5);
    world.drop_ware(player, WareKind::BOARDS, 1);
    assert_eq!(world.owned_wares(player, WareKind::BOARDS), 2);
    assert_eq!(world.owned_wares_total(player), 4);
}

#[test]
fn a_drop_is_held_to_what_the_ware_count_holds() {
    let mut world = world();
    let player = player();
    world.count_ware(player, WareKind::WOOD, 1);
    world.count_ware(player, WareKind::BOARDS, 4);
    world.drop_ware(player, WareKind::WOOD, 5);
    assert_eq!(world.owned_wares(player, WareKind::WOOD), 0);
    assert_eq!(
        world.owned_wares_total(player),
        4,
        "the total only loses the one wood"
    );
}

#[test]
fn one_players_wares_leave_the_others_alone() {
    let mut world = world();
    let first = player();
    let second = Player::new(1).expect("player 1");
    world.count_ware(first, WareKind::COINS, 2);
    assert_eq!(world.owned_wares_total(second), 0);
    world.drop_ware(second, WareKind::COINS, 1);
    assert_eq!(world.owned_wares(first, WareKind::COINS), 2);
}

#[test]
fn a_new_headquarters_counts_its_stock_and_counts_nothing_in() {
    let game = crate::settlers::tests::game();
    let owner = crate::settlers::tests::player();
    let world = game.world();
    let store = world
        .storehouses()
        .map(|(_, store)| store.wares)
        .next()
        .expect("the headquarters has a stock");
    let stocked: u16 = store.iter().sum();
    assert_eq!(world.owned_wares_total(owner), stocked);
    assert_eq!(
        world.owned_wares(owner, WareKind::BOARDS),
        store[WareKind::BOARDS.index()]
    );
    assert_eq!(
        world.ware_arrivals(owner, WareKind::BOARDS),
        0,
        "the stocking clears each ware's arrival counter again"
    );
}

#[test]
fn demolishing_a_storehouse_takes_its_whole_stock_off() {
    let mut game = crate::settlers::tests::game();
    let owner = crate::settlers::tests::player();
    let building = game
        .world()
        .buildings()
        .map(|(record, _)| record)
        .next()
        .expect("the headquarters");
    assert!(game.world().owned_wares_total(owner) > 0);
    game.demolish(building);
    assert_eq!(game.world().owned_wares_total(owner), 0);
    assert_eq!(game.world().owned_wares(owner, WareKind::BOARDS), 0);
}
