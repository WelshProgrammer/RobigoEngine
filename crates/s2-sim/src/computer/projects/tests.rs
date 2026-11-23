use super::*;
use crate::computer::chain::tests::{computer_game, with_think};
use crate::computer::jobs::{Placement, SiteSearch};

const FROM: Node = Node { x: 20, y: 20 };
const MINE: Node = Node { x: 24, y: 18 };
const SMELTER: Node = Node { x: 18, y: 22 };

/// A project of `kind` with `options` at `step`, its anchor and saved node set, as a root job.
fn project_at(ai: &mut ComputerPlayer, kind: u8, options: u16, step: u16, anchor: Node, saved: Node) -> u16 {
    let mut job = Job::project(kind, FROM, options, 0);
    if let Payload::Project {
        step: at,
        anchor: there,
        saved: kept,
        ..
    } = &mut job.payload
    {
        *at = step;
        *there = anchor;
        *kept = saved;
    }
    ai.add(job, None, None).unwrap()
}

/// The build orders in the pool: what, where, and whether the project hears them.
fn orders(ai: &ComputerPlayer) -> Vec<(BuildingKind, Node, bool)> {
    let mut orders: Vec<_> = ai
        .jobs()
        .filter_map(|(at, job)| match job.payload {
            Payload::BuildOrder { kind, node, .. } => {
                Some((at, kind, node, job.owner == Some(TaskKind::Projects)))
            }
            _ => None,
        })
        .collect();
    orders.sort_by_key(|&(at, ..)| at);
    orders
        .into_iter()
        .map(|(_, kind, node, heard)| (kind, node, heard))
        .collect()
}

fn fields(ai: &ComputerPlayer, project: u16) -> (u16, Node, Node) {
    match ai.job(project).unwrap().payload {
        Payload::Project {
            step, anchor, saved, ..
        } => (step, anchor, saved),
        _ => panic!("not a project"),
    }
}

#[test]
fn the_first_turn_orders_the_first_building_once() {
    let mut game = computer_game();
    with_think(&mut game, |ai, think| {
        let project = ai.add(Job::project(WOOD, FROM, 0, 0), None, None).unwrap();
        ai.project_turn(project, think);
        ai.project_turn(project, think);
        assert_eq!(orders(ai), [(BuildingKind::SAWMILL, FROM, true)]);
    });
}

#[test]
fn a_weapons_chain_puts_the_armoury_by_the_smelter_and_the_coal_mine_by_the_iron_mine() {
    let mut game = computer_game();
    with_think(&mut game, |ai, think| {
        let project = project_at(ai, WEAPONS, 0, 2, SMELTER, MINE);
        ai.project_step(project, think);
        assert_eq!(
            orders(ai),
            [
                (BuildingKind::ARMOURY, SMELTER, true),
                (BuildingKind::COAL_MINE, MINE, false),
            ]
        );
        assert_eq!(fields(ai, project), (2, MINE, SMELTER));
    });
}

#[test]
fn a_failed_order_puts_the_project_back_on_the_board_and_ends_it() {
    let mut game = computer_game();
    with_think(&mut game, |ai, think| {
        let project = project_at(ai, GOLD, 0, 0, FROM, FROM);
        ai.project_heard(project, 0, think);
        assert_eq!(ai.tasks.board.count(GOLD), 1);
        assert_eq!(ai.doomed(), [project]);
    });
}

#[test]
fn a_placed_order_moves_the_anchor_to_its_site_and_runs_the_next_step() {
    let mut game = computer_game();
    with_think(&mut game, |ai, think| {
        let project = project_at(ai, COAL, 0, 0, FROM, FROM);
        let order = ai
            .add(
                Job::build_order(BuildingKind::FARM, FROM, 16, 0),
                Some(project),
                Some(TaskKind::Projects),
            )
            .unwrap();
        let placement = Placement {
            kind: BuildingKind::FARM,
            node: FROM,
            radius: 16,
            found: 0,
            reports: 0,
            extra: 0,
        };
        let placement = ai
            .add(
                Job::with(JobKind::Place, Payload::Place(placement), 0),
                Some(order),
                None,
            )
            .unwrap();
        let search = SiteSearch {
            kind: BuildingKind::FARM,
            centre: FROM,
            passed: true,
            at: MINE,
            ring: 0,
            side: 0,
            step: 0,
            radius: 16,
            skip: false,
        };
        ai.add(
            Job::with(JobKind::Site, Payload::Site(search), 0),
            Some(placement),
            None,
        );
        ai.project_heard(project, DONE, think);
        assert_eq!(fields(ai, project).0, 1);
        assert_eq!(fields(ai, project).1, MINE);
        assert_eq!(ai.doomed(), [project], "a coal chain ends after its mine");
    });
}

#[test]
fn a_weapons_chain_asks_for_beer_by_the_smelter_when_there_is_no_brewery() {
    let mut game = computer_game();
    with_think(&mut game, |ai, think| {
        let project = project_at(ai, WEAPONS, 0, 3, MINE, SMELTER);
        ai.project_step(project, think);
        assert_eq!(ai.tasks.board.count(BEER), 1);
        let request = ai.tasks.board.entries().iter().flatten().next().unwrap();
        assert_eq!(request.at, (18, 22));
        assert_eq!(ai.doomed(), [project]);
    });
}

#[test]
fn a_donkey_chain_with_trees_adds_a_woodcutter_among_its_free_orders() {
    let mut game = computer_game();
    with_think(&mut game, |ai, think| {
        let project = project_at(ai, DONKEYS, WITH_WOODCUTTER, 1, MINE, FROM);
        ai.project_step(project, think);
        assert_eq!(
            orders(ai),
            [
                (BuildingKind::FARM, MINE, false),
                (BuildingKind::WOODCUTTER, MINE, false),
                (BuildingKind::FARM, MINE, false),
                (BuildingKind::WELL, MINE, false),
            ]
        );
    });
}

#[test]
fn a_stone_project_with_other_options_stays_idle() {
    let mut game = computer_game();
    with_think(&mut game, |ai, think| {
        let project = project_at(ai, STONE, WITH_WOODCUTTER, 0, FROM, FROM);
        ai.project_step(project, think);
        assert_eq!(orders(ai), []);
        assert_eq!(ai.doomed(), [0_u16; 0]);
    });
}
