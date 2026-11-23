use crate::game::Game;
use crate::gather::tests::manned;
use crate::grid::{Direction, Node};
use crate::settlers::tests::game;
use crate::world::{BuildingKind, BuildingState, MapObject, Player};

use super::{Flight, SPENT_STONE, facing};

/// An occupied barracks of player 1 with `soldiers` privates inside, its record made directly.
fn enemy_barracks(game: &mut Game, node: Node, soldiers: u8) -> u16 {
    let record = manned(game, BuildingKind::BARRACKS, node);
    let rival = Player::new(1).unwrap();
    let flag = game.world().building(record).unwrap().flag;
    game.world.building_mut(record).unwrap().owner = rival;
    game.world.flag_mut(flag).unwrap().owner = rival;
    game.start_military(record);
    let (military, _) = game.world().military_of(record).unwrap();
    let held = game.world.military.get_mut(military).unwrap();
    held.requested = 0;
    held.inside = soldiers;
    held.ranks = [soldiers, 0, 0, 0, 0];
    record
}

#[test]
fn a_catapult_faces_its_target_from_raw_coordinates() {
    let at = Node { x: 10, y: 10 };
    assert_eq!(facing(at, Node { x: 14, y: 10 }), Direction::East);
    assert_eq!(facing(at, Node { x: 6, y: 10 }), Direction::West);
    assert_eq!(facing(at, Node { x: 10, y: 16 }), Direction::SouthEast);
    assert_eq!(facing(at, Node { x: 9, y: 16 }), Direction::SouthWest);
    assert_eq!(facing(at, Node { x: 9, y: 4 }), Direction::NorthWest);
    assert_eq!(facing(at, Node { x: 11, y: 4 }), Direction::NorthEast);
}

#[test]
fn a_stone_takes_one_soldier_and_the_last_one_brings_the_building_down() {
    let mut game = game();
    let target = enemy_barracks(&mut game, Node { x: 20, y: 20 }, 2);
    let node = game.world().building(target).unwrap().node;
    game.stone_lands(node);
    assert_eq!(game.world().military_of(target).unwrap().1.inside, 1);
    let owner = game.world().building_owner(target).unwrap();
    let sent = game.world().messages(owner).list();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].id, crate::messages::MessageId::CATAPULT_HIT);
    assert_eq!(sent[0].node, Some(node));
    game.stone_hits(node);
    assert!(
        game.world().building(target).is_none(),
        "demolished with nobody inside"
    );
}

#[test]
fn a_catapult_throws_at_an_enemy_and_the_stone_lands_after_69_steps() {
    let mut game = game();
    let catapult = manned(&mut game, BuildingKind::CATAPULT, Node { x: 12, y: 14 });
    game.world.building_mut(catapult).unwrap().stock[0] = 1;
    let target = enemy_barracks(&mut game, Node { x: 20, y: 20 }, 2);
    assert!(game.start_catapult(catapult));
    let site = game.world().building(catapult).unwrap();
    assert_eq!(site.stock[0], 0);
    assert!(site.state.contains(BuildingState::WORKING));
    let (_, stone) = game.world().stones().next().unwrap();
    let to = stone.to;
    let started = game.clock().step();
    let landed = |game: &Game| {
        game.world()
            .stones()
            .any(|(_, stone)| stone.flight == Flight::Landed)
    };
    while !landed(&game) {
        game.step();
        assert!(game.clock().step() < started + 100);
    }
    let thrown = started + 1;
    assert_eq!(
        game.clock().step() - thrown,
        69,
        "landed on the throw's step + 69"
    );
    let target_node = game.world().building(target).map(|building| building.node);
    if Some(to) == target_node {
        assert_eq!(game.world().military_of(target).unwrap().1.inside, 1, "a hit");
    } else if game.world().object(to) != MapObject::Flag(game.world().building(target).unwrap().flag) {
        assert_eq!(
            game.world().object(to),
            MapObject::Decoration(SPENT_STONE),
            "a miss"
        );
    }
    for _ in 0..4 {
        game.step();
    }
    assert_eq!(game.world().stones().count(), 0, "freed four steps after landing");
}

#[test]
fn a_catapult_never_throws_at_its_own_side() {
    let mut game = game();
    let catapult = manned(&mut game, BuildingKind::CATAPULT, Node { x: 12, y: 14 });
    game.world.building_mut(catapult).unwrap().stock[0] = 1;
    let own = manned(&mut game, BuildingKind::BARRACKS, Node { x: 20, y: 20 });
    game.start_military(own);
    assert!(!game.start_catapult(catapult));
    assert_eq!(game.world().building(catapult).unwrap().stock[0], 1);
}
