use super::*;
use crate::computer::ComputerPlayer;
use crate::computer::{ComputerProfiles, Computers};
use crate::game::{BugFixes, Game};
use crate::players::{Controller, PlayerSlot, PlayerSlots, Portrait, Portraits};
use crate::rng::GameRng;
use crate::settlers::tests::{HQ, game, player};
use crate::world::Player;

/// The test map with player 0 played by the computer, with its AI.
pub(in crate::computer) fn computer_game() -> Game {
    let mut game = game();
    let mut slots = PlayerSlots::EMPTY;
    slots.set(
        player(),
        Some(PlayerSlot {
            controller: Controller::Computer,
            portrait: Portrait(0),
        }),
    );
    game.world.set_slots(slots);
    game.computers = Computers::build(
        (&game.nodes, &game.world),
        &slots,
        (&Portraits::ORIGINAL, &ComputerProfiles::ORIGINAL),
        &mut GameRng::at_start(),
    );
    game
}

/// Runs `body` with player 0's AI of `game` and a think for it. The AIs and their grid are lent
/// out of the game meanwhile, as `Game::computers_step` does.
pub(in crate::computer) fn with_think<T>(
    game: &mut Game,
    body: impl FnOnce(&mut ComputerPlayer, &mut Think<'_>) -> T,
) -> T {
    let mut computers = std::mem::take(&mut game.computers);
    let Computers { grid, players } = &mut computers;
    let ai = players[player().index()].as_mut().unwrap();
    let mut think = Think {
        player: player(),
        step: 0,
        profile: ComputerProfiles::ORIGINAL.get(0),
        game,
        grid,
    };
    let answer = body(ai, &mut think);
    game.computers = computers;
    answer
}

#[test]
fn the_site_search_walks_the_centre_and_eleven_rings_then_gives_up() {
    let mut game = computer_game();
    let centre = Node { x: 20, y: 20 };
    let search = SiteSearch {
        kind: BuildingKind::WOODCUTTER,
        centre,
        passed: false,
        at: centre,
        ring: 0,
        side: 0,
        step: 0,
        radius: 0,
        skip: false,
    };
    let walked = with_think(&mut game, |ai, think| {
        let job = ai
            .add(Job::with(JobKind::Site, Payload::Site(search), 0), None, None)
            .unwrap();
        let mut walked = vec![centre];
        while ai.spiral_on(job, think) {
            let Some(Payload::Site(search)) = ai.job(job).map(|job| job.payload) else {
                panic!("the search went");
            };
            walked.push(search.at);
        }
        walked
    });
    let expected: Vec<Node> = centre.rings(11, 48, 40).collect();
    assert_eq!(walked.len(), 397);
    assert_eq!(walked, expected);
}

#[test]
fn a_site_search_turned_after_it_ran_out_stays_where_ring_12_ended() {
    let mut game = computer_game();
    let centre = Node { x: 20, y: 20 };
    let search = SiteSearch {
        kind: BuildingKind::WOODCUTTER,
        centre,
        passed: false,
        at: centre,
        ring: 0,
        side: 0,
        step: 0,
        radius: 0,
        skip: false,
    };
    with_think(&mut game, |ai, think| {
        let job = ai
            .add(Job::with(JobKind::Site, Payload::Site(search), 0), None, None)
            .unwrap();
        while ai.spiral_on(job, think) {}
        let at = |ai: &ComputerPlayer| match ai.job(job).map(|job| job.payload) {
            Some(Payload::Site(search)) => search.at,
            _ => panic!("the search went"),
        };
        let ended = at(ai);
        for _ in 0..100 {
            ai.spiral_on(job, think);
        }
        assert_eq!(at(ai), ended, "a direction past 5 moves the cursor nowhere");
    });
}

/// Whether a site search turned until its spiral runs out, on a map where the player owns no
/// node, is still on the running list, with `fixes`.
fn run_out_search_still_runs(fixes: BugFixes) -> bool {
    let mut game = computer_game();
    game.world.bug_fixes = fixes;
    let (width, height) = (game.nodes.width(), game.nodes.height());
    for node in (0..height).flat_map(|y| (0..width).map(move |x| Node { x, y })) {
        let mut data = game.nodes.get(node);
        data.owner = None;
        game.nodes.set(node, data);
    }
    let centre = Node { x: 20, y: 20 };
    let search = SiteSearch {
        kind: BuildingKind::WOODCUTTER,
        centre,
        passed: false,
        at: centre,
        ring: 0,
        side: 0,
        step: 0,
        radius: 0,
        skip: false,
    };
    with_think(&mut game, |ai, think| {
        let job = ai
            .add(Job::with(JobKind::Site, Payload::Site(search), 0), None, None)
            .unwrap();
        ai.run(job);
        let flags = |ai: &ComputerPlayer| ai.job(job).unwrap().flags;
        for _ in 0..1000 {
            ai.search_turn(job, think);
            if flags(ai).contains(JobFlags::FINISHED) {
                return flags(ai).contains(JobFlags::RUNNING);
            }
        }
        panic!("the spiral never ran out");
    })
}

#[test]
fn a_site_search_whose_spiral_runs_out_stays_on_the_running_list() {
    assert!(run_out_search_still_runs(BugFixes::Off));
}

#[test]
fn with_the_bug_fixes_on_a_site_search_whose_spiral_runs_out_leaves_the_running_list() {
    assert!(!run_out_search_still_runs(BugFixes::On));
}

#[test]
fn a_build_order_ends_in_a_site_joined_to_the_headquarters() {
    let mut game = computer_game();
    let node = Node { x: 22, y: 16 };
    let ai = game.computers.get_mut(player()).unwrap();
    ai.add(Job::build_order(BuildingKind::WOODCUTTER, node, 6, 0), None, None);
    for _ in 0..400 {
        game.step();
    }
    let sites: Vec<_> = game
        .world()
        .buildings()
        .filter(|(_, building)| building.kind == BuildingKind::WOODCUTTER)
        .map(|(_, building)| building.flag)
        .collect();
    assert_eq!(sites.len(), 1, "one woodcutter site");
    let hq_flag = game.world().building_at_flag(sites[0]).and_then(|_| {
        let hq = game.world().object(HQ).building()?;
        Some(game.world().building(hq)?.flag)
    });
    let reached = hq_flag.and_then(|hq| {
        crate::search(
            game.world(),
            sites[0],
            200,
            crate::SearchOptions::empty(),
            |reached| reached.flag == hq,
        )
    });
    assert!(reached.is_some(), "a road joins the site to the headquarters");
    let woodcutter_order = game
        .computers()
        .get(Player::new(0).unwrap())
        .unwrap()
        .jobs()
        .any(|(_, job)| {
            matches!(
                job.payload,
                Payload::BuildOrder {
                    kind: BuildingKind::WOODCUTTER,
                    ..
                }
            )
        });
    assert!(!woodcutter_order, "the order is done");
}
