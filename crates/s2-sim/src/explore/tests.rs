use crate::commands::{Command, Refusal};
use crate::figures::{Explorer, Exploring, FigureKind};
use crate::game::Game;
use crate::grid::Node;
use crate::job::Job;
use crate::nodes::{Mineral, Resources};
use crate::settlers::tests::{HQ, game, hold, player, road_east};
use crate::world::{MapObject, Player, Sign};

use super::GEOLOGIST_VISITS;

const FLAG: Node = Node { x: 20, y: 17 };

fn geologists(game: &Game) -> u16 {
    let MapObject::Building(record) = game.world().object(HQ) else {
        panic!("no headquarters");
    };
    let (_, store) = game.world().storehouse_of(record).unwrap();
    store.jobs[Job::GEOLOGIST.index()]
}

fn signs(game: &Game) -> Vec<(Node, Sign)> {
    let (width, height) = (game.world().width(), game.world().height());
    (0..height)
        .flat_map(|y| (0..width).map(move |x| Node { x, y }))
        .filter_map(|node| match game.world().object(node) {
            MapObject::Sign(sign) => Some((node, sign)),
            _ => None,
        })
        .collect()
}

fn explorer(game: &Game) -> Option<Explorer> {
    game.world().figures().find_map(|(_, figure)| match figure.kind {
        FigureKind::Explorer(explorer) => Some(explorer),
        _ => None,
    })
}

fn water_everywhere(game: &mut Game) {
    let (width, height) = (game.nodes().width(), game.nodes().height());
    for y in 0..height {
        for x in 0..width {
            let node = Node { x, y };
            let mut data = game.nodes().get(node);
            data.resources = Resources::Water(3);
            game.nodes.set(node, data);
        }
    }
}

#[test]
fn a_geologist_signs_fifteen_nodes_round_its_flag_and_goes_home() {
    let mut game = game();
    water_everywhere(&mut game);
    road_east(&mut game, crate::settlers::tests::HQ_FLAG, 4);
    let before = geologists(&game);
    game.execute(player(), Command::SendGeologist { node: FLAG })
        .unwrap();
    let mut out = false;
    for _ in 0..20_000 {
        game.step();
        out |= explorer(&game).is_some();
        if out && explorer(&game).is_none() && geologists(&game) == before {
            break;
        }
    }
    assert!(out, "the geologist never started work");
    assert_eq!(geologists(&game), before, "the geologist is back in stock");
    let signs = signs(&game);
    assert_eq!(signs.len(), usize::from(GEOLOGIST_VISITS));
    assert!(game.world().found_sign(player(), Sign::WATER));
    assert!(!game.world().found_sign(player(), 1), "no gold was found");
    let sent = game.world().messages(player()).list();
    assert!(!sent.is_empty(), "the first find was reported");
    assert!(sent.len() <= signs.len());
    for message in sent {
        assert_eq!(message.id, crate::messages::MessageId::GEOLOGIST);
        assert_eq!(message.value, u32::from(Sign::WATER));
        assert!(signs.iter().any(|(node, _)| Some(*node) == message.node));
    }
    for (node, sign) in signs {
        assert_eq!((sign.found, sign.amount), (Sign::WATER, 0), "{node:?}");
        let (dx, dy) = (node.x.abs_diff(FLAG.x), node.y.abs_diff(FLAG.y));
        assert!(dx <= 12 && dy <= 12, "{node:?} is outside the search");
    }
}

#[test]
fn a_sign_caps_its_size_and_water_has_one_size() {
    assert_eq!(Sign::placed(Sign::WATER, 2).amount, 0);
    assert_eq!(Sign::placed(1, 6).amount, Sign::LARGEST);
    assert_eq!(Sign::placed(1, 1).amount, 1);
}

#[test]
fn what_lies_under_decides_the_sign() {
    let mut game = game();
    let node = Node { x: 24, y: 24 };
    let mut set = |resources| {
        let mut data = game.nodes().get(node);
        data.resources = resources;
        game.nodes.set(node, data);
        game.find_under(node)
    };
    assert_eq!(set(Resources::Water(5)), (Sign::WATER, 0));
    assert_eq!(set(Resources::Nothing), (Sign::NOTHING, 0));
    assert_eq!(set(Resources::Water(0)), (Sign::NOTHING, 0));
    assert_eq!(
        set(Resources::Mineral(Mineral::IronOre, 7)),
        (0, 6),
        "the iron sign off the hills, where S2.EXE lights a fire"
    );
}

#[test]
fn removing_the_flag_loses_a_geologist_walking_from_it() {
    let mut game = game();
    road_east(&mut game, crate::settlers::tests::HQ_FLAG, 4);
    game.execute(player(), Command::SendGeologist { node: FLAG })
        .unwrap();
    let walking = |game: &Game| {
        explorer(game).is_some_and(|explorer| matches!(explorer.work, Exploring::Walking { .. }))
    };
    for _ in 0..5_000 {
        if walking(&game) {
            break;
        }
        game.step();
    }
    assert!(walking(&game));
    game.execute(player(), Command::RemoveFlag { node: FLAG })
        .unwrap();
    assert!(explorer(&game).is_none());
    assert!(game.world().figures().any(|(_, figure)| matches!(
        figure.kind,
        FigureKind::Lost {
            job: Job::GEOLOGIST,
            ..
        }
    )));
}

#[test]
fn only_the_owner_sends_a_geologist_to_a_flag() {
    let mut game = game();
    road_east(&mut game, crate::settlers::tests::HQ_FLAG, 4);
    let send = |node| Command::SendGeologist { node };
    assert_eq!(
        game.execute(player(), send(Node { x: 30, y: 30 })),
        Err(Refusal::NoFlag)
    );
    let rival = Player::new(1).unwrap();
    assert_eq!(game.execute(rival, send(FLAG)), Err(Refusal::NotYours));
    assert_eq!(game.execute(player(), send(FLAG)), Ok(()));
    let orders = game
        .world()
        .orders()
        .filter(|(_, order)| order.job() == Some(Job::GEOLOGIST))
        .count();
    assert_eq!(orders, 1);
}

fn scouts(game: &Game) -> u16 {
    let MapObject::Building(record) = game.world().object(HQ) else {
        panic!("no headquarters");
    };
    let (_, store) = game.world().storehouse_of(record).unwrap();
    store.jobs[Job::SCOUT.index()]
}

#[test]
fn a_scout_walks_until_nothing_round_its_flag_is_unseen() {
    let mut game = game();
    road_east(&mut game, crate::settlers::tests::HQ_FLAG, 4);
    let before = scouts(&game);
    game.execute(player(), Command::SendScout { node: FLAG }).unwrap();
    let mut out = false;
    for _ in 0..50_000 {
        game.step();
        out |= explorer(&game).is_some();
        if out && explorer(&game).is_none() && scouts(&game) == before {
            break;
        }
    }
    assert!(out, "the scout never started");
    assert_eq!(scouts(&game), before, "the scout is back in stock");
    let unseen = game.search(FLAG, super::scout_search(player()));
    assert!(unseen.is_empty(), "still unseen round the flag: {unseen:?}");
}

#[test]
fn a_scout_with_nothing_to_see_goes_straight_home() {
    let mut game = game();
    road_east(&mut game, crate::settlers::tests::HQ_FLAG, 2);
    let (width, height) = (game.nodes().width(), game.nodes().height());
    for y in 0..height {
        for x in 0..width {
            game.nodes.reveal(Node { x, y }, player(), 1);
        }
    }
    game.execute(
        player(),
        Command::SendScout {
            node: Node { x: 18, y: 17 },
        },
    )
    .unwrap();
    for _ in 0..3_000 {
        game.step();
        assert!(explorer(&game).is_none(), "the scout went out");
    }
}

#[test]
fn a_held_node_stops_a_geologist_on_its_way() {
    let mut game = game();
    water_everywhere(&mut game);
    road_east(&mut game, crate::settlers::tests::HQ_FLAG, 4);
    game.execute(player(), Command::SendGeologist { node: FLAG })
        .unwrap();
    let mut walking = None;
    for _ in 0..20_000 {
        game.step();
        let (width, height) = (game.nodes.width(), game.nodes.height());
        walking = game
            .world()
            .figures()
            .find_map(|(record, figure)| match figure.kind {
                FigureKind::Explorer(Explorer {
                    work: Exploring::Walking { path },
                    ..
                }) if !figure.walking() => {
                    let mut ahead = game.world.paths.get(path).copied()?;
                    let direction = ahead.next()?;
                    Some((
                        record,
                        figure.node(),
                        figure.node().neighbour(direction, width, height),
                    ))
                }
                _ => None,
            });
        if walking.is_some() {
            break;
        }
    }
    let (record, node, next) = walking.expect("no geologist on its way");
    hold(&mut game, next, true);
    game.step_settler(record);
    let figure = game.world().figure(record).copied().unwrap();
    assert_eq!(figure.node(), node, "it stays where it stands");
    assert_eq!(figure.waits, 1);
    hold(&mut game, next, false);
    game.step_settler(record);
    assert_eq!(game.world().figure(record).unwrap().node(), next);
}
