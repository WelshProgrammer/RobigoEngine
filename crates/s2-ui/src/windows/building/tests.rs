use s2_sim::{Building, BuildingKind, BuildingState, Storehouse};

use super::*;
use crate::paint::{ItemId, OpKind};
use crate::skin::Look;
use crate::windows::kind;

const WINDOW: WindowId = kind::FIRST_BUILDING;
const NODE: Node = Node { x: 10, y: 10 };

fn player() -> Player {
    Player::new(0).expect("player 0")
}

/// A world with one building of `kind` in `state` on [`NODE`], and a storehouse record when it
/// is a storehouse type.
fn world(kind: BuildingKind, state: BuildingState) -> World {
    let mut world = World::empty(32, 32);
    let building = Building::new(kind, player(), NODE, 0, state);
    let record = world.add_building(building).expect("a free building slot");
    world.set_object(NODE, MapObject::Building(record));
    if kind.is_storehouse() {
        let mut store = Storehouse::new(record);
        store.wares[WareKind::BOARDS.index()] = 12;
        world.add_storehouse(store).expect("a free storehouse slot");
    }
    world
}

fn finished() -> BuildingState {
    BuildingState::FINISHED | BuildingState::MANNED
}

fn view(kind: BuildingKind, state: BuildingState) -> BuildingView {
    BuildingView::read(&world(kind, state), player(), NODE).expect("the player's building")
}

fn skin() -> Skin {
    let mut skin = Skin::new();
    skin.set_face(STYLE, Look::Idle, ItemId(1));
    for picture in [Picture::PRODUCING, Picture::PRODUCTION_STOPPED, Picture::TAKE_OUT] {
        skin.set_picture(picture, ItemId(10 + picture.0));
    }
    skin
}

/// A window for `view` open at the top-left corner, and a painter to draw it into.
fn open(view: &BuildingView) -> (Ui, BuildingWindow, Painter) {
    let (width, height) = view.page.size();
    let mut ui = Ui::new();
    ui.open(WINDOW, Rect::new(0, 0, width, height));
    (
        ui,
        BuildingWindow::new(WINDOW, view, false),
        Painter::new(Rect::new(0, 0, 640, 480)),
    )
}

/// What a press and a release at `at` ask for.
fn click(ui: &mut Ui, window: &mut BuildingWindow, view: &BuildingView, at: Point) -> Option<Outcome> {
    let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
    let on = Pointer::at(at);
    let pressed = window.run(ui, &mut painter, &skin(), view, on.pressing());
    painter.reset();
    pressed.or_else(|| window.run(ui, &mut painter, &skin(), view, on.releasing()))
}

#[test]
fn the_page_and_size_come_from_the_building() {
    let site = view(BuildingKind::WOODCUTTER, BuildingState::BUILDING);
    assert_eq!((site.page, site.page.size()), (Page::Site, (226, 194)));
    let storehouse_site = view(BuildingKind::STOREHOUSE, BuildingState::LEVELLING);
    assert_eq!(storehouse_site.page, Page::Site, "every site takes the site page");
    let woodcutter = view(BuildingKind::WOODCUTTER, finished());
    assert_eq!(woodcutter.page, Page::Workplace);
    let headquarters = view(BuildingKind::HEADQUARTERS, BuildingState::NEW_HEADQUARTERS);
    assert_eq!(
        (headquarters.page, headquarters.page.size()),
        (Page::Storehouse, (167, 416))
    );
    assert!(headquarters.stock.is_some());
    assert_eq!(view(BuildingKind::BARRACKS, finished()).page, Page::Military);
}

#[test]
fn another_players_building_has_no_window() {
    let world = world(BuildingKind::WOODCUTTER, finished());
    let other = Player::new(1).expect("player 1");
    assert_eq!(BuildingView::read(&world, other, NODE), None);
    assert_eq!(BuildingView::read(&world, player(), Node { x: 1, y: 1 }), None);
}

#[test]
fn a_finished_site_is_no_longer_the_windows_building() {
    let site = view(BuildingKind::WOODCUTTER, BuildingState::BUILDING);
    let window = BuildingWindow::new(WINDOW, &site, false);
    assert!(window.current(Some(&site)));
    let done = view(BuildingKind::WOODCUTTER, finished());
    assert!(
        !window.current(Some(&done)),
        "the site's window closes when it finishes"
    );
    assert!(!window.current(None), "and when the building goes");
}

#[test]
fn the_title_names_the_building_type() {
    let woodcutter = view(BuildingKind::SAWMILL, finished());
    let window = BuildingWindow::new(WINDOW, &woodcutter, false);
    assert_eq!(window.title(), Label::Named(BUILDING_NAMES + 5));
}

#[test]
fn a_site_shows_its_line_and_the_finished_building() {
    let site = view(BuildingKind::WOODCUTTER, BuildingState::BUILDING);
    let (mut ui, mut window, mut painter) = open(&site);
    window.run(&mut ui, &mut painter, &skin(), &site, Pointer::away());
    let body = FIRST_BUILDING_ITEM + ITEMS_PER_GROUP * site.group;
    let at = Point::new(11 + 108, 22 + 109);
    assert!(painter.ops().iter().any(|op| op.kind
        == OpKind::Game {
            sheet: Sheet::Nation,
            slot: body,
            at
        }));
    assert!(painter.ops().iter().any(
        |op| matches!(op.kind, OpKind::Text { label: Label::Slot(slot::SITE), at, .. } if at.y == 22 + 22)
    ));
    assert!(
        !painter
            .ops()
            .iter()
            .any(|op| matches!(op.kind, OpKind::Fill { .. })),
        "a site has no bar"
    );
}

#[test]
fn the_switch_stops_production_and_shows_it_at_once() {
    let woodcutter = view(BuildingKind::WOODCUTTER, finished());
    let (mut ui, mut window, _) = open(&woodcutter);
    let switch = Point::new(90 + 4, 147 + 4);
    assert_eq!(
        click(&mut ui, &mut window, &woodcutter, switch),
        Some(Outcome::Command(Command::StopProduction {
            node: NODE,
            stop: true
        }))
    );
    assert!(window.shown.stopped, "the switch's own press changes its picture");
}

#[test]
fn the_lookout_tower_has_neither_switch_nor_bar() {
    let tower = view(BuildingKind::LOOKOUT_TOWER, finished());
    let (mut ui, mut window, mut painter) = open(&tower);
    window.run(&mut ui, &mut painter, &skin(), &tower, Pointer::away());
    assert!(
        !painter
            .ops()
            .iter()
            .any(|op| matches!(op.kind, OpKind::Fill { .. }))
    );
    assert_eq!(click(&mut ui, &mut window, &tower, Point::new(94, 151)), None);
}

#[test]
fn an_input_row_darkens_its_box_three_times_and_redraws_what_is_held() {
    let mut world = World::empty(32, 32);
    let mut building = Building::new(BuildingKind::SAWMILL, player(), NODE, 0, finished());
    building.stock[0] = 2;
    let record = world.add_building(building).expect("a free building slot");
    world.set_object(NODE, MapObject::Building(record));
    let sawmill = BuildingView::read(&world, player(), NODE).expect("the sawmill");
    let input = sawmill.inputs[0];
    assert_eq!((input.ware, input.held), (WareKind::WOOD, 2));
    let (mut ui, mut window, mut painter) = open(&sawmill);
    window.run(&mut ui, &mut painter, &skin(), &sawmill, Pointer::away());
    let count = i32::from(input.capacity);
    let start = ROW_MIDDLE - count * WORK_STEP / 2;
    let shade = Rect::new(
        11 + start - 12,
        22 + 38,
        u32::try_from(count * 24).expect("fits"),
        24,
    );
    let darkened = painter
        .ops()
        .iter()
        .filter(|op| op.kind == OpKind::Darken { rect: shade })
        .count();
    assert_eq!(darkened, DARKENINGS);
    let wood = icons::ware(WareKind::WOOD).expect("wood's icon");
    let first = Point::new(11 + start, 22 + 50);
    let drawn = painter
        .ops()
        .iter()
        .filter(|op| {
            op.kind
                == OpKind::Game {
                    sheet: Sheet::Landscape,
                    slot: wood,
                    at: first,
                }
        })
        .count();
    assert_eq!(drawn, 2, "once dark and once held");
}

#[test]
fn a_cell_acts_as_the_button_goes_down() {
    let headquarters = view(BuildingKind::HEADQUARTERS, BuildingState::NEW_HEADQUARTERS);
    let (mut ui, mut window, mut painter) = open(&headquarters);
    let boards = StockWindow::cell(1, Point::new(0, 0));
    let press = Pointer::at(Point::new(boards.x - 14, boards.y - 12)).pressing();
    assert_eq!(
        window.run(&mut ui, &mut painter, &skin(), &headquarters, press),
        Some(Outcome::Command(Command::SetStock {
            node: NODE,
            item: StockItem::Ware(WareKind::BOARDS),
            setting: StockSetting::SEND,
            on: true
        })),
        "a new window edits the send out set"
    );
}

#[test]
fn stop_storage_turns_the_cells_and_select_all_to_the_stop_set() {
    let headquarters = view(BuildingKind::HEADQUARTERS, BuildingState::NEW_HEADQUARTERS);
    let (mut ui, mut window, _) = open(&headquarters);
    assert_eq!(
        click(&mut ui, &mut window, &headquarters, Point::new(63 + 4, 335 + 4)),
        None
    );
    assert_eq!(window.editing, StockSetting::STOP);
    let all = Point::new(110 + 4, 335 + 4);
    let expect = |on| {
        Some(Outcome::Command(Command::SetAllStock {
            node: NODE,
            people: false,
            setting: StockSetting::STOP,
            on,
        }))
    };
    assert_eq!(click(&mut ui, &mut window, &headquarters, all), expect(true));
    assert_eq!(click(&mut ui, &mut window, &headquarters, all), expect(false));
}

#[test]
fn the_headquarters_has_three_pages_and_the_others_two() {
    assert_eq!(Shelf::Wares.next(true), Shelf::People);
    assert_eq!(Shelf::People.next(true), Shelf::Reserve);
    assert_eq!(Shelf::Reserve.next(true), Shelf::Wares);
    assert_eq!(Shelf::People.next(false), Shelf::Wares);
}

#[test]
fn the_reserve_page_raises_a_rank_by_one() {
    let headquarters = view(BuildingKind::HEADQUARTERS, BuildingState::NEW_HEADQUARTERS);
    let (mut ui, mut window, _) = open(&headquarters);
    window.shelf = Shelf::Reserve;
    let wanted = headquarters.reserve[1].wanted;
    assert_eq!(
        click(
            &mut ui,
            &mut window,
            &headquarters,
            Point::new(116 + 4, 109 + 30 + 4)
        ),
        Some(Outcome::Command(Command::SetReserve {
            node: NODE,
            rank: Job::RANKS[1],
            wanted: wanted + 1
        }))
    );
    assert_eq!(
        click(&mut ui, &mut window, &headquarters, Point::new(110 + 4, 335 + 4)),
        None,
        "select all does nothing on the reserve page"
    );
}

#[test]
fn only_the_headquarters_lacks_the_demolish_button() {
    let store = view(BuildingKind::STOREHOUSE, finished());
    let (mut ui, mut window, _) = open(&store);
    assert_eq!(
        click(&mut ui, &mut window, &store, Point::new(50 + 4, 369 + 4)),
        Some(Outcome::Demolish(NODE, WINDOW))
    );
    let headquarters = view(BuildingKind::HEADQUARTERS, BuildingState::NEW_HEADQUARTERS);
    let (mut ui, mut window, _) = open(&headquarters);
    assert_eq!(
        click(&mut ui, &mut window, &headquarters, Point::new(50 + 4, 369 + 4)),
        None,
        "the headquarters' next page button sits there and turns the page"
    );
    assert_eq!(window.shelf, Shelf::People);
}

#[test]
fn a_paused_game_leaves_the_switch_as_it_was() {
    let woodcutter = view(BuildingKind::WOODCUTTER, finished());
    let (mut ui, mut window, _) = open(&woodcutter);
    ui.set_paused(true);
    assert_eq!(
        click(&mut ui, &mut window, &woodcutter, Point::new(94, 151)),
        None
    );
    assert!(!window.shown.stopped, "the picture keeps the running state");
    let headquarters = view(BuildingKind::HEADQUARTERS, BuildingState::NEW_HEADQUARTERS);
    let (mut ui, mut window, mut painter) = open(&headquarters);
    ui.set_paused(true);
    let boards = StockWindow::cell(1, Point::new(0, 0));
    let press = Pointer::at(Point::new(boards.x - 14, boards.y - 12)).pressing();
    assert_eq!(
        window.run(&mut ui, &mut painter, &skin(), &headquarters, press),
        None
    );
}
