//! The credits: pages of text over their own picture, a portrait on some, and a crowd of
//! settlers walking along the foot of the screen.

use s2_render::terrain::Point;
use s2_sim::GameRng;

use crate::geometry::Rect;
use crate::paint::Painter;
use crate::panel::{Gutter, TextPanel};
use crate::text::Pen;
use crate::windows::reading::PANEL_FONT;

/// The picture every page shows (`0x60454`).
pub const PICTURE: &str = "GFX/PICS2/CREDIT00.LBM";

/// The palette the screen shows in (`0x5e84e`).
pub const PALETTE: &str = "GFX/PALETTE/PAL5.BBM";

/// The file the portraits come from.
pub const PORTRAITS: &str = "DATA/CREDITS.LST";

/// How many pages `S2.EXE` shows.
const ORIGINAL_PAGES: u16 = 21;

/// How many pages there are: the original's, and Robigo's own before the original's last.
pub const PAGES: u16 = ORIGINAL_PAGES + OWN;

/// How many pages Robigo adds.
const OWN: u16 = 2;

/// How many ticks of the 60 Hz counter a page before the last shows for (`0x1fd3d4`). The last
/// stays until a key leaves.
pub const PAGE_TICKS: u32 = 500;

/// How many ticks a second the counter makes.
pub const TICKS_A_SECOND: u32 = 60;

/// Where a page's text panel sits on the screen (`0x60812`).
const TEXT_AREA: Rect = Rect::new(100, 140, 440, 200);

/// The flags the text panel is built with (`0x6088f`): style 3 and the taller line step, with
/// four characters of each line given up.
const TEXT_FLAGS: u8 = 11;

/// How far one line of the text sits below the last.
const LINE_STEP: u32 = 14;

/// The cell every glyph of a line sits in.
const CELL: u16 = 8;

/// Where a page's portrait sits on the screen (`0x60b03`). The portrait's own middle goes on the
/// area's middle and what falls outside is cut off.
pub const PORTRAIT_AREA: Rect = Rect::new(380, 60, 200, 260);

/// The item of [`PORTRAITS`] each page shows (`0x1fd3aa`).
const PORTRAIT_ITEMS: [Option<u16>; ORIGINAL_PAGES as usize] = [
    None,
    Some(4),
    Some(3),
    Some(1),
    Some(0),
    Some(5),
    Some(2),
    Some(8),
    Some(9),
    Some(10),
    Some(7),
    Some(6),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
];

/// Where a page's text comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Words {
    /// One of the original's files, by its number.
    File(u16),
    /// Robigo's own, laid out as the original's files are.
    Own(&'static [u8]),
}

/// The picture a page shows beside its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Portrait {
    /// An item of [`PORTRAITS`].
    Item(u16),
    /// Robigo's author, a picture the game carries itself.
    Author,
}

/// One page of the credits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    /// Its text.
    pub words: Words,
    /// Its portrait, if it shows one.
    pub portrait: Option<Portrait>,
}

/// The pages Robigo adds for itself, shown before the original's last: a heading as the original
/// opens with, then a page each for who made it.
const OWN_PAGES: [Page; OWN as usize] = [
    Page {
        words: Words::Own(b"Robigo was created by:"),
        portrait: None,
    },
    Page {
        words: Words::Own(
            b"WELSHPROGRAMMER\r\n\r\n\r\n\r\nProject manager\r\n\r\nProgramming\r\n\r\nReverse engineering",
        ),
        portrait: Some(Portrait::Author),
    },
];

/// Page `number` of the credits, from 0: the original's pages in their order with Robigo's own
/// before the last, which stays until a key leaves and would hide anything after it.
#[must_use]
pub fn page(number: u16) -> Option<Page> {
    let last = ORIGINAL_PAGES - 1;
    let own = usize::from(number.checked_sub(last).unwrap_or(u16::MAX));
    let original = match OWN_PAGES.get(own) {
        Some(&page) => return Some(page),
        None if number < last => number,
        None if number + 1 == PAGES => last,
        None => return None,
    };
    Some(Page {
        words: Words::File(original),
        portrait: PORTRAIT_ITEMS
            .get(usize::from(original))
            .copied()
            .flatten()
            .map(Portrait::Item),
    })
}

/// The file an original page of `number` reads its text from, for a German build or any other
/// one (`0x60aaa`).
#[must_use]
pub fn text_file(number: u16, german: bool) -> String {
    let ending = if german { "GER" } else { "ENG" };
    format!("DATA/TXT2/CREDIT{number:02}.{ending}")
}

/// Draws `text`, a page's file, in the page's panel on a screen whose corner is at `at`.
///
/// The panel breaks the text at each CR LF and wraps what is between to its columns, as the
/// readme's does. Some pages end their lines in pairs of CR instead, which the panel counts as
/// characters and the drawer breaks at: a CR or LF starts a new line and takes one more CR or LF
/// after it along. Each line, the panel's or the drawer's, sits one step below the last. Nothing
/// draws outside the panel, which cuts the column a wide first letter reaches left into.
pub fn draw_text(painter: &mut Painter, text: &[u8], at: Point) {
    let mut panel = TextPanel::new();
    panel.push(text, TextPanel::columns(TEXT_AREA, Gutter::of(TEXT_FLAGS)));
    let rows = (TEXT_AREA.height / LINE_STEP) as usize;
    let step = i32::try_from(LINE_STEP).unwrap_or_default();
    let colour = painter.first_lettering().colour;
    let lines: Vec<Vec<u8>> = panel.shown(rows).to_vec();
    let mut down = 0;
    painter.push_clip(Rect::new(
        at.x + TEXT_AREA.x,
        at.y + TEXT_AREA.y,
        TEXT_AREA.width,
        TEXT_AREA.height,
    ));
    for piece in lines.iter().flat_map(|line| pieces(line)) {
        let label = painter.run(piece);
        painter.text_spaced(
            Point::new(at.x + TEXT_AREA.x, at.y + TEXT_AREA.y + down),
            PANEL_FONT,
            colour,
            label,
            Pen::cells(CELL),
        );
        down += step;
    }
    painter.pop_clip();
}

/// `line` cut where the drawer breaks it: at each CR or LF, which takes a CR or LF straight
/// after it along.
fn pieces(line: &[u8]) -> Vec<&[u8]> {
    let breaks = |byte: u8| matches!(byte, b'\r' | b'\n');
    let mut cut = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.iter().position(|&byte| breaks(byte)) {
        cut.push(&rest[..at]);
        let paired = rest.get(at + 1).is_some_and(|&byte| breaks(byte));
        rest = &rest[at + 1 + usize::from(paired)..];
    }
    cut.push(rest);
    cut
}

/// Where the crowd's widget sits on the screen (`0x60219`).
pub const CROWD_AREA: Rect = Rect::new(0, 320, 640, 160);

/// How many settlers the crowd holds.
const SETTLERS: u16 = 400;

/// A settler's x is kept in this many parts of a pixel.
const PARTS: i32 = 256;

/// The slowest a settler walks, in parts of a pixel a step, and how much faster the fastest does.
const SLOWEST: u16 = 192;
const SPEED_SPREAD: u16 = 128;

/// How far down the widget the first settler walks. The rest are spread evenly from there to the
/// widget's foot.
const FIRST_ROW: u32 = 30;

/// The x past which a settler walking right turns, and the x below which one walking left does.
const RIGHT_TURN: i32 = 650 * PARTS;
const LEFT_TURN: i32 = -10 * PARTS;

/// The longest a settler waits before first appearing, in steps.
const LONGEST_WAIT: u16 = 8000;

/// How many walking frames there are.
const FRAMES: u8 = 8;

/// The draws a look takes: the last job and nation of a worker, the last ware of a carrier, and
/// the draw of 0 to 100 a carrier is fat above.
const LAST_JOB: u16 = 29;
const LAST_NATION: u16 = 3;
const LAST_WARE: u16 = 30;
const FAT_DRAW: u16 = 100;
const FAT_ABOVE: u16 = 75;

/// The last player colour.
const LAST_COLOUR: u16 = 7;

/// Where the player colours' ramps start in the palette, and how many shades each holds.
const FIRST_RAMP: u8 = 128;
const RAMP: u8 = 4;

/// What a settler of the crowd is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// A carrier from `CARRIER.BOB` with a ware.
    Carrier {
        /// The ware's number.
        ware: u8,
        /// Whether it has the fat body.
        fat: bool,
    },
    /// A worker from `JOBS.BOB`, looking as the nation's look table gives the job.
    Worker {
        /// The job's number.
        job: u8,
        /// The nation, 0 Romans to 3.
        nation: u8,
    },
}

/// One settler of the crowd.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settler {
    /// Its x from the widget's left edge, in 256ths of a pixel.
    x: i32,
    /// How far it walks a step.
    speed: u16,
    /// Its y from the widget's top.
    pub y: u16,
    /// Whether it walks left.
    pub leftwards: bool,
    /// Its walking frame, 0 to 7.
    pub frame: u8,
    /// What it is drawn as.
    pub look: Look,
    /// Its player colour, 0 to 7.
    pub colour: u8,
    /// Steps left before it first appears.
    wait: u16,
}

impl Settler {
    /// The first index of the colour ramp its player-colour pixels take.
    #[must_use]
    pub const fn colour_base(&self) -> u8 {
        FIRST_RAMP + RAMP * self.colour
    }

    /// Its x from the widget's left edge in pixels, rounded towards 0.
    #[must_use]
    pub const fn x(&self) -> i32 {
        self.x / PARTS
    }
}

/// The settlers walking along the foot of the credits (`0x60d24`, `0x6111c`).
#[derive(Debug, Clone)]
pub struct Crowd {
    rng: GameRng,
    settlers: Vec<Settler>,
}

impl Crowd {
    /// A crowd whose generator starts from `start`, which `S2.EXE` takes from its tick count.
    /// Every settler waits off the left edge.
    #[must_use]
    pub fn new(start: u32) -> Self {
        let mut rng = GameRng::new(start);
        let spread = CROWD_AREA.height - FIRST_ROW;
        let settlers = (0..SETTLERS)
            .map(|index| {
                let speed = SLOWEST + rng.up_to(SPEED_SPREAD);
                let row = FIRST_ROW + spread * u32::from(index) / u32::from(SETTLERS);
                let frame = low_byte(rng.up_to(u16::from(FRAMES - 1)));
                let look = draw_look(&mut rng);
                let colour = low_byte(rng.up_to(LAST_COLOUR));
                Settler {
                    x: -1,
                    speed,
                    y: u16::try_from(row).unwrap_or_default(),
                    leftwards: false,
                    frame,
                    look,
                    colour,
                    wait: rng.up_to(LONGEST_WAIT),
                }
            })
            .collect();
        Self { rng, settlers }
    }

    /// One step: every settler that has appeared walks on by its speed and takes its next
    /// frame, and one that reaches its turn goes back the other way at a new speed with a new
    /// look. The rest count their wait down.
    pub fn step(&mut self) {
        for settler in &mut self.settlers {
            if settler.wait > 0 {
                settler.wait -= 1;
                continue;
            }
            let reach = i32::from(settler.speed);
            let turned = if settler.leftwards {
                settler.x -= reach;
                settler.x < LEFT_TURN
            } else {
                settler.x += reach;
                settler.x > RIGHT_TURN
            };
            if turned {
                settler.leftwards = !settler.leftwards;
                settler.speed = SLOWEST + self.rng.up_to(SPEED_SPREAD);
                settler.look = draw_look(&mut self.rng);
            }
            settler.frame = (settler.frame + 1) % FRAMES;
        }
    }

    /// The settlers that have appeared, in the order they are drawn.
    pub fn shown(&self) -> impl Iterator<Item = &Settler> {
        self.settlers.iter().filter(|settler| settler.wait == 0)
    }
}

/// Draws what a settler looks like: a carrier or a worker, then the two draws either takes.
fn draw_look(rng: &mut GameRng) -> Look {
    if rng.up_to(1) == 0 {
        let ware = low_byte(rng.up_to(LAST_WARE));
        let fat = rng.up_to(FAT_DRAW) > FAT_ABOVE;
        Look::Carrier { ware, fat }
    } else {
        let job = low_byte(rng.up_to(LAST_JOB));
        let nation = low_byte(rng.up_to(LAST_NATION));
        Look::Worker { job, nation }
    }
}

/// A draw that fits a byte, as one.
fn low_byte(draw: u16) -> u8 {
    u8::try_from(draw).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_originals_pages_keep_their_order_with_robigos_own_before_the_last() {
        let portrait = |number| page(number).and_then(|page| page.portrait);
        assert_eq!(page(0).map(|page| page.words), Some(Words::File(0)));
        assert_eq!(portrait(0), None);
        assert_eq!(portrait(1), Some(Portrait::Item(4)));
        assert_eq!(portrait(11), Some(Portrait::Item(6)));
        assert_eq!(portrait(12), None);
        assert_eq!(page(19).map(|page| page.words), Some(Words::File(19)));
        assert_eq!(portrait(21), Some(Portrait::Author));
        assert_eq!(portrait(22), None);
        assert_eq!(page(22).map(|page| page.words), Some(Words::File(20)));
        assert_eq!((PAGES, page(PAGES)), (23, None));
        assert_eq!((0..PAGES).filter_map(portrait).count(), 12);
        assert_eq!(text_file(7, false), "DATA/TXT2/CREDIT07.ENG");
        assert_eq!(text_file(20, true), "DATA/TXT2/CREDIT20.GER");
    }

    #[test]
    fn a_line_breaks_at_each_cr_or_lf_and_a_pair_breaks_once() {
        assert_eq!(pieces(b"plain"), [&b"plain"[..]]);
        assert_eq!(pieces(b"a\r\rb"), [&b"a"[..], b"b"]);
        assert_eq!(pieces(b"a\r\r\rb"), [&b"a"[..], b"", b"b"]);
        assert_eq!(pieces(b"a\r\r\r\rb\r "), [&b"a"[..], b"", b"b", b" "]);
    }

    #[test]
    fn the_crowd_starts_off_the_left_edge_spread_down_the_widget() {
        let crowd = Crowd::new(1);
        assert_eq!(crowd.settlers.len(), 400);
        assert_eq!(crowd.settlers[0].y, 30);
        assert_eq!(crowd.settlers[200].y, 95);
        assert_eq!(crowd.settlers[399].y, 159);
        for settler in &crowd.settlers {
            assert_eq!(settler.x(), 0);
            assert!((192..=320).contains(&settler.speed));
            assert!(settler.frame < 8 && settler.colour < 8 && settler.wait <= 8000);
            match settler.look {
                Look::Carrier { ware, .. } => assert!(ware <= 30),
                Look::Worker { job, nation } => assert!(job <= 29 && nation <= 3),
            }
        }
        assert_eq!(Crowd::new(1).settlers, crowd.settlers, "the seed decides it");
        assert_ne!(Crowd::new(2).settlers, crowd.settlers);
    }

    #[test]
    fn a_settler_waits_walks_right_and_turns_back_with_a_new_look() {
        let mut crowd = Crowd::new(7);
        crowd.settlers.truncate(1);
        crowd.settlers[0].wait = 2;
        crowd.settlers[0].speed = 256;
        let colour = crowd.settlers[0].colour;
        let frame = crowd.settlers[0].frame;
        crowd.step();
        crowd.step();
        assert_eq!(crowd.shown().count(), 1);
        assert_eq!((crowd.settlers[0].x(), crowd.settlers[0].frame), (0, frame));
        crowd.step();
        assert_eq!(crowd.settlers[0].x(), 0, "from a 256th left of the edge");
        assert_eq!(crowd.settlers[0].frame, (frame + 1) % 8);
        for _ in 0..650 {
            crowd.step();
        }
        assert_eq!(crowd.settlers[0].x(), 650);
        assert!(crowd.settlers[0].leftwards, "it turned past 650 pixels");
        assert_eq!(crowd.settlers[0].colour, colour);
        while crowd.settlers[0].leftwards {
            crowd.step();
        }
        assert_eq!(crowd.settlers[0].x(), -10);
    }
}
