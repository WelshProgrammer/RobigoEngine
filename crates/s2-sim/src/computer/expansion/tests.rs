use super::*;
use crate::computer::chain::tests::{computer_game, with_think};
use crate::settlers::tests::{HQ, player};
use crate::world::Category;

#[test]
fn each_player_s_scan_counter_starts_forty_calls_after_the_last() {
    let calls: Vec<u16> = (0..4)
        .map(|index| AreaScans::new(Player::new(index).unwrap()).calls)
        .collect();
    assert_eq!(calls, [0, 40, 80, 120]);
}

#[test]
fn a_survey_of_depth_three_hears_two_rings_and_a_block_stops_the_flood_there() {
    let mut game = computer_game();
    with_think(&mut game, |_, think| {
        let mut heard = Vec::new();
        think.survey(HQ, 3, true, |node, _| {
            heard.push(node);
            Visit::Pass
        });
        let (width, height) = (think.game.nodes.width(), think.game.nodes.height());
        let mut expected: Vec<Node> = HQ
            .ring(1, width, height)
            .chain(HQ.ring(2, width, height))
            .collect();
        expected.sort_by_key(|node| (node.y, node.x));
        heard.sort_by_key(|node| (node.y, node.x));
        assert_eq!(heard, expected);

        let mut blocked = 0;
        think.survey(HQ, 3, true, |_, _| {
            blocked += 1;
            Visit::Block
        });
        assert_eq!(blocked, 6, "closed nodes are not flooded from");
    });
}

#[test]
fn military_buildings_weigh_as_the_threat_count_has_them() {
    let weights: Vec<u16> = [
        BuildingKind::HEADQUARTERS,
        BuildingKind::BARRACKS,
        BuildingKind::GUARDHOUSE,
        BuildingKind::WATCHTOWER,
        BuildingKind::FORTRESS,
        BuildingKind::CATAPULT,
        BuildingKind::LOOKOUT_TOWER,
        BuildingKind::WOODCUTTER,
    ]
    .into_iter()
    .map(military_weight)
    .collect();
    assert_eq!(weights, [12, 1, 2, 4, 6, 2, 0, 0]);
}

#[test]
fn the_threat_reads_a_building_s_owner_from_its_flag() {
    let mut game = computer_game();
    let MapObject::Building(hq) = game.world().object(HQ) else {
        panic!("no headquarters");
    };
    let flag = game.world().building(hq).unwrap().flag;
    let rival = Player::new(1).unwrap();
    let near = Node { x: HQ.x + 3, y: HQ.y };
    game.world.building_mut(hq).unwrap().owner = rival;
    with_think(&mut game, |_, think| {
        assert_eq!(think.threat(near), 0, "the flag is the player's");
    });
    game.world.building_mut(hq).unwrap().owner = player();
    game.world.flag_mut(flag).unwrap().owner = rival;
    with_think(&mut game, |_, think| {
        assert_eq!(
            think.threat(near),
            12,
            "a captured headquarters weighs for its taker"
        );
    });
}

#[test]
fn a_search_for_land_picks_a_node_off_the_player_s_land_where_a_small_building_fits() {
    let mut game = computer_game();
    with_think(&mut game, |_, think| {
        let target = ComputerPlayer::land(HQ, 0, think).unwrap();
        let data = think.game.nodes.get(target);
        assert_ne!(data.owner, Some(player()));
        assert!(fits(Size::Small, data.sites.size));
    });
}

#[test]
fn the_plan_picks_a_node_of_the_player_s_land_towards_the_target() {
    let mut game = computer_game();
    with_think(&mut game, |_, think| {
        let target = Node { x: 30, y: 16 };
        assert_ne!(think.game.nodes.get(target).owner, Some(player()));
        let chosen = think.plan(target).unwrap();
        assert_eq!(think.game.nodes.get(chosen).owner, Some(player()));
        assert!(chosen.x > HQ.x, "it lies on the target's side: {chosen:?}");
    });
}

#[test]
fn an_expansion_ends_in_a_military_site_on_the_player_s_land() {
    let mut game = computer_game();
    let target = Node { x: 30, y: 16 };
    let ai = game.computers.get_mut(player()).unwrap();
    ai.add(Job::expand(target, 0), None, None);
    for _ in 0..400 {
        game.step();
    }
    let military = game.world().buildings().any(|(_, building)| {
        building.owner == player()
            && game
                .world()
                .building_types()
                .get(building.kind)
                .is_some_and(|row| row.category == Category::Military)
            && building.kind != BuildingKind::HEADQUARTERS
    });
    assert!(military, "a barracks, guardhouse or watchtower site stands");
}
