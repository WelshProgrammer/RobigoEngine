use crate::commands::Command;
use crate::figures::{CarrierState, FigureKind};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::settlers::tests::{HQ, HQ_FLAG, game, player, road_east, run_to, ware_for};
use crate::ware::WareKind;

const BOARDS: usize = 1;
const MIDDLE: Node = Node { x: 18, y: 17 };
const END: Node = Node { x: 20, y: 17 };

/// A game where the carrier of a 4-edge road east of the headquarters has fetched a board from
/// the far end and just turned to walk it in: the game, the road, the carrier, the ware and the
/// step it turned on.
fn delivering() -> (Game, u16, u16, u16, u32) {
    let mut game = game();
    let road = road_east(&mut game, HQ_FLAG, 4);
    run_to(&mut game, 200);
    let start = game.world.flag_at(HQ_FLAG).unwrap();
    let end = game.world.flag_at(END).unwrap();
    let ware = ware_for(&mut game.world, WareKind::BOARDS, start);
    assert!(game.world.put_down(ware, end));
    let (carrier, _) = game.world().figures().next().unwrap();
    while !matches!(
        game.world().figure(carrier).unwrap().kind,
        FigureKind::Delivering(_)
    ) {
        assert!(game.clock().step() < 400, "never walked in");
        game.step();
    }
    let turned = game.clock().step();
    (game, road, carrier, ware, turned)
}

fn boards(game: &Game) -> u16 {
    game.world().storehouses().next().unwrap().1.wares[BOARDS]
}

fn kind(game: &Game, record: u16) -> FigureKind {
    game.world().figure(record).unwrap().kind
}

#[test]
fn a_carrier_walks_the_ware_in_and_comes_back_out_to_its_road() {
    let (mut game, road, carrier, ware, turned) = delivering();
    let before = boards(&game);
    let figure = *game.world().figure(carrier).unwrap();
    assert_eq!(
        (figure.node(), figure.direction, figure.remaining),
        (HQ, Direction::NorthWest, 242)
    );
    let FigureKind::Delivering(delivery) = figure.kind else {
        unreachable!()
    };
    assert_eq!(
        (delivery.road, delivery.ware, delivery.inside),
        (Some(road), Some(ware), true)
    );
    assert!(delivery.shows.is_some(), "the board shows on the way in");
    run_to(&mut game, turned + 18);
    assert!(!game.world().figure(carrier).unwrap().walking());
    assert!(game.world().ware(ware).is_some(), "not yet handed over");
    run_to(&mut game, turned + 19);
    assert!(game.world().ware(ware).is_none());
    assert_eq!(boards(&game), before + 1);
    assert_eq!(game.world().orders().count(), 0);
    let figure = *game.world().figure(carrier).unwrap();
    assert_eq!((figure.node(), figure.direction), (HQ_FLAG, Direction::SouthEast));
    run_to(&mut game, turned + 37);
    assert!(matches!(kind(&game, carrier), FigureKind::Delivering(_)));
    run_to(&mut game, turned + 38);
    let FigureKind::Carrier(back) = kind(&game, carrier) else {
        panic!("not a carrier again");
    };
    assert_eq!(
        (back.on.road, back.on.position, back.state, back.ware, back.shows),
        (road, 0, CarrierState::Idle, None, None)
    );
    let road = game.world().road(road).unwrap();
    assert_eq!((road.carriers, road.traffic), (1, 1));
}

#[test]
fn a_carrier_whose_road_is_removed_inside_delivers_then_comes_out_lost() {
    let (mut game, road, carrier, ware, turned) = delivering();
    let before = boards(&game);
    game.execute(player(), Command::RemoveRoad { node: MIDDLE })
        .unwrap();
    assert!(game.world().road(road).is_none());
    let FigureKind::Delivering(delivery) = kind(&game, carrier) else {
        panic!("dropped its delivery");
    };
    assert_eq!((delivery.road, delivery.ware), (None, Some(ware)));
    run_to(&mut game, turned + 19);
    assert_eq!(boards(&game), before + 1);
    run_to(&mut game, turned + 38);
    assert!(matches!(
        kind(&game, carrier),
        FigureKind::Lost { job: Job::HELPER, .. }
    ));
}

#[test]
fn a_pack_donkey_back_at_a_flag_with_no_road_is_lost_as_a_pack_donkey() {
    let (mut game, road, carrier, _, turned) = delivering();
    if let Some(FigureKind::Delivering(delivery)) = game.figure_kind_mut(carrier) {
        delivery.transport = crate::figures::Transport::Donkey;
    }
    game.execute(player(), Command::RemoveRoad { node: MIDDLE })
        .unwrap();
    assert!(game.world().road(road).is_none());
    run_to(&mut game, turned + 38);
    assert!(matches!(
        kind(&game, carrier),
        FigureKind::Lost {
            job: Job::PACK_DONKEY,
            ..
        }
    ));
}

#[test]
fn a_split_gives_the_carrier_inside_the_half_at_its_building() {
    let (mut game, _, carrier, _, turned) = delivering();
    game.execute(
        player(),
        Command::PlaceFlag {
            node: MIDDLE,
            water: false,
        },
    )
    .unwrap();
    let FigureKind::Delivering(delivery) = kind(&game, carrier) else {
        panic!("dropped its delivery");
    };
    let half = game.world().road(delivery.road.unwrap()).unwrap();
    assert_eq!(game.world().flag(half.start).unwrap().node, HQ_FLAG);
    assert_eq!((half.carriers, half.served, half.order), (1, true, None));
    run_to(&mut game, turned + 38);
    let FigureKind::Carrier(back) = kind(&game, carrier) else {
        panic!("not a carrier again");
    };
    assert_eq!((back.on.road, back.on.position), (delivery.road.unwrap(), 0));
}
