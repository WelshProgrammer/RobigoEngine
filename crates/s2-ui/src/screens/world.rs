//! The World Campaign's world screen and its continents: the continent table (`0x1f9b2c` in the
//! Gold Edition's executable) with each one's map, picture, markers and links, the screen of kind 10
//! the player picks a continent on, and the line a won continent leads to. The game draws the
//! backdrop and the world picture; this module the markers and the controls over them.

use s2_render::terrain::Point;

use crate::geometry::Rect;
use crate::paint::{Align, ItemId, Painter};
use crate::skin::Skin;
use crate::store::Ui;
use crate::text::{Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};

/// The world screen's backdrop (`0x60381`'s Gold Edition copy, kind 10).
pub const BACKDROP: &str = "GFX/PICS/SETUP990.LBM";

/// The world picture and its mask, whose pixels hold each continent's number plus 1 and 0 for
/// the sea.
pub const PICTURE: &str = "GFX/PICS/WORLD.LBM";
/// See [`PICTURE`].
pub const MASK: &str = "GFX/PICS/WORLDMSK.LBM";

/// The pictures a won continent leads to: one while continents are left, one once all are won.
pub const WON_PICTURE: &str = "GFX/PICS/SETUP667.LBM";
/// See [`WON_PICTURE`].
pub const LAST_WON_PICTURE: &str = "GFX/PICS/SETUP666.LBM";

/// The markers: the chosen continent's cross and a won continent's headquarters.
pub const CROSS: &str = "iobob_03";
/// See [`CROSS`].
pub const HEADQUARTERS: &str = "iobob_04";

/// How far down the screen the world picture's top sits; across it is centred.
const PICTURE_TOP: i32 = 30;

/// The buttons: their text slots and corners, 160 by 20 in the red face (`0x5f6d5`).
const START: (u16, (i32, i32)) = (574, (240, 380));
const RETURN: (u16, (i32, i32)) = (354, (240, 410));
const BUTTON: (u32, u32) = (160, 20);
const RED: u16 = 1;

/// The lines a won continent leads to, in the won chapter's place (`0x5f532`).
const RULE_CONTINENT: u16 = 581;
const RULE_WORLD: u16 = 582;
const WON_AREA: (i32, i32, u32, u32) = (40, 420, 560, 40);
const WON_STYLE: u16 = 5;

/// The screen's width, which the world picture is centred across.
const SCREEN_WIDTH: i32 = 640;

/// The size of the table [`REMAP`]'s header comes before.
const REMAP_HEADER: usize = 4;

/// One continent of the World Campaign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continent {
    /// The continents a win here opens, by number.
    pub links: Vec<u8>,
    /// The map's file name in `DATA/MAPS2`.
    pub map: &'static str,
    /// Where the chosen continent's cross (`iobob_03`) stands, from the world picture's top left
    /// corner.
    pub cross: Point,
    /// Where a won continent's headquarters (`iobob_04`) stands, from the same corner.
    pub won: Point,
    /// The picture's file name in `GFX/PICS/MISSION`, which the loading screen shows.
    pub picture: &'static str,
}

/// The World Campaign's continents, by number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continents(Vec<Continent>);

impl Continents {
    /// The continent every new campaign opens on, and the only one open with nothing won.
    pub const FIRST: u8 = 0;

    /// The table in the Gold Edition's `S2.EXE`.
    #[must_use]
    pub fn original() -> Self {
        let continent = |links: &[u8], map, cross: (i32, i32), won: (i32, i32), picture| Continent {
            links: links.to_vec(),
            map,
            cross: Point::new(cross.0, cross.1),
            won: Point::new(won.0, won.1),
            picture,
        };
        Self(vec![
            continent(&[1, 4, 6], "EUROPE.WLD", (242, 96), (262, 94), "EUROPE.LBM"),
            continent(&[0, 7], "AFRICA.WLD", (243, 187), (223, 158), "AFRICA.LBM"),
            continent(&[4, 3], "NAMERICA.WLD", (63, 80), (55, 67), "NAMERICA.LBM"),
            continent(&[2], "SAMERICA.WLD", (119, 190), (120, 202), "SAMERICA.LBM"),
            continent(&[2, 0], "GREEN.WLD", (164, 47), (174, 25), "GREEN.LBM"),
            continent(&[7, 8], "AUSTRA.WLD", (429, 262), (460, 262), "AUSTRA.LBM"),
            continent(&[0, 8, 7], "NASIA.WLD", (366, 79), (403, 80), "NASIA.LBM"),
            continent(&[1, 6, 5], "SASIA.WLD", (361, 142), (395, 142), "SASIA.LBM"),
            continent(&[6, 5], "JAPAN.WLD", (467, 149), (482, 125), "JAPAN.LBM"),
        ])
    }

    /// Continent `number`, if the table has it.
    #[must_use]
    pub fn get(&self, number: u8) -> Option<&Continent> {
        self.0.get(usize::from(number))
    }

    /// How many continents there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The data path of continent `number`'s map.
    #[must_use]
    pub fn map_path(&self, number: u8) -> Option<String> {
        self.get(number)
            .map(|continent| format!("DATA/MAPS2/{}", continent.map))
    }

    /// The continent whose map is at data path `path`, in any case.
    #[must_use]
    pub fn by_map(&self, path: &str) -> Option<u8> {
        (0..self.0.len())
            .filter_map(|at| u8::try_from(at).ok())
            .find(|&number| {
                self.map_path(number)
                    .is_some_and(|map| map.eq_ignore_ascii_case(path))
            })
    }

    /// Whether the player may pick continent `number` with the continents `won` marks
    /// (`0x1738c`): the first continent always, a continent already won, and any continent a won
    /// continent links to.
    #[must_use]
    pub fn open(&self, number: u8, won: &[bool]) -> bool {
        let is_won = |at: u8| won.get(usize::from(at)).copied().unwrap_or(false);
        number == Self::FIRST
            || is_won(number)
            || (0..self.0.len())
                .filter_map(|at| u8::try_from(at).ok())
                .filter(|&at| at != number && is_won(at))
                .any(|at| self.get(at).is_some_and(|other| other.links.contains(&number)))
    }
}

/// What a press on the world screen asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldPick {
    /// An open continent, which becomes the chosen one.
    Continent(u8),
    /// Start: play the chosen continent.
    Start,
    /// Return: back to the main menu.
    Return,
}

/// What the world screen shows.
#[derive(Clone, Copy)]
pub struct WorldView<'a> {
    /// The continent table.
    pub continents: &'a Continents,
    /// The world picture's width and height.
    pub picture: (u32, u32),
    /// The mask's pixel at a point of the picture.
    pub mask: &'a dyn Fn(u32, u32) -> u8,
    /// Which continents are won, by number.
    pub won: &'a [bool],
    /// The chosen continent.
    pub chosen: u8,
    /// The cross and the headquarters marker.
    pub markers: (Option<ItemId>, Option<ItemId>),
}

impl std::fmt::Debug for WorldView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldView")
            .field("picture", &self.picture)
            .field("won", &self.won)
            .field("chosen", &self.chosen)
            .finish_non_exhaustive()
    }
}

/// The world screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldScreen {
    /// The id its buttons are kept under.
    pub id: WindowId,
}

impl Default for WorldScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldScreen {
    /// The screen, its buttons kept under [`crate::windows::kind::WORLD`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            id: crate::windows::kind::WORLD,
        }
    }

    /// Where the top-left corner of a world picture `width` wide sits on a screen whose corner is
    /// `at`.
    #[must_use]
    pub fn picture_corner(width: u32, at: Point) -> Point {
        let width = i32::try_from(width).unwrap_or(SCREEN_WIDTH);
        Point::new(at.x + (SCREEN_WIDTH - width) / 2, at.y + PICTURE_TOP)
    }

    /// Where Start and Return sit on a screen whose corner is `at`.
    #[must_use]
    pub fn button(pick: WorldPick, at: Point) -> Rect {
        let (_, (x, y)) = if pick == WorldPick::Return { RETURN } else { START };
        Rect::new(at.x + x, at.y + y, BUTTON.0, BUTTON.1)
    }

    /// Takes `mouse`, draws the markers and the buttons onto a screen whose corner is `at`, and
    /// gives back what a press asks for: a button on its release, an open continent as the
    /// button goes down.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &WorldView<'_>,
        at: Point,
        mouse: Pointer,
    ) -> Option<WorldPick> {
        let corner = Self::picture_corner(view.picture.0, at);
        let marked = |painter: &mut Painter, item: Option<ItemId>, point: Point| {
            if let Some(item) = item {
                painter.item(item, Point::new(corner.x + point.x, corner.y + point.y));
            }
        };
        let (cross, headquarters) = view.markers;
        for number in (0..view.continents.len()).filter_map(|at| u8::try_from(at).ok()) {
            if view.won.get(usize::from(number)).copied().unwrap_or(false)
                && let Some(continent) = view.continents.get(number)
            {
                marked(painter, headquarters, continent.won);
            }
        }
        if let Some(continent) = view.continents.get(view.chosen) {
            marked(painter, cross, continent.cross);
        }
        let mut picked = None;
        for (place, (pick, (text, _))) in [(WorldPick::Start, START), (WorldPick::Return, RETURN)]
            .into_iter()
            .enumerate()
        {
            let rect = Self::button(pick, at);
            let widget = WidgetId::new(self.id, u16::try_from(place).unwrap_or_default());
            let acted = ui.press(widget, rect, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.face(RED, look) {
                painter.frame_edges(item, skin.edge(RED), rect, look.edges());
            }
            if let Some(style) = painter.lettering(RED) {
                painter.text_aligned(
                    Point::new(rect.x, rect.y),
                    style.font,
                    style.colour,
                    Label::Slot(text),
                    Pen::TIGHT,
                    Align::CentredInBox(rect.width, rect.height),
                );
            }
            if acted && picked.is_none() {
                picked = Some(pick);
            }
        }
        picked.or_else(|| {
            mouse
                .pressed
                .then(|| Self::continent_at(view, corner, mouse.at))
                .flatten()
        })
    }

    /// The open continent under `point`, with the world picture's corner at `corner`.
    fn continent_at(view: &WorldView<'_>, corner: Point, point: Point) -> Option<WorldPick> {
        let x = u32::try_from(point.x - corner.x).ok()?;
        let y = u32::try_from(point.y - corner.y).ok()?;
        if x >= view.picture.0 || y >= view.picture.1 {
            return None;
        }
        let number = (view.mask)(x, y).checked_sub(1)?;
        view.continents
            .open(number, view.won)
            .then_some(WorldPick::Continent(number))
    }
}

/// The table in the bytes of `DATA/REMAP.DAT` (a 4-byte header, then an entry per palette
/// index, 0 meaning the pixel stays), or `None` when they are too short.
#[must_use]
pub fn remap_table(bytes: &[u8]) -> Option<[u8; 256]> {
    bytes.get(REMAP_HEADER..REMAP_HEADER + 256)?.try_into().ok()
}

/// Recolours the world picture's `pixels` where the `mask` of the same size names a continent
/// `closed` says is closed, through `table`, leaving a pixel whose entry is 0 as it is
/// (`0x61cc9`).
pub fn recolour(pixels: &mut [u8], mask: &[u8], table: &[u8; 256], closed: &dyn Fn(u8) -> bool) {
    for (pixel, &continent) in pixels.iter_mut().zip(mask) {
        if let Some(number) = continent.checked_sub(1)
            && closed(number)
        {
            let into = table[usize::from(*pixel)];
            if into != 0 {
                *pixel = into;
            }
        }
    }
}

/// The picture a won continent leads to, `all` once every continent is won.
#[must_use]
pub const fn won_picture(all: bool) -> &'static str {
    if all { LAST_WON_PICTURE } else { WON_PICTURE }
}

/// Draws the line a won continent leads to onto a screen whose corner is `at`, `all` once every
/// continent is won.
pub fn draw_won(painter: &mut Painter, all: bool, at: Point) {
    let (x, y, width, height) = WON_AREA;
    if let Some(lettering) = painter.lettering(WON_STYLE) {
        painter.text_aligned(
            Point::new(at.x + x, at.y + y),
            lettering.font,
            lettering.colour,
            Label::Slot(if all { RULE_WORLD } else { RULE_CONTINENT }),
            Pen::TIGHT,
            Align::CentredInBox(width, height),
        );
    }
}

impl Default for Continents {
    fn default() -> Self {
        Self::original()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_centres_across_and_closed_continents_take_the_table() {
        assert_eq!(
            WorldScreen::picture_corner(512, Point::new(0, 0)),
            Point::new(64, 30)
        );
        let mut table = [0u8; 256];
        table[5] = 9;
        let mut pixels = [5, 5, 6, 5];
        recolour(&mut pixels, &[1, 2, 2, 0], &table, &|number| number == 1);
        assert_eq!(pixels, [5, 9, 6, 5], "an entry of 0 and the sea stay");
    }

    #[test]
    fn a_press_on_an_open_continent_picks_it_and_a_closed_one_does_nothing() {
        let continents = Continents::original();
        let mask = |x: u32, _y: u32| if x < 10 { 1 } else { 3 };
        let won = [false; 9];
        let view = WorldView {
            continents: &continents,
            picture: (20, 20),
            mask: &mask,
            won: &won,
            chosen: 0,
            markers: (None, None),
        };
        let mut screen = WorldScreen::new();
        let mut ui = Ui::new();
        let mut press = |x| {
            let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
            let mouse = Pointer::at(Point::new(310 + x, 35)).pressing();
            screen.run(
                &mut ui,
                &mut painter,
                &Skin::new(),
                &view,
                Point::new(0, 0),
                mouse,
            )
        };
        assert_eq!(press(0), Some(WorldPick::Continent(0)));
        assert_eq!(press(15), None, "North America is closed");
    }

    #[test]
    fn every_link_is_named_back() {
        let table = Continents::original();
        for number in 0..u8::try_from(table.len()).unwrap() {
            for &link in &table.get(number).unwrap().links {
                assert!(
                    table.get(link).unwrap().links.contains(&number),
                    "{number} and {link}"
                );
            }
        }
    }

    #[test]
    fn a_new_campaign_opens_europe_alone_and_a_win_opens_its_links() {
        let table = Continents::original();
        let none = [false; 9];
        let opened: Vec<u8> = (0..9).filter(|&n| table.open(n, &none)).collect();
        assert_eq!(opened, [0]);
        let mut won = none;
        won[0] = true;
        won[1] = true;
        let opened: Vec<u8> = (0..9).filter(|&n| table.open(n, &won)).collect();
        assert_eq!(
            opened,
            [0, 1, 4, 6, 7],
            "as the capture shows them in their own colours"
        );
    }
}
