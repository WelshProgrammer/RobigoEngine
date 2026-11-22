use crate::commands::Command;
use crate::figures::FigureKind;
use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::settlers::tests::{HQ, HQ_FLAG, game, headquarters, player, road_east, run_to};
use crate::world::{BuildingState, Player};

use super::*;

use Direction::{East, NorthEast, SouthEast, West};

const TO: Node = Node { x: 0, y: 0 };

fn steps_of(path: &mut Path) -> Vec<Direction> {
    std::iter::from_fn(|| path.next()).collect()
}

#[test]
fn a_path_reads_back_the_steps_it_was_given() {
    let steps = [East, East, SouthEast, West, West, West];
    let (mut path, stopped) = Path::encode(&steps, TO);
    assert!(!stopped);
    assert_eq!(path.len, 3);
    assert_eq!(steps_of(&mut path), steps);
    assert_eq!(path.next(), None);
}

#[test]
fn a_run_longer_than_31_takes_more_than_one_byte() {
    let steps = [East; 40];
    let (mut path, _) = Path::encode(&steps, TO);
    assert_eq!(
        path.runs[..2]
            .iter()
            .map(|run| (run.direction, run.steps))
            .collect::<Vec<_>>(),
        [(East, 31), (East, 9)]
    );
    assert_eq!(path.runs[0].byte(), 0b1111_1000);
    assert_eq!(steps_of(&mut path).len(), 40);
}

#[test]
fn a_zigzag_fills_the_record_and_stops_at_the_turn_that_finds_it_full() {
    let zigzag: Vec<Direction> = (0..40)
        .map(|i| if i % 2 == 0 { East } else { NorthEast })
        .collect();
    assert!(!Path::encode(&zigzag[..30], TO).1, "30 runs, the last still open");
    let (mut path, stopped) = Path::encode(&zigzag[..31], TO);
    assert!(stopped, "the 31st step closes the 30th run");
    assert_eq!(path.len, 30);
    assert_eq!(steps_of(&mut path), zigzag[..30]);
}

#[test]
fn a_stranded_carrier_finds_its_way_back_into_the_headquarters() {
    let mut game = game();
    road_east(&mut game, HQ_FLAG, 4);
    run_to(&mut game, 200);
    let (carrier, _) = game.world().figures().next().unwrap();
    assert_eq!(headquarters(&game).1, 51);
    let people = |game: &Game| {
        let MapObject::Building(building) = game.world().object(HQ) else {
            panic!("no headquarters");
        };
        let jobs = game.world().storehouse_of(building).unwrap().1.jobs;
        jobs[Job::HELPER.index()] + jobs[Job::PRIVATE.index()]
    };
    let (before, from) = (people(&game), game.clock().step());
    let mut expected = game.rng;
    let _ = expected.up_to(100);
    let _ = expected.up_to(0);
    let _ = expected.up_to(0);
    game.execute(
        player(),
        Command::RemoveRoad {
            node: Node { x: 18, y: 17 },
        },
    )
    .unwrap();
    game.step();
    let FigureKind::Homeward { path, .. } = game.world().figure(carrier).unwrap().kind else {
        panic!("not on its way home");
    };
    assert_eq!(
        game.rng, expected,
        "one draw for getting lost, one per edge of a two-edge path"
    );
    assert_eq!(game.world().paths.get(path).unwrap().to, HQ_FLAG);
    let start = game.clock().step();
    run_to(&mut game, start + 2 * 19 + 1);
    assert!(matches!(
        game.world().figure(carrier).unwrap().kind,
        FigureKind::Entering { job: Job::HELPER, .. }
    ));
    assert!(game.world().paths.is_empty());
    run_to(&mut game, start + 3 * 19 + 1);
    assert!(game.world().figure(carrier).is_none());
    let made: u16 = (from + 1..=game.clock().step())
        .filter(|step| (step + 2).is_multiple_of(240))
        .count()
        .try_into()
        .unwrap();
    assert_eq!(
        people(&game),
        before + 1 + made,
        "the carrier, and a helper per pass"
    );
    assert!(headquarters(&game).0.contains(BuildingState::DOOR));
}

#[test]
fn a_lost_settler_with_no_storehouse_wanders_mostly_ahead() {
    let mut game = game();
    let at = Node { x: 30, y: 20 };
    let rival = Player::new(1).unwrap();
    let mut figure = crate::figures::Figure::new(FigureKind::Lost {
        owner: rival,
        job: Job::HELPER,
        countdown: 100,
    });
    figure.direction = East;
    let record = game.world.add_figure(at, figure);
    let mut ahead = 0;
    for _ in 0..40 {
        while game.world().figure(record).unwrap().walking() {
            game.step();
        }
        let before = game.world().figure(record).unwrap().direction;
        game.step();
        let after = game.world().figure(record).unwrap().direction;
        if after.index() == (before.index() + 1) % 6 || after.index() == (before.index() + 5) % 6 {
            ahead += 1;
        }
    }
    assert!(ahead > 30, "{ahead} of 40 steps turned one way or the other");
}

#[test]
fn a_search_leaves_the_shared_position_on_its_target_or_past_its_last_ring() {
    let mut game = game();
    let from = Node { x: 20, y: 20 };
    let to = Node { x: 24, y: 20 };
    let (steps, cursor) = game.search_cross_country(from, to, Crossing::Walkable, false, SEARCH_START);
    assert_eq!(steps.map(|steps| steps.len()), Some(4));
    assert_eq!(cursor, Some(to), "a route found ends on its target");
    let (steps, cursor) = game.search_cross_country(from, to, Crossing::Walkable, false, 3);
    assert_eq!(steps, None, "two rings do not reach four steps away");
    let last = cursor.unwrap();
    let (width, height) = (game.nodes.width(), game.nodes.height());
    assert!(
        to.rings(1, width, height)
            .skip(1)
            .any(|ring| ring.neighbour(Direction::NorthWest, width, height) == last),
        "it stops north-west of a node of the last ring it spread from"
    );
}
