//! The buildings a player owns: one cell per type with its finished buildings and its sites.
//!
//! A type a campaign chapter has not given the player keeps its cell and its counts and shows
//! `io170` in place of its picture.

use s2_render::terrain::Point;
use s2_sim::{BuildingKind, World};
use s2_systems::player::Player;

use crate::geometry::Rect;
use crate::paint::{Align, Painter};
use crate::picture::Picture;
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::text::{FontId, Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{BUILDING_NAMES, Outcome, kind};

/// How many cells the screen has, one per building type but the headquarters.
pub const CELLS: usize = 31;

/// The help page the window's own button opens, which command 10944 hands to `0x452f8`.
pub const HELP_PAGE: u16 = 1008;

/// The building type each cell shows. `S2.EXE` holds the same run backwards at `0x1fd498` and
/// reads it from the end.
pub const ORDER: [u8; CELLS] = [
    28, 31, 27, 26, 25, 29, 23, 22, 21, 20, 19, 18, 16, 15, 14, 13, 12, 8, 17, 7, 9, 6, 10, 11, 30, 4, 3, 5,
    2, 1, 0,
];

/// The graphics group of `kind`, which is the item its picture comes from.
fn group(world: &World, kind: BuildingKind) -> u16 {
    world.building_types().get(kind).map_or(0, |kind| kind.group)
}

/// What one cell shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildingsRow {
    /// The type the cell stands for.
    pub kind: BuildingKind,
    /// How many of them are finished.
    pub finished: u16,
    /// How many are under construction.
    pub sites: u16,
    /// Its graphics group, which is the item its picture comes from in the nation's own
    /// archive.
    pub group: u16,
    /// Whether the chapter has not given the player the type yet.
    pub locked: bool,
}

/// What the buildings screen shows, read without a renderer so a test can assert it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingsView {
    /// One row per cell, in cell order.
    pub rows: Vec<BuildingsRow>,
}

impl BuildingsView {
    /// What `player` holds. `allowed` says whether the player may build a type.
    #[must_use]
    pub fn read(world: &World, player: Player, allowed: impl Fn(BuildingKind) -> bool) -> Self {
        Self {
            rows: ORDER
                .iter()
                .map(|&kind| {
                    let kind = BuildingKind(kind);
                    BuildingsRow {
                        kind,
                        finished: world.finished_buildings(player, kind),
                        sites: world.building_sites(player, kind),
                        group: group(world, kind),
                        locked: !allowed(kind),
                    }
                })
                .collect(),
        }
    }
}

/// The buildings screen.
///
/// `text_colour` stands in until the window's style record is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildingsWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
    /// The palette index the counts draw in.
    pub text_colour: u8,
}

impl BuildingsWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 187;
    /// How tall the original's window is.
    pub const HEIGHT: u32 = 370;
    /// How many cells stand side by side.
    pub const COLUMNS: usize = 4;
    /// How many cells stand in a column.
    pub const ROWS: usize = 8;
    /// How big a cell is.
    const CELL: u32 = 32;
    /// Where the rightmost column's cells sit, and how far left each column after it goes.
    const COLUMN: (i32, i32) = (138, 40);
    /// Where the top cell sits and how far below it each row after it goes.
    const ROW: (i32, i32) = (20, 39);
    /// The help button: how far in from the left, how far up from the bottom, and its size.
    const HELP: (i32, i32, u32, u32) = (18, 68, 30, 32);

    /// The window at `id`, drawing its counts in `text_colour`.
    #[must_use]
    pub const fn new(id: WindowId, text_colour: u8) -> Self {
        Self { id, text_colour }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where cell `place` sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn cell(place: usize, at: Point) -> Option<Rect> {
        if place >= CELLS {
            return None;
        }
        let column = i32::try_from(place / Self::ROWS).unwrap_or_default();
        let row = i32::try_from(place % Self::ROWS).unwrap_or_default();
        let (right, step) = Self::COLUMN;
        let (top, drop) = Self::ROW;
        Some(Rect::new(
            at.x + right - column * step,
            at.y + top + row * drop,
            Self::CELL,
            Self::CELL,
        ))
    }

    /// Where the button that opens the help page sits in a window of `area`.
    #[must_use]
    pub fn help(area: Rect) -> Rect {
        let (left, up, width, height) = Self::HELP;
        Rect::new(area.x + left, area.bottom() - up, width, height)
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release does.
    pub fn run(
        &self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &BuildingsView,
        mouse: Pointer,
    ) -> Option<Outcome> {
        let window = *ui.window(self.id)?;
        let area = window.area;
        painter.push_clip(area);
        if window.rolled() {
            painter.pop_clip();
            return None;
        }
        let at = Point::new(area.x, area.y);
        for (place, row) in view.rows.iter().enumerate() {
            let Some(cell) = Self::cell(place, at) else {
                continue;
            };
            if row.locked {
                if let Some(item) = skin.picture(Picture::LOCKED_TYPE) {
                    painter.item_centred(item, cell);
                }
            } else {
                ui.note_help(cell, mouse, Label::Named(BUILDING_NAMES + u16::from(row.kind.0)));
                painter.building(row.group, cell.middle());
            }
            painter.text_aligned(
                Point::new(cell.middle().x, cell.bottom()),
                FontId::Small,
                self.text_colour,
                Label::Pair(i32::from(row.finished), i32::from(row.sites)),
                Pen::TIGHT,
                Align::Centre,
            );
        }
        let help = Self::help(area);
        let widget = WidgetId::new(self.id, Self::HELP_SLOT);
        ui.note_help(help, mouse, Label::Slot(crate::windows::HELP_BUTTON));
        let acted = ui.press(widget, help, mouse, true);
        let look = ui.button(widget).look(true, false);
        if let Some(item) = skin.item(Control::Button, look) {
            painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), help, look.edges());
        }
        if let Some(item) = skin.picture(Picture::HELP) {
            painter.item_centred(item, help);
        }
        painter.pop_clip();
        acted.then_some(Outcome::Help(HELP_PAGE))
    }

    /// The widget slot the help button takes.
    const HELP_SLOT: u16 = 0;
}

/// The kind the buildings screen opens as.
pub const KIND: WindowId = kind::BUILDINGS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::{ItemId, OpKind};

    const TEXT: u8 = 1;

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Button, Look::Idle, ItemId(2));
        skin.set(Control::Tick, Look::Idle, ItemId(3));
        skin.set(Control::Title, Look::Idle, ItemId(4));
        skin
    }

    fn open() -> (BuildingsWindow, Ui, Painter) {
        let mut ui = Ui::new();
        let (width, height) = BuildingsWindow::size();
        ui.open(KIND, Rect::new(0, 0, width, height));
        (
            BuildingsWindow::new(KIND, TEXT),
            ui,
            Painter::new(Rect::new(0, 0, 640, 480)),
        )
    }

    fn view() -> BuildingsView {
        BuildingsView {
            rows: ORDER
                .iter()
                .map(|&kind| BuildingsRow {
                    kind: BuildingKind(kind),
                    finished: u16::from(kind),
                    sites: 1,
                    group: u16::from(kind),
                    locked: false,
                })
                .collect(),
        }
    }

    #[test]
    fn the_cells_fill_four_columns_from_the_right() {
        let at = Point::new(0, 0);
        assert_eq!(BuildingsWindow::cell(0, at), Some(Rect::new(138, 20, 32, 32)));
        assert_eq!(BuildingsWindow::cell(7, at), Some(Rect::new(138, 293, 32, 32)));
        assert_eq!(BuildingsWindow::cell(8, at), Some(Rect::new(98, 20, 32, 32)));
        assert_eq!(BuildingsWindow::cell(30, at), Some(Rect::new(18, 254, 32, 32)));
        assert_eq!(BuildingsWindow::cell(CELLS, at), None);
        let width = i32::try_from(BuildingsWindow::WIDTH).expect("fits");
        let height = i32::try_from(BuildingsWindow::HEIGHT).expect("fits");
        for place in 0..CELLS {
            let cell = BuildingsWindow::cell(place, at).expect("a cell");
            assert!(cell.x >= 0 && cell.right() <= width, "{cell:?}");
            assert!(cell.y >= 0 && cell.bottom() <= height, "{cell:?}");
        }
        let area = Rect::new(0, 0, BuildingsWindow::WIDTH, BuildingsWindow::HEIGHT);
        assert_eq!(BuildingsWindow::help(area), Rect::new(18, 302, 30, 32));
    }

    #[test]
    fn every_type_but_the_headquarters_has_one_cell() {
        let mut kinds = ORDER;
        kinds.sort_unstable();
        let mut expected: Vec<u8> = (0..32)
            .filter(|&kind| kind != BuildingKind::HEADQUARTERS.0)
            .collect();
        expected.sort_unstable();
        assert_eq!(kinds.to_vec(), expected);
        assert_eq!(ORDER[0], BuildingKind::FORTRESS.0);
        assert_eq!(ORDER[CELLS - 1], 0);
    }

    #[test]
    fn a_world_with_one_headquarters_counts_it_nowhere_on_this_screen() {
        let world = World::empty(32, 32);
        let player = Player::new(0).expect("player 0");
        let view = BuildingsView::read(&world, player, |_| true);
        assert_eq!(view.rows.len(), CELLS);
        assert!(view.rows.iter().all(|row| row.finished == 0 && row.sites == 0));
        assert!(view.rows.iter().all(|row| row.kind != BuildingKind::HEADQUARTERS));
    }

    #[test]
    fn a_type_the_chapter_has_not_given_is_locked_and_keeps_its_counts() {
        let world = World::empty(32, 32);
        let player = Player::new(0).expect("player 0");
        let view = BuildingsView::read(&world, player, |kind| kind == BuildingKind::SAWMILL);
        for row in &view.rows {
            assert_eq!(row.locked, row.kind != BuildingKind::SAWMILL, "{:?}", row.kind);
        }
        let (window, mut ui, mut painter) = open();
        window.run(
            &mut ui,
            &mut painter,
            &skin(),
            &view,
            Pointer::at(Point::new(-1, -1)),
        );
        let (mut counts, mut pictures) = (0, 0);
        for op in painter.ops() {
            match op.kind {
                OpKind::Text { .. } => counts += 1,
                OpKind::Game { .. } => pictures += 1,
                _ => {}
            }
        }
        assert_eq!(counts, CELLS);
        assert_eq!(pictures, 1, "only the sawmill draws its own picture");
    }

    #[test]
    fn each_cell_draws_its_two_counts() {
        let (window, mut ui, mut painter) = open();
        window.run(
            &mut ui,
            &mut painter,
            &skin(),
            &view(),
            Pointer::at(Point::new(-1, -1)),
        );
        let counts = painter
            .ops()
            .iter()
            .filter(|op| matches!(op.kind, OpKind::Text { .. }))
            .count();
        assert_eq!(counts, CELLS);
    }

    #[test]
    fn the_help_button_opens_the_page() {
        let (window, mut ui, mut painter) = open();
        let area = Rect::new(0, 0, BuildingsWindow::WIDTH, BuildingsWindow::HEIGHT);
        let place = BuildingsWindow::help(area);
        let on = Pointer::at(Point::new(place.x + 1, place.y + 1));
        let view = view();
        window.run(&mut ui, &mut painter, &skin(), &view, on.pressing());
        painter.reset();
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), &view, on.releasing()),
            Some(Outcome::Help(HELP_PAGE))
        );
    }
}
