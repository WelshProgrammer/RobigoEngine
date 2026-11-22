//! The screens that only show a file: the readme, the keyboard layout and the help pages.
//!
//! Each holds one [`TextPanel`] and nothing else, so they share one window here. What differs is
//! the size, the title and where the text came from. The diary holds the same panel in a window
//! of its own ([`crate::windows::diary`]).

use s2_render::terrain::Point;

use crate::chrome::Chrome;
use crate::geometry::Rect;
use crate::paint::Painter;
use crate::panel::{Gutter, TextPanel};
use crate::picture::Picture;
use crate::skin::{Look, Skin};
use crate::store::Ui;
use crate::style;
use crate::text::{FontId, Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::kind;

/// The text slot the readme's title shows.
const README_SLOT: u16 = 172;

/// The text slot the keyboard layout's title shows.
const KEYS_SLOT: u16 = 182;

/// The text slot a help page's title shows (`0x453bf`).
const HELP_SLOT: u16 = 183;

/// The first page the size table covers (`0x45380`).
const FIRST_LISTED_PAGE: u16 = 1000;

/// How big each page from [`FIRST_LISTED_PAGE`] up is, from the 12-byte records at `0x1fd564`.
const LISTED_SIZES: [(u32, u32); 11] = [
    (240, 120),
    (260, 164),
    (330, 270),
    (260, 160),
    (260, 145),
    (290, 100),
    (290, 100),
    (290, 100),
    (320, 100),
    (320, 100),
    (320, 100),
];

/// How big the list of repairs opens.
const REPAIRS_SIZE: (u32, u32) = (360, 260);

/// How big a page the table does not cover is (`0x453ba`).
const UNLISTED_SIZE: (u32, u32) = (280, 164);

/// The flags a help page's panel takes: the style index 5 from the record with bit 3 added
/// (`0x453ef`). Bit 2 is what [`Gutter::of`] reads, so a line takes the whole width.
const HELP_FLAGS: u8 = 5 | TALL_LINES;

/// How far one line of a panel sits below the last, with bit 3 of the flags set and without it
/// (`0x59958`).
const LINE_STEPS: (u32, u32) = (14, 11);

/// The bit of the flags that picks the taller step.
const TALL_LINES: u8 = 1 << 3;

/// The byte DOS writes at the end of a text file, which the pages the game ships carry.
const END_OF_FILE: u8 = 0x1a;

/// How many lines `text` holds past its first, which is one per newline in it.
#[allow(clippy::naive_bytecount, reason = "a help page is a few hundred bytes")]
fn newlines(text: &[u8]) -> usize {
    text.iter().filter(|&&byte| byte == b'\n').count()
}

/// What a help page's window takes besides its lines. The size its table gives is where the
/// window starts, and `0x59b14` grows it to `lines * step + 33` once the file is in.
const HELP_FRAME: u32 = 33;

/// The folder the help pages live in and the name each file takes before its language ending.
pub const HELP_FOLDER: &str = "DATA/ONLINE";

/// The ending a help page's file name takes in a German build and in every other one
/// (`0x45400`, `0x45407`).
pub const HELP_ENDINGS: (&str, &str) = ("GER", "ENG");

/// How far the column of scroll buttons sits from the window's right edge, where the first one
/// starts, how far apart they sit and how big one is (`0x59b50`).
const COLUMN: (i32, i32, i32, u32) = (44, 24, 32, 30);

/// How many lines a button moves the panel by: the page buttons and the line buttons
/// (`0x59e8d`, `0x59e7e`).
const STEPS: (usize, usize) = (12, 2);

/// Which font a panel draws its lines in, which its own line height paces.
pub const PANEL_FONT: FontId = FontId::Medium;

/// The cell every glyph of a line sits in, which the draw puts in the pen's own field
/// (`0x597ec`) and the wrap counts a line's characters in.
const CELL: u16 = 8;

/// The file the readme reads.
pub const README_FILE: &str = "README.TXT";

/// The file the keyboard layout reads in an English build.
pub const KEYS_FILE: &str = "DATA\\TXT2\\KEYS.ENG";

/// The file it reads in a German one.
pub const KEYS_FILE_GERMAN: &str = "DATA\\TXT2\\KEYS.GER";

/// Which of the screens a window is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// A help page, by the number the screen's own button hands over.
    Help(u16),
    /// The shipped notes.
    Readme,
    /// The keyboard layout.
    Keys,
    /// Robigo's list of what a load took out of a save made by other rules.
    Repairs,
}

impl Reading {
    /// The two that have a kind of their own. A help page is made from its number instead.
    pub const ALL: [Self; 2] = [Self::Readme, Self::Keys];

    /// The file a help page of `page` reads, inside [`HELP_FOLDER`], for a German build or any
    /// other one (`0x170aec`).
    #[must_use]
    pub fn help_file(page: u16, german: bool) -> String {
        let (first, second) = HELP_ENDINGS;
        let ending = if german { first } else { second };
        format!("HELP{page:04}.{ending}")
    }

    /// The window kind it opens as.
    #[must_use]
    pub const fn kind(self) -> WindowId {
        match self {
            Self::Help(_) => kind::HELP_PAGE,
            Self::Readme => kind::README,
            Self::Keys => kind::KEYS,
            Self::Repairs => kind::REPAIRS,
        }
    }

    /// The title it shows.
    #[must_use]
    pub const fn title(self) -> Label {
        match self {
            Self::Help(_) => Label::Slot(HELP_SLOT),
            Self::Readme => Label::Slot(README_SLOT),
            Self::Keys => Label::Slot(KEYS_SLOT),
            Self::Repairs => Label::Text("Save migrated"),
        }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size(self) -> (u32, u32) {
        match self {
            Self::Help(page) => match page.checked_sub(FIRST_LISTED_PAGE) {
                Some(listed) if (listed as usize) < LISTED_SIZES.len() => LISTED_SIZES[listed as usize],
                _ => UNLISTED_SIZE,
            },
            Self::Readme | Self::Keys => (440, 400),
            Self::Repairs => REPAIRS_SIZE,
        }
    }

    /// How far one line of its panel sits below the last (`0x59958`).
    #[must_use]
    pub const fn line_step(self) -> u32 {
        let (tall, short) = LINE_STEPS;
        match self {
            Self::Help(_) if HELP_FLAGS & TALL_LINES != 0 => tall,
            _ => short,
        }
    }

    /// Whether its panel carries the column of scroll buttons. A panel whose lines take the
    /// whole width carries none, which `0x59ba3` leaves the builder over.
    #[must_use]
    pub const fn scrolls(self) -> bool {
        !matches!(self.gutter(), Gutter::None)
    }

    /// How many characters a line of its panel gives up, which the flags the window passes pick.
    /// The file viewers pass the bit that takes four, which also leaves the button that goes
    /// back to the top out of the column (`0x59b50`).
    #[must_use]
    pub const fn gutter(self) -> Gutter {
        match self {
            Self::Help(_) => Gutter::of(HELP_FLAGS),
            Self::Readme | Self::Keys | Self::Repairs => Gutter::Four,
        }
    }
}

/// One of the buttons down the right side of a file viewer, in the order `0x59b50` builds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    /// A page back.
    PageUp,
    /// A line back.
    LineUp,
    /// Back to the top, which a panel with the narrow gutter leaves out.
    Top,
    /// A line on.
    LineDown,
    /// A page on.
    PageDown,
}

impl Scroll {
    /// Every one of them, in the order the original builds them.
    pub const ALL: [Self; 5] = [
        Self::PageUp,
        Self::LineUp,
        Self::Top,
        Self::LineDown,
        Self::PageDown,
    ];

    /// The picture it draws.
    #[must_use]
    pub const fn picture(self) -> Picture {
        match self {
            Self::PageUp | Self::LineUp => Picture::LINE_UP,
            Self::Top => Picture::TO_THE_TOP,
            Self::LineDown | Self::PageDown => Picture::LINE_DOWN,
        }
    }

    /// Where it takes a panel showing line `first`.
    #[must_use]
    pub const fn landing(self, first: usize) -> usize {
        let (page, line) = STEPS;
        match self {
            Self::PageUp => first.saturating_sub(page),
            Self::LineUp => first.saturating_sub(line),
            Self::Top => 0,
            Self::LineDown => first.saturating_add(line),
            Self::PageDown => first.saturating_add(page),
        }
    }
}

/// One of the screens that only show a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingWindow {
    /// Which of them it is.
    pub reading: Reading,
    /// The lines it holds.
    pub panel: TextPanel,
    /// How much of its width a line gives up.
    pub gutter: Gutter,
    /// The lines of the file past its end-of-file byte, which the original still counts when it
    /// sizes a help page.
    past_the_end: usize,
}

impl ReadingWindow {
    /// An empty window for `reading`.
    #[must_use]
    pub const fn new(reading: Reading) -> Self {
        Self {
            reading,
            panel: TextPanel::new(),
            gutter: reading.gutter(),
            past_the_end: 0,
        }
    }

    /// Where the panel sits inside a window of `area`: the client area.
    #[must_use]
    pub fn panel(reading: Reading, area: Rect) -> Rect {
        let inside = Chrome::inside(area);
        if reading.scrolls() {
            return inside;
        }
        // A panel with no column of buttons takes whole lines, and the rows left over at the
        // bottom stay as the frame drew them. Measured on S2: a help page of eight lines in a
        // window 173 tall ends its panel on row 160 of the window, one line short of the client
        // area, while the readme's panel fills its own.
        let step = reading.line_step();
        inside.with_height(inside.height / step * step)
    }

    /// Where the button for `scroll` sits in a window of `area`, or nothing for the one a panel
    /// with the narrow gutter leaves out.
    #[must_use]
    pub fn button(&self, scroll: Scroll, area: Rect) -> Option<Rect> {
        if !self.reading.scrolls() {
            return None;
        }
        let (right, top, step, size) = COLUMN;
        let skipped = self.gutter == Gutter::Four;
        if skipped && scroll == Scroll::Top {
            return None;
        }
        let mut place = Scroll::ALL.iter().position(|&one| one == scroll)?;
        if skipped && place > 1 {
            place -= 1;
        }
        let down = step * i32::try_from(place).unwrap_or_default();
        let across = i32::try_from(area.width).unwrap_or_default() - right;
        Some(Rect::new(area.x + across, area.y + top + down, size, size))
    }

    /// Takes `text` in, wrapping it to the width `area` leaves. A file that carries the DOS
    /// end-of-file marker ends there, which is where the pages the game ships end.
    pub fn read(&mut self, text: &[u8], area: Rect) {
        let panel = Self::panel(self.reading, area);
        let (text, past_the_end) = match text.iter().position(|&byte| byte == END_OF_FILE) {
            Some(end) => (&text[..end], newlines(&text[end..])),
            None => (text, 0),
        };
        self.past_the_end = past_the_end;
        self.panel.push(text, TextPanel::columns(panel, self.gutter));
    }

    /// How tall a help page's window ends up for the lines it holds, or `None` for a window
    /// that keeps the height its table gives. `0x59b14` counts every line the file wraps to,
    /// the blank one its last newline leaves among them. Read it after [`Self::read`].
    #[must_use]
    pub fn grown(&self) -> Option<u32> {
        if !matches!(self.reading, Reading::Help(_)) {
            return None;
        }
        let held = self.panel.lines().len() + self.past_the_end;
        let lines = u32::try_from(held).unwrap_or_default();
        (lines > 0).then(|| HELP_FRAME + self.reading.line_step() * lines)
    }

    /// Draws the window, one line per [`Reading::line_step`] pixels of the panel, and takes a press on
    /// the column of scroll buttons.
    pub fn run(&mut self, ui: &mut Ui, painter: &mut Painter, skin: &Skin, mouse: Pointer) {
        let Some(window) = ui.window(self.reading.kind()).copied() else {
            return;
        };
        let area = window.area;
        painter.push_clip(area);
        if window.rolled() {
            painter.pop_clip();
            return;
        }
        let panel = Self::panel(self.reading, area);
        if let Some(item) = skin.picture(Picture::TEXT_BACKDROP) {
            // The backdrop repeats from the window's own corner and shows only inside the panel,
            // the way a window's own background does.
            painter.push_clip(panel);
            painter.tile(item, area);
            painter.pop_clip();
        }
        let mut landed = None;
        for scroll in Scroll::ALL {
            let Some(place) = self.button(scroll, area) else {
                continue;
            };
            let slot = u16::try_from(scroll as usize).unwrap_or_default();
            let widget = WidgetId::new(self.reading.kind(), slot);
            let acted = ui.press(widget, place, mouse, true);
            let look = ui.button(widget).look(true, false);
            let face = if look == Look::Hover {
                style::FLAT.other
            } else {
                style::FLAT.face
            };
            painter.flat_frame((face, style::FLAT.edge), place, look.edges());
            if let Some(item) = skin.picture(scroll.picture()) {
                painter.item_centred(item, place);
            }
            if acted && landed.is_none() {
                landed = Some(scroll.landing(self.panel.first()));
            }
        }
        if let Some(line) = landed {
            self.panel.show_from(line);
        }
        painter.push_clip(panel);
        // `0x5996d` takes one off what the height divides into and leaves the last line a
        // panel could hold out. Showing it is one of the departures in docs/design/ui.md.
        let line_step = self.reading.line_step();
        let rows = panel.height.checked_div(line_step).unwrap_or_default() as usize;
        let step = i32::try_from(line_step).unwrap_or_default();
        let shown: Vec<Vec<u8>> = self.panel.shown(rows).to_vec();
        for (row, line) in shown.iter().enumerate() {
            let down = i32::try_from(row).unwrap_or_default() * step;
            let label = painter.run(line);
            let colour = painter.first_lettering().colour;
            painter.text_spaced(
                Point::new(panel.x, panel.y + down),
                PANEL_FONT,
                colour,
                label,
                Pen::cells(CELL),
            );
        }
        painter.pop_clip();
        painter.pop_clip();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_screen_has_its_own_size_and_title() {
        assert_eq!(Reading::Readme.size(), (440, 400));
        assert_eq!(Reading::Keys.size(), (440, 400));
        assert_eq!(Reading::Readme.title(), Label::Slot(172));
        assert_eq!(Reading::Keys.title(), Label::Slot(182));
        let kinds: Vec<u16> = Reading::ALL.map(|reading| reading.kind().0).to_vec();
        assert_eq!(kinds, [18, 19]);
    }

    #[test]
    fn a_help_page_takes_its_size_and_its_file_from_its_number() {
        assert_eq!(Reading::Help(1000).size(), (240, 120));
        assert_eq!(Reading::Help(1002).size(), (330, 270));
        assert_eq!(Reading::Help(1010).size(), (320, 100));
        assert_eq!(
            Reading::Help(1011).size(),
            (280, 164),
            "a page past the table takes the size of one below it"
        );
        assert_eq!(Reading::Help(7).size(), (280, 164));
        assert_eq!(Reading::Help(7).kind(), kind::HELP_PAGE);
        assert_eq!(Reading::Help(7).title(), Label::Slot(183));
        assert_eq!(
            Reading::Help(7).gutter(),
            Gutter::None,
            "its lines take the whole width"
        );
        assert_eq!(Reading::help_file(1002, false), "HELP1002.ENG");
        assert_eq!(Reading::help_file(7, true), "HELP0007.GER");
    }

    #[test]
    fn a_file_viewers_panel_fills_the_client_area() {
        let (width, height) = Reading::Readme.size();
        let viewer = Rect::new(0, 0, width, height);
        assert_eq!(
            ReadingWindow::panel(Reading::Readme, viewer),
            Rect::new(11, 21, 418, 368),
            "a file viewer's panel fills the client area"
        );
    }

    #[test]
    fn the_column_leaves_out_the_button_the_narrow_gutter_drops() {
        let (width, height) = Reading::Readme.size();
        let area = Rect::new(0, 0, width, height);
        let window = ReadingWindow::new(Reading::Readme);
        assert_eq!(window.gutter, Gutter::Four);
        assert_eq!(window.button(Scroll::Top, area), None);
        assert_eq!(
            window.button(Scroll::PageUp, area),
            Some(Rect::new(396, 24, 30, 30))
        );
        assert_eq!(
            window.button(Scroll::LineUp, area),
            Some(Rect::new(396, 56, 30, 30))
        );
        assert_eq!(
            window.button(Scroll::LineDown, area),
            Some(Rect::new(396, 88, 30, 30)),
            "the one left out takes its step with it"
        );
        assert_eq!(
            window.button(Scroll::PageDown, area),
            Some(Rect::new(396, 120, 30, 30))
        );
        assert_eq!(Scroll::PageUp.landing(20), 8);
        assert_eq!(Scroll::LineUp.landing(1), 0);
        assert_eq!(Scroll::PageDown.landing(0), 12);
    }

    #[test]
    fn a_window_wraps_what_it_reads_and_draws_a_row_per_line_step() {
        let (width, height) = Reading::Keys.size();
        let area = Rect::new(0, 0, width, height);
        let mut window = ReadingWindow::new(Reading::Keys);
        window.read(b"one two three\r\nfour", area);
        assert_eq!(window.panel.lines().len(), 2);
        let mut ui = Ui::new();
        ui.open(Reading::Keys.kind(), area);
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        window.run(&mut ui, &mut painter, &Skin::new(), Pointer::default());
        assert_eq!(painter.runs().len(), 2);
        assert_eq!(painter.runs().get(1), b"four");
    }

    #[test]
    fn a_help_page_holds_no_scroll_buttons_and_sits_its_lines_further_apart() {
        let page = Reading::Help(1002);
        let area = Rect::new(0, 0, page.size().0, page.size().1);
        let window = ReadingWindow::new(page);
        for scroll in Scroll::ALL {
            assert_eq!(window.button(scroll, area), None, "{scroll:?}");
        }
        assert_eq!(page.line_step(), 14);
        assert_eq!(Reading::Readme.line_step(), 11);
        assert!(Reading::Readme.scrolls());
        assert!(!page.scrolls());
        assert_eq!(
            ReadingWindow::panel(page, area),
            Chrome::inside(area),
            "its panel stops at the frame, which the window grows to leave room for"
        );
        let readme = Rect::new(0, 0, 440, 400);
        assert_eq!(
            ReadingWindow::panel(Reading::Readme, readme),
            Chrome::inside(readme),
            "a panel with the column of buttons stops at the frame"
        );
    }
}
