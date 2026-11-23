use crate::commands::{Command, Refusal, StockItem};
use crate::figures::FigureKind;
use crate::game::Game;
use crate::job::Job;
use crate::settlers::tests::{HQ, HQ_FLAG, game, player, run_to};
use crate::transport::WareState;
use crate::ware::WareKind;
use crate::world::{MapObject, Player, StockSetting, Storehouse};

fn headquarters(game: &Game) -> u16 {
    let MapObject::Building(record) = game.world().object(HQ) else {
        panic!("no headquarters");
    };
    record
}

fn stock(game: &Game) -> &Storehouse {
    game.world().storehouse_of(headquarters(game)).unwrap().1
}

fn set(game: &mut Game, item: StockItem, setting: StockSetting, on: bool) {
    let command = Command::SetStock {
        node: HQ,
        item,
        setting,
        on,
    };
    game.execute(player(), command).unwrap();
}

/// Runs until `done` holds, at most `steps` more steps.
fn run_until(game: &mut Game, steps: u32, done: impl Fn(&Game) -> bool) {
    let until = game.clock().step() + steps;
    while !done(game) {
        assert!(
            game.clock().step() < until,
            "gave up at step {}",
            game.clock().step()
        );
        game.step();
    }
}

#[test]
fn wares_go_out_in_transport_order_and_wait_stranded_until_taken_back() {
    let mut game = game();
    let hq = headquarters(&game);
    set(
        &mut game,
        StockItem::Ware(WareKind::WOOD),
        StockSetting::SEND,
        true,
    );
    set(
        &mut game,
        StockItem::Ware(WareKind::STONES),
        StockSetting::SEND,
        true,
    );
    let (wood, stones) = (stock(&game).wares[0], stock(&game).wares[2]);
    game.send_stock_out(hq);
    assert_eq!(
        (stock(&game).wares[0], stock(&game).wares[2]),
        (wood, stones - 1),
        "stones rank above wood in the default transport list"
    );
    let flag = game.world().flag_at(HQ_FLAG).unwrap();
    run_until(&mut game, 100, |game| game.world().wares_at(flag).count() > 0);
    let (_, ware) = game.world().wares_at(flag).next().unwrap();
    assert_eq!((ware.kind, ware.state), (WareKind::STONES, WareState::Stranded));

    set(
        &mut game,
        StockItem::Ware(WareKind::WOOD),
        StockSetting::SEND,
        false,
    );
    set(
        &mut game,
        StockItem::Ware(WareKind::STONES),
        StockSetting::SEND,
        false,
    );
    run_until(&mut game, 1000, |game| game.world().wares_at(flag).count() == 0);
    assert_eq!(stock(&game).wares[2], stones, "handed straight back in");
}

#[test]
fn a_storehouse_that_stops_a_ware_is_no_destination_for_it() {
    let mut game = game();
    let flag = game.world().flag_at(HQ_FLAG).unwrap();
    let hq = Some(headquarters(&game));
    assert_eq!(game.world().destination_for(flag, WareKind::BOARDS), hq);
    set(
        &mut game,
        StockItem::Ware(WareKind::BOARDS),
        StockSetting::STOP,
        true,
    );
    assert_eq!(game.world().destination_for(flag, WareKind::BOARDS), None);
    assert_eq!(game.world().destination_for(flag, WareKind::STONES), hq);
}

#[test]
fn settlers_take_turns_with_wares_and_walk_back_in_unless_stopped() {
    let mut game = game();
    let hq = headquarters(&game);
    let woodcutters = stock(&game).jobs[Job::WOODCUTTER.index()];
    set(
        &mut game,
        StockItem::Job(Job::WOODCUTTER),
        StockSetting::SEND,
        true,
    );
    game.send_stock_out(hq);
    assert_eq!(
        game.world().figures().count(),
        0,
        "a ware turn with no ware to send"
    );
    game.send_stock_out(hq);
    let (_, figure) = game.world().figures().next().unwrap();
    assert!(matches!(
        figure.kind,
        FigureKind::Leaving {
            job: Job::WOODCUTTER,
            order: None,
            ..
        }
    ));
    assert_eq!(stock(&game).jobs[Job::WOODCUTTER.index()], woodcutters - 1);
    run_until(&mut game, 200, |game| game.world().figures().count() == 0);
    assert_eq!(stock(&game).jobs[Job::WOODCUTTER.index()], woodcutters);
    assert_eq!(game.world().orders().count(), 0);

    set(
        &mut game,
        StockItem::Job(Job::WOODCUTTER),
        StockSetting::STOP,
        true,
    );
    let started = game.clock().step();
    run_to(&mut game, started + 200);
    assert!(
        game.world()
            .figures()
            .any(|(_, figure)| matches!(figure.kind, FigureKind::Lost { .. })),
        "nowhere takes a woodcutter in"
    );
}

#[test]
fn only_the_owner_sets_a_finished_storehouse_and_only_known_stock() {
    let mut game = game();
    let command = |node, item| Command::SetStock {
        node,
        item,
        setting: StockSetting::STOP,
        on: true,
    };
    let rival = Player::new(1).unwrap();
    let stones = StockItem::Ware(WareKind::STONES);
    assert_eq!(game.execute(rival, command(HQ, stones)), Err(Refusal::NotYours));
    assert_eq!(
        game.execute(player(), command(HQ_FLAG, stones)),
        Err(Refusal::NoBuilding)
    );
    assert_eq!(
        game.execute(player(), command(HQ, StockItem::Job(Job::WATER_ROAD))),
        Err(Refusal::NoSuchStock)
    );
    let all = Command::SetAllStock {
        node: HQ,
        people: true,
        setting: StockSetting::STOP,
        on: true,
    };
    game.execute(player(), all).unwrap();
    assert!(
        stock(&game)
            .job_settings
            .iter()
            .all(|setting| *setting == StockSetting::STOP)
    );
    assert!(stock(&game).ware_settings.iter().all(StockSetting::is_empty));
}
