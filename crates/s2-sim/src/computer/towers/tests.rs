use super::*;
use crate::computer::ComputerProfiles;
use crate::computer::chain::tests::{computer_game, with_think};
use crate::computer::jobs::Payload;
use crate::settlers::tests::player;

#[test]
fn valuable_buildings_and_military_ones_weigh_by_the_profile() {
    let towers = ComputerProfiles::ORIGINAL.get(0).towers;
    assert_eq!(weights(BuildingKind::STOREHOUSE, &towers), (120, 50));
    assert_eq!(weights(BuildingKind::MINT, &towers), (60, 50));
    assert_eq!(weights(BuildingKind::FORTRESS, &towers), (0, 50));
    assert_eq!(weights(BuildingKind::HEADQUARTERS, &towers), (0, 0));
}

#[test]
fn a_scan_runs_every_profile_s_number_of_calls_over_its_number_of_cells() {
    let mut game = computer_game();
    with_think(&mut game, |ai, think| {
        let towers = think.profile.towers;
        for _ in 0..towers.every - 1 {
            ai.guard(think);
        }
        assert_eq!(ai.tasks.guards.cursor, (0, 0));
        ai.guard(think);
        let (columns, _) = think.grid.size();
        assert_eq!(
            ai.tasks.guards.cursor,
            (towers.cells % columns, towers.cells / columns)
        );
    });
}

#[test]
fn a_finished_storehouse_brings_a_tower_order_to_its_cell() {
    let mut game = computer_game();
    let record = game
        .world
        .place_site(
            &mut game.nodes,
            Node { x: 20, y: 22 },
            BuildingKind::STOREHOUSE,
            player(),
        )
        .unwrap();
    game.finish(record);
    let towers = with_think(&mut game, |ai, think| {
        for _ in 0..20 {
            ai.tasks.guards.cursor = (1, 2);
            ai.guard_cell(think);
        }
        ai.jobs()
            .filter_map(|(_, job)| match job.payload {
                Payload::BuildOrder { kind, node, .. } => Some((kind, node)),
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    let centre = Node { x: 20, y: 20 };
    assert!(
        towers.iter().any(|&(kind, node)| node == centre
            && matches!(kind, BuildingKind::WATCHTOWER | BuildingKind::FORTRESS)),
        "{towers:?}"
    );
}
