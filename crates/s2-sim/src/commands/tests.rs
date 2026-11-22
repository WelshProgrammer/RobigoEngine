use super::*;
use crate::nodes::{NodeData, NodeFlags};
use crate::sites::site;
use crate::world::StartGoods;

const WIDTH: u16 = 48;
const HEIGHT: u16 = 40;
const MEADOW: u8 = 8;
const WATER: u8 = 5;
const HQ: Node = Node { x: 16, y: 16 };
const HQ_FLAG: Node = Node { x: 16, y: 17 };
const RIVAL_HQ: Node = Node { x: 36, y: 16 };

fn player(index: u8) -> Player {
    Player::new(index).unwrap()
}

fn node(x: u16, y: u16) -> Node {
    Node { x, y }
}

/// Meadow with slot 8 worked out everywhere, player 0's headquarters at [`HQ`] and player 1's at
/// [`RIVAL_HQ`].
fn game() -> Game {
    let data = NodeData {
        altitude: 10,
        terrain: [MEADOW; 2],
        ..NodeData::default()
    };
    let mut nodes = NodeMap::filled(WIDTH, HEIGHT, data).unwrap();
    let mut world = World::empty(WIDTH, HEIGHT);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let mut data = nodes.get(node(x, y));
            data.sites = site(&nodes, &world, node(x, y));
            nodes.set(node(x, y), data);
        }
    }
    world.place_headquarters(&mut nodes, HQ, player(0), StartGoods::Normal);
    world.place_headquarters(&mut nodes, RIVAL_HQ, player(1), StartGoods::Normal);
    Game::from_parts(nodes, world)
}

fn run(game: &mut Game, commands: &[Command]) -> Result<(), Refusal> {
    commands
        .iter()
        .try_for_each(|&command| game.execute(player(0), command))
}

fn steps(directions: &[Direction]) -> Vec<Command> {
    directions
        .iter()
        .map(|&direction| Command::ExtendRoad { direction })
        .collect()
}

fn start() -> Command {
    Command::StartRoad {
        node: HQ_FLAG,
        water: false,
    }
}

fn flag_at(at: Node) -> Command {
    Command::PlaceFlag {
        node: at,
        water: false,
    }
}

use Direction::{East, NorthEast, SouthEast, SouthWest, West};

#[test]
fn a_road_drawn_onto_one_of_the_players_flags_is_finished() {
    let mut game = game();
    run(&mut game, &[flag_at(node(20, 17)), start()]).unwrap();
    run(&mut game, &steps(&[East, East, East])).unwrap();
    assert!(game.road_draft(player(0)).is_some());
    run(&mut game, &steps(&[East])).unwrap();
    assert_eq!(game.road_draft(player(0)), None);
    let hq_flag = game.world.flag_at(HQ_FLAG).unwrap();
    let end = game.world.flag_at(node(20, 17)).unwrap();
    let roads: Vec<_> = game.world.roads().map(|(_, road)| road.clone()).collect();
    assert_eq!(roads.len(), 1);
    assert_eq!((roads[0].start, roads[0].end), (hq_flag, Some(end)));
    assert_eq!(roads[0].path, [node(17, 17), node(18, 17), node(19, 17)]);
}

#[test]
fn every_step_redraws_the_road_and_a_step_back_takes_one_back() {
    let mut game = game();
    run(&mut game, &[start()]).unwrap();
    run(&mut game, &steps(&[East, East, SouthEast])).unwrap();
    let drawn = |game: &Game| {
        let road = game.road_draft(player(0)).unwrap().road.unwrap();
        game.world.road(road).unwrap().path.clone()
    };
    assert_eq!(drawn(&game), [node(17, 17), node(18, 17), node(19, 18)]);
    assert!(game.nodes.get(node(19, 18)).roads.marker().is_some());
    run(&mut game, &steps(&[Direction::NorthWest])).unwrap();
    assert_eq!(drawn(&game), [node(17, 17), node(18, 17)]);
    assert!(game.nodes.get(node(19, 18)).roads.is_empty());
}

#[test]
fn a_road_cannot_cross_itself_and_a_loop_to_its_own_flag_is_refused() {
    let mut game = game();
    run(&mut game, &[start()]).unwrap();
    run(&mut game, &steps(&[East, East, SouthWest, West])).unwrap();
    assert_eq!(
        game.execute(player(0), Command::ExtendRoad { direction: NorthEast }),
        Err(Refusal::Blocked)
    );
    assert_eq!(
        game.execute(
            player(0),
            Command::ExtendRoad {
                direction: Direction::NorthWest
            }
        ),
        Err(Refusal::Road(RoadError::SameFlag)),
        "back onto the start flag finishes the road there, which create refuses"
    );
    assert_eq!(game.road_draft(player(0)), None);
    assert_eq!(game.world.roads().count(), 0);
}

#[test]
fn a_flag_on_the_head_finishes_the_road() {
    let mut game = game();
    run(&mut game, &[start()]).unwrap();
    run(&mut game, &steps(&[East, East, East])).unwrap();
    assert_eq!(
        game.execute(player(0), flag_at(node(18, 17))),
        Err(Refusal::NotAtHead)
    );
    run(&mut game, &[flag_at(node(19, 17))]).unwrap();
    assert_eq!(game.road_draft(player(0)), None);
    let (_, road) = game.world.roads().next().unwrap();
    assert_eq!(road.path, [node(17, 17), node(18, 17)]);
    assert_eq!(road.end, game.world.flag_at(node(19, 17)));
}

#[test]
fn stepping_onto_another_road_puts_a_flag_there_and_ends_on_it() {
    let mut game = game();
    run(&mut game, &[flag_at(node(12, 21)), flag_at(node(20, 21))]).unwrap();
    run(
        &mut game,
        &[Command::StartRoad {
            node: node(12, 21),
            water: false,
        }],
    )
    .unwrap();
    run(&mut game, &steps(&[East; 8])).unwrap();
    assert_eq!(game.world.roads().count(), 1);
    run(&mut game, &[start()]).unwrap();
    run(&mut game, &steps(&[SouthEast, SouthWest, SouthEast, SouthWest])).unwrap();
    assert_eq!(game.road_draft(player(0)), None);
    assert!(game.world.flag_at(node(16, 21)).is_some());
    assert_eq!(
        game.world.roads().count(),
        3,
        "the crossed road split in two, plus the new one"
    );
}

#[test]
fn a_road_stays_inside_its_owners_border() {
    let mut game = game();
    run(&mut game, &[start()]).unwrap();
    let mut refusal = None;
    for _ in 0..20 {
        if let Err(r) = game.execute(player(0), Command::ExtendRoad { direction: East }) {
            refusal = Some(r);
            break;
        }
    }
    assert_eq!(refusal, Some(Refusal::Blocked));
    let head = game.road_draft(player(0)).unwrap().head(&game.world).unwrap();
    assert!(!game.nodes.get(head).flags.contains(NodeFlags::BORDER));
    assert!(
        game.nodes
            .get(head.neighbour(East, WIDTH, HEIGHT))
            .flags
            .contains(NodeFlags::BORDER)
    );
}

#[test]
fn players_act_only_on_their_own_land_and_flags() {
    let mut game = game();
    let rival = player(1);
    assert_eq!(game.execute(rival, start()), Err(Refusal::NotYours));
    assert_eq!(game.execute(rival, flag_at(node(20, 17))), Err(Refusal::NotYours));
    assert_eq!(
        game.execute(rival, Command::RemoveFlag { node: HQ_FLAG }),
        Err(Refusal::NotYours)
    );
    assert_eq!(game.execute(rival, Command::CancelRoad), Err(Refusal::NotDrawing));
    assert_eq!(
        game.execute(player(0), Command::RemoveFlag { node: HQ_FLAG }),
        Err(Refusal::Headquarters)
    );
    assert_eq!(
        game.execute(player(0), flag_at(node(17, 17))),
        Err(Refusal::NoRoom)
    );
    assert_eq!(
        game.execute(
            player(0),
            Command::PlaceFlag {
                node: node(20, 17),
                water: true
            }
        ),
        Err(Refusal::NoWater)
    );
}

#[test]
fn cancelling_or_removing_the_start_flag_drops_the_drawn_road() {
    let mut game = game();
    run(&mut game, &[flag_at(node(20, 13)), start()]).unwrap();
    run(&mut game, &steps(&[East, East])).unwrap();
    run(&mut game, &[Command::CancelRoad]).unwrap();
    assert_eq!(game.world.roads().count(), 0);
    assert!(game.nodes.get(node(17, 17)).roads.is_empty());

    run(
        &mut game,
        &[Command::StartRoad {
            node: node(20, 13),
            water: false,
        }],
    )
    .unwrap();
    run(&mut game, &steps(&[East, East])).unwrap();
    run(&mut game, &[Command::RemoveFlag { node: node(20, 13) }]).unwrap();
    assert_eq!(game.road_draft(player(0)), None);
    assert_eq!(game.world.roads().count(), 0);
    assert!(game.nodes.get(node(21, 13)).roads.is_empty());
}

#[test]
fn a_road_is_removed_through_one_of_its_inner_nodes() {
    let mut game = game();
    run(&mut game, &[flag_at(node(20, 17)), start()]).unwrap();
    run(&mut game, &steps(&[East; 4])).unwrap();
    assert_eq!(
        game.execute(player(1), Command::RemoveRoad { node: node(18, 17) }),
        Err(Refusal::NotYours)
    );
    assert_eq!(
        game.execute(player(0), Command::RemoveRoad { node: node(20, 17) }),
        Err(Refusal::NoRoad),
        "a flag is no road's inner node"
    );
    run(&mut game, &[Command::RemoveRoad { node: node(18, 17) }]).unwrap();
    assert_eq!(game.world.roads().count(), 0);
    assert!(game.world.flag_at(node(20, 17)).is_some(), "the flags stay");
}

#[test]
fn a_water_road_needs_water_on_both_sides_and_ends_after_four_segments() {
    let water = NodeData {
        altitude: 10,
        terrain: [WATER; 2],
        owner: Some(player(0)),
        ..NodeData::default()
    };
    let nodes = NodeMap::filled(16, 16, water).unwrap();
    let world = World::empty(16, 16);
    let (from, to) = (node(4, 4), node(5, 4));
    for segments in 0..4 {
        assert!(step_allowed(
            &nodes,
            &world,
            from,
            East,
            to,
            RoadKind::Water,
            player(0),
            segments
        ));
    }
    assert!(!step_allowed(
        &nodes,
        &world,
        from,
        East,
        to,
        RoadKind::Water,
        player(0),
        4
    ));
    assert!(!step_allowed(
        &nodes,
        &world,
        from,
        East,
        to,
        RoadKind::Water,
        player(1),
        0
    ));
    assert!(
        !step_allowed(&nodes, &world, from, East, to, RoadKind::Normal, player(0), 0),
        "nothing walkable"
    );
}

#[test]
fn a_road_holds_at_most_a_hundred_segments() {
    let mut game = game();
    run(&mut game, &[start()]).unwrap();
    run(&mut game, &steps(&[East])).unwrap();
    let draft = game.drafts[0].as_mut().unwrap();
    draft.path = vec![node(17, 17); MAX_SEGMENTS];
    assert_eq!(
        game.execute(player(0), Command::ExtendRoad { direction: East }),
        Err(Refusal::Blocked)
    );
}

#[test]
fn a_replayed_log_gives_the_same_game() {
    let log: Vec<Stamped> = [(3, flag_at(node(20, 17))), (3, start())]
        .into_iter()
        .chain((10..14).map(|step| (step, Command::ExtendRoad { direction: East })))
        .map(|(step, command)| Stamped {
            step,
            player: player(0),
            command,
        })
        .collect();
    let mut first = game();
    first.run(&log, 40);
    let mut second = game();
    second.run(&log, 20);
    second.run(&log, 40);
    assert_eq!(first.state_hash(), second.state_hash());
    assert_eq!(first.world.roads().count(), 1);
    let mut idle = game();
    idle.run(&[], 40);
    assert_ne!(first.state_hash(), idle.state_hash());
}

/// Pins a replay with roads drawn, split and removed. Like the plain run's pin, the same number
/// on every OS and word size is part of the M3 exit gate.
#[test]
fn a_replay_hashes_the_same_everywhere() {
    let commands = [
        (5, flag_at(node(20, 17))),
        (5, start()),
        (9, Command::ExtendRoad { direction: East }),
        (9, Command::ExtendRoad { direction: East }),
        (12, Command::ExtendRoad { direction: East }),
        (12, Command::ExtendRoad { direction: East }),
        (30, flag_at(node(18, 17))),
        (40, Command::RemoveRoad { node: node(17, 17) }),
    ];
    let log: Vec<Stamped> = commands
        .into_iter()
        .map(|(step, command)| Stamped {
            step,
            player: player(0),
            command,
        })
        .collect();
    let mut game = game();
    game.run(&log, 1000);
    assert_eq!(game.world.roads().count(), 1);
    assert_eq!(game.state_hash(), 10_411_864_317_057_225_343);
}

#[test]
fn the_preview_sorts_each_neighbour_of_the_head_as_the_icons_show_it() {
    let mut game = game();
    assert_eq!(game.preview_step(player(0), East), None);
    run(&mut game, &[flag_at(node(19, 17)), start()]).unwrap();
    let preview = |game: &Game, direction| game.preview_step(player(0), direction);
    let back = |game: &Game| game.road_draft(player(0)).unwrap().back(&game.world);
    assert_eq!(preview(&game, Direction::NorthWest), Some(StepPreview::Building));
    assert_eq!(back(&game), None);
    run(&mut game, &steps(&[East, East])).unwrap();
    assert_eq!(preview(&game, East), Some(StepPreview::Flag));
    assert_eq!(preview(&game, SouthEast), Some(StepPreview::Free));
    assert_eq!(
        preview(&game, West),
        Some(StepPreview::Blocked),
        "the drawn road's own node"
    );
    assert_eq!(back(&game), Some(West));
}

#[test]
fn a_military_building_keeps_four_rings_from_a_headquarters_and_from_its_own_kind() {
    let mut game = game();
    let barracks = |node| Command::PlaceBuilding {
        node,
        kind: BuildingKind::BARRACKS,
    };
    let east = |steps| HQ.towards(Direction::East, steps, WIDTH, HEIGHT);
    assert_eq!(
        game.execute(player(0), barracks(east(MILITARY_SPACING))),
        Err(Refusal::TooClose)
    );
    let first = east(MILITARY_SPACING + 1);
    game.execute(player(0), barracks(first)).unwrap();
    let away = |steps| first.towards(Direction::NorthWest, steps, WIDTH, HEIGHT);
    assert_eq!(
        game.execute(player(0), barracks(away(MILITARY_SPACING))),
        Err(Refusal::TooClose),
        "an unfinished barracks of the same player blocks"
    );
    game.execute(player(0), barracks(away(MILITARY_SPACING + 1)))
        .unwrap();
}

#[test]
fn another_players_military_building_blocks_only_once_it_is_finished() {
    let mut game = game();
    let kind = BuildingKind::BARRACKS;
    let theirs = RIVAL_HQ.towards(Direction::West, MILITARY_SPACING + 1, WIDTH, HEIGHT);
    game.execute(player(1), Command::PlaceBuilding { node: theirs, kind })
        .unwrap();
    let mine = theirs.towards(Direction::West, MILITARY_SPACING, WIDTH, HEIGHT);
    assert!(
        game.military_room(player(0), mine),
        "a site of theirs leaves room"
    );
    assert!(!game.military_room(player(1), mine), "their own site does not");
    let MapObject::Building(record) = game.world.object(theirs) else {
        panic!("no site");
    };
    game.world.building_mut(record).unwrap().state |= BuildingState::FINISHED;
    assert!(
        !game.military_room(player(0), mine),
        "a finished one of theirs blocks"
    );
}

fn route(to: Node) -> Command {
    Command::RouteRoad { node: to }
}

#[test]
fn a_click_further_from_the_head_takes_the_road_there_and_draws_once_a_step() {
    let mut game = game();
    run(&mut game, &[start()]).unwrap();
    let before = game.rng;
    run(&mut game, &[route(node(22, 17))]).unwrap();
    let draft = game.road_draft(player(0)).unwrap();
    assert_eq!(draft.head(&game.world), Some(node(22, 17)));
    assert_eq!(draft.path.len(), 6);
    let mut expected = before;
    for _ in 0..6 {
        expected.up_to(0);
    }
    assert_eq!(game.rng, expected);
}

#[test]
fn a_route_onto_one_of_the_players_flags_finishes_the_road() {
    let mut game = game();
    run(&mut game, &[flag_at(node(21, 15)), start()]).unwrap();
    run(&mut game, &[route(node(21, 15))]).unwrap();
    assert_eq!(game.road_draft(player(0)), None);
    let end = game.world.flag_at(node(21, 15)).unwrap();
    let roads: Vec<_> = game.world.roads().map(|(_, road)| road.clone()).collect();
    assert_eq!(roads.len(), 1);
    assert_eq!(roads[0].end, Some(end));
    assert_eq!(roads[0].path.len(), 5);
}

#[test]
fn a_route_onto_another_road_puts_a_flag_there_and_ends_on_it() {
    let mut game = game();
    run(&mut game, &[flag_at(node(12, 21)), flag_at(node(20, 21))]).unwrap();
    run(
        &mut game,
        &[Command::StartRoad {
            node: node(12, 21),
            water: false,
        }],
    )
    .unwrap();
    run(&mut game, &steps(&[East; 8])).unwrap();
    run(&mut game, &[start(), route(node(16, 21))]).unwrap();
    assert_eq!(game.road_draft(player(0)), None);
    assert!(game.world.flag_at(node(16, 21)).is_some());
    assert_eq!(game.world.roads().count(), 3);
}

#[test]
fn a_click_the_search_cannot_reach_changes_nothing() {
    let mut game = game();
    run(&mut game, &[start(), route(node(19, 17))]).unwrap();
    let before = game.clone();
    assert_eq!(
        game.execute(player(0), route(node(16, 37))),
        Err(Refusal::NoRoute),
        "twenty rows away"
    );
    assert_eq!(game, before);
}

#[test]
fn a_route_stops_at_the_first_refused_step_and_keeps_the_steps_before_it() {
    let water = NodeData {
        altitude: 10,
        terrain: [WATER; 2],
        owner: Some(player(0)),
        ..NodeData::default()
    };
    let nodes = NodeMap::filled(WIDTH, HEIGHT, water).unwrap();
    let mut game = Game::from_parts(nodes, World::empty(WIDTH, HEIGHT));
    let quay = node(8, 8);
    run(
        &mut game,
        &[
            Command::PlaceFlag {
                node: quay,
                water: true,
            },
            Command::StartRoad {
                node: quay,
                water: true,
            },
            route(node(14, 8)),
        ],
    )
    .unwrap();
    let draft = game.road_draft(player(0)).unwrap();
    assert_eq!(draft.path.len(), MAX_WATER_SEGMENTS);
}

#[test]
fn a_route_goes_round_other_roads_instead_of_across_them() {
    let mut game = game();
    run(
        &mut game,
        &[
            flag_at(node(20, 14)),
            Command::StartRoad {
                node: node(20, 14),
                water: false,
            },
        ],
    )
    .unwrap();
    run(
        &mut game,
        &steps(&[SouthWest, SouthEast, SouthWest, SouthEast, SouthWest, SouthEast]),
    )
    .unwrap();
    let wall_end = game.road_draft(player(0)).unwrap().head(&game.world).unwrap();
    run(&mut game, &[flag_at(wall_end)]).unwrap();
    let wall = game.world.roads().next().unwrap().1.path.clone();
    run(&mut game, &[start(), route(node(23, 17))]).unwrap();
    let draft = game.road_draft(player(0)).unwrap();
    assert_eq!(draft.head(&game.world), Some(node(23, 17)));
    assert!(draft.path.len() > 7, "longer than the straight line");
    assert!(draft.path.iter().all(|node| !wall.contains(node)));
}
