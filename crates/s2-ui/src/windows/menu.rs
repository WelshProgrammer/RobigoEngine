//! The game menu.
//!
//! `F12` opens it. The settings it shows live outside the sim, so the window takes them as a
//! [`MenuView`] and hands back what the player asked to change.
//!
//! Robigo adds one control S2 does not have, the button that opens its own options, where S2's
//! button that leaves the game sits; that button moves down by its height and the window grows
//! by as much. Everything else sits where S2 puts it.

use s2_render::terrain::Point;

use crate::bar;
use crate::geometry::Rect;
use crate::paint::{Align, Painter};
use crate::picture::Picture;
use crate::skin::Skin;
use crate::store::Ui;
use crate::style::Lettering;
use crate::text::{Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{Outcome, kind};

/// The loudest a volume goes, which the original keeps as 0 to 127 and shows as a percentage.
pub const LOUDEST: u32 = 127;

/// How far one press of an arrow moves a volume (`0x4b6de`, `0x4b6ee`), so sixteen presses go
/// from full to silent.
pub const STEP: i32 = 8;

/// The bar's own scale, in percent.
const FULL: u32 = 100;

/// Where the title card sits.
const CARD: (i32, i32, u32, u32) = (12, 30, 222, 16);

/// The ten labels, as the 16-byte records at `0x1fbf24` hold them: where one sits, the text slot
/// it shows and the style it draws in. The first four are the title card's lines, in order: the
/// game's name, its edition, the copyright line and the build date.
const LABELS: [(i32, i32, u32, u32, u16, u16); 10] = [
    (12, 46, 222, 16, 0, 0),
    (12, 58, 222, 16, 1, 0),
    (12, 70, 222, 16, 2, 0),
    (12, 82, 222, 16, 3, 4),
    (55, 108, 162, 36, 182, 0),
    (55, 148, 162, 36, 251, 0),
    (55, 192, 134, 36, 129, 0),
    (55, 232, 134, 36, 128, 0),
    (17, 330, 120, 20, 223, 0),
    (17, 352, 120, 20, 221, 0),
];

/// How many of the labels belong to the title card.
const CARD_LINES: usize = 4;

/// Where the two volume bars sit and how big one is.
const BARS: [(i32, i32); 2] = [(157, 330), (157, 352)];

/// How big a volume bar is.
const BAR: (u32, u32) = (50, 20);

/// The style every control but the one that leaves the game draws in.
const PLAIN: u16 = 0;

/// The style that one draws in, which is the red of the original's table.
const LEAVE_STYLE: u16 = 1;

/// How tall Robigo's options button is, which the button that leaves the game moves down by.
const OPTIONS_HEIGHT: u32 = 24;

/// The text slot that says a control has no line of help of its own.
const NO_HELP: u16 = 200;

/// What the music switch shows when the music is on, and when it is off.
const MUSIC_PICTURES: (Picture, Picture) = (Picture::MUSIC_ON, Picture::MUSIC_OFF);

/// What the effects switch shows when effects are on, and when they are off.
const EFFECT_PICTURES: (Picture, Picture) = (Picture::EFFECTS_ON, Picture::EFFECTS_OFF);

/// What a press on the menu asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// Show the keyboard layout.
    Keys,
    /// Show the shipped notes.
    Readme,
    /// Open the screen that loads a game.
    Load,
    /// Open the screen that saves one.
    Save,
    /// Turn the music on or off.
    Music,
    /// Turn the effects on or off.
    Effects,
    /// Open the MIDI player.
    Midi,
    /// Open the CD player.
    Cd,
    /// Step the music volume by this much.
    MusicVolume(i32),
    /// Step the effect volume by this much.
    EffectVolume(i32),
    /// Leave the game.
    Leave,
    /// Open Robigo's own options.
    Options,
}

/// One control of the menu, as the original's two tables and five records lay them out.
struct Button {
    rect: Rect,
    picture: Option<Picture>,
    text: Option<Label>,
    style: u16,
    /// What the line of help shows while the pointer is over it. Slot 200 says there is none,
    /// which both arrow pairs carry.
    help: Label,
    pick: Pick,
}

/// What the menu shows, which comes from the settings rather than the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuView {
    /// Whether the music is on (`0x1f7909`).
    pub music: bool,
    /// Whether the effects are on (`0x7b36b`).
    pub effects: bool,
    /// How loud the music is, 0 to [`LOUDEST`] (`0x7b27a`).
    pub music_volume: u32,
    /// How loud the effects are, 0 to [`LOUDEST`] (`0x7b319`).
    pub effect_volume: u32,
}

impl MenuView {
    /// A volume as the bar shows it, in percent.
    #[must_use]
    pub const fn percent(volume: u32) -> u32 {
        volume * FULL / LOUDEST
    }

    /// The item the music switch shows.
    #[must_use]
    pub const fn music_picture(self) -> Picture {
        if self.music {
            MUSIC_PICTURES.0
        } else {
            MUSIC_PICTURES.1
        }
    }

    /// The item the effects switch shows.
    #[must_use]
    pub const fn effect_picture(self) -> Picture {
        if self.effects {
            EFFECT_PICTURES.0
        } else {
            EFFECT_PICTURES.1
        }
    }
}

/// The game menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
}

impl MenuWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 244;
    /// How tall it is: S2's 434, and the height of the options button.
    pub const HEIGHT: u32 = 434 + OPTIONS_HEIGHT;
    /// The `IO.DAT` item the title card draws (`0x4b265`).
    pub const TITLE_CARD: Picture = Picture::MENU_TITLE;

    /// The menu at its own kind.
    #[must_use]
    pub const fn new() -> Self {
        Self { id: kind::MENU }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where the title card sits.
    #[must_use]
    pub fn card(at: Point) -> Rect {
        let (x, y, width, height) = CARD;
        Rect::new(at.x + x, at.y + y, width, height)
    }

    /// Where line `line` of the title card sits.
    #[must_use]
    pub fn line(line: usize, at: Point) -> Rect {
        let (x, y, width, height, _, _) = LABELS[line.min(CARD_LINES - 1)];
        Rect::new(at.x + x, at.y + y, width, height)
    }

    /// Where volume bar `bar` sits, 0 for the music and 1 for the effects.
    #[must_use]
    pub fn bar(bar: usize, at: Point) -> Rect {
        let (x, y) = BARS[bar.min(BARS.len() - 1)];
        let (width, height) = BAR;
        Rect::new(at.x + x, at.y + y, width, height)
    }

    /// Every control the menu builds, in the order the original builds them.
    fn buttons(at: Point) -> Vec<Button> {
        let icon = |x: i32, y: i32, w: u32, h: u32, picture: Picture, help: u16, pick: Pick| Button {
            rect: Rect::new(at.x + x, at.y + y, w, h),
            picture: Some(picture),
            text: None,
            style: PLAIN,
            help: Label::Slot(help),
            pick,
        };
        let worded = |x: i32, y: i32, w: u32, h: u32, text: u16, style: u16, help: u16, pick| Button {
            rect: Rect::new(at.x + x, at.y + y, w, h),
            picture: None,
            text: Some(Label::Slot(text)),
            style,
            help: Label::Slot(help),
            pick,
        };
        vec![
            icon(17, 108, 36, 36, Picture::OPEN_KEYS, 182, Pick::Keys),
            icon(17, 148, 36, 36, Picture::OPEN_README, 251, Pick::Readme),
            icon(17, 192, 36, 36, Picture::OPEN_LOAD_GAME, 129, Pick::Load),
            icon(17, 232, 36, 36, Picture::OPEN_SAVE_GAME, 128, Pick::Save),
            icon(
                137,
                330,
                20,
                20,
                Picture::STEP_DOWN,
                NO_HELP,
                Pick::MusicVolume(-STEP),
            ),
            icon(
                207,
                330,
                20,
                20,
                Picture::STEP_UP,
                NO_HELP,
                Pick::MusicVolume(STEP),
            ),
            icon(
                137,
                352,
                20,
                20,
                Picture::STEP_DOWN,
                NO_HELP,
                Pick::EffectVolume(-STEP),
            ),
            icon(
                207,
                352,
                20,
                20,
                Picture::STEP_UP,
                NO_HELP,
                Pick::EffectVolume(STEP),
            ),
            icon(17, 280, 36, 36, Picture::EFFECTS_ON, 220, Pick::Effects),
            icon(58, 280, 36, 36, Picture::MUSIC_ON, 222, Pick::Music),
            worded(99, 280, 128, 18, 188, PLAIN, 188, Pick::Midi),
            worded(99, 298, 128, 18, 181, PLAIN, 181, Pick::Cd),
            Button {
                rect: Rect::new(at.x + 39, at.y + 385, 168, OPTIONS_HEIGHT - 4),
                picture: None,
                text: Some(Label::Text("Options")),
                style: PLAIN,
                help: Label::Text("Settings of Robigo's own"),
                pick: Pick::Options,
            },
            worded(
                39,
                385 + i32::try_from(OPTIONS_HEIGHT).unwrap_or_default(),
                168,
                24,
                125,
                LEAVE_STYLE,
                211,
                Pick::Leave,
            ),
        ]
    }

    /// The picture a control shows, which the two switches take from the settings (`0x4b45d`).
    fn shows(held: &Button, view: &MenuView) -> Option<Picture> {
        match held.pick {
            Pick::Music => Some(view.music_picture()),
            Pick::Effects => Some(view.effect_picture()),
            _ => held.picture,
        }
    }

    /// Draws one line of text centred across and down `rect`.
    fn line_of(painter: &mut Painter, rect: Rect, style: Lettering, text: Label) {
        painter.text_aligned(
            Point::new(rect.x, rect.y),
            style.font,
            style.colour,
            text,
            Pen::TIGHT,
            Align::CentredInBox(rect.width, rect.height),
        );
    }

    /// Draws one volume bar, which shows its reading as the share of its width that takes the
    /// lit face (`0x50f7c`).
    ///
    /// The bar carries the shaded pair of edges, the mirror of the pair a button at rest shows,
    /// so its top and left rows are the black the frame leaves behind.
    fn volume(painter: &mut Painter, skin: &Skin, rect: Rect, percent: u32) {
        bar::draw(painter, skin, rect, percent, false);
    }

    /// Takes `mouse`, draws the menu into `painter` and gives back what a release asks for.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &MenuView,
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
        if let Some(item) = skin.picture(Self::TITLE_CARD) {
            // The card is 16 pixels tall and the item is 26, and the widget shows only what its
            // own rectangle covers, which cuts the plume and the feet off.
            let card = Self::card(at);
            painter.push_clip(card);
            painter.item_centred(item, card);
            painter.pop_clip();
        }
        for &(x, y, width, height, slot, index) in &LABELS {
            if let Some(style) = painter.lettering(index) {
                let rect = Rect::new(at.x + x, at.y + y, width, height);
                Self::line_of(painter, rect, style, Label::Slot(slot));
            }
        }
        let mut asked = None;
        for (place, held) in Self::buttons(at).into_iter().enumerate() {
            let widget = WidgetId::new(self.id, u16::try_from(place).unwrap_or_default());
            ui.note_help(held.rect, mouse, held.help);
            let acted = ui.press(widget, held.rect, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.face(held.style, look) {
                painter.frame_edges(item, skin.edge(held.style), held.rect, look.edges());
            }
            if let Some(item) = Self::shows(&held, view).and_then(|show| skin.picture(show)) {
                painter.item_centred(item, held.rect);
            }
            if let (Some(text), Some(style)) = (held.text, painter.lettering(held.style)) {
                Self::line_of(painter, held.rect, style, text);
            }
            if acted && asked.is_none() {
                asked = Some(held.pick);
            }
        }
        for (bar, volume) in [view.music_volume, view.effect_volume].into_iter().enumerate() {
            Self::volume(painter, skin, Self::bar(bar, at), MenuView::percent(volume));
        }
        painter.pop_clip();
        asked.map(Outcome::Menu)
    }
}

impl Default for MenuWindow {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ItemId;
    use crate::skin::Look;

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set_face(PLAIN, Look::Idle, ItemId(1));
        skin.set_face(LEAVE_STYLE, Look::Idle, ItemId(2));
        skin
    }

    fn open() -> (Ui, MenuWindow, Painter) {
        let (width, height) = MenuWindow::size();
        let mut ui = Ui::new();
        ui.open(kind::MENU, Rect::new(0, 0, width, height));
        (ui, MenuWindow::new(), Painter::new(Rect::new(0, 0, 640, 480)))
    }

    fn click(place: Rect) -> Pointer {
        Pointer::at(Point::new(place.x + 1, place.y + 1))
    }

    fn pressed(pick: Pick) -> Rect {
        MenuWindow::buttons(Point::new(0, 0))
            .into_iter()
            .find(|button| button.pick == pick)
            .expect("the menu has that button")
            .rect
    }

    #[test]
    fn the_title_card_and_the_bars_sit_where_the_original_puts_them() {
        assert_eq!(
            MenuWindow::size(),
            (244, 458),
            "S2's 434 and the options button's 24"
        );
        let at = Point::new(0, 0);
        assert_eq!(MenuWindow::card(at), Rect::new(12, 30, 222, 16));
        assert_eq!(MenuWindow::line(0, at), Rect::new(12, 46, 222, 16));
        assert_eq!(MenuWindow::line(3, at), Rect::new(12, 82, 222, 16));
        assert_eq!(MenuWindow::bar(0, at), Rect::new(157, 330, 50, 20));
        assert_eq!(MenuWindow::bar(1, at), Rect::new(157, 352, 50, 20));
        assert_eq!(pressed(Pick::Options), Rect::new(39, 385, 168, 20));
        assert_eq!(pressed(Pick::Leave), Rect::new(39, 409, 168, 24));
    }

    #[test]
    fn every_label_sits_where_the_original_table_puts_it() {
        assert_eq!(LABELS.len(), 10);
        let at = Point::new(0, 0);
        for line in 0..CARD_LINES {
            assert_eq!(MenuWindow::line(line, at).width, 222);
        }
        assert_eq!(
            LABELS[3].5, 4,
            "the build date takes the style with the grey text"
        );
        let picks: Vec<Pick> = MenuWindow::buttons(at).into_iter().map(|b| b.pick).collect();
        assert_eq!(picks.len(), 14);
        let leave = MenuWindow::buttons(at)
            .into_iter()
            .find(|b| b.pick == Pick::Leave)
            .expect("the menu has that button");
        assert_eq!(
            leave.style, LEAVE_STYLE,
            "only the button that leaves takes the red style"
        );
        assert_eq!(leave.text, Some(Label::Slot(125)));
        assert!(leave.picture.is_none());
    }

    #[test]
    fn a_volume_shows_as_a_percentage_of_a_hundred_and_twenty_seven() {
        assert_eq!(MenuView::percent(0), 0);
        assert_eq!(MenuView::percent(127), 100);
        assert_eq!(MenuView::percent(64), 50);
    }

    #[test]
    fn the_switches_show_the_item_their_setting_names() {
        let on = MenuView {
            music: true,
            effects: true,
            ..MenuView::default()
        };
        assert_eq!(on.music_picture(), Picture::MUSIC_ON);
        assert_eq!(on.effect_picture(), Picture::EFFECTS_ON);
        let off = MenuView::default();
        assert_eq!(off.music_picture(), Picture::MUSIC_OFF);
        assert_eq!(off.effect_picture(), Picture::EFFECTS_OFF);
    }

    #[test]
    fn each_button_asks_for_its_own_pick() {
        for pick in [
            Pick::Keys,
            Pick::Readme,
            Pick::Load,
            Pick::Save,
            Pick::Music,
            Pick::Effects,
            Pick::Midi,
            Pick::Cd,
            Pick::Options,
            Pick::MusicVolume(STEP),
            Pick::MusicVolume(-STEP),
            Pick::EffectVolume(STEP),
            Pick::EffectVolume(-STEP),
            Pick::Leave,
        ] {
            let (mut ui, mut window, mut painter) = open();
            let view = MenuView::default();
            let on = click(pressed(pick));
            window.run(&mut ui, &mut painter, &skin(), &view, on.pressing());
            painter.reset();
            assert_eq!(
                window.run(&mut ui, &mut painter, &skin(), &view, on.releasing()),
                Some(Outcome::Menu(pick)),
                "{pick:?}"
            );
        }
    }
}
