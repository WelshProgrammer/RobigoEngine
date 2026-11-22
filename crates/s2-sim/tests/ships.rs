//! An expedition on the generated lake map: player 0 builds a shipyard on the shore and a
//! harbour on one of the lake's harbour sites, the shipyard builds a ship, and the harbour sends
//! it out to found a second harbour on the far shore. The replay `replays/lake-expedition.replay`
//! records it and the replays test pins its hashes. The map holds no game data, so this runs
//! everywhere.
//!
//! `ROBIGO_SHIPS=write cargo test -p s2-sim --test ships -- --ignored write` plans the buildings
//! and the expedition again and rewrites the replay's commands; bless the replay afterwards.

mod common;

use common::{Planner, building_at, headquarters, new_game, player, recorded, run, write_replay};

use s2_sim::{
    BuildingKind, BuildingState, Command, Direction, Game, Node, NodeFlags, Player, Setup, ShipMode,
    StartGoods,
};

/// The map, a file beside the replay.
const MAP: &str = "lake.wld";

/// The replay this test writes and plays.
const REPLAY: &str = "lake-expedition.replay";

/// Steps the replay runs: the ship is launched by about 13,900, the expedition lands by about
/// 17,800 and the second harbour stands by about 22,100.
const END: u32 = 24_000;

fn setup() -> Setup {
    Setup {
        players: None,
        animals: true,
        goods: StartGoods::Normal,
        slots: None,
        starts: s2_sim::StartOrder::InSequence,
        objective: s2_sim::Objective::None,
        bug_fixes: s2_sim::BugFixes::Off,
        mission: None,
    }
}

fn waypoints() -> Vec<u8> {
    vec![0]
}

/// Nodes of the map in row order.
fn all(game: &Game) -> Vec<Node> {
    let (width, height) = (game.nodes().width(), game.nodes().height());
    (0..height)
        .flat_map(|y| (0..width).map(move |x| Node { x, y }))
        .collect()
}

/// Whether `node` is on land with water beside it and dry land for its flag, which a shipyard
/// needs: its shipwright walks out of the flag and round the shore to the ship's site.
fn by_water(game: &Game, node: Node) -> bool {
    let (width, height) = (game.nodes().width(), game.nodes().height());
    let sea = |at: Node| game.nodes().is_sea(game.nodes().get(at).area);
    !sea(node)
        && !sea(node.neighbour(Direction::SouthEast, width, height))
        && Direction::ALL
            .into_iter()
            .any(|direction| sea(node.neighbour(direction, width, height)))
}

/// The player's harbour sites, nearest the headquarters first.
fn harbour_sites(game: &Game, owner: Player) -> Vec<Node> {
    let home = headquarters(game, owner);
    let mut sites: Vec<Node> = all(game)
        .into_iter()
        .filter(|&node| {
            game.nodes().get(node).owner == Some(owner)
                && game.nodes().get(node).flags.contains(NodeFlags::HARBOUR)
        })
        .collect();
    sites.sort_by_key(|&node| u32::from(node.x.abs_diff(home.x)) + u32::from(node.y.abs_diff(home.y)));
    sites
}

/// How many ships the player has.
fn ships(game: &Game, owner: Player) -> usize {
    game.world()
        .ships()
        .filter(|(_, ship)| ship.owner == owner)
        .count()
}

/// How many finished harbours the player has.
fn harbours(game: &Game, owner: Player) -> usize {
    game.world()
        .harbours()
        .filter(|(_, harbour)| {
            harbour.owner == owner
                && game
                    .world()
                    .building(harbour.building)
                    .is_some_and(|building| building.state.contains(BuildingState::FINISHED))
        })
        .count()
}

/// The player's first ship and its record.
fn ship_of(game: &Game, owner: Player) -> Option<(u16, s2_sim::Ship)> {
    game.world()
        .ships()
        .find(|(_, ship)| ship.owner == owner)
        .map(|(record, ship)| (record, *ship))
}

#[test]
#[ignore = "writes the expedition replay; run with ROBIGO_SHIPS=write"]
fn write() {
    if std::env::var_os("ROBIGO_SHIPS").is_none() {
        return;
    }
    let (game, mut replay) = new_game(MAP, setup(), waypoints(), END);
    let mut planner = Planner::new(game, player(0));
    let sites = harbour_sites(&planner.game, player(0));
    let harbour = planner
        .place(BuildingKind::HARBOUR, &sites)
        .expect("no room for a harbour");
    let shore: Vec<Node> = planner
        .owned()
        .into_iter()
        .filter(|&node| by_water(&planner.game, node))
        .collect();
    let yard = planner
        .place(BuildingKind::SHIPYARD, &shore)
        .expect("no room for a shipyard");
    eprintln!("harbour at {harbour:?}, shipyard at {yard:?}");
    let built = planner.run_until(END, |game| {
        game.world()
            .building(building_at(game, yard))
            .is_some_and(|building| building.state.contains(BuildingState::FINISHED))
    });
    assert!(built, "the shipyard was never finished");
    eprintln!("the shipyard stands at step {}", planner.step());
    assert!(planner.execute(Command::ShipyardMode {
        node: yard,
        ships: true,
    }));
    let afloat = planner.run_until(END, |game| ships(game, player(0)) > 0);
    assert!(afloat, "no ship was ever launched");
    eprintln!("a ship at step {}", planner.step());
    assert!(planner.execute(Command::PrepareExpedition { node: harbour }));
    let ready = planner.run_until(END, |game| {
        ship_of(game, player(0)).is_some_and(|(_, ship)| {
            matches!(ship.mode, ShipMode::Expedition { .. })
                && !ship.sailing
                && ship.sectors.iter().any(|&on| on)
        })
    });
    assert!(ready, "the expedition never loaded");
    let (ship, record) = ship_of(&planner.game, player(0)).unwrap();
    let sector = record.sectors.iter().position(|&on| on).unwrap();
    eprintln!(
        "expedition aboard ship {ship} at step {}, sectors {:?}",
        planner.step(),
        record.sectors
    );
    assert!(planner.execute(Command::SailExpedition {
        ship,
        sector: u8::try_from(sector).unwrap(),
    }));
    let landed = planner.run_until(END, |game| {
        game.world()
            .ship(ship)
            .is_some_and(|ship| !ship.sailing && ship.unclaimed)
    });
    assert!(landed, "the expedition never reached a harbour site");
    eprintln!("landed at step {}", planner.step());
    assert!(planner.execute(Command::FoundHarbour { ship }));
    let founded = planner.run_until(END, |game| harbours(game, player(0)) >= 2);
    assert!(founded, "the second harbour was never finished");
    eprintln!("two harbours at step {}", planner.step());
    replay.commands = planner.commands;
    let header = "# Player 0 builds a harbour and a shipyard on the lake of a generated map, \
                  the shipyard builds\n# a ship, and an expedition sails it to a harbour site on \
                  the far shore and founds a second\n# harbour there. Written by tests/ships.rs.\n";
    write_replay(REPLAY, &replay, header);
}

#[test]
fn the_expedition_founds_a_harbour_on_the_far_shore() {
    let (mut game, replay) = recorded(MAP, REPLAY, setup(), waypoints(), END);
    let mut sailed = false;
    let mut loaded = false;
    run(&mut game, &replay, replay.end, |game| {
        for (_, ship) in game.world().ships() {
            sailed |= ship.sailing;
            loaded |= matches!(ship.mode, ShipMode::Expedition { .. });
        }
    });
    assert!(loaded, "no expedition went aboard");
    assert!(sailed, "the ship never sailed");
    assert_eq!(harbours(&game, player(0)), 2, "two harbours in the end");
    let standing: Vec<Node> = game
        .world()
        .harbours()
        .filter_map(|(_, harbour)| game.world().building(harbour.building))
        .map(|building| building.node)
        .collect();
    for node in &standing {
        assert!(
            game.nodes().get(*node).flags.contains(NodeFlags::HARBOUR),
            "{node:?} is no harbour site"
        );
    }
    let records: Vec<_> = game.world().harbours().map(|(_, harbour)| *harbour).collect();
    assert!(
        records[0].shares_sea(&records[1]),
        "the two harbours are on one sea: {records:?}"
    );
    assert_eq!(game.world().ships().count(), 1, "the ship is still afloat");
    let broken = game.world().broken_links();
    assert!(broken.is_empty(), "{broken:#?}");
}
