use super::*;

#[test]
fn only_player_0_starts_with_a_leftover_group() {
    let leftovers = Player::ALL.map(leftover_at_start);
    assert_eq!(leftovers, [0, 92, 184, 20, 112, 204, 40]);
}

#[test]
fn ranking_a_group_swaps_places_and_rewrites_both_groups_values() {
    let mut world = World::empty(4, 4);
    let player = Player::ALL[1];
    let coins = transport_group(WareKind::COINS).unwrap();
    let wood = transport_group(WareKind::WOOD).unwrap();
    let wood_rank = world.transport_ranks[1][usize::from(wood)];
    let swapped = world.rank_transport(player, wood, 0, u8::MAX);
    assert_eq!(swapped, coins);
    assert_eq!(world.transport_ranks[1][usize::from(coins)], wood_rank);
    assert_eq!(world.priorities[1][WareKind::WOOD.index()], 75);
    assert_eq!(world.priorities[1][WareKind::COINS.index()], priority(wood_rank));
}

#[test]
fn a_reset_gives_the_leftover_groups_wares_the_last_groups_value() {
    let mut world = World::empty(4, 4);
    let player = Player::ALL[1];
    let stones = transport_group(WareKind::STONES).unwrap();
    world.rank_transport(player, stones, 0, u8::MAX);
    let leftover = world.reset_transport(player, stones);
    assert_eq!(leftover, stones);
    assert_eq!(world.transport_ranks[1], *world.economy.ranks());
    assert_eq!(world.priorities[1][WareKind::STONES.index()], priority(13));
    assert_eq!(world.priorities[1][WareKind::BOARDS.index()], 40);
}

mod commands {
    use crate::commands::{Command, Refusal};
    use crate::gather::tests::manned;
    use crate::grid::Node;
    use crate::settlers::tests::{HQ, game, player};
    use crate::ware::WareKind;
    use crate::world::{BuildingKind, BuildingState, Player};

    use super::super::{MAX_WEIGHT, transport_group};

    #[test]
    fn a_stopped_workplace_wants_nothing_until_restarted() {
        let mut game = game();
        let node = Node { x: 12, y: 14 };
        let record = manned(&mut game, BuildingKind::SAWMILL, node);
        let stop = |stop| Command::StopProduction { node, stop };
        let rival = Player::new(1).unwrap();
        assert_eq!(game.execute(rival, stop(true)), Err(Refusal::NotYours));
        game.execute(player(), stop(true)).unwrap();
        let state = game.world().building(record).unwrap().state;
        assert!(state.contains(BuildingState::STOPPED));
        assert_eq!(game.world().demand(record, WareKind::WOOD), 0);
        game.execute(player(), stop(false)).unwrap();
        assert!(game.world().demand(record, WareKind::WOOD) > 0);
    }

    #[test]
    fn only_finished_workplaces_that_make_something_offer_a_stop() {
        let mut game = game();
        let stop = |node| Command::StopProduction { node, stop: true };
        assert_eq!(game.execute(player(), stop(HQ)), Err(Refusal::NotOffered));
        let tower = Node { x: 12, y: 14 };
        manned(&mut game, BuildingKind::LOOKOUT_TOWER, tower);
        assert_eq!(game.execute(player(), stop(tower)), Err(Refusal::NotOffered));
        let forester = Node { x: 20, y: 14 };
        manned(&mut game, BuildingKind::FORESTER, forester);
        assert_eq!(game.execute(player(), stop(forester)), Ok(()));
    }

    #[test]
    fn a_food_slider_sets_all_three_foods_and_a_reset_puts_them_back() {
        let mut game = game();
        let set = |group, consumer, weight| Command::SetDistribution {
            group,
            consumer,
            weight,
        };
        game.execute(player(), set(0, 0, 70)).unwrap();
        let world = game.world();
        assert_eq!(world.slider(player(), 0, 0), Some(70));
        let granite = BuildingKind::GRANITE_MINE;
        for food in [WareKind::FISH, WareKind::BREAD, WareKind::MEAT] {
            assert_eq!(world.distribution(player()).weight(granite, food), 70);
        }
        assert_eq!(
            world.slider(player(), 0, 3),
            Some(100),
            "the gold mine is left alone"
        );
        assert_eq!(game.execute(player(), set(0, 4, 50)), Err(Refusal::OutOfRange));
        assert_eq!(game.execute(player(), set(6, 0, 50)), Err(Refusal::OutOfRange));
        assert_eq!(
            game.execute(player(), set(0, 0, MAX_WEIGHT + 1)),
            Err(Refusal::OutOfRange)
        );
        game.execute(player(), Command::ResetDistribution { group: 0 })
            .unwrap();
        assert_eq!(game.world().slider(player(), 0, 0), Some(30));
        assert_eq!(
            game.world()
                .distribution(player())
                .weight(granite, WareKind::FISH),
            30
        );
    }

    #[test]
    fn the_boards_page_sets_a_sites_weight_for_boards_only() {
        let mut game = game();
        let command = Command::SetDistribution {
            group: 4,
            consumer: 0,
            weight: 20,
        };
        game.execute(player(), command).unwrap();
        let settings = game.world().distribution(player());
        assert_eq!(settings.construction(WareKind::BOARDS), 20);
        assert_eq!(settings.construction(WareKind::STONES), 100);
    }

    #[test]
    fn tool_weights_change_one_at_a_time_and_reset_together() {
        let mut game = game();
        let set = |tool, weight| Command::SetToolWeight { tool, weight };
        game.execute(player(), set(WareKind::AXE, 0)).unwrap();
        game.execute(player(), set(WareKind::BOW, 100)).unwrap();
        let tools = *game.world().distribution(player()).tools();
        assert_eq!((tools[1], tools[11]), (0, 100));
        assert_eq!(
            game.execute(player(), set(WareKind::WOOD, 10)),
            Err(Refusal::OutOfRange)
        );
        game.execute(player(), Command::ResetToolWeights).unwrap();
        let tools = *game.world().distribution(player()).tools();
        assert_eq!(&tools, game.world().economy_tables().tools());
    }

    #[test]
    fn moving_a_group_to_the_top_shifts_the_ones_above_it_down() {
        let mut game = game();
        let wood = transport_group(WareKind::WOOD).unwrap();
        let before = *game.world().transport_ranks(player());
        let command = Command::MoveTransport { group: wood, rank: 0 };
        game.execute(player(), command).unwrap();
        let after = *game.world().transport_ranks(player());
        for (group, (&old, &new)) in before.iter().zip(&after).enumerate() {
            let expected = match old {
                _ if group == usize::from(wood) => 0,
                rank if rank < before[usize::from(wood)] => rank + 1,
                rank => rank,
            };
            assert_eq!(new, expected, "group {group}");
        }
        let values = game.world().transport_priorities(player());
        assert_eq!(values[WareKind::WOOD.index()], 75);
        assert_eq!(values[WareKind::COINS.index()], 70);
        let past = Command::MoveTransport {
            group: wood,
            rank: 14,
        };
        assert_eq!(game.execute(player(), past), Err(Refusal::OutOfRange));
    }

    #[test]
    fn the_transport_default_puts_every_value_back_even_player_0s_wood() {
        let mut game = game();
        let coins = transport_group(WareKind::COINS).unwrap();
        let wood = WareKind::WOOD.index();
        assert_eq!(game.world().transport_priorities(player())[wood], 10);
        game.execute(
            player(),
            Command::MoveTransport {
                group: coins,
                rank: 13,
            },
        )
        .unwrap();
        game.execute(player(), Command::ResetTransport).unwrap();
        let world = game.world();
        assert_eq!(world.transport_ranks(player()), world.economy_tables().ranks());
        let values = world.transport_priorities(player());
        assert_eq!(values[WareKind::COINS.index()], 75);
        assert_eq!(values[wood], 30);
        assert_eq!(values[WareKind::BOARDS.index()], 40);
    }
}
