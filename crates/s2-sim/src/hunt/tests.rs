use super::facing;
use crate::figures::{AnimalState, Figure, FigureKind, Species};
use crate::gather::tests::manned;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::settlers::tests::game;
use crate::ware::WareKind;
use crate::world::BuildingKind;

#[test]
fn the_hunter_faces_the_sector_the_animal_is_in() {
    let at = Node { x: 10, y: 10 };
    let cases = [
        (Node { x: 13, y: 10 }, Direction::East),
        (Node { x: 7, y: 10 }, Direction::West),
        (Node { x: 10, y: 14 }, Direction::SouthEast),
        (Node { x: 9, y: 14 }, Direction::SouthWest),
        (Node { x: 10, y: 6 }, Direction::NorthEast),
        (Node { x: 9, y: 6 }, Direction::NorthWest),
        (at, Direction::East),
    ];
    for (to, way) in cases {
        assert_eq!(facing(at, to), way, "{to:?}");
    }
}

#[test]
fn only_counted_species_with_no_hunter_are_prey() {
    let mut game = game();
    let node = Node { x: 30, y: 20 };
    game.world.add_figure(node, Figure::animal(Species::Duck));
    assert_eq!(game.prey_at(node), None);
    let sheep = game.world.add_figure(node, Figure::animal(Species::Sheep));
    assert_eq!(game.prey_at(node), Some(sheep));
    if let Some(FigureKind::Animal { hunter, .. }) = game.figure_kind_mut(sheep) {
        *hunter = Some(0);
    }
    assert_eq!(game.prey_at(node), None);
}

#[test]
fn a_hunter_with_no_game_about_keeps_trying_and_draws_nothing() {
    let mut game = game();
    let hut = manned(&mut game, BuildingKind::HUNTER, Node { x: 12, y: 14 });
    let rng = game.rng;
    assert!(!game.start_hunter(hut));
    assert_eq!(game.rng, rng);
    assert!(!game.world().building(hut).unwrap().exhausted);
}

#[test]
fn a_hunter_shoots_butchers_and_brings_meat_home() {
    let mut game = game();
    let node = Node { x: 12, y: 14 };
    let hut = manned(&mut game, BuildingKind::HUNTER, node);
    let sheep = game
        .world
        .add_figure(Node { x: 8, y: 14 }, Figure::animal(Species::Sheep));
    let flag = game.world().building(hut).unwrap().flag;
    let mut dead = false;
    while game.world().wares_at(flag).count() == 0 {
        game.step();
        if let Some(FigureKind::Animal { state, hunter, .. }) = game.world().figure(sheep).map(|f| f.kind)
            && state == AnimalState::Dead
        {
            dead = true;
            assert!(hunter.is_some());
        }
        assert!(game.clock().step() < 20_000, "no meat");
    }
    assert!(dead, "the sheep was shot");
    assert_eq!(game.world().wares_at(flag).next().unwrap().1.kind, WareKind::MEAT);
    assert!(
        !game
            .world()
            .figures()
            .any(|(_, f)| matches!(f.kind, FigureKind::Animal { .. })),
        "the carcass is gone"
    );
    let hunter = game.world().figures().find_map(|(_, f)| match f.kind {
        FigureKind::Gatherer(gatherer) => Some(gatherer.job),
        _ => None,
    });
    assert_eq!(hunter, Some(Job::HUNTER));
}
