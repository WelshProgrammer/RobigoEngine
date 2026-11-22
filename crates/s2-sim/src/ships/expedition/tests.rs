use crate::commands::{Command, Refusal};
use crate::game::Game;
use crate::grid::Node;
use crate::harbours::tests::{LAUNCH_NODE, WEST, harbour, record, two_shores};
use crate::job::Job;
use crate::nodes::{NodeFlags, SiteSize};
use crate::settlers::tests::player;
use crate::ships::{SECTORS, Ship, ShipMode};
use crate::supply::EXPEDITION_TAG;
use crate::ware::WareKind;
use crate::world::{BuildingKind, BuildingState, MapObject, Player};

/// A harbour site on the east shore, two nodes from the strait.
const SITE: Node = Node { x: 29, y: 26 };

/// The two shores with the east one belonging to nobody and a harbour site on it.
fn far_coast() -> Game {
    let mut game = two_shores();
    for y in 0..40 {
        for x in 28..48 {
            let at = Node { x, y };
            let mut data = game.nodes.get(at);
            data.owner = None;
            game.nodes.set(at, data);
        }
    }
    for y in 0..40 {
        for x in 28..48 {
            let at = Node { x, y };
            let mut data = game.nodes.get(at);
            data.seen = crate::sight::Seen::empty();
            game.nodes.set(at, data);
        }
    }
    let mut data = game.nodes.get(SITE);
    data.flags |= NodeFlags::HARBOUR;
    data.sites.size = SiteSize::Large;
    game.nodes.set(SITE, data);
    game
}

/// The west harbour's building, its stock set to `boards`, `stones` and `builders`.
fn stocked(game: &mut Game, boards: u16, stones: u16, builders: u16) -> u16 {
    let building = harbour(game, WEST);
    let (store, _) = game.world().storehouse_of(building).unwrap();
    let store = game.world.storehouse_mut(store).unwrap();
    store.wares[WareKind::BOARDS.index()] = boards;
    store.wares[WareKind::STONES.index()] = stones;
    store.jobs = [0; Job::PEOPLE];
    store.jobs[Job::BUILDER.index()] = builders;
    building
}

fn ship(game: &Game) -> (u16, Ship) {
    game.world()
        .ships()
        .next()
        .map(|(record, ship)| (record, *ship))
        .unwrap()
}

fn run(game: &mut Game, limit: u32, done: impl Fn(&Game) -> bool) {
    for _ in 0..limit {
        game.step();
        let broken = game.world().broken_links();
        assert!(broken.is_empty(), "{broken:?}");
        if done(game) {
            return;
        }
    }
    panic!("not done in {limit} steps");
}

#[test]
fn preparing_takes_what_the_harbour_holds_and_orders_the_rest() {
    let mut game = far_coast();
    let building = stocked(&mut game, 2, 9, 0);
    game.execute(player(), Command::PrepareExpedition { node: WEST })
        .unwrap();
    assert_eq!(
        game.execute(player(), Command::PrepareExpedition { node: WEST }),
        Err(Refusal::NotOffered),
        "already preparing"
    );
    let harbour = record(&game, building);
    assert!(harbour.expedition);
    assert_eq!((harbour.boards, harbour.stones, harbour.builders), (2, 6, 0));
    let stock = game.world().storehouse_of(building).unwrap().1;
    assert_eq!(stock.wares[WareKind::BOARDS.index()], 0);
    assert_eq!(stock.wares[WareKind::STONES.index()], 3);
    let flag = game.world().building(building).unwrap().flag;
    let mut tagged: Vec<_> = game
        .world()
        .orders()
        .filter(|(_, order)| order.flag == flag && order.tag == EXPEDITION_TAG)
        .map(|(_, order)| order.wanted)
        .collect();
    tagged.sort_by_key(|wanted| format!("{wanted:?}"));
    assert_eq!(tagged.len(), 3, "two boards and a builder: {tagged:?}");
}

#[test]
fn expedition_orders_fall_back_on_the_harbours_own_stock() {
    let mut game = far_coast();
    let building = stocked(&mut game, 0, 6, 0);
    game.execute(player(), Command::PrepareExpedition { node: WEST })
        .unwrap();
    let flag = game.world().building(building).unwrap().flag;
    let tagged = |game: &Game| -> Vec<u16> {
        game.world()
            .orders()
            .filter(|(_, order)| order.flag == flag && order.tag == EXPEDITION_TAG)
            .map(|(record, _)| record)
            .collect()
    };
    let (store, _) = game.world().storehouse_of(building).unwrap();
    let stock = game.world.storehouse_mut(store).unwrap();
    stock.wares[WareKind::BOARDS.index()] = 1;
    stock.jobs[Job::HELPER.index()] = 1;
    stock.wares[WareKind::HAMMER.index()] = 1;
    for order in tagged(&game) {
        if game.world().order(order).unwrap().ware().is_some() {
            game.serve_ware(order);
        } else {
            game.serve_person(order);
        }
    }
    let harbour = record(&game, building);
    assert_eq!((harbour.boards, harbour.builders), (1, 1));
    let stock = game.world().storehouse_of(building).unwrap().1;
    assert_eq!(stock.wares[WareKind::BOARDS.index()], 0);
    assert_eq!(
        (stock.jobs[Job::BUILDER.index()], stock.jobs[Job::HELPER.index()]),
        (1, 0),
        "the helper became a builder and stayed in stock"
    );
    assert_eq!(tagged(&game).len(), 3, "the other three boards still wanted");
}

#[test]
fn a_harbour_preparing_an_expedition_scores_sixty_for_boards_and_stones() {
    let mut game = far_coast();
    let building = stocked(&mut game, 0, 0, 1);
    let score = |game: &Game, ware| game.world().storehouse_score(building, ware);
    assert_eq!(score(&game, WareKind::BOARDS), 5);
    game.execute(player(), Command::PrepareExpedition { node: WEST })
        .unwrap();
    assert_eq!(score(&game, WareKind::BOARDS), 60);
    assert_eq!(score(&game, WareKind::STONES), 60);
    assert_eq!(score(&game, WareKind::WOOD), 5);
}

#[test]
fn a_ship_takes_a_ready_expedition_aboard_and_founds_a_harbour_on_the_far_coast() {
    let mut game = far_coast();
    let building = stocked(&mut game, 10, 10, 2);
    game.execute(player(), Command::PrepareExpedition { node: WEST })
        .unwrap();
    game.launch_ship(LAUNCH_NODE, player()).unwrap();
    run(&mut game, 3000, |game| {
        matches!(ship(game).1.mode, ShipMode::Expedition { .. })
    });
    let (record_id, aboard) = ship(&game);
    assert!(!record(&game, building).expedition);
    assert_eq!(aboard.wares[WareKind::BOARDS.index()], 4);
    assert_eq!(aboard.wares[WareKind::STONES.index()], 6);
    assert_eq!(aboard.people[Job::BUILDER.index()], 1);
    let sector = (0..SECTORS).find(|&sector| aboard.sectors[sector]).unwrap();
    assert_eq!(
        aboard.sectors.iter().filter(|&&found| found).count(),
        1,
        "{:?}",
        aboard.sectors
    );
    assert_eq!(
        game.execute(player(), Command::FoundHarbour { ship: record_id }),
        Err(Refusal::NotOffered),
        "on its own land"
    );
    let sector = u8::try_from(sector).unwrap();
    game.execute(
        player(),
        Command::SailExpedition {
            ship: record_id,
            sector,
        },
    )
    .unwrap();
    assert!(ship(&game).1.sailing);
    run(&mut game, 3000, |game| !ship(game).1.sailing);
    let landed = ship(&game).1;
    assert!(landed.unclaimed);
    let node = game.world().figure(landed.figure).unwrap().node();
    assert!(node.x >= 26 && node.y >= 24, "beside the site at {node:?}");
    let inland = |east| Node {
        x: node.x + east,
        y: node.y,
    };
    assert!(
        game.nodes().fully_seen(inland(5), player()),
        "the landing reveals 6"
    );
    assert!(!game.nodes().fully_seen(inland(7), player()));
    game.execute(player(), Command::FoundHarbour { ship: record_id })
        .unwrap();
    let MapObject::Building(founded) = game.world().object(SITE) else {
        panic!("no harbour site");
    };
    let site = *game.world().building(founded).unwrap();
    assert_eq!(site.kind, BuildingKind::HARBOUR);
    assert!(
        site.state
            .contains(BuildingState::BUILDING | BuildingState::PRESENT)
    );
    assert_eq!((site.stock[0], site.stock[1]), (4, 6));
    assert!(site.builder.is_some());
    assert_eq!(game.nodes().get(SITE).owner, Some(player()));
    assert!(!game.nodes().get(SITE).protected);
    let after = ship(&game).1;
    assert_eq!(after.mode, ShipMode::Transport { harbour: None });
    assert_eq!(after.wares, [0; crate::world::WARE_TYPES]);
    assert_eq!(after.people, [0; Job::PEOPLE]);
    let flag = site.flag;
    assert!(
        game.world()
            .orders()
            .all(|(_, order)| order.flag != flag || order.job() != Some(Job::BUILDER)),
        "the builder order settled"
    );
    let found = game.world().broken_links();
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn with_no_harbour_site_in_reach_the_expedition_goes_back_into_stock() {
    let mut game = two_shores();
    let building = stocked(&mut game, 10, 10, 2);
    game.execute(player(), Command::PrepareExpedition { node: WEST })
        .unwrap();
    game.launch_ship(LAUNCH_NODE, player()).unwrap();
    run(&mut game, 3000, |game| !record(game, building).expedition);
    let stock = game.world().storehouse_of(building).unwrap().1;
    assert_eq!(stock.wares[WareKind::BOARDS.index()], 10);
    assert_eq!(stock.wares[WareKind::STONES.index()], 10);
    assert_eq!(stock.jobs[Job::BUILDER.index()], 2);
    assert!(matches!(
        ship(&game).1.mode,
        ShipMode::Transport { .. } | ShipMode::Anchoring { .. }
    ));
    let harbour = record(&game, building);
    assert_eq!(
        (harbour.boards, harbour.stones, harbour.builders),
        (4, 6, 1),
        "left as they were"
    );
}

#[test]
fn a_cancelled_expedition_keeps_its_cargo_as_a_transport_ship_with_no_harbour() {
    let mut game = far_coast();
    stocked(&mut game, 10, 10, 2);
    game.execute(player(), Command::PrepareExpedition { node: WEST })
        .unwrap();
    game.launch_ship(LAUNCH_NODE, player()).unwrap();
    run(&mut game, 3000, |game| {
        matches!(ship(game).1.mode, ShipMode::Expedition { .. })
    });
    let (record_id, _) = ship(&game);
    assert_eq!(
        game.execute(
            Player::new(1).unwrap(),
            Command::CancelExpedition { ship: record_id }
        ),
        Err(Refusal::NotYours)
    );
    game.execute(player(), Command::CancelExpedition { ship: record_id })
        .unwrap();
    let after = ship(&game).1;
    assert_eq!(after.mode, ShipMode::Transport { harbour: None });
    assert_eq!(after.wares[WareKind::BOARDS.index()], 4);
    assert_eq!(
        game.execute(player(), Command::CancelExpedition { ship: record_id }),
        Err(Refusal::NotOffered)
    );
    assert_eq!(
        game.execute(player(), Command::CancelExpedition { ship: 9 }),
        Err(Refusal::NoShip)
    );
}

#[test]
fn a_board_made_for_the_expedition_claims_its_order_and_counts_as_collected() {
    let mut game = far_coast();
    let building = stocked(&mut game, 0, 6, 1);
    game.execute(player(), Command::PrepareExpedition { node: WEST })
        .unwrap();
    let flag = game.world().building(building).unwrap().flag;
    let open = |game: &Game| {
        game.world()
            .orders()
            .filter(|(_, order)| order.flag == flag && order.tag == EXPEDITION_TAG)
            .count()
    };
    assert_eq!(open(&game), 4);
    assert!(game.put_out(flag, WareKind::BOARDS));
    assert_eq!(
        record(&game, building).boards,
        1,
        "handed straight in at its own flag"
    );
    assert_eq!(open(&game), 3, "its order claimed and done");
    let stock = game.world().storehouse_of(building).unwrap().1;
    assert_eq!(stock.wares[WareKind::BOARDS.index()], 0, "kept out of stock");
}
