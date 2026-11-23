use crate::commands::{Command, Refusal};
use crate::figures::FigureKind;
use crate::game::{BugFixes, Game};
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::settlers::HOME_MODE;
use crate::settlers::tests::{HQ, HQ_FLAG, game, player, road_east, run_to};
use crate::transport::WareState;
use crate::world::BuildingKind;
use crate::world::{BuildingState, Burning, MapObject, ObjectWord, Player, Remnant};

const END: Node = Node { x: 20, y: 17 };

/// A game with a 4-edge road east of the headquarters and a site of `kind` at its far flag.
fn with_site(kind: BuildingKind) -> (Game, u16, Node) {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    let node = END.neighbour(Direction::NorthWest, game.nodes.width(), game.nodes.height());
    game.execute(player(), Command::PlaceBuilding { node, kind })
        .unwrap();
    let MapObject::Building(site) = game.world().object(node) else {
        panic!("no site");
    };
    (game, site, node)
}

fn demolish(game: &mut Game, node: Node) {
    game.execute(player(), Command::Demolish { node }).unwrap();
}

fn lost_builders(game: &Game, node: Node) -> usize {
    game.world()
        .figures_at(node)
        .filter(|(_, figure)| {
            matches!(
                figure.kind,
                FigureKind::Lost {
                    job: Job::BUILDER,
                    ..
                }
            )
        })
        .count()
}

#[test]
fn a_fresh_site_goes_without_a_trace_or_a_draw() {
    let (mut game, site, node) = with_site(BuildingKind::WOODCUTTER);
    let flag = game.world().flag_at(END).unwrap();
    let rng = game.rng;
    let sites_before = game.nodes.get(node).sites;
    demolish(&mut game, node);
    assert!(game.world().building(site).is_none());
    assert_eq!(game.world().object(node), MapObject::Nothing);
    assert!(
        game.world().orders().all(|(_, order)| order.flag != flag),
        "the builder order is freed"
    );
    assert_eq!(game.rng, rng);
    assert_ne!(
        game.nodes.get(node).sites,
        sites_before,
        "the node offers a site again"
    );
    assert!(game.world().flag_at(END).is_some(), "the flag stays");
}

#[test]
fn a_builder_at_work_is_lost_on_the_site_and_its_record_freed() {
    let (mut game, site, node) = with_site(BuildingKind::WOODCUTTER);
    run_to(&mut game, 400);
    let building = *game.world().building(site).unwrap();
    assert!(
        building
            .state
            .contains(BuildingState::BUILDING | BuildingState::PRESENT)
    );
    let builder = building.builder.unwrap();
    let mut rng = game.rng;
    demolish(&mut game, node);
    assert_eq!(lost_builders(&game, node), 1);
    assert!(
        game.world()
            .figure(builder)
            .is_none_or(|figure| !matches!(figure.kind, FigureKind::Builder(_)))
    );
    rng.up_to(100);
    assert_eq!(game.rng, rng, "one draw, for the lost builder");
}

#[test]
fn a_builder_on_its_way_walks_home() {
    let (mut game, _, node) = with_site(BuildingKind::WOODCUTTER);
    let walking = loop {
        game.step();
        let found = game
            .world()
            .figures()
            .find_map(|(record, figure)| match figure.kind {
                FigureKind::Walker(walker) if walker.job == Job::BUILDER => Some(record),
                _ => None,
            });
        if let Some(record) = found {
            break record;
        }
        assert!(game.clock().step() < 400, "no builder on the road");
    };
    demolish(&mut game, node);
    let Some(FigureKind::Walker(walker)) = game.world().figure(walking).map(|figure| figure.kind) else {
        panic!("no longer walking: {:?}", game.world().figure(walking));
    };
    assert_eq!(walker.mode, HOME_MODE);
    assert_eq!(walker.order, game.world().flag_at(HQ_FLAG).unwrap());
}

#[test]
fn boards_on_their_way_are_stranded_where_they_wait() {
    let (mut game, site, node) = with_site(BuildingKind::WOODCUTTER);
    run_to(&mut game, 200);
    let flag = game.world().building(site).unwrap().flag;
    let ordered: Vec<u16> = game
        .world()
        .orders()
        .filter(|(_, order)| order.flag == flag && order.mode == crate::transport::WARE_MODE)
        .filter_map(|(_, order)| order.on_way)
        .collect();
    assert!(!ordered.is_empty(), "boards on their way");
    demolish(&mut game, node);
    for ware in ordered {
        let Some(ware) = game.world().ware(ware) else {
            continue;
        };
        assert!(
            matches!(ware.state, WareState::Stranded | WareState::Unrouted),
            "{ware:?}"
        );
        assert_eq!(ware.order, None);
        if ware.state == WareState::Stranded {
            let at = ware.flag.unwrap();
            assert!(game.world().wares_at(at).count() > 0);
        }
    }
}

/// A ware waiting on its way to a woodcutter's site whose next flag points nowhere, after the
/// site is demolished with `fixes`: the flag it ends with and the flag whose list holds it.
fn strand_a_ware_the_search_cannot_find(fixes: BugFixes) -> (Option<u16>, u16) {
    let (mut game, site, node) = with_site(BuildingKind::WOODCUTTER);
    game.world.bug_fixes = fixes;
    let flag = game.world().building(site).unwrap().flag;
    let mut waiting = None;
    for _ in 0..2000 {
        game.step();
        waiting = game
            .world()
            .orders()
            .filter(|(_, order)| order.flag == flag)
            .filter_map(|(_, order)| order.on_way)
            .find(|&ware| {
                game.world()
                    .ware(ware)
                    .is_some_and(|ware| ware.state == WareState::Waiting)
            });
        if waiting.is_some() {
            break;
        }
    }
    let ware = waiting.expect("no ware waiting on its way to the site");
    let held = game
        .world()
        .flags()
        .map(|(flag, _)| flag)
        .find(|&flag| game.world().wares_at(flag).any(|(record, _)| record == ware))
        .expect("the ware waits at a flag");
    game.world.wares.get_mut(ware).unwrap().flag = Some(u16::MAX - 1);
    demolish(&mut game, node);
    let ware = game.world().ware(ware).unwrap();
    assert_eq!(ware.state, WareState::Stranded);
    (ware.flag, held)
}

#[test]
fn a_ware_the_flag_search_cannot_find_is_stranded_with_no_flag() {
    assert_eq!(
        strand_a_ware_the_search_cannot_find(BugFixes::Off).0,
        None,
        "the search found it at no flag"
    );
}

#[test]
fn with_the_bug_fixes_on_a_ware_the_flag_search_cannot_find_is_stranded_where_it_waits() {
    let (flag, held) = strand_a_ware_the_search_cannot_find(BugFixes::On);
    assert_eq!(flag, Some(held));
}

#[test]
fn the_record_counts_a_building_through_its_life() {
    let (mut game, site, node) = with_site(BuildingKind::WOODCUTTER);
    let world = game.world();
    assert_eq!(world.building_sites(player(), BuildingKind::WOODCUTTER), 1);
    assert_eq!(world.all_building_sites(player()), 1);
    assert_eq!(world.finished_buildings(player(), BuildingKind::WOODCUTTER), 0);
    assert_eq!(
        world.finished_buildings(player(), BuildingKind::HEADQUARTERS),
        1,
        "the headquarters never passes through a site"
    );
    assert_eq!(world.all_finished_buildings(player()), 1);
    run_to(&mut game, 3000);
    assert!(
        game.world()
            .building(site)
            .unwrap()
            .state
            .contains(BuildingState::FINISHED)
    );
    let world = game.world();
    assert_eq!(world.building_sites(player(), BuildingKind::WOODCUTTER), 0);
    assert_eq!(world.all_building_sites(player()), 0);
    assert_eq!(world.finished_buildings(player(), BuildingKind::WOODCUTTER), 1);
    assert_eq!(world.all_finished_buildings(player()), 2);
    demolish(&mut game, node);
    let world = game.world();
    assert_eq!(world.finished_buildings(player(), BuildingKind::WOODCUTTER), 0);
    assert_eq!(world.all_finished_buildings(player()), 1);
}

#[test]
fn demolishing_a_site_takes_it_off_the_site_count() {
    let (mut game, _, node) = with_site(BuildingKind::WOODCUTTER);
    run_to(&mut game, 400);
    demolish(&mut game, node);
    let world = game.world();
    assert_eq!(world.building_sites(player(), BuildingKind::WOODCUTTER), 0);
    assert_eq!(world.all_building_sites(player()), 0);
    assert_eq!(
        world.finished_buildings(player(), BuildingKind::WOODCUTTER),
        0,
        "a site never reached the finished count"
    );
}

#[test]
fn a_finished_building_burns_and_its_remains_go() {
    let (mut game, site, node) = with_site(BuildingKind::WOODCUTTER);
    run_to(&mut game, 3000);
    assert!(
        game.world()
            .building(site)
            .unwrap()
            .state
            .contains(BuildingState::FINISHED)
    );
    demolish(&mut game, node);
    assert_eq!(game.world().object(node), MapObject::Fire(Burning::fire(false)));
    let start = game.clock().step();
    let mut changes = Vec::new();
    let mut last = game.world().object(node);
    while game.world().object(node) != MapObject::Nothing {
        game.step();
        let now = game.world().object(node);
        if now != last {
            changes.push((game.clock().step() - start, ObjectWord::from(now).0));
            last = now;
        }
        assert!(game.clock().step() - start < 3000, "still burning");
    }
    let words: Vec<u16> = changes.iter().map(|&(_, word)| word).collect();
    assert_eq!(
        words,
        [0xdc04, 0xdc03, 0xdc02, 0xdc01, 0xdc00, 0xe002, 0xe001, 0xe000, 0]
    );
    for pair in changes.windows(2) {
        assert_eq!(pair[1].0 - pair[0].0, 256, "a visit every 256 steps");
    }
}

#[test]
fn a_medium_building_burns_large() {
    let (mut game, site, node) = with_site(BuildingKind::SAWMILL);
    game.world.building_mut(site).unwrap().state = BuildingState::FINISHED;
    demolish(&mut game, node);
    assert_eq!(ObjectWord::from(game.world().object(node)), ObjectWord(0xdd05));
}

#[test]
fn removing_the_flag_demolishes_the_site_first() {
    let (mut game, site, node) = with_site(BuildingKind::WOODCUTTER);
    run_to(&mut game, 400);
    game.execute(player(), Command::RemoveFlag { node: END }).unwrap();
    assert!(game.world().building(site).is_none());
    assert!(game.world().flag_at(END).is_none());
    assert_eq!(lost_builders(&game, node), 1);
}

#[test]
fn only_the_owner_demolishes_and_never_the_headquarters() {
    let (mut game, _, node) = with_site(BuildingKind::WOODCUTTER);
    let rival = Player::new(1).unwrap();
    assert_eq!(
        game.execute(rival, Command::Demolish { node }),
        Err(Refusal::NotYours)
    );
    assert_eq!(
        game.execute(player(), Command::Demolish { node: HQ }),
        Err(Refusal::Headquarters)
    );
    assert_eq!(
        game.execute(player(), Command::Demolish { node: END }),
        Err(Refusal::NoBuilding)
    );
}

#[test]
fn a_skeleton_ages_into_its_second_look_and_goes() {
    let mut game = game();
    let node = Node { x: 30, y: 5 };
    game.world.set_object(node, MapObject::Remnant(Remnant::SKELETON));
    let mut seen = vec![ObjectWord::from(game.world().object(node)).0];
    for _ in 0..256 * 20 {
        game.step();
        let now = ObjectWord::from(game.world().object(node)).0;
        if seen.last() != Some(&now) {
            seen.push(now);
        }
        if now == 0 {
            break;
        }
    }
    assert_eq!(
        seen,
        [
            0xd7af, 0xd72f, 0xd6af, 0xd62f, 0xd5af, 0xd52f, 0xd4af, 0xd42f, 0xd7b0, 0xd730, 0xd6b0, 0xd630,
            0xd5b0, 0xd530, 0xd4b0, 0xd430, 0
        ]
    );
}

/// A helper and a woodcutter walking home with the number of a woodcutter's site left in their
/// countdown, after the site is demolished with `fixes`: the game, the helper and the woodcutter.
fn homeward_with_the_sites_number(fixes: BugFixes) -> (Game, u16, u16) {
    let (mut game, site, node) = with_site(BuildingKind::WOODCUTTER);
    game.world.bug_fixes = fixes;
    let countdown = u8::try_from(site).unwrap();
    let at = Node { x: 30, y: 25 };
    let mut walker = |job| {
        let path = game
            .find_path(at, HQ_FLAG, crate::homeward::Crossing::Walkable, false)
            .unwrap();
        game.world.add_figure(
            at,
            crate::figures::Figure::new(FigureKind::Homeward {
                owner: player(),
                job,
                path,
                countdown,
            }),
        )
    };
    let helper = walker(Job::HELPER);
    let woodcutter = walker(Job::WOODCUTTER);
    demolish(&mut game, node);
    (game, helper, woodcutter)
}

#[test]
fn a_settler_walking_home_with_the_sites_number_left_is_lost_again_unless_a_helper() {
    let (game, helper, woodcutter) = homeward_with_the_sites_number(BugFixes::Off);
    assert!(matches!(
        game.world().figure(helper).unwrap().kind,
        FigureKind::Homeward { .. }
    ));
    assert!(matches!(
        game.world().figure(woodcutter).unwrap().kind,
        FigureKind::Lost { .. }
    ));
}

#[test]
fn with_the_bug_fixes_on_settlers_walking_home_carry_on_whatever_their_countdown() {
    let (game, helper, woodcutter) = homeward_with_the_sites_number(BugFixes::On);
    for record in [helper, woodcutter] {
        assert!(matches!(
            game.world().figure(record).unwrap().kind,
            FigureKind::Homeward { .. }
        ));
    }
}
