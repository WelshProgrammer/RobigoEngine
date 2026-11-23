//! Robigo's options: the settings S2 does not have, which the game menu opens. The window is drawn
//! with S2's own faces, arrows and letters, a row a setting, and hands back what a press asks to
//! change; the caller owns the settings and shows them again.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::Painter;
use crate::skin::Skin;
use crate::store::Ui;
use crate::text::Label;
use crate::widget::{Pointer, WindowId};
use crate::windows::rows::{self, Control, Row};
use crate::windows::{Outcome, kind};

/// What the options show, from the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is a setting of its own the player flips"
)]
pub struct OptionsView {
    /// Whether the window fills the screen.
    pub fullscreen: bool,
    /// The window's size when it does not.
    pub size: (u32, u32),
    /// The sizes the window can be set to, smallest first.
    pub sizes: &'static [(u32, u32)],
    /// How many screen pixels an interface pixel covers.
    pub scale: u8,
    /// Whether the game draws the mouse pointer itself, as S2 does.
    pub software_pointer: bool,
    /// Whether a drag moves the view the way the pointer goes, as S2 does.
    pub drag_moves_view: bool,
    /// Whether the game starts on the title without playing the intro.
    pub skip_intro: bool,
    /// Whether a picture shows for a few seconds while a game starts.
    pub loading_pictures: bool,
    /// Whether a new game fixes the engine bugs copied from S2.
    pub bug_fixes: bool,
    /// Ambient sound chances a second.
    pub ambient: u16,
    /// Whether the songs play as General MIDI rather than as the OPL3 plays them.
    pub general_midi: bool,
    /// The sound fonts the General MIDI songs can be rendered with, by name.
    pub sound_fonts: &'static [&'static str],
    /// Which of them the settings name, when it is one of them.
    pub sound_font: Option<usize>,
}

/// What a press on the options asks to change: a switch flips, a choice is picked, a stepped
/// value moves by the step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Fullscreen on or off.
    Fullscreen,
    /// The window's size, by its place in [`OptionsView::sizes`].
    Size(usize),
    /// The interface's scale, a step up or down.
    Scale(i8),
    /// Whether the game draws the mouse pointer itself.
    Pointer(bool),
    /// Whether a drag moves the view the way the pointer goes.
    Drag(bool),
    /// Whether the intro plays at the start.
    Intro,
    /// Whether a picture shows while a game starts.
    LoadingPictures,
    /// Whether a new game fixes S2's engine bugs.
    BugFixes,
    /// The ambient sounds' chances, a preset up or down.
    Ambient(i8),
    /// Whether the songs play as General MIDI.
    Music(bool),
    /// The sound font, by its place in [`OptionsView::sound_fonts`].
    SoundFont(usize),
    /// Open the sound test.
    SoundTest,
}

/// What the mouse pointer row offers, the system's first.
const POINTERS: [Label; 2] = [Label::Text("Hardware"), Label::Text("Software")];

/// What the drag row offers, pulling the map first.
const DRAGS: [Label; 2] = [Label::Text("Inverted"), Label::Text("Normal")];

/// What the music row offers, the OPL3 first.
const RENDERINGS: [Label; 2] = [Label::Text("OPL3"), Label::Text("MIDI")];

/// A size as its row shows it.
fn size_label((width, height): (u32, u32)) -> Label {
    Label::Size(
        i32::try_from(width).unwrap_or(i32::MAX),
        i32::try_from(height).unwrap_or(i32::MAX),
    )
}

/// The rows, top to bottom, for `view`, with its window sizes and sound fonts as the labels
/// `sizes` and `fonts`.
fn rows<'a>(view: &OptionsView, sizes: &'a [Label], fonts: &'a [Label]) -> [Row<'a, Change>; 12] {
    let on_off = |on: bool| Label::Text(if on { "On" } else { "Off" });
    let number = |value: u32| Label::Number(i32::try_from(value).unwrap_or(i32::MAX));
    [
        Row {
            name: "Fullscreen",
            help: "Fill the screen, or windowed",
            control: Control::Button(on_off(view.fullscreen), Change::Fullscreen, true),
        },
        Row {
            name: "Window size",
            help: "Has no effect when full screen or maximised",
            control: Control::Choice {
                shown: size_label(view.size),
                choices: sizes,
                chosen: view.sizes.iter().position(|&size| size == view.size),
                pick: Change::Size,
            },
        },
        Row {
            name: "Interface scale",
            help: "How large the windows and toolbar draw",
            control: Control::Stepper(number(u32::from(view.scale)), Change::Scale),
        },
        Row {
            name: "Mouse pointer",
            help: "Hardware is best, use software if you have problems",
            control: Control::Choice {
                shown: POINTERS[usize::from(view.software_pointer)],
                choices: &POINTERS,
                chosen: Some(usize::from(view.software_pointer)),
                pick: |place| Change::Pointer(place == 1),
            },
        },
        Row {
            name: "Scrolling",
            help: "Set which direction the map moves when you drag it",
            control: Control::Choice {
                shown: DRAGS[usize::from(view.drag_moves_view)],
                choices: &DRAGS,
                chosen: Some(usize::from(view.drag_moves_view)),
                pick: |place| Change::Drag(place == 1),
            },
        },
        Row {
            name: "Ambient sounds",
            help: "Higher = Play more often, affects animals, sea and some others",
            control: Control::Stepper(number(u32::from(view.ambient)), Change::Ambient),
        },
        Row {
            name: "Music",
            help: "Songs as an FM card or General MIDI",
            control: Control::Choice {
                shown: RENDERINGS[usize::from(view.general_midi)],
                choices: &RENDERINGS,
                chosen: Some(usize::from(view.general_midi)),
                pick: |place| Change::Music(place == 1),
            },
        },
        Row {
            name: "MIDI soundfont",
            help: "Change the instruments used, requires a restart.",
            control: Control::Choice {
                shown: view
                    .sound_font
                    .and_then(|place| fonts.get(place).copied())
                    .unwrap_or(Label::Text("")),
                choices: fonts,
                chosen: view.sound_font,
                pick: Change::SoundFont,
            },
        },
        Row {
            name: "Intro Video",
            help: "Whether the intro video plays",
            control: Control::Button(
                Label::Text(if view.skip_intro { "Disabled" } else { "Enabled" }),
                Change::Intro,
                true,
            ),
        },
        Row {
            name: "Loading screens",
            help: "Show the splash picture as the game starts",
            control: Control::Button(on_off(view.loading_pictures), Change::LoadingPictures, true),
        },
        Row {
            name: "Disable bug fixes",
            help: "See the README before disabling fixes; only affects new games",
            control: Control::Button(on_off(!view.bug_fixes), Change::BugFixes, true),
        },
        Row {
            name: "Sound test",
            help: "Play any of the game's effects or songs",
            control: Control::Button(Label::Text("Open"), Change::SoundTest, true),
        },
    ]
}

/// The options window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionsWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
    /// The row whose list of choices is open.
    open: Option<usize>,
}

impl Default for OptionsWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl OptionsWindow {
    /// The window at its own kind.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            id: kind::OPTIONS,
            open: None,
        }
    }

    /// How big the window is.
    #[must_use]
    pub fn size() -> (u32, u32) {
        rows::size(rows(&PLACEHOLDER, &[], &[]).len())
    }

    /// Where row `row`'s control sits, in a window whose corner is `at`.
    #[must_use]
    pub fn control(row: usize, at: Point) -> Rect {
        rows::control(row, at)
    }

    /// Takes `mouse`, draws the options for `view` into `painter` and gives back what a release
    /// asks to change.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &OptionsView,
        mouse: Pointer,
    ) -> Option<Outcome> {
        let window = ui.window(self.id).copied().filter(|window| !window.rolled());
        let Some(window) = window else {
            self.open = None;
            return None;
        };
        let area = window.area;
        painter.push_clip(area);
        let at = Point::new(area.x, area.y);
        let sizes: Vec<Label> = view.sizes.iter().copied().map(size_label).collect();
        let fonts: Vec<Label> = view.sound_fonts.iter().copied().map(Label::Text).collect();
        let rows = rows(view, &sizes, &fonts);
        let asked = rows::run(ui, painter, skin, (self.id, at), &rows, &mut self.open, mouse);
        painter.pop_clip();
        asked.map(Outcome::Options)
    }
}

/// A view for laying the window out, whose values do not matter.
const PLACEHOLDER: OptionsView = OptionsView {
    fullscreen: false,
    size: (0, 0),
    scale: 1,
    software_pointer: false,
    drag_moves_view: false,
    skip_intro: false,
    loading_pictures: true,
    bug_fixes: true,
    ambient: 0,
    general_midi: false,
    sound_fonts: &[],
    sound_font: None,
    sizes: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_rows_fit_the_window() {
        let (width, height) = OptionsWindow::size();
        assert_eq!(width, rows::WIDTH);
        let last = OptionsWindow::control(11, Point::new(0, 0));
        assert!(last.y + i32::try_from(last.height).unwrap() < i32::try_from(height).unwrap());
        assert!(last.x + i32::try_from(last.width).unwrap() < i32::try_from(width).unwrap());
    }

    const SIZES: [(u32, u32); 3] = [(640, 480), (800, 600), (1024, 768)];
    const FONTS: [&str; 3] = ["GeneralUser GS", "MuseScore General", "Roland sounds"];
    const VIEW: OptionsView = OptionsView {
        size: (800, 600),
        sizes: &SIZES,
        sound_fonts: &FONTS,
        sound_font: Some(0),
        ..PLACEHOLDER
    };

    struct Desk {
        ui: Ui,
        window: OptionsWindow,
        painter: Painter,
        skin: Skin,
    }

    impl Desk {
        fn new() -> Self {
            let (width, height) = OptionsWindow::size();
            let mut ui = Ui::new();
            ui.open(kind::OPTIONS, Rect::new(0, 0, width, height));
            Self {
                ui,
                window: OptionsWindow::new(),
                painter: Painter::new(Rect::new(0, 0, 640, 480)),
                skin: Skin::new(),
            }
        }

        /// A press and its release at `at`, and what the release asks for.
        fn click(&mut self, at: Point) -> Option<Outcome> {
            let on = Pointer::at(at);
            self.window
                .run(&mut self.ui, &mut self.painter, &self.skin, &VIEW, on.pressing());
            self.painter.reset();
            let asked = self
                .window
                .run(&mut self.ui, &mut self.painter, &self.skin, &VIEW, on.releasing());
            self.painter.reset();
            asked
        }
    }

    fn on_row(row: usize, dx: i32) -> Point {
        let control = OptionsWindow::control(row, Point::new(0, 0));
        Point::new(control.x + dx, control.y + 2)
    }

    fn on_choice(row: usize, number: usize, choices: usize) -> Point {
        let rect = rows::choice(row, (number, choices), 12, Point::new(0, 0));
        Point::new(rect.x + 2, rect.y + 2)
    }

    #[test]
    fn a_press_on_a_button_or_an_arrow_asks_for_its_change() {
        let mut desk = Desk::new();
        let up = i32::try_from(rows::CONTROL_WIDTH - rows::ARROW).unwrap() + 2;
        for (at, change) in [
            (on_row(0, 2), Change::Fullscreen),
            (on_row(2, 2), Change::Scale(-1)),
            (on_row(2, up), Change::Scale(1)),
            (on_row(5, 2), Change::Ambient(-1)),
            (on_row(8, 2), Change::Intro),
            (on_row(9, 2), Change::LoadingPictures),
            (on_row(10, 2), Change::BugFixes),
            (on_row(11, 2), Change::SoundTest),
        ] {
            assert_eq!(desk.click(at), Some(Outcome::Options(change)), "{change:?}");
        }
    }

    #[test]
    fn a_list_opens_under_its_row_and_a_choice_asks_for_its_change() {
        let mut desk = Desk::new();
        for (row, number, choices, change) in [
            (1, 2, SIZES.len(), Change::Size(2)),
            (3, 1, 2, Change::Pointer(true)),
            (4, 0, 2, Change::Drag(false)),
            (6, 1, 2, Change::Music(true)),
            (7, 2, FONTS.len(), Change::SoundFont(2)),
        ] {
            assert_eq!(desk.click(on_row(row, 2)), None, "opening asks for nothing");
            assert_eq!(desk.window.open, Some(row));
            assert_eq!(
                desk.click(on_choice(row, number, choices)),
                Some(Outcome::Options(change)),
                "{change:?}"
            );
            assert_eq!(desk.window.open, None, "a pick closes the list");
        }
    }

    #[test]
    fn an_open_list_keeps_the_rows_under_it_from_acting_and_closes_on_a_press_elsewhere() {
        let mut desk = Desk::new();
        desk.click(on_row(7, 2));
        let under = on_choice(7, 1, FONTS.len());
        let row_under = OptionsWindow::control(8, Point::new(0, 0));
        assert!(
            row_under.contains(under.x, under.y),
            "the list covers the intro row"
        );
        assert_eq!(
            desk.click(under),
            Some(Outcome::Options(Change::SoundFont(1))),
            "the choice, with the intro row left alone"
        );
        desk.click(on_row(7, 2));
        assert_eq!(
            desk.click(on_row(0, 2)),
            None,
            "a press elsewhere only closes the list"
        );
        assert_eq!(desk.window.open, None);
        assert_eq!(
            desk.click(on_row(0, 2)),
            Some(Outcome::Options(Change::Fullscreen))
        );
        desk.click(on_row(7, 2));
        desk.click(on_row(7, 2));
        assert_eq!(desk.window.open, None, "a second press on the control closes it");
    }

    #[test]
    fn a_list_near_the_foot_of_the_window_opens_over_its_row() {
        let (_, height) = OptionsWindow::size();
        let at = Point::new(0, 0);
        let under = rows::choice(1, (9, 10), 12, at);
        assert!(under.y > OptionsWindow::control(1, at).y);
        let over = rows::choice(11, (9, 10), 12, at);
        assert!(over.y < OptionsWindow::control(11, at).y);
        assert!(under.y + 16 <= i32::try_from(height).unwrap());
    }
}
