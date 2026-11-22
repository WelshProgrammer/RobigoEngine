use super::*;
use crate::computer::chain::tests::computer_game;
use crate::settlers::tests::player;

#[test]
fn the_only_tool_short_gets_the_whole_weight_and_the_rest_none() {
    let mut game = computer_game();
    let held = game.world.people(player(), Job::WOODCUTTER) + game.world.owned_wares(player(), WareKind::AXE);
    for _ in 0..held + 3 {
        game.world.count_site(player(), BuildingKind::WOODCUTTER);
    }
    game.tool_pass(player());
    let weights = game.world.distribution(player()).tools().to_owned();
    let axe = WareKind::AXE.tool().unwrap();
    for (slot, &weight) in weights.iter().enumerate() {
        assert_eq!(weight, if slot == axe { 100 } else { 0 }, "tool {slot}");
    }
}

#[test]
fn nothing_short_leaves_the_weights_alone() {
    let mut game = computer_game();
    let before = game.world.distribution(player()).tools().to_owned();
    game.tool_pass(player());
    assert_eq!(game.world.distribution(player()).tools(), &before[..]);
}
