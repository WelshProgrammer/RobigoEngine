use super::resource_search;
use crate::game::Game;
use crate::gather::Find;
use crate::gather::tests::manned;
use crate::grid::{Direction, Node};
use crate::nodes::{Mineral, Resources};
use crate::settlers::tests::{game, player};
use crate::ware::WareKind;
use crate::world::BuildingKind;

fn set_resources(game: &mut Game, node: Node, resources: Resources) {
    let mut data = game.nodes.get(node);
    data.resources = resources;
    game.nodes.set(node, data);
}

/// Mountain over `x` 8 to 16 of rows 10 to 18, round a mine on (12, 14).
fn mountain(game: &mut Game) {
    const MOUNTAIN: u8 = 1;
    for y in 10..=18 {
        for x in 8..=16 {
            let at = Node { x, y };
            let mut data = game.nodes.get(at);
            data.terrain = [MOUNTAIN; 2];
            game.nodes.set(at, data);
        }
    }
    crate::sites::refresh_sites(&mut game.nodes, &game.world, Node { x: 12, y: 14 }, 6);
}

fn resources(game: &Game, node: Node) -> Resources {
    game.nodes.get(node).resources
}

fn gold(amount: u8) -> Resources {
    Resources::Mineral(Mineral::Gold, amount)
}

#[test]
fn a_centre_hit_and_a_ring_hit_make_two_although_one_is_kept() {
    let mut game = game();
    let centre = Node { x: 30, y: 20 };
    let ring = centre.neighbour(Direction::East, 48, 40);
    let search = resource_search(Find::Mineral(Mineral::Gold));
    set_resources(&mut game, ring, gold(3));
    assert_eq!(game.search(centre, search), [ring]);
    set_resources(&mut game, centre, gold(1));
    assert_eq!(game.search(centre, search), [centre, ring]);
    set_resources(&mut game, ring, gold(0));
    assert_eq!(
        game.search(centre, search),
        [centre],
        "an amount of 0 is not found"
    );
}

#[test]
fn a_mine_takes_ore_and_its_first_food_and_works_inside() {
    let mut game = game();
    mountain(&mut game);
    let node = Node { x: 12, y: 14 };
    let mine = manned(&mut game, BuildingKind::GOLD_MINE, node);
    set_resources(&mut game, node, gold(7));
    game.world.building_mut(mine).unwrap().stock = [0, 2, 1, 0];
    assert!(game.start_mine(mine));
    assert_eq!(resources(&game, node), gold(6));
    assert_eq!(
        game.world().building(mine).unwrap().stock[..3],
        [0, 1, 1],
        "bread first with no fish"
    );
    assert!(
        !game
            .world()
            .figures()
            .any(|(_, f)| matches!(f.kind, crate::figures::FigureKind::Gatherer(_)))
    );
}

#[test]
fn a_mine_without_food_draws_nothing_and_keeps_its_ore() {
    let mut game = game();
    mountain(&mut game);
    let node = Node { x: 12, y: 14 };
    let mine = manned(&mut game, BuildingKind::GOLD_MINE, node);
    set_resources(&mut game, node, gold(7));
    let rng = game.rng;
    assert!(!game.start_mine(mine));
    assert_eq!(game.rng, rng);
    assert_eq!(resources(&game, node), gold(7));
    assert!(!game.world().building(mine).unwrap().exhausted);
}

#[test]
fn a_mine_with_no_ore_gives_up_and_keeps_its_food() {
    let mut game = game();
    mountain(&mut game);
    let mine = manned(&mut game, BuildingKind::GOLD_MINE, Node { x: 12, y: 14 });
    game.world.building_mut(mine).unwrap().stock = [1, 0, 0, 0];
    assert!(!game.start_mine(mine));
    let node = game.world().building(mine).unwrap().node;
    let held = game.world().building(mine).unwrap();
    assert!(held.exhausted);
    assert_eq!(held.stock[0], 1);
    let sent = game.world().messages(player()).list();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].id, crate::messages::MessageId::MINE_DRY);
    assert_eq!(sent[0].node, Some(node));
    assert!(!game.start_mine(mine), "a mine that has given up starts nothing");
    assert_eq!(
        game.world().messages(player()).list().len(),
        1,
        "and says so once"
    );
}

#[test]
fn a_mine_runs_its_cycle_and_puts_out_gold() {
    let mut game = game();
    mountain(&mut game);
    let node = Node { x: 12, y: 14 };
    let mine = manned(&mut game, BuildingKind::GOLD_MINE, node);
    set_resources(&mut game, node, gold(7));
    game.world.building_mut(mine).unwrap().stock = [2, 0, 0, 0];
    let flag = game.world().building(mine).unwrap().flag;
    while game.world().wares_at(flag).count() == 0 {
        game.step();
        assert!(game.clock().step() < 2000, "no gold");
    }
    assert_eq!(game.world().wares_at(flag).next().unwrap().1.kind, WareKind::GOLD);
}

#[test]
fn a_well_never_uses_its_water_up() {
    let mut game = game();
    let node = Node { x: 12, y: 14 };
    let well = manned(&mut game, BuildingKind::WELL, node);
    set_resources(&mut game, node, Resources::Water(1));
    for _ in 0..3 {
        assert!(game.start_well(well));
        game.world.building_mut(well).unwrap().state -= crate::world::BuildingState::WORKING;
    }
    assert_eq!(resources(&game, node), Resources::Water(1));
}
