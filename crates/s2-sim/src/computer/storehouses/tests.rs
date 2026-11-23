use super::*;
use crate::computer::chain::tests::{computer_game, with_think};
use crate::computer::jobs::Payload;
use crate::world::FlagKind;

#[test]
fn every_fifth_call_moves_the_cursor_a_cell_along_the_row_and_then_down() {
    let mut game = computer_game();
    let columns = with_think(&mut game, |ai, think| {
        for _ in 0..EVERY {
            ai.storehouses(think);
        }
        assert_eq!(ai.tasks.storehouses.cursor, (1, 0));
        let (columns, _) = think.grid.size();
        for _ in 0..EVERY * (columns - 1) {
            ai.storehouses(think);
        }
        assert_eq!(ai.tasks.storehouses.cursor, (0, 1));
        columns
    });
    assert!(columns > 1);
}

#[test]
fn a_storehouse_is_ordered_at_a_flag_by_an_owned_cell_on_open_land() {
    let mut game = computer_game();
    let flag = Node { x: 21, y: 20 };
    game.world
        .place_flag(&mut game.nodes, flag, FlagKind::Normal, true)
        .unwrap();
    let orders = with_think(&mut game, |ai, think| {
        ai.tasks.storehouses.cursor = (1, 2);
        for _ in 0..EVERY {
            ai.storehouses(think);
        }
        ai.jobs()
            .filter_map(|(_, job)| match job.payload {
                Payload::BuildOrder { kind, node, .. } => Some((kind, node)),
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(orders.first(), Some(&(BuildingKind::STOREHOUSE, flag)));
}

#[test]
fn the_flag_on_a_rings_north_west_corner_comes_last() {
    let mut game = computer_game();
    let corner = Node { x: 19, y: 19 };
    let east = Node { x: 21, y: 20 };
    for flag in [corner, east] {
        game.world
            .place_flag(&mut game.nodes, flag, FlagKind::Normal, true)
            .unwrap();
    }
    let order = with_think(&mut game, |ai, think| {
        ai.tasks.storehouses.cursor = (1, 2);
        for _ in 0..EVERY {
            ai.storehouses(think);
        }
        ai.jobs().find_map(|(_, job)| match job.payload {
            Payload::BuildOrder { node, .. } => Some(node),
            _ => None,
        })
    });
    assert_eq!(order, Some(east), "the cell centre is (20, 20)");
}

#[test]
fn two_storehouse_sites_hold_task_11_back() {
    let mut game = computer_game();
    for node in [Node { x: 12, y: 20 }, Node { x: 20, y: 12 }] {
        game.world
            .place_site(
                &mut game.nodes,
                node,
                BuildingKind::STOREHOUSE,
                crate::settlers::tests::player(),
            )
            .unwrap();
    }
    with_think(&mut game, |ai, think| {
        for _ in 0..EVERY {
            ai.storehouses(think);
        }
        assert_eq!(ai.tasks.storehouses.cursor, (0, 0), "the cursor stays put");
    });
}
