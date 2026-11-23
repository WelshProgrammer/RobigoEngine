use super::*;
use crate::computer::chain::tests::{computer_game, with_think};
use crate::game::Game;
use crate::settlers::tests::{HQ_FLAG, player};

/// Task 10's calls `times` over for player 0.
fn call(game: &mut Game, times: u16) {
    with_think(game, |ai, think| {
        for _ in 0..times {
            ai.keep_flags(think);
        }
    });
}

fn flag_on(game: &mut Game, node: Node) -> u16 {
    game.world
        .place_flag(&mut game.nodes, node, FlagKind::Normal, true)
        .unwrap()
}

#[test]
fn the_next_flag_wraps_round_and_comes_back_to_the_cursor_last() {
    let mut game = computer_game();
    let headquarters = game.world().flag_at(HQ_FLAG).unwrap();
    let other = flag_on(&mut game, Node { x: 22, y: 20 });
    let (low, high) = (headquarters.min(other), headquarters.max(other));
    with_think(&mut game, |_, think| {
        assert_eq!(think.next_flag(None), Some(low));
        assert_eq!(think.next_flag(Some(low)), Some(high));
        assert_eq!(think.next_flag(Some(high)), Some(low));
    });
}

#[test]
fn a_flag_with_no_building_and_no_road_is_taken_away() {
    let mut game = computer_game();
    let lone = Node { x: 22, y: 20 };
    flag_on(&mut game, lone);
    call(&mut game, EVERY * 4);
    assert_eq!(game.world().flag_at(lone), None);
}

#[test]
fn a_quarry_with_nothing_left_is_taken_down_with_its_flag() {
    let mut game = computer_game();
    let node = Node { x: 20, y: 20 };
    let record = game
        .world
        .place_site(&mut game.nodes, node, BuildingKind::QUARRY, player())
        .unwrap();
    game.finish(record);
    let quarry = game.world.building_mut(record).unwrap();
    quarry.state |= BuildingState::MANNED;
    quarry.exhausted = true;
    let door = node.neighbour(Direction::SouthEast, game.nodes.width(), game.nodes.height());
    call(&mut game, EVERY * 4);
    assert_eq!(game.world().building(record), None);
    assert_eq!(game.world().flag_at(door), None);
}

/// A finished, manned woodcutter of player 0 at (20, 20) that has felled nothing lately.
fn idle_woodcutter(game: &mut Game) -> u16 {
    let node = Node { x: 20, y: 20 };
    let record = game
        .world
        .place_site(&mut game.nodes, node, BuildingKind::WOODCUTTER, player())
        .unwrap();
    game.finish(record);
    let woodcutter = game.world.building_mut(record).unwrap();
    woodcutter.state |= BuildingState::MANNED;
    woodcutter.productivity = 0;
    record
}

#[test]
fn an_idle_woodcutter_with_no_tree_near_is_taken_down() {
    let mut game = computer_game();
    let record = idle_woodcutter(&mut game);
    call(&mut game, EVERY * 4);
    assert_eq!(game.world().building(record), None);
}

#[test]
fn any_grown_tree_keeps_an_idle_woodcutter_standing_even_one_it_leaves() {
    let mut game = computer_game();
    let record = idle_woodcutter(&mut game);
    let palm = crate::world::MapObject::Tree(Tree {
        kind: 5,
        stage: Tree::GROWN,
        felling: false,
        phase: 0,
    });
    game.world.set_object(Node { x: 22, y: 20 }, palm);
    assert!(
        !game.world().fellable(Node { x: 22, y: 20 }),
        "a type no woodcutter fells"
    );
    call(&mut game, EVERY * 4);
    assert!(game.world().building(record).is_some());
}

#[test]
fn only_quarries_fisheries_mines_and_wells_keep_the_nothing_left_flag() {
    assert!(exhausted(BuildingKind::WELL, true));
    assert!(exhausted(BuildingKind::COAL_MINE, true));
    assert!(!exhausted(BuildingKind::QUARRY, false));
    assert!(!exhausted(BuildingKind::WOODCUTTER, true));
}

#[test]
fn task_3_lays_a_road_between_two_flags_it_can_flood_between() {
    let mut game = computer_game();
    let headquarters = game.world().flag_at(HQ_FLAG).unwrap();
    let other = flag_on(&mut game, Node { x: 22, y: 17 });
    with_think(&mut game, |_, think| think.shortcut(headquarters, other, 12));
    let joined = crate::search(game.world(), other, 200, SearchOptions::empty(), |reached| {
        reached.flag == headquarters
    });
    assert!(joined.is_some(), "a road joins the two flags");
}
