use crate::commands::Command;
use crate::figures::FigureKind;
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::settlers::tests::{HQ_FLAG, game, player, road_east};
use crate::world::BuildingKind;
use crate::world::{Building, BuildingState};

const END: Node = Node { x: 20, y: 17 };

/// A game with a woodcutter site at the end of a 4-edge road east of the headquarters.
fn with_woodcutter() -> (Game, u16) {
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
    let site = game
        .world()
        .building_at_flag(game.world().flag_at(END).unwrap())
        .unwrap();
    (game, site)
}

fn building(game: &Game, record: u16) -> Building {
    *game.world().building(record).unwrap()
}

#[test]
fn a_finished_workplace_orders_its_worker_who_walks_in() {
    let (mut game, site) = with_woodcutter();
    while !building(&game, site).state.contains(BuildingState::FINISHED) {
        game.step();
        assert!(game.clock().step() < 5000, "never finished");
    }
    let flag = building(&game, site).flag;
    let order = game
        .world()
        .orders()
        .find(|(_, order)| order.flag == flag && order.job() == Some(Job::WOODCUTTER))
        .map(|(record, _)| record);
    assert!(order.is_some(), "no worker order");
    let mut settling_from = None;
    while !building(&game, site).state.contains(BuildingState::MANNED) {
        game.step();
        let settling = game.world().figures().any(|(_, figure)| {
            matches!(
                figure.kind,
                FigureKind::Settling {
                    job: Job::WOODCUTTER,
                    ..
                }
            )
        });
        if settling && settling_from.is_none() {
            settling_from = Some(game.clock().step());
        }
        assert!(game.clock().step() < 8000, "never manned");
    }
    let manned = game.clock().step();
    assert_eq!(manned - settling_from.unwrap(), 19, "19 steps walking in");
    let state = building(&game, site).state;
    assert!(state.contains(BuildingState::PRESENT));
    assert!(!state.contains(BuildingState::DOOR));
    assert!(game.world().order(order.unwrap()).is_none(), "the order is freed");
    assert!(
        !game
            .world()
            .figures()
            .any(|(_, figure)| matches!(figure.kind, FigureKind::Settling { .. }))
    );
}

#[test]
fn a_manned_woodcutter_demolished_loses_its_worker() {
    let (mut game, site) = with_woodcutter();
    while !building(&game, site).state.contains(BuildingState::MANNED) {
        game.step();
        assert!(game.clock().step() < 8000, "never manned");
    }
    let node = building(&game, site).node;
    game.execute(player(), Command::Demolish { node }).unwrap();
    assert!(game.world().figures_at(node).any(|(_, figure)| matches!(
        figure.kind,
        FigureKind::Lost {
            job: Job::WOODCUTTER,
            ..
        }
    )));
}

#[test]
fn productivity_keeps_two_thirds_and_adds_the_tally() {
    let (mut game, site) = with_woodcutter();
    for (old, tally, new) in [
        (0, 0, 0),
        (60, 30, 70),
        (90, 60, 100),
        (99, 0, 66),
        (100, 49, 115 - 15),
    ] {
        let building = game.world.building_mut(site).unwrap();
        building.productivity = old;
        building.tally = tally;
        game.rate_productivity(site);
        let building = game.world().building(site).unwrap();
        assert_eq!(building.productivity, new, "{old} {tally}");
        assert_eq!(building.tally, 0);
    }
}

#[test]
fn the_average_halves_towards_each_later_building() {
    let (mut game, first) = with_woodcutter();
    let node = Node { x: 12, y: 14 };
    game.execute(
        player(),
        Command::PlaceBuilding {
            node,
            kind: BuildingKind::WOODCUTTER,
        },
    )
    .unwrap();
    let crate::world::MapObject::Building(second) = game.world().object(node) else {
        panic!("no second site");
    };
    for (record, productivity) in [(first, 80), (second, 40)] {
        let building = game.world.building_mut(record).unwrap();
        building.state = BuildingState::FINISHED;
        building.productivity = productivity;
    }
    game.average_productivity();
    assert_eq!(game.world().averages[0][BuildingKind::WOODCUTTER.index()], 60);
    assert_eq!(game.world().averages[1][BuildingKind::WOODCUTTER.index()], 0);
}
