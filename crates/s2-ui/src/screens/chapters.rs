//! The campaign's screens: over the main menu's picture, the chapter, with Choose chapter, Play
//! chapter and Back, and the chapter list, with a button for each chapter and Back; and the
//! picture a won chapter leads to. The screens draw what they are given and hand back what a release
//! on a button asks for; the caller keeps the chapter and which chapters are open.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::{Align, Painter};
use crate::skin::Skin;
use crate::store::Ui;
use crate::text::{Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};

/// How many chapters the list offers.
pub const CHAPTERS: usize = 10;

/// The picture a won chapter leads to (`0x60381`, kind 8), and the one after the last chapter.
pub const WON_PICTURE: &str = "GFX/PICS/SETUP896.LBM";
/// See [`WON_PICTURE`].
pub const LAST_WON_PICTURE: &str = "GFX/PICS/SETUP895.LBM";

/// The line under a won chapter's picture, which takes the chapter's number from 1
/// (`0x5ed8d`).
const COMPLETED: u16 = 422;
const COMPLETED_AREA: (i32, i32, u32, u32) = (40, 420, 560, 40);

/// What a release on either screen asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChapterPick {
    /// The chapter list.
    Choose,
    /// Start the chapter the screen shows.
    Play,
    /// The main menu.
    Back,
    /// This chapter, from 0, on the chapter screen.
    Chapter(u8),
}

/// The faces the buttons take.
const GREEN: u16 = 3;
const RED: u16 = 1;

/// The styles the title and the chapter's name draw in.
const TITLE_STYLE: u16 = 5;
const PLAIN: u16 = 0;

/// The title of each screen and where it sits (`0x6006e`, `0x60133`).
const CAMPAIGN: u16 = 394;
const CHAPTER_SELECTION: u16 = 398;
const TITLE_AREA: (i32, i32, u32, u32) = (170, 40, 300, 20);

/// The first chapter's name; the others follow it.
const FIRST_NAME: u16 = 400;
const NAME_AREA: (i32, i32, u32, u32) = (170, 130, 300, 20);

/// The chapter screen's buttons (`0x1fd230`).
const CHOOSE: (u16, (i32, i32)) = (395, (220, 150));
const PLAY: (u16, (i32, i32)) = (396, (220, 260));
const BACK: (u16, (i32, i32)) = (397, (220, 440));
const BUTTON: (u32, u32) = (200, 20);

/// The chapter list's buttons (`0x1fd26c`): a column of chapters, then Back.
const LIST_LEFT: i32 = 170;
const LIST_TOP: i32 = 125;
const LIST_STEP: i32 = 25;
const LIST_BUTTON: (u32, u32) = (300, 20);
const LIST_BACK: u16 = 399;

/// The campaign's two screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chapters {
    /// The id the chapter screen keeps its buttons under.
    pub chapter_id: WindowId,
    /// The id the chapter list keeps its buttons under.
    pub list_id: WindowId,
}

impl Default for Chapters {
    fn default() -> Self {
        Self::new()
    }
}

impl Chapters {
    /// The screens, their buttons kept under [`crate::windows::kind::CHAPTER`] and
    /// [`crate::windows::kind::CHAPTER_LIST`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            chapter_id: crate::windows::kind::CHAPTER,
            list_id: crate::windows::kind::CHAPTER_LIST,
        }
    }

    /// Where the chapter screen's button for `pick` sits on a screen whose corner is `at`: Choose,
    /// Play or Back.
    #[must_use]
    pub fn chapter_button(pick: ChapterPick, at: Point) -> Rect {
        let (_, (x, y)) = match pick {
            ChapterPick::Choose => CHOOSE,
            ChapterPick::Play => PLAY,
            ChapterPick::Back | ChapterPick::Chapter(_) => BACK,
        };
        Rect::new(at.x + x, at.y + y, BUTTON.0, BUTTON.1)
    }

    /// Where the chapter list's button for `pick` sits: a chapter's, or Back.
    #[must_use]
    pub fn list_button(pick: ChapterPick, at: Point) -> Rect {
        match pick {
            ChapterPick::Chapter(chapter) => Rect::new(
                at.x + LIST_LEFT,
                at.y + LIST_TOP + LIST_STEP * i32::from(chapter),
                LIST_BUTTON.0,
                LIST_BUTTON.1,
            ),
            _ => Self::chapter_button(ChapterPick::Back, at),
        }
    }

    /// Takes `mouse`, draws the chapter screen for `chapter`, from 0, onto a screen whose corner
    /// is `at`, and gives back what a release on a button asks for.
    pub fn run_chapter(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        chapter: u8,
        at: Point,
        mouse: Pointer,
    ) -> Option<ChapterPick> {
        let mut drawer = Drawer::new(ui, painter, skin, self.chapter_id, mouse);
        drawer.label(Label::Slot(CAMPAIGN), area(TITLE_AREA, at), TITLE_STYLE);
        let name = FIRST_NAME.saturating_add(u16::from(chapter));
        drawer.label(Label::Slot(name), area(NAME_AREA, at), PLAIN);
        for (text, pick, style) in [
            (CHOOSE.0, ChapterPick::Choose, GREEN),
            (PLAY.0, ChapterPick::Play, GREEN),
            (BACK.0, ChapterPick::Back, RED),
        ] {
            drawer.button(Self::chapter_button(pick, at), text, style, pick, true);
        }
        drawer.picked
    }

    /// Takes `mouse`, draws the chapter list onto a screen whose corner is `at`, a chapter that
    /// `open` refuses greyed and deaf to clicks (`0x4dbac`), and gives back what a release on a
    /// button asks for.
    pub fn run_list(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        open: &[bool; CHAPTERS],
        at: Point,
        mouse: Pointer,
    ) -> Option<ChapterPick> {
        let mut drawer = Drawer::new(ui, painter, skin, self.list_id, mouse);
        drawer.label(Label::Slot(CHAPTER_SELECTION), area(TITLE_AREA, at), TITLE_STYLE);
        for (chapter, &enabled) in (0u8..).zip(open) {
            let pick = ChapterPick::Chapter(chapter);
            let text = FIRST_NAME + u16::from(chapter);
            drawer.button(Self::list_button(pick, at), text, GREEN, pick, enabled);
        }
        let back = ChapterPick::Back;
        drawer.button(Self::list_button(back, at), LIST_BACK, RED, back, true);
        drawer.picked
    }
}

/// Draws the line under a won chapter's picture for `chapter`, from 0, onto a screen whose corner
/// is `at`. The last chapter's picture has no line.
pub fn draw_won(painter: &mut Painter, chapter: u8, at: Point) {
    if usize::from(chapter) + 1 >= CHAPTERS {
        return;
    }
    let rect = area(COMPLETED_AREA, at);
    if let Some(lettering) = painter.lettering(TITLE_STYLE) {
        painter.text_aligned(
            Point::new(rect.x, rect.y),
            lettering.font,
            lettering.colour,
            Label::Numbered(COMPLETED, i32::from(chapter) + 1),
            Pen::TIGHT,
            Align::CentredInBox(rect.width, rect.height),
        );
    }
}

/// The picture a won `chapter`, from 0, leads to.
#[must_use]
pub fn won_picture(chapter: u8) -> &'static str {
    if usize::from(chapter) + 1 >= CHAPTERS {
        LAST_WON_PICTURE
    } else {
        WON_PICTURE
    }
}

/// `(x, y, width, height)` on a screen whose corner is `at`.
fn area((x, y, width, height): (i32, i32, u32, u32), at: Point) -> Rect {
    Rect::new(at.x + x, at.y + y, width, height)
}

/// Draws the controls one after another, numbering their widgets in order.
struct Drawer<'a> {
    ui: &'a mut Ui,
    painter: &'a mut Painter,
    skin: &'a Skin,
    id: WindowId,
    widget: u16,
    mouse: Pointer,
    picked: Option<ChapterPick>,
}

impl<'a> Drawer<'a> {
    fn new(ui: &'a mut Ui, painter: &'a mut Painter, skin: &'a Skin, id: WindowId, mouse: Pointer) -> Self {
        Self {
            ui,
            painter,
            skin,
            id,
            widget: 0,
            mouse,
            picked: None,
        }
    }

    /// Draws `label` in `style`, centred in `rect`.
    fn label(&mut self, label: Label, rect: Rect, style: u16) {
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

    /// Draws a button over `rect` in `style` showing slot `text`, which asks for `pick` when
    /// released on. A button not `enabled` draws greyed and swallows the click.
    fn button(&mut self, rect: Rect, text: u16, style: u16, pick: ChapterPick, enabled: bool) {
        let widget = WidgetId::new(self.id, self.widget);
        self.widget += 1;
        let acted = self.ui.press(widget, rect, self.mouse, enabled);
        let look = self.ui.button(widget).look(true, false);
        if let Some(face) = self.skin.face(style, look) {
            self.painter
                .frame_edges(face, self.skin.edge(style), rect, look.edges());
        }
        self.label(Label::Slot(text), rect, style);
        if !enabled {
            self.painter.disable(rect);
        }
        if acted && self.picked.is_none() {
            self.picked = Some(pick);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_OPEN: [bool; CHAPTERS] = [true; CHAPTERS];

    fn release(
        rect: Rect,
        mut run: impl FnMut(&mut Ui, &mut Painter, Pointer) -> Option<ChapterPick>,
    ) -> Option<ChapterPick> {
        let mut ui = Ui::new();
        let on = Pointer::at(Point::new(rect.x + 4, rect.y + 4));
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        run(&mut ui, &mut painter, on.pressing());
        painter.reset();
        run(&mut ui, &mut painter, on.releasing())
    }

    fn on_chapter(rect: Rect) -> Option<ChapterPick> {
        let mut screens = Chapters::new();
        let skin = Skin::new();
        release(rect, |ui, painter, mouse| {
            screens.run_chapter(ui, painter, &skin, 0, Point::new(0, 0), mouse)
        })
    }

    fn on_list(rect: Rect, open: &[bool; CHAPTERS]) -> Option<ChapterPick> {
        let mut screens = Chapters::new();
        let skin = Skin::new();
        release(rect, |ui, painter, mouse| {
            screens.run_list(ui, painter, &skin, open, Point::new(0, 0), mouse)
        })
    }

    #[test]
    fn the_buttons_sit_where_the_original_builds_them() {
        let at = Point::new(0, 0);
        assert_eq!(
            Chapters::chapter_button(ChapterPick::Choose, at),
            Rect::new(220, 150, 200, 20)
        );
        assert_eq!(
            Chapters::chapter_button(ChapterPick::Play, at),
            Rect::new(220, 260, 200, 20)
        );
        assert_eq!(
            Chapters::chapter_button(ChapterPick::Back, at),
            Rect::new(220, 440, 200, 20)
        );
        assert_eq!(
            Chapters::list_button(ChapterPick::Chapter(0), at),
            Rect::new(170, 125, 300, 20)
        );
        assert_eq!(
            Chapters::list_button(ChapterPick::Chapter(9), at),
            Rect::new(170, 350, 300, 20)
        );
    }

    #[test]
    fn a_release_asks_for_its_button() {
        let at = Point::new(0, 0);
        for pick in [ChapterPick::Choose, ChapterPick::Play, ChapterPick::Back] {
            assert_eq!(on_chapter(Chapters::chapter_button(pick, at)), Some(pick));
        }
        for pick in [ChapterPick::Chapter(3), ChapterPick::Back] {
            assert_eq!(on_list(Chapters::list_button(pick, at), &ALL_OPEN), Some(pick));
        }
    }

    #[test]
    fn a_closed_chapter_swallows_the_click() {
        let mut open = [false; CHAPTERS];
        open[0] = true;
        let at = Point::new(0, 0);
        assert_eq!(
            on_list(Chapters::list_button(ChapterPick::Chapter(1), at), &open),
            None
        );
        assert_eq!(
            on_list(Chapters::list_button(ChapterPick::Chapter(0), at), &open),
            Some(ChapterPick::Chapter(0))
        );
    }
}
