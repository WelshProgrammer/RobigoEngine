//! How long each building takes: twenty rows, each a picture and a bar of the type's average
//! productivity.
//!
//! A type a campaign chapter has not given the player shows `io170` in place of its picture and
//! no bar.

use s2_render::terrain::Point;
use s2_sim::{BuildingKind, World};
use s2_systems::player::Player;

use crate::bar;
use crate::geometry::Rect;
use crate::paint::Painter;
use crate::picture::Picture;
use crate::skin::Skin;
use crate::store::Ui;
use crate::widget::WindowId;
use crate::windows::kind;

/// How many rows the screen has.
pub const ROWS: usize = 20;

/// The building type each row shows, the table at `0x1fa8a8` that the sample's average
/// productivity uses as well.
pub const ORDER: [u8; ROWS] = [
    0, 2, 3, 4, 5, 10, 11, 30, 6, 7, 9, 13, 14, 15, 17, 16, 18, 19, 20, 21,
];

/// The graphics group of `kind`, which is the item its picture comes from.
fn group(world: &World, kind: BuildingKind) -> u16 {
    world.building_types().get(kind).map_or(0, |kind| kind.group)
}

/// What one row shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductionRow {
    /// The type the row stands for.
    pub kind: BuildingKind,
    /// How productive the player's buildings of that type are on average.
    pub productivity: u8,
    /// Its graphics group, which is the item its picture comes from in the nation's own
    /// archive.
    pub group: u16,
    /// Whether the chapter has not given the player the type yet.
    pub locked: bool,
}

/// What the screen shows, read without a renderer so a test can assert it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionView {
    /// One row per cell, in row order.
    pub rows: Vec<ProductionRow>,
}

impl ProductionView {
    /// What `player`'s buildings manage. `allowed` says whether the player may build a type.
    #[must_use]
    pub fn read(world: &World, player: Player, allowed: impl Fn(BuildingKind) -> bool) -> Self {
        Self {
            rows: ORDER
                .iter()
                .map(|&kind| {
                    let kind = BuildingKind(kind);
                    ProductionRow {
                        kind,
                        productivity: world.productivity(player, kind),
                        group: group(world, kind),
                        locked: !allowed(kind),
                    }
                })
                .collect(),
        }
    }
}

/// The screen that shows how productive each building type is.
///
/// `text_colour` stands in until the window's style record is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductionWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
    /// The palette index the readings draw in.
    pub text_colour: u8,
}

impl ProductionWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 246;
    /// How tall the original's window is.
    pub const HEIGHT: u32 = 376;
    /// How many rows stand in a column.
    pub const COLUMN: usize = 10;
    /// Where the first column sits and how far right the second one starts.
    const COLUMN_AT: (i32, i32) = (15, 110);
    /// Where the top row sits and how far below it each row after it goes.
    const ROW_AT: (i32, i32) = (18, 32);
    /// The picture.
    const PICTURE: (u32, u32) = (30, 32);
    /// The bar: how far right and down from the picture, and its size.
    const BAR: (i32, i32, u32, u32) = (33, 12, 70, 12);
    /// How wide the cell that covers the picture and the bar is.
    const CELL: u32 = 103;
    /// The label: its x, how far up from the bottom, how much narrower than the window, and how
    /// tall.
    const LABEL: (i32, i32, u32, u32) = (12, 34, 24, 12);

    /// The window at `id`, drawing its readings in `text_colour`.
    #[must_use]
    pub const fn new(id: WindowId, text_colour: u8) -> Self {
        Self { id, text_colour }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where row `place` starts in a window whose top-left corner is `at`.
    fn corner(place: usize, at: Point) -> Option<Point> {
        if place >= ROWS {
            return None;
        }
        let column = i32::try_from(place / Self::COLUMN).unwrap_or_default();
        let row = i32::try_from(place % Self::COLUMN).unwrap_or_default();
        let (left, apart) = Self::COLUMN_AT;
        let (top, drop) = Self::ROW_AT;
        Some(Point::new(at.x + left + column * apart, at.y + top + row * drop))
    }

    /// Where row `place`'s picture sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn picture(place: usize, at: Point) -> Option<Rect> {
        Self::corner(place, at).map(|corner| Rect::new(corner.x, corner.y, Self::PICTURE.0, Self::PICTURE.1))
    }

    /// Where row `place`'s bar sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn bar(place: usize, at: Point) -> Option<Rect> {
        let (right, down, width, height) = Self::BAR;
        Self::corner(place, at).map(|corner| Rect::new(corner.x + right, corner.y + down, width, height))
    }

    /// Where row `place`'s cell sits in a window whose top-left corner is `at`, which covers the
    /// picture and the bar.
    #[must_use]
    pub fn cell(place: usize, at: Point) -> Option<Rect> {
        Self::corner(place, at).map(|corner| Rect::new(corner.x, corner.y, Self::CELL, Self::PICTURE.1))
    }

    /// Where the label sits in a window of `area`.
    #[must_use]
    pub fn label(area: Rect) -> Rect {
        let (left, up, narrower, height) = Self::LABEL;
        Rect::new(
            area.x + left,
            area.bottom() - up,
            area.width.saturating_sub(narrower),
            height,
        )
    }

    /// Draws the window into `painter`. Nothing on this screen takes a press.
    pub fn run(&self, ui: &Ui, painter: &mut Painter, skin: &Skin, view: &ProductionView) {
        let Some(&window) = ui.window(self.id) else {
            return;
        };
        let area = window.area;
        painter.push_clip(area);
        if window.rolled() {
            painter.pop_clip();
            return;
        }
        let at = Point::new(area.x, area.y);
        for (place, row) in view.rows.iter().enumerate() {
            if let Some(cell) = Self::picture(place, at) {
                painter.push_clip(cell);
                if !row.locked {
                    painter.building(row.group, cell.middle());
                } else if let Some(item) = skin.picture(Picture::LOCKED_TYPE) {
                    painter.item_centred(item, cell);
                }
                painter.pop_clip();
            }
            if row.locked {
                continue;
            }
            if let Some(place) = Self::bar(place, at) {
                bar::draw(painter, skin, place, u32::from(row.productivity), true);
            }
        }
        painter.pop_clip();
    }
}

/// The kind the screen opens as.
pub const KIND: WindowId = kind::PRODUCTION;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::{ItemId, OpKind};
    use crate::skin::{Control, Look};

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Tick, Look::Idle, ItemId(2));
        skin.set(Control::ScrollTrack, Look::Idle, ItemId(3));
        skin.set(Control::Title, Look::Idle, ItemId(4));
        skin
    }

    fn view() -> ProductionView {
        ProductionView {
            rows: ORDER
                .iter()
                .map(|&kind| ProductionRow {
                    kind: BuildingKind(kind),
                    productivity: kind * 3,
                    group: u16::from(kind),
                    locked: false,
                })
                .collect(),
        }
    }

    #[test]
    fn the_rows_fill_two_columns_of_ten() {
        let at = Point::new(0, 0);
        assert_eq!(ProductionWindow::picture(0, at), Some(Rect::new(15, 18, 30, 32)));
        assert_eq!(ProductionWindow::bar(0, at), Some(Rect::new(48, 30, 70, 12)));
        assert_eq!(ProductionWindow::cell(0, at), Some(Rect::new(15, 18, 103, 32)));
        assert_eq!(ProductionWindow::picture(9, at), Some(Rect::new(15, 306, 30, 32)));
        assert_eq!(
            ProductionWindow::picture(10, at),
            Some(Rect::new(125, 18, 30, 32))
        );
        assert_eq!(ProductionWindow::picture(ROWS, at), None);
        assert_eq!(ProductionWindow::bar(ROWS, at), None);
        let width = i32::try_from(ProductionWindow::WIDTH).expect("fits");
        let height = i32::try_from(ProductionWindow::HEIGHT).expect("fits");
        for place in 0..ROWS {
            let cell = ProductionWindow::cell(place, at).expect("a cell");
            assert!(cell.right() <= width, "{cell:?}");
            assert!(cell.bottom() <= height, "{cell:?}");
            let bar = ProductionWindow::bar(place, at).expect("a bar");
            assert_eq!(bar.right(), cell.right(), "the cell ends where the bar does");
        }
        let area = Rect::new(0, 0, ProductionWindow::WIDTH, ProductionWindow::HEIGHT);
        assert_eq!(ProductionWindow::label(area), Rect::new(12, 342, 222, 12));
    }

    #[test]
    fn the_rows_run_in_the_table_order() {
        let world = World::empty(32, 32);
        let view = ProductionView::read(&world, Player::new(0).expect("player 0"), |_| true);
        assert_eq!(view.rows.len(), ROWS);
        assert_eq!(view.rows[0].kind, BuildingKind::WOODCUTTER);
        assert_eq!(view.rows[ROWS - 1].kind, BuildingKind::GRANITE_MINE);
        assert!(view.rows.iter().all(|row| row.productivity == 0));
        assert!(
            !ORDER.contains(&BuildingKind::DONKEY_BREEDER.0),
            "the pass never stores the donkey breeder"
        );
        assert!(!ORDER.contains(&BuildingKind::IRON_SMELTER.0));
    }

    #[test]
    fn a_type_the_chapter_has_not_given_is_locked() {
        let world = World::empty(32, 32);
        let player = Player::new(0).expect("player 0");
        let view = ProductionView::read(&world, player, |kind| kind == BuildingKind::SAWMILL);
        for row in &view.rows {
            assert_eq!(row.locked, row.kind != BuildingKind::SAWMILL, "{:?}", row.kind);
        }
    }

    #[test]
    fn every_row_draws_a_bar_and_its_reading() {
        let mut ui = Ui::new();
        let (width, height) = ProductionWindow::size();
        ui.open(KIND, Rect::new(0, 0, width, height));
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let window = ProductionWindow::new(KIND, 1);
        window.run(&ui, &mut painter, &skin(), &view());
        let shares = painter
            .ops()
            .iter()
            .filter(|op| matches!(op.kind, OpKind::Fill { .. }))
            .count();
        assert_eq!(shares, ROWS, "one share per row");
        let readings = painter
            .ops()
            .iter()
            .filter(|op| matches!(op.kind, OpKind::Text { .. }))
            .count();
        assert_eq!(readings, ROWS);
    }

    #[test]
    fn a_rolled_up_window_draws_nothing() {
        let mut ui = Ui::new();
        let (width, height) = ProductionWindow::size();
        ui.open(KIND, Rect::new(0, 0, width, height));
        ui.roll(KIND);
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        ProductionWindow::new(KIND, 1).run(&ui, &mut painter, &skin(), &view());
        let found = painter.ops();
        assert!(found.is_empty(), "{found:?}");
    }
}
