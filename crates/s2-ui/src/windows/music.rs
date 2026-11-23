//! The CD player and the MIDI player.
//!
//! Both screens lay their buttons out the same way, so the grid and the wide controls along the
//! bottom live here once. Neither window keeps any state: what a release does goes back to the
//! caller, which owns the music.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::{Align, Painter};
use crate::skin::Skin;
use crate::store::Ui;
use crate::text::{Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{Outcome, kind};

/// The window style a track button draws in, which both screens build with `0x4df2c`.
const TRACK_STYLE: u16 = 0;

/// The one the mode buttons and the button that stops playing draw in, which both screens build
/// with `0x4dbdc` (`0x4cad6`, `0x4ce7b`).
const NAMED_STYLE: u16 = 3;

/// How many tracks the CD player looks for.
pub const MOST_TRACKS: usize = 50;

/// How many songs the MIDI player offers.
pub const SONGS: usize = 14;

/// The text slot the button that stops playing shows.
const STOP_SLOT: u16 = 314;

/// The text slot the label shows when the drive holds nothing to play.
const NOTHING_SLOT: u16 = 315;

/// Where the first button sits and how big one is.
const BUTTON: (i32, i32, u32, u32) = (11, 21, 40, 30);

/// The column a button goes back to the left at.
const WRAP: i32 = 211;

/// Where the wide controls sit and how big they are.
const WIDE: (i32, u32, u32) = (31, 160, 20);

/// How far above the bottom edge the button that stops playing sits on each screen.
const STOP_UP: (i32, i32) = (36, 35);

/// How tall a row of buttons is and what the rest of the window adds to it.
const HEIGHT: (u32, u32, u32) = (30, 62, 84);

/// How the disc carries on once a track ends, which the three mode buttons set (`0x1f7929`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Plays the same track again.
    Repeat,
    /// Picks a track from the tick count.
    Random,
    /// Plays the next track, back to the first past the last.
    Through,
}

impl Mode {
    /// The three in the order the buttons sit.
    pub const ALL: [Self; 3] = [Self::Repeat, Self::Through, Self::Random];

    /// What the original keeps in `0x1f7929` for it.
    #[must_use]
    pub const fn stored(self) -> u32 {
        match self {
            Self::Repeat => 0,
            Self::Random => 1,
            Self::Through => 2,
        }
    }

    /// The text slot its button shows.
    #[must_use]
    pub const fn slot(self) -> u16 {
        match self {
            Self::Repeat => 320,
            Self::Random => 321,
            Self::Through => 322,
        }
    }

    /// How far above the bottom edge its button sits.
    const fn up(self) -> i32 {
        match self {
            Self::Repeat => 112,
            Self::Through => 92,
            Self::Random => 64,
        }
    }
}

/// What a release on one of the two screens asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Play {
    /// Play this CD track, counting from 1.
    Track(u16),
    /// Play this song, counting from 1.
    Song(u16),
    /// Stop the disc.
    StopDisc,
    /// Stop the music.
    StopMusic,
    /// Carry on like this once a track ends.
    Mode(Mode),
}

/// Where button `place` of a grid whose window corner is `at` sits.
#[must_use]
pub fn button(place: usize, at: Point) -> Rect {
    let (x, y, width, height) = BUTTON;
    let step = i32::try_from(width).unwrap_or_default();
    let down = i32::try_from(height).unwrap_or_default();
    let column = i32::try_from(place % row()).unwrap_or_default();
    let line = i32::try_from(place / row()).unwrap_or_default();
    Rect::new(at.x + x + column * step, at.y + y + line * down, width, height)
}

/// How many buttons fit on a row before one goes back to the left.
#[must_use]
pub fn row() -> usize {
    let (x, _, width, _) = BUTTON;
    let step = i32::try_from(width).unwrap_or(1);
    usize::try_from((WRAP - x) / step).unwrap_or(1).max(1)
}

/// Where one of the wide controls sits, `up` pixels above the bottom edge of `area`.
fn wide(up: i32, area: Rect) -> Rect {
    let (x, width, height) = WIDE;
    Rect::new(area.x + x, area.bottom() - up, width, height)
}

/// The CD player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CdWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
}

impl CdWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 222;

    /// The window at `id`.
    #[must_use]
    pub const fn new(id: WindowId) -> Self {
        Self { id }
    }

    /// How big the window is with `tracks` tracks to show.
    #[must_use]
    pub fn size(tracks: usize) -> (u32, u32) {
        let (step, rest, modes) = HEIGHT;
        let rows = u32::try_from(tracks.div_ceil(row())).unwrap_or_default();
        let height = rows * step + rest + if tracks == 0 { 0 } else { modes };
        (Self::WIDTH, height)
    }

    /// Where the button for `mode` sits.
    #[must_use]
    pub fn mode(mode: Mode, area: Rect) -> Rect {
        wide(mode.up(), area)
    }

    /// Where the button that stops the disc sits.
    #[must_use]
    pub fn stop(area: Rect) -> Rect {
        wide(STOP_UP.0, area)
    }

    /// Where the label that says there is nothing to play sits.
    #[must_use]
    pub fn nothing(area: Rect) -> Rect {
        wide(STOP_UP.1, area)
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release asks for.
    /// `tracks` holds the playable track numbers, in the order the buttons show them.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        tracks: &[u16],
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
        let mut asked = None;
        let empty = tracks.is_empty();
        let mut buttons = Buttons {
            ui,
            painter,
            skin,
            id: self.id,
            mouse,
        };
        for (place, &track) in tracks.iter().take(MOST_TRACKS).enumerate() {
            if buttons.numbered(place, at, i32::from(track)) && asked.is_none() {
                asked = Some(Play::Track(track));
            }
        }
        if !empty {
            for mode in Mode::ALL {
                let rect = Self::mode(mode, area);
                let widget = MODE_WIDGET + u16::try_from(mode.stored()).unwrap_or_default();
                if buttons.one(widget, rect, Label::Slot(mode.slot()), NAMED_STYLE) && asked.is_none() {
                    asked = Some(Play::Mode(mode));
                }
            }
            let rect = Self::stop(area);
            if buttons.one(STOP_WIDGET, rect, Label::Slot(STOP_SLOT), NAMED_STYLE) && asked.is_none() {
                asked = Some(Play::StopDisc);
            }
        }
        if empty {
            let rect = Self::nothing(area);
            let look = painter.first_lettering();
            painter.text_aligned(
                Point::new(rect.x, rect.y),
                look.font,
                look.colour,
                Label::Slot(NOTHING_SLOT),
                Pen::TIGHT,
                Align::CentredInBox(rect.width, rect.height),
            );
        }
        painter.pop_clip();
        asked.map(Outcome::Play)
    }
}

/// The MIDI player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
}

impl MidiWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 222;
    /// How tall it is.
    pub const HEIGHT: u32 = 152;

    /// The window at `id`.
    #[must_use]
    pub const fn new(id: WindowId) -> Self {
        Self { id }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where the button that stops the music sits.
    #[must_use]
    pub fn stop(area: Rect) -> Rect {
        wide(STOP_UP.1, area)
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release asks for.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
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
        let mut asked = None;
        let mut buttons = Buttons {
            ui,
            painter,
            skin,
            id: self.id,
            mouse,
        };
        for place in 0..SONGS {
            let song = u16::try_from(place + 1).unwrap_or_default();
            if buttons.numbered(place, at, i32::from(song)) && asked.is_none() {
                asked = Some(Play::Song(song));
            }
        }
        let rect = Self::stop(area);
        if buttons.one(STOP_WIDGET, rect, Label::Slot(STOP_SLOT), NAMED_STYLE) && asked.is_none() {
            asked = Some(Play::StopMusic);
        }
        painter.pop_clip();
        asked.map(Outcome::Play)
    }
}

/// The widget slot the first track or song button takes.
const NUMBER_WIDGET: u16 = 0;
/// The widget slot the first mode button takes.
const MODE_WIDGET: u16 = 96;
/// The widget slot the button that stops playing takes.
const STOP_WIDGET: u16 = 112;

/// What both screens need to draw a button.
struct Buttons<'a> {
    ui: &'a mut Ui,
    painter: &'a mut Painter,
    skin: &'a Skin,
    id: WindowId,
    mouse: Pointer,
}

impl Buttons<'_> {
    /// Draws the button for `place` showing `number` and gives back whether a release acted.
    fn numbered(&mut self, place: usize, at: Point, number: i32) -> bool {
        let rect = button(place, at);
        let widget = NUMBER_WIDGET + u16::try_from(place).unwrap_or_default();
        self.one(widget, rect, Label::Number(number), TRACK_STYLE)
    }

    /// Draws the control at `rect` showing `label` in window style `style` and gives back
    /// whether a release acted.
    fn one(&mut self, widget: u16, rect: Rect, label: Label, style: u16) -> bool {
        let widget = WidgetId::new(self.id, widget);
        let acted = self.ui.press(widget, rect, self.mouse, true);
        let look = self.ui.button(widget).look(true, false);
        if let Some(item) = self.skin.face(style, look) {
            self.painter
                .frame_edges(item, self.skin.edge(style), rect, look.edges());
        }
        let look = self.painter.first_lettering();
        self.painter.text_aligned(
            Point::new(rect.x, rect.y),
            look.font,
            look.colour,
            label,
            Pen::TIGHT,
            Align::CentredInBox(rect.width, rect.height),
        );
        acted
    }
}

/// The kinds the two windows open as.
pub const CD_KIND: WindowId = kind::CD_PLAYER;
/// The MIDI player's kind.
pub const MIDI_KIND: WindowId = kind::MIDI_PLAYER;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ItemId;
    use crate::skin::Look;

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set_face(TRACK_STYLE, Look::Idle, ItemId(2));
        skin.set_face(NAMED_STYLE, Look::Idle, ItemId(3));
        skin
    }

    fn click(place: Rect) -> Pointer {
        Pointer::at(Point::new(place.x + 1, place.y + 1))
    }

    #[test]
    fn five_buttons_fit_a_row() {
        assert_eq!(row(), 5);
        let at = Point::new(0, 0);
        assert_eq!(button(0, at), Rect::new(11, 21, 40, 30));
        assert_eq!(button(4, at), Rect::new(171, 21, 40, 30));
        assert_eq!(button(5, at), Rect::new(11, 51, 40, 30));
        assert_eq!(button(13, at), Rect::new(131, 81, 40, 30));
    }

    #[test]
    fn the_cd_window_grows_with_its_tracks() {
        assert_eq!(CdWindow::size(0), (222, 62));
        assert_eq!(CdWindow::size(1), (222, 30 + 62 + 84));
        assert_eq!(CdWindow::size(5), (222, 30 + 62 + 84));
        assert_eq!(CdWindow::size(6), (222, 2 * 30 + 62 + 84));
    }

    #[test]
    fn the_wide_controls_sit_above_the_bottom_edge() {
        let (width, height) = CdWindow::size(3);
        let area = Rect::new(0, 0, width, height);
        let bottom = i32::try_from(height).expect("a small height");
        assert_eq!(
            CdWindow::mode(Mode::Repeat, area),
            Rect::new(31, bottom - 112, 160, 20)
        );
        assert_eq!(
            CdWindow::mode(Mode::Through, area),
            Rect::new(31, bottom - 92, 160, 20)
        );
        assert_eq!(
            CdWindow::mode(Mode::Random, area),
            Rect::new(31, bottom - 64, 160, 20)
        );
        assert_eq!(CdWindow::stop(area), Rect::new(31, bottom - 36, 160, 20));
        let midi = Rect::new(0, 0, MidiWindow::WIDTH, MidiWindow::HEIGHT);
        assert_eq!(MidiWindow::stop(midi), Rect::new(31, 152 - 35, 160, 20));
    }

    #[test]
    fn a_track_button_asks_for_its_own_number() {
        let tracks = [2u16, 5, 9];
        let (width, height) = CdWindow::size(tracks.len());
        let mut ui = Ui::new();
        ui.open(CD_KIND, Rect::new(0, 0, width, height));
        let mut window = CdWindow::new(CD_KIND);
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let on = click(button(1, Point::new(0, 0)));
        window.run(&mut ui, &mut painter, &skin(), &tracks, on.pressing());
        painter.reset();
        let outcome = window.run(&mut ui, &mut painter, &skin(), &tracks, on.releasing());
        assert_eq!(outcome, Some(Outcome::Play(Play::Track(5))));
    }

    #[test]
    fn the_mode_and_stop_buttons_only_exist_with_a_track() {
        let (width, height) = CdWindow::size(1);
        let mut ui = Ui::new();
        ui.open(CD_KIND, Rect::new(0, 0, width, height));
        let mut window = CdWindow::new(CD_KIND);
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let area = Rect::new(0, 0, width, height);
        let on = click(CdWindow::mode(Mode::Random, area));
        window.run(&mut ui, &mut painter, &skin(), &[1], on.pressing());
        painter.reset();
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), &[1], on.releasing()),
            Some(Outcome::Play(Play::Mode(Mode::Random)))
        );
        painter.reset();
        let (width, height) = CdWindow::size(0);
        let mut ui = Ui::new();
        ui.open(CD_KIND, Rect::new(0, 0, width, height));
        let on = click(CdWindow::nothing(Rect::new(0, 0, width, height)));
        window.run(&mut ui, &mut painter, &skin(), &[], on.pressing());
        painter.reset();
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), &[], on.releasing()),
            None,
            "an empty drive leaves nothing to press"
        );
    }

    #[test]
    fn a_song_button_asks_for_its_own_song() {
        let (width, height) = MidiWindow::size();
        let mut ui = Ui::new();
        ui.open(MIDI_KIND, Rect::new(0, 0, width, height));
        let mut window = MidiWindow::new(MIDI_KIND);
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let on = click(button(13, Point::new(0, 0)));
        window.run(&mut ui, &mut painter, &skin(), on.pressing());
        painter.reset();
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), on.releasing()),
            Some(Outcome::Play(Play::Song(14)))
        );
        painter.reset();
        let on = click(MidiWindow::stop(Rect::new(0, 0, width, height)));
        window.run(&mut ui, &mut painter, &skin(), on.pressing());
        painter.reset();
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), on.releasing()),
            Some(Outcome::Play(Play::StopMusic))
        );
    }
}
