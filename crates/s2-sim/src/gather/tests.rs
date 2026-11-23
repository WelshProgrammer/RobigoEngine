use super::{QUARRY_SEARCH, WOODCUTTER_SEARCH};
use crate::commands::Command;
use crate::figures::{FigureKind, Gatherer, Trip};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::homeward::HELD_TRIES;
use crate::job::Job;
use crate::settlers::tests::{game, hold, player};
use crate::transport::WareState;
use crate::ware::WareKind;
use crate::work::tool_for;
use crate::world::BuildingKind;
use crate::world::{BuildingState, Field, FieldStage, Granite, MapObject, Remnant, Tree};

/// A grown tree of type 0.
const GROWN: Tree = Tree {
    kind: 0,
    stage: Tree::GROWN,
    felling: false,
    phase: 0,
};

fn granite(size: u8) -> MapObject {
    MapObject::Granite(Granite {
        kind: 0,
        size,
        spare: 0,
    })
}

fn field(kind: u8, stage: FieldStage) -> MapObject {
    MapObject::Field(Field {
        stage,
        ..Field::sown(kind)
    })
}

/// A workplace of `kind` on `node`, finished and manned with nothing ordered, and its record.
pub(crate) fn manned(game: &mut Game, kind: BuildingKind, node: Node) -> u16 {
    game.execute(player(), Command::PlaceBuilding { node, kind })
        .unwrap();
    let MapObject::Building(record) = game.world().object(node) else {
        panic!("no building");
    };
    let building = game.world.building_mut(record).unwrap();
    building.state = BuildingState::FINISHED | BuildingState::MANNED | BuildingState::PRESENT;
    building.requested = 0;
    building.on_way = [0; 4];
    building.stock = [0; 4];
    record
}

fn step(node: Node, direction: Direction) -> Node {
    node.neighbour(direction, 48, 40)
}

#[test]
fn the_ring_search_starts_north_east_and_goes_round() {
    let mut game = game();
    let centre = Node { x: 30, y: 20 };
    for direction in [Direction::West, Direction::East] {
        game.world.set_object(step(centre, direction), granite(3));
    }
    let far = step(step(centre, Direction::NorthEast), Direction::NorthEast);
    game.world.set_object(far, granite(3));
    assert_eq!(
        game.search(centre, QUARRY_SEARCH),
        [step(centre, Direction::East), step(centre, Direction::West)],
        "the first two hits, in ring order"
    );
    game.world
        .set_object(step(centre, Direction::West), MapObject::Nothing);
    assert_eq!(
        game.search(centre, QUARRY_SEARCH),
        [step(centre, Direction::East), far]
    );
}

#[test]
fn only_grown_standing_fellable_trees_are_found() {
    let mut game = game();
    let centre = Node { x: 30, y: 20 };
    let trees = [
        (
            Direction::NorthEast,
            Tree {
                felling: true,
                ..GROWN
            },
        ),
        (Direction::East, Tree { stage: 2, ..GROWN }),
        (Direction::SouthEast, Tree { kind: 5, ..GROWN }),
        (Direction::SouthWest, GROWN),
    ];
    for (direction, tree) in trees {
        game.world
            .set_object(step(centre, direction), MapObject::Tree(tree));
    }
    assert_eq!(
        game.search(centre, WOODCUTTER_SEARCH),
        [step(centre, Direction::SouthWest)],
        "not falling, not growing, not type 5"
    );
}

#[test]
fn a_tree_falls_in_stages_and_leaves_a_stump() {
    let mut game = game();
    let node = Node { x: 30, y: 20 };
    let tree = Tree { phase: 5, ..GROWN };
    game.world.set_object(node, MapObject::Tree(tree));
    for stage in 0..4 {
        game.world.fell(&mut game.nodes, node, stage);
        let fallen = Tree {
            stage: u8::try_from(stage).unwrap(),
            felling: true,
            ..tree
        };
        assert_eq!(game.world().object(node), MapObject::Tree(fallen));
    }
    game.world.fell(&mut game.nodes, node, 4);
    let stump = MapObject::Remnant(Remnant::STUMP);
    assert_eq!(game.world().object(node), stump);
    game.world.fell(&mut game.nodes, node, 0);
    assert_eq!(game.world().object(node), stump, "only a tree falls");
}

#[test]
fn a_woodcutter_brings_wood_to_its_flag() {
    let mut game = game();
    let node = Node { x: 12, y: 14 };
    let record = manned(&mut game, BuildingKind::WOODCUTTER, node);
    let tree = step(step(node, Direction::West), Direction::West);
    game.world.set_object(tree, MapObject::Tree(GROWN));
    let flag = game.world().building(record).unwrap().flag;
    let mut went = None;
    while game.world().wares_at(flag).count() == 0 {
        game.step();
        if went.is_none()
            && game
                .world()
                .figures()
                .any(|(_, f)| matches!(f.kind, FigureKind::Gatherer(_)))
        {
            went = Some(game.clock());
        }
        assert!(game.clock().step() < 2000, "no wood");
    }
    assert!(went.unwrap().every(48, 2), "sent out on a building tick");
    let (_, wood) = game.world().wares_at(flag).next().unwrap();
    assert_eq!(wood.kind, WareKind::WOOD);
    assert_eq!(wood.state, WareState::Stranded, "no road to anywhere");
    while !game
        .world()
        .building(record)
        .unwrap()
        .state
        .contains(BuildingState::PRESENT)
    {
        game.step();
    }
    assert!(
        !game
            .world()
            .figures()
            .any(|(_, f)| matches!(f.kind, FigureKind::Gatherer(_)))
    );
}

#[test]
fn wood_is_counted_as_the_tree_comes_down_rather_than_at_the_flag() {
    let mut game = game();
    let node = Node { x: 12, y: 14 };
    let record = manned(&mut game, BuildingKind::WOODCUTTER, node);
    let tree = step(step(node, Direction::West), Direction::West);
    game.world.set_object(tree, MapObject::Tree(GROWN));
    let owner = game.world().building(record).unwrap().owner;
    let flag = game.world().building(record).unwrap().flag;
    while game.world().ware_arrivals(owner, WareKind::WOOD) == 0 {
        game.step();
        assert!(game.clock().step() < 2000, "no wood");
    }
    assert_eq!(game.world().ware_arrivals(owner, WareKind::WOOD), 1);
    assert_eq!(
        game.world().wares_at(flag).count(),
        0,
        "counted before the wood is out at the flag"
    );
}

#[test]
fn a_forester_plants_a_sapling() {
    let mut game = game();
    let node = Node { x: 12, y: 14 };
    manned(&mut game, BuildingKind::FORESTER, node);
    let sapling = |game: &Game| {
        (0..40)
            .flat_map(|y| (0..48).map(move |x| Node { x, y }))
            .find(|&n| game.world().object(n).tree().is_some_and(|tree| tree.stage == 0))
    };
    while sapling(&game).is_none() {
        game.step();
        assert!(game.clock().step() < 2000, "nothing planted");
    }
    let at = sapling(&game).unwrap();
    assert!(at.x.abs_diff(node.x) <= 6 && at.y.abs_diff(node.y) <= 6);
}

#[test]
fn a_quarry_with_no_granite_gives_up_for_good() {
    let mut game = game();
    let record = manned(&mut game, BuildingKind::QUARRY, Node { x: 12, y: 14 });
    game.world.building_mut(record).unwrap().productivity = 50;
    while !game.world().building(record).unwrap().exhausted {
        game.step();
        assert!(game.clock().step() < 200);
    }
    assert_eq!(game.world().building(record).unwrap().productivity, 0);
    game.world.set_object(Node { x: 14, y: 14 }, granite(1));
    for _ in 0..500 {
        game.step();
    }
    assert!(
        !game
            .world()
            .figures()
            .any(|(_, f)| matches!(f.kind, FigureKind::Gatherer(_)))
    );
}

#[test]
fn a_stonemason_takes_the_last_piece_of_granite() {
    let mut game = game();
    let node = Node { x: 12, y: 14 };
    let record = manned(&mut game, BuildingKind::QUARRY, node);
    let granite = step(step(node, Direction::East), Direction::East);
    game.world.set_object(granite, self::granite(1));
    let flag = game.world().building(record).unwrap().flag;
    while game.world().wares_at(flag).count() == 0 {
        game.step();
        assert!(game.clock().step() < 2000, "no stone");
    }
    assert_eq!(
        game.world().wares_at(flag).next().unwrap().1.kind,
        WareKind::STONES
    );
    assert_eq!(game.world().object(granite), MapObject::Nothing);
}

#[test]
fn a_tool_is_picked_by_spans_that_share_their_ends() {
    let weights = [10, 40, 20, 50, 70, 10, 30, 10, 20, 10, 20, 10];
    let picks: Vec<WareKind> = [0, 10, 11, 50, 51, 300]
        .iter()
        .map(|&roll| tool_for(&weights, roll))
        .collect();
    assert_eq!(
        picks,
        [
            WareKind::TONGS,
            WareKind::TONGS,
            WareKind::AXE,
            WareKind::AXE,
            WareKind::SAW,
            WareKind::BOW
        ]
    );
    assert_eq!(tool_for(&[0; 12], 0), WareKind::TONGS);
    assert_eq!(
        tool_for(&[0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 0),
        WareKind::TONGS,
        "the first tool wins 0"
    );
}

#[test]
fn a_farmer_sows_a_field_with_nothing_ripe_nearby() {
    let mut game = game();
    let node = Node { x: 12, y: 14 };
    manned(&mut game, BuildingKind::FARM, node);
    let field = |game: &Game| {
        (0..40)
            .flat_map(|y| (0..48).map(move |x| Node { x, y }))
            .find(|&n| game.world().field_stage(n).is_some())
    };
    while field(&game).is_none() {
        game.step();
        assert!(game.clock().step() < 2000, "nothing sown");
    }
    let at = field(&game).unwrap();
    assert_eq!(game.world().field_stage(at), Some(FieldStage::Sown));
    assert!(
        [self::field(0, FieldStage::Sown), self::field(1, FieldStage::Sown)]
            .contains(&game.world().object(at)),
        "type 0 or 1, stage 0"
    );
    assert!(at.x.abs_diff(node.x) <= 2 && at.y.abs_diff(node.y) <= 2);
}

#[test]
fn a_farmer_harvests_a_ripe_field_into_grain_and_stubble() {
    let mut game = game();
    let node = Node { x: 12, y: 14 };
    let record = manned(&mut game, BuildingKind::FARM, node);
    let field = step(step(node, Direction::East), Direction::East);
    game.world.set_object(field, self::field(1, FieldStage::Ripe));
    let flag = game.world().building(record).unwrap().flag;
    while game.world().wares_at(flag).count() == 0 {
        game.step();
        assert!(game.clock().step() < 2000, "no grain");
    }
    assert_eq!(
        game.world().wares_at(flag).next().unwrap().1.kind,
        WareKind::GRAIN
    );
    assert_eq!(
        game.world().object(field),
        MapObject::Remnant(Field::sown(1).harvested()),
        "type 1's stubble"
    );
    let farmer = game.world().figures().find_map(|(_, f)| match f.kind {
        FigureKind::Gatherer(gatherer) => Some(gatherer.job),
        _ => None,
    });
    assert_eq!(farmer, Some(Job::FARMER), "back to the farmer's own job");
}

#[test]
fn an_unripe_field_is_not_harvested() {
    let mut game = game();
    let centre = Node { x: 30, y: 20 };
    game.world
        .set_object(step(centre, Direction::East), field(0, FieldStage::Growing));
    let found = game.search(centre, super::HARVEST_SEARCH);
    assert!(found.is_empty(), "{found:?}");
    game.world
        .set_object(step(centre, Direction::East), field(0, FieldStage::Ripe));
    assert_eq!(
        game.search(centre, super::HARVEST_SEARCH),
        [step(centre, Direction::East)]
    );
}

const WATER: u8 = 5;

/// Fills `x` 4 to 9 of rows 10 to 16 with water and returns the all-water node furthest east
/// in row 14, the one nearest a fishery on (12, 14).
pub(crate) fn lake(game: &mut Game) -> Node {
    for y in 10..=16 {
        for x in 4..=9 {
            let at = Node { x, y };
            let mut data = game.nodes.get(at);
            data.terrain = [WATER; 2];
            game.nodes.set(at, data);
        }
    }
    crate::sites::refresh_sites(&mut game.nodes, &game.world, Node { x: 7, y: 13 }, 6);
    (4..=9)
        .rev()
        .map(|x| Node { x, y: 14 })
        .find(|&at| {
            game.nodes
                .property(at)
                .contains(crate::ground::Property::ALL_WATER)
        })
        .unwrap()
}

fn set_resources(game: &mut Game, node: Node, byte: u8) {
    let mut data = game.nodes.get(node);
    data.resources = crate::nodes::Resources::from_byte(byte);
    game.nodes.set(node, data);
}

#[test]
fn fish_is_found_in_open_water_with_no_mineral_bits_and_some_left() {
    let mut game = game();
    let water = lake(&mut game);
    let centre = Node { x: 12, y: 14 };
    for byte in [0x80, 0x8f] {
        set_resources(&mut game, water, byte);
        assert!(game.search(centre, super::FISHERY_SEARCH).is_empty(), "{byte:#x}");
    }
    set_resources(&mut game, water, 0x27);
    assert_eq!(
        game.search(centre, super::FISHERY_SEARCH),
        [water],
        "the kind is not looked at"
    );
    set_resources(&mut game, water, 0);
    let shore = step(water, Direction::East);
    set_resources(&mut game, shore, 0x87);
    assert!(
        game.search(centre, super::FISHERY_SEARCH).is_empty(),
        "not all water"
    );
}

#[test]
fn a_fisher_fishes_from_the_shore_and_each_catch_takes_two() {
    let mut game = game();
    let water = lake(&mut game);
    set_resources(&mut game, water, 0x87);
    let record = manned(&mut game, BuildingKind::FISHERY, Node { x: 12, y: 14 });
    let flag = game.world().building(record).unwrap().flag;
    while game.world().wares_at(flag).count() == 0 {
        game.step();
        for (_, figure) in game.world().figures() {
            if matches!(figure.kind, FigureKind::Gatherer(_)) {
                let at = figure.node();
                assert!(
                    !game
                        .nodes
                        .property(at)
                        .contains(crate::ground::Property::ALL_WATER),
                    "on the water at {at:?}"
                );
            }
        }
        assert!(game.clock().step() < 2000, "no fish");
    }
    assert_eq!(game.world().wares_at(flag).next().unwrap().1.kind, WareKind::FISH);
    assert_eq!(game.nodes.get(water).resources, crate::nodes::Resources::Fish(5));
    let fisher = game.world().figures().find_map(|(_, f)| match f.kind {
        FigureKind::Gatherer(gatherer) => Some(gatherer.job),
        _ => None,
    });
    assert_eq!(fisher, Some(Job::FISHER));
}

#[test]
fn seven_fish_give_four_catches_and_leave_none() {
    let mut game = game();
    let water = lake(&mut game);
    set_resources(&mut game, water, 0x87);
    let record = manned(&mut game, BuildingKind::FISHERY, Node { x: 12, y: 14 });
    while !game.world().building(record).unwrap().exhausted {
        game.step();
        assert!(game.clock().step() < 20_000);
    }
    let flag = game.world().building(record).unwrap().flag;
    assert_eq!(
        game.world()
            .wares_at(flag)
            .filter(|(_, w)| w.kind == WareKind::FISH)
            .count(),
        4
    );
    assert_eq!(game.nodes.get(water).resources, crate::nodes::Resources::Fish(0));
}

/// Steps the whole game until a gatherer stands on its cross-country path, and returns its
/// figure, its path record and the node its next edge leads to.
fn on_its_way(game: &mut Game) -> (u16, u16, Node) {
    for _ in 0..2000 {
        game.step();
        let walking = game
            .world()
            .figures()
            .find_map(|(record, figure)| match figure.kind {
                FigureKind::Gatherer(Gatherer {
                    trip: Trip::Walking { path },
                    ..
                }) if !figure.walking() => Some((record, path, figure.node())),
                _ => None,
            });
        if let Some((record, path, node)) = walking {
            let mut ahead = game.world.paths.get(path).copied().unwrap();
            let direction = ahead.next().unwrap();
            return (record, path, step(node, direction));
        }
    }
    panic!("no gatherer on its way");
}

#[test]
fn a_held_node_stops_a_woodcutter_until_it_clears() {
    let mut game = game();
    let node = Node { x: 12, y: 14 };
    manned(&mut game, BuildingKind::WOODCUTTER, node);
    let tree = step(step(node, Direction::West), Direction::West);
    game.world.set_object(tree, MapObject::Tree(GROWN));
    let (record, _, next) = on_its_way(&mut game);
    let standing = game.world().figure(record).unwrap().node();
    hold(&mut game, next, true);
    game.step_settler(record);
    let figure = game.world().figure(record).copied().unwrap();
    assert_eq!(figure.node(), standing, "it stays where it stands");
    assert_eq!(figure.waits, 1);
    hold(&mut game, next, false);
    game.step_settler(record);
    let figure = game.world().figure(record).copied().unwrap();
    assert_eq!(figure.node(), next, "it takes the edge in the step it clears");
    assert_eq!(figure.waits, 0);
}

#[test]
fn a_woodcutter_with_no_other_way_keeps_its_path_and_waits_again() {
    let mut game = game();
    let node = Node { x: 12, y: 14 };
    manned(&mut game, BuildingKind::WOODCUTTER, node);
    let tree = step(step(node, Direction::West), Direction::West);
    game.world.set_object(tree, MapObject::Tree(GROWN));
    let (record, path, next) = on_its_way(&mut game);
    let standing = game.world().figure(record).unwrap().node();
    let (width, height) = (game.nodes.width(), game.nodes.height());
    for direction in Direction::ALL {
        let around = standing.neighbour(direction, width, height);
        game.world.add_figure(
            around,
            crate::figures::Figure::animal(crate::figures::Species::Fox),
        );
    }
    hold(&mut game, next, true);
    let walked = game.world.paths.get(path).copied().unwrap();
    for _ in 0..=HELD_TRIES {
        game.step_settler(record);
    }
    let figure = game.world().figure(record).copied().unwrap();
    assert_eq!(figure.node(), standing, "it stays where it stands");
    assert_eq!(
        figure.waits, 1,
        "the count starts again, the next search 20 steps on"
    );
    assert_eq!(
        game.world.paths.get(path).copied(),
        Some(walked),
        "its path is left as it was"
    );
}

#[test]
fn a_woodcutter_held_for_twenty_one_steps_looks_for_another_way() {
    let mut game = game();
    let node = Node { x: 12, y: 14 };
    manned(&mut game, BuildingKind::WOODCUTTER, node);
    let tree = step(step(node, Direction::West), Direction::West);
    game.world.set_object(tree, MapObject::Tree(GROWN));
    let (record, _, next) = on_its_way(&mut game);
    hold(&mut game, next, true);
    for waits in 1..=HELD_TRIES {
        let drawn = game.rng;
        game.step_settler(record);
        assert_eq!(game.world().figure(record).unwrap().waits, waits);
        assert_eq!(game.rng, drawn, "no search while it waits");
    }
    let drawn = game.rng;
    game.step_settler(record);
    assert_ne!(game.rng, drawn, "the twenty-first step searches");
    assert_eq!(
        game.world().figure(record).unwrap().waits,
        0,
        "the count starts again, so a new route held too waits a full count"
    );
}

#[test]
fn a_gatherer_whose_path_runs_out_short_of_its_target_searches_again_and_walks_on() {
    let mut game = game();
    let node = Node { x: 12, y: 14 };
    manned(&mut game, BuildingKind::WOODCUTTER, node);
    let tree = step(step(node, Direction::West), Direction::West);
    game.world.set_object(tree, MapObject::Tree(GROWN));
    let (record, path, _) = on_its_way(&mut game);
    let target = game.world.paths.get(path).unwrap().to;
    *game.world.paths.get_mut(path).unwrap() = crate::homeward::Path::empty(target);
    game.step_settler(record);
    let figure = game.world().figure(record).copied().unwrap();
    assert!(matches!(
        figure.kind,
        FigureKind::Gatherer(Gatherer {
            trip: Trip::Walking { path: kept },
            ..
        }) if kept == path
    ));
    assert!(figure.walking(), "it starts the new route's first edge at once");
    assert_eq!(game.world.paths.get(path).unwrap().to, target);
}
