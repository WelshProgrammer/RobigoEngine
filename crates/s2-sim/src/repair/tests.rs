use super::{BeyondRepair, Repair, Repaired};
use crate::commands::Command;
use crate::figures::FigureKind;
use crate::game::Game;
use crate::gather::tests::manned;
use crate::grid::Node;
use crate::job::Job;
use crate::settlers::tests::{HQ_FLAG, game, player, road_east};
use crate::transport::Wanted;
use crate::transport::WareState;
use crate::ware::WareKind;
use crate::world::{BuildingKind, BuildingState, BuildingType, BuildingTypes, Category, MapObject, Size};

const SAWMILL: Node = Node { x: 12, y: 14 };

fn found(kind: BuildingKind, node: Node, site: bool, what: Repair) -> Repaired {
    Repaired {
        kind,
        node,
        owner: player(),
        site,
        what,
    }
}

/// Changes the row of `kind` in the game's building types and returns the table as it was.
fn change(game: &mut Game, kind: BuildingKind, change: impl Fn(&mut BuildingType)) -> BuildingTypes {
    let saved = game.world().building_types().clone();
    let mut types = saved.types().to_vec();
    change(&mut types[kind.index()]);
    game.world.set_building_types(BuildingTypes::new(types).unwrap());
    saved
}

/// The same for the sawmill.
fn change_sawmill(game: &mut Game, with: impl Fn(&mut BuildingType)) -> BuildingTypes {
    change(game, BuildingKind::SAWMILL, with)
}

/// A game with a manned sawmill that has ordered its first wood.
fn with_wood_ordered() -> (Game, u16) {
    let mut game = game();
    let record = manned(&mut game, BuildingKind::SAWMILL, SAWMILL);
    while game.world().building(record).unwrap().on_way[0] == 0 {
        game.step();
        assert!(game.clock().step() < 300);
    }
    (game, record)
}

#[test]
fn a_state_made_with_these_tables_is_left_alone() {
    let (mut game, _) = with_wood_ordered();
    let before = game.state_hash();
    let saved = game.world().building_types().clone();
    assert_eq!(game.repair(&saved), []);
    assert_eq!(game.state_hash(), before);
}

#[test]
fn stock_above_a_lowered_capacity_is_cut_to_it_and_comes_off_the_owners_count() {
    let mut game = game();
    let record = manned(&mut game, BuildingKind::SAWMILL, SAWMILL);
    game.world.building_mut(record).unwrap().stock[0] = 6;
    let owned = game.world().owned_wares(player(), WareKind::WOOD);
    let saved = change_sawmill(&mut game, |row| row.capacity[0] = 2);
    assert_eq!(
        game.repair(&saved),
        [found(
            BuildingKind::SAWMILL,
            SAWMILL,
            false,
            Repair::Overstock {
                ware: WareKind::WOOD,
                kept: 2
            }
        )]
    );
    assert_eq!(game.world().building(record).unwrap().stock[0], 2);
    assert_eq!(
        game.world().owned_wares(player(), WareKind::WOOD),
        owned.wrapping_sub(4)
    );
    assert_eq!(game.repair(&saved), [], "a second pass finds nothing");
}

#[test]
fn an_order_for_a_ware_the_building_no_longer_takes_goes() {
    let (mut game, record) = with_wood_ordered();
    let flag = game.world().building(record).unwrap().flag;
    let saved = change_sawmill(&mut game, |row| {
        row.inputs[0] = Some(WareKind::STONES);
    });
    assert_eq!(
        game.repair(&saved),
        [found(
            BuildingKind::SAWMILL,
            SAWMILL,
            false,
            Repair::Orders { cancelled: 1 }
        )]
    );
    let building = game.world().building(record).unwrap();
    assert_eq!((building.on_way, building.requested), ([0; 4], 0));
    let wares_wanted = |game: &Game| {
        game.world()
            .orders()
            .filter(|(_, order)| order.flag == flag && order.ware().is_some())
            .count()
    };
    assert_eq!(wares_wanted(&game), 0);
    for _ in 0..600 {
        game.step();
    }
    let building = game.world().building(record).unwrap();
    assert_eq!(building.on_way[0], 1, "it orders the new ware");
}

#[test]
fn orders_past_a_lowered_capacity_go_and_their_wares_are_stranded() {
    let (mut game, record) = with_wood_ordered();
    let flag = game.world().building(record).unwrap().flag;
    let on_way: Vec<u16> = game
        .world()
        .orders()
        .filter(|(_, order)| order.flag == flag)
        .filter_map(|(_, order)| order.on_way)
        .collect();
    game.world.building_mut(record).unwrap().stock[0] = 2;
    let saved = change_sawmill(&mut game, |row| row.capacity[0] = 2);
    assert_eq!(
        game.repair(&saved),
        [found(
            BuildingKind::SAWMILL,
            SAWMILL,
            false,
            Repair::Orders { cancelled: 1 }
        )]
    );
    for ware in on_way {
        let ware = game.world().ware(ware).unwrap();
        assert!(matches!(ware.state, WareState::Stranded | WareState::Unrouted));
        assert_eq!(ware.order, None);
    }
    let building = game.world().building(record).unwrap();
    assert_eq!(
        (building.stock[0], building.on_way[0], building.requested),
        (2, 0, 0)
    );
}

#[test]
fn the_stock_of_a_slot_whose_ware_changed_goes_as_the_ware_it_was() {
    let mut game = game();
    let record = manned(&mut game, BuildingKind::SAWMILL, SAWMILL);
    game.world.building_mut(record).unwrap().stock[0] = 3;
    let owned = game.world().owned_wares(player(), WareKind::WOOD);
    let saved = change_sawmill(&mut game, |row| row.inputs[0] = Some(WareKind::STONES));
    assert_eq!(
        game.repair(&saved),
        [found(
            BuildingKind::SAWMILL,
            SAWMILL,
            false,
            Repair::WrongStock {
                ware: Some(WareKind::WOOD),
                lost: 3
            }
        )]
    );
    assert_eq!(game.world().building(record).unwrap().stock[0], 0);
    assert_eq!(
        game.world().owned_wares(player(), WareKind::WOOD),
        owned.wrapping_sub(3)
    );
}

#[test]
fn a_workplace_with_another_worker_loses_the_old_one_and_orders_the_new() {
    let (mut game, record) = with_wood_ordered();
    let flag = game.world().building(record).unwrap().flag;
    let saved = change_sawmill(&mut game, |row| row.worker = Some(Job::MILLER));
    assert_eq!(
        game.repair(&saved),
        [found(BuildingKind::SAWMILL, SAWMILL, false, Repair::Worker)]
    );
    let building = game.world().building(record).unwrap();
    assert!(
        !building
            .state
            .intersects(BuildingState::MANNED | BuildingState::PRESENT)
    );
    assert_eq!((building.on_way, building.requested), ([0; 4], 0));
    let lost = game.world().figures_at(SAWMILL).any(|(_, figure)| {
        matches!(
            figure.kind,
            FigureKind::Lost {
                job: Job::CARPENTER,
                ..
            }
        )
    });
    assert!(lost, "the carpenter is a lost settler on the building's node");
    let orders: Vec<_> = game
        .world()
        .orders()
        .filter(|(_, order)| order.flag == flag)
        .map(|(_, order)| order.wanted)
        .collect();
    assert_eq!(orders, [Wanted::Person(Job::MILLER)]);
    assert_eq!(game.repair(&game.world().building_types().clone()), []);
    for _ in 0..2000 {
        game.step();
    }
}

/// A barracks on `node` with two privates inside.
fn barracks(game: &mut Game, node: Node) -> u16 {
    let record = manned(game, BuildingKind::BARRACKS, node);
    game.start_military(record);
    let (military, _) = game.world().military_of(record).unwrap();
    let held = game.world.military.get_mut(military).unwrap();
    held.inside = 2;
    held.ranks[0] = 2;
    record
}

#[test]
fn soldiers_above_a_lowered_capacity_walk_home() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 6);
    let record = barracks(&mut game, Node { x: 22, y: 16 });
    let saved = change(&mut game, BuildingKind::BARRACKS, |row| row.soldiers = 1);
    assert_eq!(
        game.repair(&saved),
        [found(
            BuildingKind::BARRACKS,
            Node { x: 22, y: 16 },
            false,
            Repair::Soldiers { left: 1 }
        )]
    );
    let (_, held) = game.world().military_of(record).unwrap();
    assert_eq!((held.capacity, held.inside, held.ranks[0]), (1, 1, 1));
    let flag = game.world().building(record).unwrap().flag;
    let flag = game.world().flag(flag).unwrap().node;
    let leaving = game
        .world()
        .figures_at(flag)
        .any(|(_, figure)| matches!(figure.kind, FigureKind::Porter(porter) if porter.job == Job::PRIVATE));
    assert!(leaving, "one private walks out of the door");
    assert_eq!(game.repair(&saved), [], "a second pass finds nothing");
}

#[test]
fn soldiers_with_no_way_home_are_gone() {
    let mut game = game();
    let record = barracks(&mut game, SAWMILL);
    let figures = game.world().figures().count();
    let saved = change(&mut game, BuildingKind::BARRACKS, |row| row.soldiers = 1);
    assert_eq!(game.repair(&saved).len(), 1);
    let (_, held) = game.world().military_of(record).unwrap();
    assert_eq!((held.inside, held.ranks[0]), (1, 1));
    assert_eq!(game.world().figures().count(), figures);
}

/// A sawmill site that has ordered its two boards and two stones, with both boards and one
/// stone arrived and the other stone still on order.
fn site(game: &mut Game) -> u16 {
    game.execute(
        player(),
        Command::PlaceBuilding {
            node: SAWMILL,
            kind: BuildingKind::SAWMILL,
        },
    )
    .unwrap();
    let MapObject::Building(record) = game.world().object(SAWMILL) else {
        panic!("no site");
    };
    let flag = game.world().building(record).unwrap().flag;
    game.world.order_ware(flag, WareKind::STONES, 0);
    let site = game.world.building_mut(record).unwrap();
    assert!(site.state.contains(BuildingState::BUILDING));
    site.stock = [2, 1, 0, 0];
    site.on_way = [2, 1, 0, 0];
    site.requested = 8;
    record
}

fn wares_ordered(game: &Game, record: u16) -> Vec<WareKind> {
    let flag = game.world().building(record).unwrap().flag;
    game.world()
        .orders()
        .filter(|(_, order)| order.flag == flag)
        .filter_map(|(_, order)| order.ware())
        .collect()
}

#[test]
fn a_site_keeps_what_its_type_now_costs() {
    let mut game = game();
    let record = site(&mut game);
    let saved = change_sawmill(&mut game, |row| (row.boards, row.stones) = (1, 1));
    assert_eq!(
        game.repair(&saved),
        [
            found(
                BuildingKind::SAWMILL,
                SAWMILL,
                true,
                Repair::Materials { lost: 1 }
            ),
            found(
                BuildingKind::SAWMILL,
                SAWMILL,
                true,
                Repair::Orders { cancelled: 1 }
            ),
        ]
    );
    let site = game.world().building(record).unwrap();
    assert_eq!((site.stock, site.on_way), ([1, 1, 0, 0], [1, 1, 0, 0]));
    assert_eq!(wares_ordered(&game, record), []);
    assert_eq!(game.repair(&saved), [], "a second pass finds nothing");
}

#[test]
fn a_site_whose_cost_has_risen_orders_the_difference() {
    let mut game = game();
    let record = site(&mut game);
    let saved = change_sawmill(&mut game, |row| row.boards = 4);
    assert_eq!(game.repair(&saved), []);
    assert_eq!(
        wares_ordered(&game, record),
        [WareKind::STONES, WareKind::BOARDS, WareKind::BOARDS]
    );
}

#[test]
fn a_stopped_workplace_that_can_no_longer_be_stopped_works_again() {
    let mut game = game();
    let record = manned(&mut game, BuildingKind::SAWMILL, SAWMILL);
    game.world.building_mut(record).unwrap().state |= BuildingState::STOPPED;
    let saved = change_sawmill(&mut game, |row| row.stoppable = false);
    assert_eq!(
        game.repair(&saved),
        [found(BuildingKind::SAWMILL, SAWMILL, false, Repair::Restarted)]
    );
    let state = game.world().building(record).unwrap().state;
    assert!(!state.contains(BuildingState::STOPPED));
}

#[test]
fn a_building_whose_type_changed_size_is_beyond_repair() {
    let mut game = game();
    let saved = change_sawmill(&mut game, |row| row.size = Size::Large);
    let same = game.world().building_types().clone();
    assert_eq!(game.beyond_repair(&saved), None, "no sawmill stands");
    manned(&mut game, BuildingKind::SAWMILL, SAWMILL);
    assert_eq!(
        game.beyond_repair(&saved),
        Some(BeyondRepair::Size(BuildingKind::SAWMILL))
    );
    assert_eq!(game.beyond_repair(&same), None);
}

#[test]
fn an_occupied_military_building_whose_type_changed_strength_is_beyond_repair() {
    let mut game = game();
    let record = barracks(&mut game, SAWMILL);
    let saved = change(&mut game, BuildingKind::BARRACKS, |row| row.strength -= 1);
    assert_eq!(
        game.beyond_repair(&saved),
        Some(BeyondRepair::Strength(BuildingKind::BARRACKS))
    );
    game.world.building_mut(record).unwrap().state -= BuildingState::MANNED;
    assert_eq!(game.beyond_repair(&saved), None, "nobody has occupied it");
}

#[test]
fn a_building_of_another_category_is_demolished() {
    let mut game = game();
    manned(&mut game, BuildingKind::SAWMILL, SAWMILL);
    let saved = change_sawmill(&mut game, |row| row.category = Category::Military);
    assert_eq!(
        game.repair(&saved),
        [found(BuildingKind::SAWMILL, SAWMILL, false, Repair::Removed)]
    );
    assert!(!matches!(game.world().object(SAWMILL), MapObject::Building(_)));
}
