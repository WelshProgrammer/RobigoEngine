use crate::figures::FigureKind;
use crate::gather::tests::manned;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::settlers::tests::{HQ_FLAG, game, road_east};
use crate::transport::{OrderKind, WareState};
use crate::ware::WareKind;
use crate::world::BuildingKind;
use crate::world::BuildingState;

#[test]
fn a_sawmill_turns_wood_into_boards_and_carries_them_out() {
    let mut game = game();
    let record = manned(&mut game, BuildingKind::SAWMILL, Node { x: 12, y: 14 });
    game.world.building_mut(record).unwrap().stock[0] = 2;
    let flag = game.world().building(record).unwrap().flag;
    let mut started = None;
    let mut made = None;
    while game.world().wares_at(flag).count() == 0 {
        game.step();
        let building = *game.world().building(record).unwrap();
        if started.is_none() && building.state.contains(BuildingState::WORKING) {
            started = Some(game.clock().step());
            assert_eq!(building.stock[0], 1, "one wood used");
        }
        if made.is_none() && building.outputs[0] == Some(WareKind::BOARDS) {
            made = Some(game.clock().step());
        }
        assert!(game.clock().step() < 1000, "no boards");
    }
    assert_eq!(made.unwrap() - started.unwrap(), 5 * 48, "five work ticks");
    assert!(
        game.world()
            .figures()
            .any(|(_, f)| matches!(f.kind, FigureKind::Porter(porter) if porter.inside))
    );
    let (_, boards) = game.world().wares_at(flag).next().unwrap();
    assert_eq!(boards.kind, WareKind::BOARDS);
}

#[test]
fn a_stocked_sawmill_makes_a_board_every_eight_ticks() {
    let mut game = game();
    let record = manned(&mut game, BuildingKind::SAWMILL, Node { x: 12, y: 14 });
    game.world.building_mut(record).unwrap().stock[0] = 6;
    let mut starts = Vec::new();
    let mut working = false;
    while starts.len() < 3 {
        game.step();
        let now = game
            .world()
            .building(record)
            .unwrap()
            .state
            .contains(BuildingState::WORKING);
        if now && !working {
            starts.push(game.clock().step());
        }
        working = now;
        assert!(game.clock().step() < 3000, "fewer than three cycles");
    }
    assert_eq!(
        starts[1] - starts[0],
        8 * 48,
        "start, five work ticks, carry out, clear"
    );
    assert_eq!(starts[2] - starts[1], 8 * 48);
}

#[test]
fn each_ware_a_workshop_makes_is_counted_for_its_owner() {
    let mut game = game();
    let record = manned(&mut game, BuildingKind::SAWMILL, Node { x: 12, y: 14 });
    game.world.building_mut(record).unwrap().stock[0] = 2;
    let owner = game.world().building(record).unwrap().owner;
    assert_eq!(game.world().ware_arrivals(owner, WareKind::BOARDS), 0);
    while game.world().ware_arrivals(owner, WareKind::BOARDS) == 0 {
        game.step();
        assert!(game.clock().step() < 1000, "no boards");
    }
    assert_eq!(game.world().ware_arrivals(owner, WareKind::BOARDS), 1);
    assert_eq!(game.world().coins_counted(owner), 0, "boards are not gold");
}

#[test]
fn the_coins_a_mint_makes_count_as_gold_as_well() {
    let mut game = game();
    let record = manned(&mut game, BuildingKind::MINT, Node { x: 12, y: 14 });
    let building = game.world.building_mut(record).unwrap();
    building.stock[0] = 1;
    building.stock[1] = 1;
    let owner = game.world().building(record).unwrap().owner;
    while game.world().coins_counted(owner) == 0 {
        game.step();
        assert!(game.clock().step() < 1000, "no coins");
    }
    assert_eq!(game.world().ware_arrivals(owner, WareKind::COINS), 1);
    assert_eq!(game.world().coins_counted(owner), 1);
}

#[test]
fn a_finished_sawmill_orders_one_wood_at_a_time() {
    let mut game = game();
    let record = manned(&mut game, BuildingKind::SAWMILL, Node { x: 12, y: 14 });
    let flag = game.world().building(record).unwrap().flag;
    let wood_orders = |game: &crate::game::Game| {
        game.world()
            .orders()
            .filter(|(_, order)| order.flag == flag && order.ware() == Some(WareKind::WOOD))
            .map(|(_, order)| (order.kind(), order.urgency))
            .collect::<Vec<_>>()
    };
    while wood_orders(&game).is_empty() {
        game.step();
        assert!(game.clock().step() < 300);
    }
    assert_eq!(game.clock().cycle() % 240, 0);
    assert_eq!(
        wood_orders(&game),
        [(OrderKind::Ware, 70 + 1 - 40)],
        "the weight, aged once and cut by 40 with no road to the headquarters"
    );
    let building = game.world().building(record).unwrap();
    assert_eq!((building.on_way[0], building.requested), (1, 1));
    for _ in 0..240 {
        game.step();
    }
    assert_eq!(wood_orders(&game).len(), 1, "one still on its way");
}

#[test]
fn demand_falls_as_the_input_fills() {
    let mut game = game();
    let record = manned(&mut game, BuildingKind::SAWMILL, Node { x: 12, y: 14 });
    assert_eq!(
        game.world().demand(record, WareKind::WOOD),
        77,
        "(3 × 70 + 100) / 4"
    );
    game.world.building_mut(record).unwrap().stock[0] = 5;
    assert_eq!(
        game.world().demand(record, WareKind::WOOD),
        56,
        "five of six held"
    );
    game.world.building_mut(record).unwrap().on_way[0] = 1;
    assert_eq!(game.world().demand(record, WareKind::WOOD), 0, "full");
    assert_eq!(game.world().demand(record, WareKind::BOARDS), 0, "not an input");
}

#[test]
fn wood_put_out_goes_to_a_sawmill_before_the_headquarters() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    let sawmill_flag = game.world().flag_at(Node { x: 20, y: 17 }).unwrap();
    let node = Node { x: 20, y: 17 }.neighbour(Direction::NorthWest, 48, 40);
    let record = manned(&mut game, BuildingKind::SAWMILL, node);
    assert_eq!(game.world().building(record).unwrap().flag, sawmill_flag);
    let hq_flag = game.world().flag_at(HQ_FLAG).unwrap();
    assert!(game.put_out(hq_flag, WareKind::WOOD));
    let (ware, wood) = game.world().wares_at(hq_flag).next().unwrap();
    assert_eq!(wood.state, WareState::Waiting);
    assert_eq!(game.world().destination(ware), Some(sawmill_flag));
    assert_eq!(wood.flag, Some(sawmill_flag), "the next flag on its way");
    assert_eq!(game.world().building(record).unwrap().on_way[0], 1);
    assert_eq!(
        game.world().destination_for(hq_flag, WareKind::BOARDS),
        game.world().building_at_flag(hq_flag),
        "boards go to the headquarters, its own flag included in the search"
    );
}

fn donkeys_in_stock(game: &crate::game::Game) -> u16 {
    game.world()
        .storehouses()
        .map(|(_, store)| store.jobs[Job::PACK_DONKEY.index()])
        .sum()
}

#[test]
fn a_bred_donkey_walks_the_roads_into_the_nearest_storehouse() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    let node = Node { x: 20, y: 17 }.neighbour(Direction::NorthWest, 48, 40);
    let breeder = manned(&mut game, BuildingKind::DONKEY_BREEDER, node);
    game.world.building_mut(breeder).unwrap().stock = [1, 1, 0, 0];
    let before = donkeys_in_stock(&game);
    let mut seen = false;
    while donkeys_in_stock(&game) == before {
        game.step();
        seen |= game
            .world()
            .figures()
            .any(|(_, f)| matches!(f.kind, FigureKind::Walker(w) if w.job == Job::PACK_DONKEY));
        assert!(game.clock().step() < 5000, "no donkey");
    }
    assert!(seen, "it walked the roads");
    assert_eq!(game.world().building(breeder).unwrap().stock[..2], [0, 0]);
    assert!(
        game.world()
            .orders()
            .all(|(_, order)| order.job() != Some(Job::PACK_DONKEY)),
        "its order is freed"
    );
}

#[test]
fn a_donkey_with_no_storehouse_in_reach_is_lost_outside_the_breeder() {
    let mut game = game();
    let node = Node { x: 12, y: 14 };
    let breeder = manned(&mut game, BuildingKind::DONKEY_BREEDER, node);
    game.world.building_mut(breeder).unwrap().stock = [1, 1, 0, 0];
    let lost = |game: &crate::game::Game| {
        game.world()
            .figures_at(node)
            .any(|(_, f)| matches!(f.kind, FigureKind::Lost { job, .. } if job == Job::PACK_DONKEY))
    };
    while !lost(&game) {
        game.step();
        assert!(game.clock().step() < 2000, "no donkey");
    }
}

/// Runs `kind` stocked with `stock` until it sends a porter out, and gives back the porter's look
/// and ware as it leaves the door.
fn first_carry_out(kind: BuildingKind, stock: [u8; 4]) -> (Job, Option<WareKind>) {
    let mut game = game();
    let record = manned(&mut game, kind, Node { x: 12, y: 14 });
    game.world.building_mut(record).unwrap().stock = stock;
    loop {
        game.step();
        let leaving = game.world().figures().find_map(|(_, figure)| match figure.kind {
            FigureKind::Porter(porter) if porter.building == record && !porter.inside => {
                Some((porter.job, porter.ware))
            }
            _ => None,
        });
        if let Some(leaving) = leaving {
            return leaving;
        }
        assert!(game.clock().step() < 3000, "{kind:?} sent nobody out");
    }
}

#[test]
fn the_miller_and_baker_carry_their_ware_out() {
    assert_eq!(
        first_carry_out(BuildingKind::MILL, [2, 0, 0, 0]),
        (Job(52), Some(WareKind::FLOUR))
    );
    assert_eq!(
        first_carry_out(BuildingKind::BAKERY, [2, 2, 0, 0]),
        (Job(53), Some(WareKind::BREAD))
    );
}

#[test]
fn a_mills_flour_goes_to_the_bakery_beside_it_until_the_bakery_stops() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    road_east(&mut game, Node { x: 20, y: 17 }, 2);
    let at = |x| Node { x, y: 17 }.neighbour(Direction::NorthWest, 48, 40);
    let mill = manned(&mut game, BuildingKind::MILL, at(20));
    let bakery = manned(&mut game, BuildingKind::BAKERY, at(22));
    let mill_flag = game.world().building(mill).unwrap().flag;
    let bakery_flag = game.world().building(bakery).unwrap().flag;
    game.world.building_mut(mill).unwrap().stock[0] = 1;
    let mut ordered = 0;
    while game.world().wares_at(mill_flag).count() == 0 {
        ordered = game.world().building(bakery).unwrap().on_way[0];
        game.step();
        assert!(game.clock().step() < 1000, "no flour");
    }
    let (ware, flour) = game.world().wares_at(mill_flag).next().unwrap();
    assert_eq!(flour.kind, WareKind::FLOUR);
    assert_eq!(game.world().destination(ware), Some(bakery_flag));
    assert_eq!(flour.flag, Some(bakery_flag), "the next flag on its way");
    assert_eq!(
        game.world().building(bakery).unwrap().on_way[0],
        ordered + 1,
        "counted on its way, besides the bakery's own order"
    );
    game.world.building_mut(bakery).unwrap().state |= BuildingState::STOPPED;
    assert_eq!(
        game.world().destination_for(mill_flag, WareKind::FLOUR),
        game.world()
            .building_at_flag(game.world().flag_at(HQ_FLAG).unwrap()),
        "a stopped bakery wants nothing, and the headquarters takes it"
    );
}

#[test]
fn a_farm_s_work_makes_nothing_to_carry_out_since_its_farmer_brings_the_grain() {
    let mut game = game();
    let record = manned(&mut game, BuildingKind::FARM, Node { x: 12, y: 14 });
    let mut worked = false;
    for _ in 0..2000 {
        game.step();
        let building = *game.world().building(record).unwrap();
        worked |= building.state.contains(BuildingState::WORKING);
        assert!(
            !building.state.contains(BuildingState::OUTPUT),
            "a farm never holds a ware"
        );
        assert_eq!(building.outputs, [None; 4]);
    }
    assert!(worked, "the farm worked");
}
