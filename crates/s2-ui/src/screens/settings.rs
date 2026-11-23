//! Unlimited play's settings: a row for each player the map offers, the six settings under them,
//! and Start game and Return. The screen draws what a [`SettingsView`] holds and hands back what a
//! release on a control asks for; the caller keeps the roster and the settings.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::{Align, ItemId, Painter};
use crate::picture::Picture;
use crate::skin::Skin;
use crate::store::Ui;
use crate::text::{Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};

/// The settings screen's picture (`0x60381`, kind 2).
pub const PICTURE: &str = "GFX/PICS/SETUP015.LBM";

/// The items the In game? button shows for a player in the game and out of it.
pub const IN_GAME: &str = "io014";
/// See [`IN_GAME`].
pub const OUT_OF_GAME: &str = "io022";

/// One player's row as the screen shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowView {
    /// Whether the player is in the game.
    pub in_game: bool,
    /// Its face, 0 to 11.
    pub face: u8,
    /// Who plays it: 0 the first person, 1 the second, 2 the computer, 3 nobody any more.
    pub controller: u8,
}

/// What the screen shows.
#[derive(Debug, Clone, Copy)]
pub struct SettingsView<'a> {
    /// A row for each player the map offers, at most seven.
    pub rows: &'a [RowView],
    /// The map's name, as the Scenario button shows it between quotes.
    pub map: &'a [u8],
    /// The alliance setting, 0 to 2.
    pub alliances: u8,
    /// The game objective, 0 to 2.
    pub objective: u8,
    /// The merchandise in the headquarters, 0 to 2.
    pub goods: u8,
    /// Whether exploration is on.
    pub exploration: bool,
    /// Whether the players take the start positions in sequence rather than at random.
    pub in_sequence: bool,
    /// Whether the game can start, which decides whether Start game is there at all.
    pub can_start: bool,
}

/// What a release on the screen asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// Put the player of this row in the game or take it out.
    InGame(u8),
    /// The next face for the player of this row.
    Leader(u8),
    /// The next controller for the player of this row.
    Mode(u8),
    /// The map list.
    Scenario,
    /// The next alliance setting.
    Alliances,
    /// The next game objective.
    Objective,
    /// The next amount of merchandise.
    Goods,
    /// Exploration on or off.
    Exploration,
    /// Start positions in sequence or at random.
    StartPosition,
    /// Start the game.
    Start,
    /// Back to the main menu.
    Return,
}

/// The faces controls take: grey for the settings, green for Start game and red for Return.
const GREY: u16 = 0;
const RED: u16 = 1;
const GREEN: u16 = 3;

/// The style the title draws in.
const TITLE_STYLE: u16 = 5;

/// The title and the three headings over the rows.
const TITLE: (u16, (i32, i32, u32, u32)) = (348, (70, 20, 500, 20));
const HEADINGS: [(u16, (i32, i32, u32, u32)); 3] = [
    (350, (100, 70, 150, 20)),
    (351, (230, 70, 130, 20)),
    (352, (440, 70, 100, 20)),
];

/// The three columns of settings, each with a label above its button.
const COLUMNS: [i32; 3] = [40, 230, 420];
const SETTING: (u32, u32) = (170, 20);
const UPPER_LABEL: i32 = 320;
const LOWER_LABEL: i32 = 360;
const BELOW_LABEL: i32 = 20;

/// The labels of the six settings.
const SCENARIO: u16 = 357;
const ALLIANCES: u16 = 429;
const OBJECTIVE: u16 = 384;
const GOODS: u16 = 425;
const EXPLORATION: u16 = 423;
const START_POSITION: u16 = 424;

/// The first text of each setting's values.
const ALLIANCE_TEXTS: u16 = 430;
const OBJECTIVE_TEXTS: u16 = 385;
const GOODS_TEXTS: u16 = 426;
const ON: u16 = 418;
const OFF: u16 = 419;
const IN_SEQUENCE: u16 = 420;
const RANDOMLY: u16 = 421;

/// Start game and Return.
const FOOT: i32 = 414;
const START: u16 = 353;
const RETURN: u16 = 354;

/// The rows: the first at y 90, each 30 below the last, 20 tall.
const FIRST_ROW: i32 = 90;
const ROW_STEP: i32 = 30;
const ROW_HEIGHT: u32 = 20;

/// The parts of a row across: x and width.
const COLOUR: (i32, u32) = (30, 20);
const NAME: (i32, u32) = (50, 100);
const IN_GAME_BUTTON: (i32, u32) = (155, 40);
const LEADER: (i32, u32) = (230, 130);
const MODE: (i32, u32) = (440, 100);

/// The portrait: its x, 10 further on odd rows, its size, and how far above the row it starts.
const PORTRAIT_X: i32 = 375;
const PORTRAIT_STAGGER: i32 = 10;
const PORTRAIT: (u32, u32) = (36, 47);
const PORTRAIT_RISE: i32 = 14;

/// The name label, with the player's number in its `%ld`.
const PLAYER: u16 = 349;

/// The palette index of player 0's colour; each player's is 4 further on.
const FIRST_COLOUR: u8 = 129;
const COLOUR_STEP: u8 = 4;

/// The leaders' names and the controllers', in the names table.
const LEADERS: u16 = 250;
const CONTROLLERS: u16 = 170;

/// The rows that can hold a person and so have a Mode button.
const PEOPLE: u8 = 2;

/// The style the labels draw in.
const PLAIN: u16 = 0;

/// The settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsScreen {
    /// The id its controls are kept under.
    pub id: WindowId,
}

impl Default for SettingsScreen {
    fn default() -> Self {
        Self::new()
    }
}

/// What the screen draws its pictures with.
#[derive(Debug, Clone, Copy)]
pub struct Items {
    /// The In game? button's picture for a player in the game.
    pub in_game: Option<ItemId>,
    /// Its picture for a player out of it.
    pub out_of_game: Option<ItemId>,
}

impl SettingsScreen {
    /// The screen, its controls kept under [`crate::windows::kind::SETTINGS`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            id: crate::windows::kind::SETTINGS,
        }
    }

    /// Where row `row`'s control `part` (x and width) sits on a screen whose corner is `at`.
    #[must_use]
    pub fn row_part(row: u8, (x, width): (i32, u32), at: Point) -> Rect {
        Rect::new(
            at.x + x,
            at.y + FIRST_ROW + ROW_STEP * i32::from(row),
            width,
            ROW_HEIGHT,
        )
    }

    /// Where the In game? button of row `row` sits.
    #[must_use]
    pub fn in_game_button(row: u8, at: Point) -> Rect {
        Self::row_part(row, IN_GAME_BUTTON, at)
    }

    /// Where the Leader button of row `row` sits.
    #[must_use]
    pub fn leader_button(row: u8, at: Point) -> Rect {
        Self::row_part(row, LEADER, at)
    }

    /// Where the Mode button of row `row` sits.
    #[must_use]
    pub fn mode_button(row: u8, at: Point) -> Rect {
        Self::row_part(row, MODE, at)
    }

    /// Where the button of the setting in `column` (0 to 2), `lower` for the second of the
    /// column's two, sits.
    #[must_use]
    pub fn setting(column: usize, lower: bool, at: Point) -> Rect {
        let label = if lower { LOWER_LABEL } else { UPPER_LABEL };
        Rect::new(
            at.x + COLUMNS[column.min(COLUMNS.len() - 1)],
            at.y + label + BELOW_LABEL,
            SETTING.0,
            SETTING.1,
        )
    }

    /// Where Start game, or Return when `back`, sits.
    #[must_use]
    pub fn foot(back: bool, at: Point) -> Rect {
        let column = if back { COLUMNS[2] } else { COLUMNS[0] };
        Rect::new(at.x + column, at.y + FOOT, SETTING.0, SETTING.1)
    }

    /// Takes `mouse`, draws the screen's controls for `view` onto a screen whose corner is `at`,
    /// and gives back what a release on a control asks for.
    #[allow(clippy::too_many_arguments, reason = "a screen needs all of it")]
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        items: Items,
        view: &SettingsView<'_>,
        at: Point,
        mouse: Pointer,
    ) -> Option<Pick> {
        let mut drawer = Drawer {
            ui,
            painter,
            skin,
            id: self.id,
            widget: 0,
            mouse,
            picked: None,
        };
        drawer.label(TITLE.0, TITLE.1, at, TITLE_STYLE);
        for (text, area) in HEADINGS {
            drawer.label(text, area, at, PLAIN);
        }
        for (row, shown) in (0u8..).zip(view.rows) {
            drawer.row(row, *shown, items, at);
        }
        drawer.settings(view, at);
        if view.can_start {
            drawer.button(
                Self::foot(false, at),
                Some(Label::Slot(START)),
                GREEN,
                Pick::Start,
            );
        } else {
            drawer.widget += 1;
        }
        drawer.button(Self::foot(true, at), Some(Label::Slot(RETURN)), RED, Pick::Return);
        drawer.picked
    }
}

/// Draws the controls one after another, numbering their widgets in order.
struct Drawer<'a> {
    ui: &'a mut Ui,
    painter: &'a mut Painter,
    skin: &'a Skin,
    id: WindowId,
    widget: u16,
    mouse: Pointer,
    picked: Option<Pick>,
}

impl Drawer<'_> {
    /// Draws `text` in `style`, centred in `area` on a screen whose corner is `at`.
    fn label(&mut self, text: u16, (x, y, width, height): (i32, i32, u32, u32), at: Point, style: u16) {
        self.text(
            Label::Slot(text),
            Rect::new(at.x + x, at.y + y, width, height),
            style,
        );
    }

    /// Draws `label` in `style`, centred in `rect`.
    fn text(&mut self, label: Label, rect: Rect, style: u16) {
        if let Some(lettering) = self.painter.lettering(style) {
            self.painter.text_aligned(
                Point::new(rect.x, rect.y),
                lettering.font,
                lettering.colour,
                label,
                Pen::TIGHT,
                Align::CentredInBox(rect.width, rect.height),
            );
        }
    }

    /// Draws a button over `rect` in `style` showing `text`, which asks for `pick` when released
    /// on.
    fn button(&mut self, rect: Rect, text: Option<Label>, style: u16, pick: Pick) -> WidgetId {
        let widget = WidgetId::new(self.id, self.widget);
        self.widget += 1;
        let acted = self.ui.press(widget, rect, self.mouse, true);
        let look = self.ui.button(widget).look(true, false);
        if let Some(face) = self.skin.face(style, look) {
            self.painter
                .frame_edges(face, self.skin.edge(style), rect, look.edges());
        }
        if let Some(text) = text {
            self.text(text, rect, style);
        }
        if acted && self.picked.is_none() {
            self.picked = Some(pick);
        }
        widget
    }

    /// Draws row `row` for `shown`.
    fn row(&mut self, row: u8, shown: RowView, items: Items, at: Point) {
        let colour = FIRST_COLOUR.saturating_add(COLOUR_STEP.saturating_mul(row));
        self.painter
            .fill(SettingsScreen::row_part(row, COLOUR, at), colour);
        let name = Label::Numbered(PLAYER, i32::from(row) + 1);
        self.text(name, SettingsScreen::row_part(row, NAME, at), PLAIN);
        let tick = SettingsScreen::in_game_button(row, at);
        self.button(tick, None, GREY, Pick::InGame(row));
        let picture = if shown.in_game {
            items.in_game
        } else {
            items.out_of_game
        };
        if let Some(item) = picture {
            self.painter.item_centred(item, tick);
        }
        if !shown.in_game {
            self.widget += 2;
            return;
        }
        let leader = Label::Named(LEADERS + u16::from(shown.face));
        self.button(
            SettingsScreen::leader_button(row, at),
            Some(leader),
            GREY,
            Pick::Leader(row),
        );
        let row_y = SettingsScreen::row_part(row, COLOUR, at).y;
        let stagger = if row % 2 == 1 { PORTRAIT_STAGGER } else { 0 };
        let portrait = Rect::new(
            at.x + PORTRAIT_X + stagger,
            row_y - PORTRAIT_RISE,
            PORTRAIT.0,
            PORTRAIT.1,
        );
        if let Some(item) = self.skin.picture(Picture::portrait(u16::from(shown.face))) {
            self.painter.item_centred(item, portrait);
        }
        let mode = Label::Named(CONTROLLERS + u16::from(shown.controller));
        let place = SettingsScreen::mode_button(row, at);
        if row < PEOPLE {
            self.button(place, Some(mode), GREY, Pick::Mode(row));
        } else {
            self.widget += 1;
            self.text(mode, place, PLAIN);
        }
    }

    /// Draws the six settings with their labels.
    fn settings(&mut self, view: &SettingsView<'_>, at: Point) {
        let map = {
            let mut quoted = Vec::with_capacity(view.map.len() + 2);
            quoted.push(b'\'');
            quoted.extend_from_slice(view.map);
            quoted.push(b'\'');
            self.painter.run(&quoted)
        };
        let settings = [
            (0, false, SCENARIO, map, Pick::Scenario),
            (
                0,
                true,
                ALLIANCES,
                Label::Slot(ALLIANCE_TEXTS + u16::from(view.alliances)),
                Pick::Alliances,
            ),
            (
                1,
                false,
                OBJECTIVE,
                Label::Slot(OBJECTIVE_TEXTS + u16::from(view.objective)),
                Pick::Objective,
            ),
            (
                1,
                true,
                GOODS,
                Label::Slot(GOODS_TEXTS + u16::from(view.goods)),
                Pick::Goods,
            ),
            (
                2,
                false,
                EXPLORATION,
                Label::Slot(if view.exploration { ON } else { OFF }),
                Pick::Exploration,
            ),
            (
                2,
                true,
                START_POSITION,
                Label::Slot(if view.in_sequence { IN_SEQUENCE } else { RANDOMLY }),
                Pick::StartPosition,
            ),
        ];
        for (column, lower, label, value, pick) in settings {
            let button = SettingsScreen::setting(column, lower, at);
            let above = Rect::new(button.x, button.y - BELOW_LABEL, button.width, button.height);
            self.text(Label::Slot(label), above, PLAIN);
            self.button(button, Some(value), GREY, pick);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(rows: &[RowView], can_start: bool) -> SettingsView<'_> {
        SettingsView {
            rows,
            map: b"Island of Hills",
            alliances: 0,
            objective: 0,
            goods: 1,
            exploration: true,
            in_sequence: true,
            can_start,
        }
    }

    fn click(screen: &mut SettingsScreen, ui: &mut Ui, view: &SettingsView<'_>, rect: Rect) -> Option<Pick> {
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let items = Items {
            in_game: None,
            out_of_game: None,
        };
        let on = Pointer::at(Point::new(rect.x + 4, rect.y + 4));
        let skin = Skin::new();
        screen.run(
            ui,
            &mut painter,
            &skin,
            items,
            view,
            Point::new(0, 0),
            on.pressing(),
        );
        painter.reset();
        screen.run(
            ui,
            &mut painter,
            &skin,
            items,
            view,
            Point::new(0, 0),
            on.releasing(),
        )
    }

    #[test]
    fn the_controls_sit_where_the_original_builds_them() {
        let at = Point::new(0, 0);
        assert_eq!(SettingsScreen::in_game_button(0, at), Rect::new(155, 90, 40, 20));
        assert_eq!(SettingsScreen::leader_button(3, at), Rect::new(230, 180, 130, 20));
        assert_eq!(SettingsScreen::setting(1, true, at), Rect::new(230, 380, 170, 20));
        assert_eq!(SettingsScreen::foot(true, at), Rect::new(420, 414, 170, 20));
    }

    #[test]
    fn a_release_asks_for_its_control() {
        let rows = [
            RowView {
                in_game: true,
                face: 0,
                controller: 0,
            },
            RowView {
                in_game: false,
                face: 4,
                controller: 2,
            },
        ];
        let shown = view(&rows, true);
        let (mut screen, mut ui) = (SettingsScreen::new(), Ui::new());
        let at = Point::new(0, 0);
        for (rect, pick) in [
            (SettingsScreen::in_game_button(1, at), Pick::InGame(1)),
            (SettingsScreen::leader_button(0, at), Pick::Leader(0)),
            (SettingsScreen::mode_button(0, at), Pick::Mode(0)),
            (SettingsScreen::setting(0, false, at), Pick::Scenario),
            (SettingsScreen::setting(2, true, at), Pick::StartPosition),
            (SettingsScreen::foot(false, at), Pick::Start),
            (SettingsScreen::foot(true, at), Pick::Return),
        ] {
            assert_eq!(click(&mut screen, &mut ui, &shown, rect), Some(pick), "{pick:?}");
        }
        assert_eq!(
            click(&mut screen, &mut ui, &shown, SettingsScreen::leader_button(1, at)),
            None,
            "a player out of the game has no leader button"
        );
    }

    #[test]
    fn there_is_no_start_game_when_the_game_cannot_start() {
        let rows = [RowView {
            in_game: true,
            face: 0,
            controller: 2,
        }];
        let shown = view(&rows, false);
        let (mut screen, mut ui) = (SettingsScreen::new(), Ui::new());
        let start = SettingsScreen::foot(false, Point::new(0, 0));
        assert_eq!(click(&mut screen, &mut ui, &shown, start), None);
    }
}
