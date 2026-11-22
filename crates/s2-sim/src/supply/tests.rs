use crate::commands::Command;
use crate::figures::FigureKind;
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::settlers::tests::{FIRST_PASS, HQ_FLAG, game, player, road_east, run_to};
use crate::transport::{CONSTRUCTION_WEIGHT, OrderKind, WareState};
use crate::ware::WareKind;
use crate::world::BuildingKind;
use crate::world::{Building, MapObject};

const END: Node = Node { x: 20, y: 17 };

/// A game with a 4-edge road east of the headquarters and a woodcutter site at its far flag.
fn with_site() -> (Game, u16) {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    let node = END.neighbour(Direction::NorthWest, game.nodes.width(), game.nodes.height());
    game.execute(
        player(),
        Command::PlaceBuilding {
            node,
            kind: BuildingKind::WOODCUTTER,
        },
    )
    .unwrap();
    let MapObject::Building(site) = game.world().object(node) else {
        panic!("no site");
    };
    (game, site)
}

fn site(game: &Game, record: u16) -> Building {
    *game.world().building(record).unwrap()
}

fn hq_boards(game: &Game) -> u16 {
    game.world().storehouses().next().unwrap().1.wares[WareKind::BOARDS.index()]
}

fn board_orders(game: &Game) -> Vec<(OrderKind, u8)> {
    game.world()
        .orders()
        .filter(|(_, order)| {
            order.ware() == Some(WareKind::BOARDS) && order.mode == crate::transport::WARE_MODE
        })
        .map(|(_, order)| (order.kind(), order.urgency))
        .collect()
}

#[test]
fn the_builder_order_finding_a_storehouse_orders_the_sites_boards() {
    let (mut game, record) = with_site();
    run_to(&mut game, FIRST_PASS);
    let builder = game
        .world()
        .orders()
        .find(|(_, order)| order.job() == Some(Job::BUILDER))
        .map(|(_, order)| order.kind());
    assert_eq!(builder, Some(OrderKind::Person), "the helper went first");
    assert_eq!(board_orders(&game), [(OrderKind::Ware, CONSTRUCTION_WEIGHT); 2]);
    assert_eq!(site(&game, record).requested, 3);
}

#[test]
fn the_headquarters_brings_boards_out_and_carriers_take_them_into_the_site() {
    let (mut game, record) = with_site();
    let before = hq_boards(&game);
    let mut out = 0;
    let mut sent = 0;
    for _ in 0..1500 {
        game.step();
        sent = sent.max(site(&game, record).on_way[0]);
        out = out.max(
            game.world()
                .figures()
                .filter(|(_, figure)| matches!(figure.kind, FigureKind::Bringing { inside: false, .. }))
                .count(),
        );
    }
    assert_eq!(out, 1, "one at a time out of the door");
    assert_eq!(hq_boards(&game), before - 2);
    let building = site(&game, record);
    assert_eq!(sent, 2);
    assert_eq!(
        building.stock[0] + building.used[0],
        2,
        "arrived, and used by the builder"
    );
    assert!(board_orders(&game).is_empty(), "both orders used up");
    assert_eq!(game.world().wares().count(), 0);
}

#[test]
fn a_ware_put_down_at_the_storehouse_flag_heads_for_the_site() {
    let (mut game, _) = with_site();
    let mut seen = None;
    for _ in 0..400 {
        game.step();
        if let Some((record, ware)) = game.world().wares().next()
            && ware.state == WareState::Waiting
        {
            seen = Some((record, *ware));
            break;
        }
    }
    let (_, ware) = seen.expect("no board put down");
    let start = game.world().flag_at(HQ_FLAG).unwrap();
    let end = game.world().flag_at(END).unwrap();
    assert_eq!((ware.kind, ware.flag), (WareKind::BOARDS, Some(end)));
    assert_eq!(game.world().wares_at(start).count(), 1);
}

#[test]
fn removing_the_road_strands_the_board_and_reopens_the_sites_order() {
    let (mut game, record) = with_site();
    for _ in 0..400 {
        game.step();
        if game
            .world()
            .wares()
            .any(|(_, ware)| ware.state == WareState::Waiting)
        {
            break;
        }
    }
    let on_way = site(&game, record).on_way[0];
    game.execute(
        player(),
        Command::RemoveRoad {
            node: Node { x: 18, y: 17 },
        },
    )
    .unwrap();
    let (_, ware) = game.world().wares().next().unwrap();
    assert_eq!(ware.state, WareState::Stranded);
    assert_eq!(site(&game, record).on_way[0], on_way - 1);
    assert_eq!(
        game.world().order(ware.order.unwrap()).unwrap().kind(),
        OrderKind::Ware
    );
}

#[test]
fn the_emergency_programme_keeps_the_last_boards_for_the_first_woodcutter() {
    let (mut game, record) = with_site();
    let (store, _) = game.world().storehouses().next().unwrap();
    game.world.storehouse_mut(store).unwrap().wares[WareKind::BOARDS.index()] = 4;
    run_to(&mut game, FIRST_PASS + 16);
    let owner = player().index();
    let programme = game.world().emergency[owner];
    assert!(
        programme.on,
        "the first board order switched it on and was refused"
    );
    assert_eq!(programme.woodcutter, Some(record), "the second claimed it");
    let urgencies: Vec<u8> = board_orders(&game).iter().map(|&(_, u)| u).collect();
    assert_eq!(urgencies[0], 60, "refused: 100 less 40");
    run_to(&mut game, 1500);
    assert_eq!(hq_boards(&game), 2);
}
