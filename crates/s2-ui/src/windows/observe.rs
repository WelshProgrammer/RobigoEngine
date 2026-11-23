//! The observation window: a second picture of the world around a node.

use s2_render::terrain::Point;
use s2_sim::Node;

use crate::chrome::Chrome;
use crate::geometry::Rect;
use crate::paint::{Painter, Sheet};
use crate::picture::Picture;
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::text::Label;
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::map::MARKER;
use crate::windows::{Outcome, kind};

/// The three sizes the window steps through (`0x495e0`).
const SIZES: [(u32, u32); 3] = [(262, 192), (302, 248), (342, 312)];

/// How wide and tall a button is (`0x49600`).
const BUTTON: u32 = 36;

/// How far the top of the row of buttons sits above the window's bottom.
const ROW_ABOVE_BOTTOM: i32 = 50;

/// The buttons left to right: what each one asks for, the picture it draws and the text slot it
/// reports while the pointer is over it.
const BUTTONS: [(Asked, Picture, u16); 4] = [
    (Asked::Zoom, Picture::ZOOM, 206),
    (Asked::Follow, Picture::FOLLOW, 207),
    (Asked::GoTo, Picture::GO_TO, 219),
    (Asked::Size, Picture::OBSERVE_SIZE, 200),
];

/// How many zoom factors the picture steps through (`0x494b2`).
pub const ZOOMS: u8 = 3;

/// What a press on the window asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// The next zoom factor.
    Zoom,
    /// The next settler to watch.
    Follow,
    /// The main view should move to the node this window shows.
    GoTo,
    /// The next window size.
    Size,
}

/// What the window's picture keeps in its middle (the view's `+0x27` and `+0x3b`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Watch {
    /// Just opened: on a building it stays where it is, and anywhere else it looks for a figure
    /// to follow (`0x5bbe9` opens the window with 2 over a building and 1 otherwise).
    #[default]
    Opened,
    /// Looking for the figure nearest its middle, which it follows from the next frame
    /// (`0x48db0`).
    Seeking,
    /// Following the figure in this record until the record is given back (`0x48cb6`).
    Following(u16),
    /// Staying where it is.
    Still,
}

/// A window watching one node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserveWindow {
    /// The node in the middle of the picture, or the one a followed figure is on.
    pub node: Node,
    /// What the picture keeps in its middle.
    pub watch: Watch,
    /// The world pixel in the middle of the picture once it has followed a figure, which it
    /// keeps when the following ends. Until then the picture stands on its node.
    pub middle: Option<Point>,
    /// How far the right button has dragged the picture since the game last took it, in world
    /// pixels.
    pub dragged: Point,
    /// Which of [`SIZES`] the window has.
    size: usize,
    /// How many times the picture is doubled.
    zoom: u8,
    /// The picture's pixels, row by row, filled by [`ObserveWindow::read`].
    pixels: Vec<u8>,
}

impl ObserveWindow {
    /// Which window kind it opens as.
    pub const KIND: WindowId = kind::OBSERVE;

    /// A window on `node` as the original opens one, with nothing drawn yet: the smallest size,
    /// and the picture doubled once (`0x46107`).
    #[must_use]
    pub const fn new(node: Node) -> Self {
        Self {
            node,
            watch: Watch::Opened,
            middle: None,
            dragged: Point::new(0, 0),
            size: 0,
            zoom: 1,
            pixels: Vec::new(),
        }
    }

    /// How big the window is.
    #[must_use]
    pub const fn size(&self) -> (u32, u32) {
        SIZES[self.size % SIZES.len()]
    }

    /// How many times the picture is doubled, which is how the original magnifies a view
    /// (`0x201bb`).
    #[must_use]
    pub const fn zoom(&self) -> u8 {
        self.zoom
    }

    /// Where the picture sits inside a window of `area`: the whole client area, with the buttons
    /// drawn over its lower part.
    #[must_use]
    pub fn picture(area: Rect) -> Rect {
        Chrome::inside(area)
    }

    /// Whether a right press at `at` in a window of `area` takes hold of the picture: anywhere
    /// on it but the buttons.
    #[must_use]
    pub fn grabs(area: Rect, at: Point) -> bool {
        Self::picture(area).contains(at.x, at.y)
            && (0..BUTTONS.len()).all(|place| !Self::button(place, area).contains(at.x, at.y))
    }

    /// Takes hold of the picture, which ends any following (`0x48912`).
    pub const fn grab(&mut self) {
        self.watch = Watch::Still;
    }

    /// Moves the picture's middle by `by` world pixels. The original adds the mouse's movement
    /// to the view's scroll position whatever the zoom (`0x489cc`).
    pub const fn drag(&mut self, by: Point) {
        self.dragged = Point::new(self.dragged.x + by.x, self.dragged.y + by.y);
    }

    /// Where the marker's origin goes: the middle of `picture`, halved toward zero.
    #[must_use]
    pub fn marker(picture: Rect) -> Point {
        let half = |length: u32| i32::try_from(length / 2).unwrap_or_default();
        Point::new(picture.x + half(picture.width), picture.y + half(picture.height))
    }

    /// Where button `place` sits inside a window of `area`. The row of four is centred.
    #[must_use]
    pub fn button(place: usize, area: Rect) -> Rect {
        let place = i32::try_from(place.min(BUTTONS.len() - 1)).unwrap_or_default();
        let side = i32::try_from(BUTTON).unwrap_or_default();
        let width = i32::try_from(area.width).unwrap_or_default();
        let left = area.x + width / 2 - side * 2 + side * place;
        Rect::new(left, area.bottom() - ROW_ABOVE_BOTTOM, BUTTON, BUTTON)
    }

    /// Takes the picture's pixels, one byte a pixel, row by row.
    pub fn read(&mut self, pixels: &[u8]) {
        self.pixels.clear();
        self.pixels.extend_from_slice(pixels);
    }

    /// Steps the window on to its next size, giving back the size it now has.
    pub fn resize(&mut self) -> (u32, u32) {
        self.size = (self.size + 1) % SIZES.len();
        self.size()
    }

    /// Takes a press on the follow button (`0x48d94`): a picture following a figure lets it go
    /// and stays where it is, and any other looks for the nearest figure.
    pub const fn toggle_follow(&mut self) {
        self.watch = match self.watch {
            Watch::Following(_) => Watch::Still,
            _ => Watch::Seeking,
        };
    }

    /// Steps the picture on to its next zoom factor.
    pub fn zoom_in(&mut self) {
        self.zoom = (self.zoom + 1) % ZOOMS;
    }

    /// Takes `mouse`, draws the window and gives back what a release asks for.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        mouse: Pointer,
    ) -> Option<Outcome> {
        let window = *ui.window(Self::KIND)?;
        let area = window.area;
        painter.push_clip(area);
        if window.rolled() {
            painter.pop_clip();
            return None;
        }
        let picture = Self::picture(area);
        if self.pixels.len() == (picture.width * picture.height) as usize {
            let image = painter.picture(&self.pixels, picture.width, picture.height);
            painter.push_clip(picture);
            painter.image(image, Point::new(picture.x, picture.y));
            if self.watch == Watch::Still {
                painter.game(Sheet::Landscape, MARKER, Self::marker(picture));
            }
            painter.pop_clip();
        }
        let mut asked = None;
        for (place, &(what, drawn, help)) in BUTTONS.iter().enumerate() {
            let rect = Self::button(place, area);
            let widget = WidgetId::new(Self::KIND, u16::try_from(place).unwrap_or_default());
            if ui.press(widget, rect, mouse, true) && asked.is_none() {
                asked = Some(what);
            }
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), rect, look.edges());
            }
            if let Some(item) = skin.picture(drawn) {
                painter.item_centred(item, rect);
            }
            ui.note_help(rect, mouse, Label::Slot(help));
        }
        painter.pop_clip();
        match asked? {
            Asked::Zoom => {
                self.zoom_in();
                Some(Outcome::Observe(Asked::Zoom))
            }
            Asked::GoTo => Some(Outcome::Centre(self.node)),
            Asked::Follow => {
                self.toggle_follow();
                Some(Outcome::Observe(Asked::Follow))
            }
            Asked::Size => Some(Outcome::Observe(Asked::Size)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ItemId;

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Button, Look::Idle, ItemId(1));
        skin
    }

    fn window_area(ui: &mut Ui, observe: &ObserveWindow) -> Rect {
        let (width, height) = observe.size();
        let area = Rect::new(0, 0, width, height);
        ui.open(ObserveWindow::KIND, area);
        area
    }

    #[test]
    fn the_sizes_are_the_three_the_original_steps_through() {
        let mut window = ObserveWindow::new(Node { x: 1, y: 2 });
        assert_eq!(window.size(), (262, 192));
        assert_eq!(window.resize(), (302, 248));
        assert_eq!(window.resize(), (342, 312));
        assert_eq!(
            window.resize(),
            (262, 192),
            "the third size comes back to the first"
        );
    }

    #[test]
    fn the_row_of_four_buttons_is_centred_above_the_bottom() {
        let area = Rect::new(0, 0, 262, 192);
        let first = ObserveWindow::button(0, area);
        let last = ObserveWindow::button(3, area);
        assert_eq!(first, Rect::new(131 - 72, 192 - 50, 36, 36));
        assert_eq!(last.right(), first.x + 4 * 36);
        assert_eq!(first.y, last.y);
    }

    #[test]
    fn a_right_press_takes_hold_of_the_picture_but_not_of_a_button() {
        let area = Rect::new(0, 0, SIZES[0].0, SIZES[0].1);
        let picture = ObserveWindow::picture(area);
        assert!(ObserveWindow::grabs(area, ObserveWindow::marker(picture)));
        assert!(!ObserveWindow::grabs(
            area,
            centre(ObserveWindow::button(1, area))
        ));
        assert!(
            !ObserveWindow::grabs(area, Point::new(area.x, area.y)),
            "the frame"
        );
    }

    #[test]
    fn taking_hold_ends_the_following_and_drags_add_up() {
        let mut window = ObserveWindow::new(Node { x: 7, y: 9 });
        window.watch = Watch::Following(3);
        window.grab();
        assert_eq!(window.watch, Watch::Still);
        window.drag(Point::new(4, -2));
        window.drag(Point::new(1, 5));
        assert_eq!(window.dragged, Point::new(5, 3));
    }

    #[test]
    fn the_picture_fills_the_client_area_with_the_marker_in_its_middle() {
        // An S2 capture of the smallest window at (0, 0): the picture covers x 11 to 250 and
        // y 21 to 180, under the buttons, and MAP 23 stands with its origin at (131, 101).
        let area = Rect::new(0, 0, 262, 192);
        let picture = ObserveWindow::picture(area);
        assert_eq!(picture, Rect::new(11, 21, 240, 160));
        assert_eq!(ObserveWindow::marker(picture), Point::new(131, 101));
        assert_eq!(ObserveWindow::button(0, area), Rect::new(59, 142, 36, 36));
    }

    #[test]
    fn the_zoom_button_steps_the_picture_through_three_factors() {
        let mut ui = Ui::new();
        let mut window = ObserveWindow::new(Node { x: 1, y: 2 });
        let area = window_area(&mut ui, &window);
        let at = centre(ObserveWindow::button(0, area));
        assert_eq!(window.zoom(), 1, "the window opens with the picture doubled");
        for factor in [2, 0, 1] {
            press(&mut ui, &mut window, at);
            assert_eq!(window.zoom(), factor);
        }
    }

    #[test]
    fn the_go_to_button_asks_for_the_node_the_window_watches() {
        let mut ui = Ui::new();
        let mut window = ObserveWindow::new(Node { x: 7, y: 9 });
        let area = window_area(&mut ui, &window);
        let at = centre(ObserveWindow::button(2, area));
        let outcome = press(&mut ui, &mut window, at);
        assert_eq!(outcome, Some(Outcome::Centre(Node { x: 7, y: 9 })));
    }

    #[test]
    fn the_follow_button_looks_for_a_figure_and_lets_one_go() {
        let mut ui = Ui::new();
        let mut window = ObserveWindow::new(Node { x: 1, y: 2 });
        let area = window_area(&mut ui, &window);
        let at = centre(ObserveWindow::button(1, area));
        assert_eq!(window.watch, Watch::Opened);
        press(&mut ui, &mut window, at);
        assert_eq!(window.watch, Watch::Seeking);
        window.watch = Watch::Following(7);
        press(&mut ui, &mut window, at);
        assert_eq!(window.watch, Watch::Still, "a second press lets the figure go");
        press(&mut ui, &mut window, at);
        assert_eq!(window.watch, Watch::Seeking);
    }

    fn centre(rect: Rect) -> Point {
        Point::new(
            rect.x + i32::try_from(rect.width).unwrap_or_default() / 2,
            rect.y + i32::try_from(rect.height).unwrap_or_default() / 2,
        )
    }

    fn press(ui: &mut Ui, window: &mut ObserveWindow, at: Point) -> Option<Outcome> {
        let skin = skin();
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let down = Pointer {
            at,
            pressed: true,
            ..Pointer::default()
        };
        window.run(ui, &mut painter, &skin, down);
        let up = Pointer {
            at,
            released: true,
            ..Pointer::default()
        };
        window.run(ui, &mut painter, &skin, up)
    }

    #[test]
    fn a_button_reports_the_line_of_help_the_pointer_is_over() {
        let mut ui = Ui::new();
        let mut window = ObserveWindow::new(Node { x: 1, y: 2 });
        let area = window_area(&mut ui, &window);
        let skin = skin();
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let mouse = Pointer {
            at: centre(ObserveWindow::button(1, area)),
            ..Pointer::default()
        };
        ui.forget_help();
        window.run(&mut ui, &mut painter, &skin, mouse);
        assert_eq!(ui.help(), Some(Label::Slot(207)));
    }
}
