use crate::commands::Command;
use crate::construction::BUILDER_MODE;
use crate::figures::{Builder, Figure, FigureKind, OnRoad, Walker};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::settlers::tests::{HQ_FLAG, game, player, road_east};
use crate::world::BuildingKind;
use crate::world::{Building, BuildingState, MapObject, WAITING};

const END: Node = Node { x: 20, y: 17 };

/// A game with a 4-edge road east of the headquarters and a site of `kind` at its far flag.
fn with_site(kind: BuildingKind) -> (Game, u16) {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    let node = END.neighbour(Direction::NorthWest, game.nodes.width(), game.nodes.height());
    game.execute(player(), Command::PlaceBuilding { node, kind })
        .unwrap();
    let MapObject::Building(site) = game.world().object(node) else {
        panic!("no site");
    };
    (game, site)
}

fn site(game: &Game, record: u16) -> Building {
    *game.world().building(record).unwrap()
}

/// Puts a builder walker on the site's flag and lets it arrive. Returns the builder figure.
fn arrive(game: &mut Game, record: u16) -> u16 {
    let flag = site(game, record).flag;
    let (order, _) = game
        .world()
        .orders()
        .find(|(_, order)| order.job() == Some(Job::BUILDER) && order.flag == flag)
        .unwrap();
    let walker = game.world.add_figure(
        END,
        Figure::new(FigureKind::Walker(Walker {
            owner: player(),
            job: Job::BUILDER,
            order,
            mode: BUILDER_MODE,
            on: OnRoad {
                road: 0,
                position: 0,
                to_start: false,
            },
        })),
    );
    game.builder_arrives(walker, order);
    site(game, record).builder.unwrap()
}

fn builder(game: &Game, record: u16) -> Builder {
    let Some(FigureKind::Builder(builder)) = game.world().figure(record).map(|figure| figure.kind) else {
        panic!("not a builder");
    };
    builder
}

#[test]
fn an_arriving_builder_starts_on_the_flag_and_waits_for_materials() {
    let (mut game, record) = with_site(BuildingKind::WOODCUTTER);
    let figure = arrive(&mut game, record);
    let building = site(&game, record);
    assert!(building.state.contains(BuildingState::PRESENT));
    assert_eq!((building.waypoint, building.timer), (5, WAITING));
    let builder = builder(&game, figure);
    assert_eq!((builder.x, builder.y, builder.leg), (28, 28, None));
    assert!(
        game.world()
            .orders()
            .all(|(_, order)| order.job() != Some(Job::BUILDER))
    );
    assert!(
        game.world().figures_at(END).next().is_none(),
        "the walker is gone"
    );
}

#[test]
fn the_builder_walks_pixel_by_pixel_and_draws_only_at_inner_waypoints() {
    let (mut game, record) = with_site(BuildingKind::WOODCUTTER);
    let figure = arrive(&mut game, record);
    let rng = game.rng;
    for _ in 0..21 {
        game.step_builder(figure);
    }
    let walked = builder(&game, figure);
    assert_eq!((walked.x, walked.y), (43, 7));
    assert_eq!(walked.facing, Direction::NorthEast, "latched on the first step");
    assert_eq!(site(&game, record).waypoint, 4, "back from the end, with no draw");
    assert_eq!(game.rng, rng);
    for _ in 0..12 {
        game.step_builder(figure);
    }
    let walked = builder(&game, figure);
    assert_eq!(
        (walked.x, walked.y, walked.facing),
        (31, 13, Direction::SouthWest)
    );
    let mut expected = rng;
    let flip = expected.up_to(1);
    assert_eq!(game.rng, expected, "one draw at waypoint 4");
    assert_eq!(site(&game, record).waypoint, if flip == 0 { 5 } else { 3 });
}

#[test]
fn each_board_takes_nine_building_ticks() {
    let (mut game, record) = with_site(BuildingKind::WOODCUTTER);
    arrive(&mut game, record);
    game.world.building_mut(record).unwrap().stock[0] = 2;
    game.building_tick();
    assert_eq!(
        (site(&game, record).used[0], site(&game, record).timer),
        (1, 1),
        "a waiting site retries on every tick"
    );
    for _ in 0..8 {
        game.building_tick();
    }
    assert_eq!((site(&game, record).used[0], site(&game, record).timer), (1, 9));
    game.building_tick();
    assert_eq!(
        (site(&game, record).used[0], site(&game, record).stock[0]),
        (2, 0)
    );
}

#[test]
fn a_finished_workplace_sends_its_builder_home() {
    let (mut game, record) = with_site(BuildingKind::WOODCUTTER);
    let figure = arrive(&mut game, record);
    game.world.building_mut(record).unwrap().used = [2, 0];
    game.building_tick();
    let building = site(&game, record);
    assert_eq!(building.state, BuildingState::FINISHED | BuildingState::DOOR);
    assert_eq!(building.builder, None);
    assert!(
        game.world()
            .figure(figure)
            .is_none_or(|figure| !matches!(figure.kind, FigureKind::Builder(_)))
    );
    let (_, walker) = game.world().figures_at(END).next().unwrap();
    assert!(matches!(
        walker.kind,
        FigureKind::Walker(Walker {
            job: Job::BUILDER,
            mode: 1,
            ..
        })
    ));
    assert!(walker.held);
}

#[test]
fn a_finished_storehouse_keeps_its_builder() {
    let (mut game, record) = with_site(BuildingKind::STOREHOUSE);
    arrive(&mut game, record);
    game.world.building_mut(record).unwrap().used = [4, 3];
    game.building_tick();
    let building = site(&game, record);
    assert_eq!(
        building.state,
        BuildingState::FINISHED | BuildingState::MANNED | BuildingState::PRESENT
    );
    let (_, store) = game.world().storehouse_of(record).unwrap();
    assert_eq!(store.jobs[Job::BUILDER.index()], 1);
    assert!(game.world().figures_at(END).next().is_none());
}

#[test]
fn a_site_is_built_and_its_builder_goes_home() {
    let (mut game, record) = with_site(BuildingKind::WOODCUTTER);
    let (_, store) = game.world().storehouses().next().unwrap();
    let builders = store.jobs[Job::BUILDER.index()];
    crate::settlers::tests::run_to(&mut game, 3000);
    let building = site(&game, record);
    assert!(building.state.contains(BuildingState::FINISHED), "{building:?}");
    assert_eq!(building.used, [2, 0]);
    let (_, store) = game.world().storehouses().next().unwrap();
    assert_eq!(store.jobs[Job::BUILDER.index()], builders, "back in stock");
}

#[test]
fn the_hammering_animations_last_as_rom_anim_says() {
    let Some(script) = crate::settlers::tests::shipped_script() else {
        eprintln!("S2_DATA_PLAIN not set; skipped");
        return;
    };
    let mut game = game();
    game.set_animations(script);
    for (start, calls) in [(3424, 29), (3386, 36), (3402, 29)] {
        let at = Node { x: 0, y: 0 };
        let animation = game.start_animation(start, at).unwrap();
        let mut made = 1;
        while game.run_animation(animation, at) {
            made += 1;
        }
        assert_eq!(made + 1, calls, "{start:#x}: frames, then the end");
    }
}
