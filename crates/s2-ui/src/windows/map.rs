//! The map screen.
//!
//! The `l` key opens it. The window is the world's own size plus a margin, the picture inside is
//! one pixel per node, and the three buttons along the bottom turn the territory, building and
//! road overlays on and off. The fourth button swaps the window between that shape and a fixed
//! one.

use s2_render::colour_base;
use s2_render::terrain::Point;
use s2_sim::Player;

use crate::geometry::Rect;
use crate::paint::{Painter, Sheet};
use crate::picture::Picture;
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{Outcome, kind};

/// The palette index a node nobody has seen draws in.
pub const UNSEEN: u8 = 160;

/// The index a node a road leaves draws in.
pub const ROAD: u8 = 37;

/// The index a node holding a building draws in.
pub const BUILDING: u8 = 255;

/// The node count a map below it is drawn at instead, in both directions.
const LEAST_NODES: u16 = 144;

/// How much wider and taller the window is than its picture.
const MARGIN: (u32, u32) = (38, 88);

/// Where the picture's top left corner sits.
const PICTURE_AT: (i32, i32) = (19, 29);

/// The style record the three overlay buttons draw from (`0x1fbe1b`).
const OVERLAY_STYLE: u16 = 1;

/// The landscape set's marker, MAP 23: four corners round its origin. The map window puts it
/// over the part of the world the main view shows (`0x4a4fc`), and the observation window over
/// the node it watches.
pub const MARKER: u16 = 23;

/// The size the fourth button swaps to.
const SWAPPED: (u32, u32) = (262, 312);

/// Where the row of buttons sits: how far up from the window's bottom edge, the first button's
/// left edge, the step between them and their size.
const BUTTONS: (i32, i32, i32, u32) = (52, 19, 36, 36);

/// How far in from the window's right edge the fourth button sits.
const SWAP_FROM_RIGHT: i32 = 55;

/// The picture each of the three overlay buttons draws.
pub const OVERLAY_PICTURES: [Picture; 3] =
    [Picture::MAP_TERRITORY, Picture::MAP_BUILDINGS, Picture::MAP_ROADS];

/// The picture the button that swaps the size draws.
pub const SWAP_PICTURE: Picture = Picture::MAP_SIZE;

/// The widget slot the button that swaps the size takes, past the three overlay buttons.
const SWAP_WIDGET: u16 = 16;

/// Which overlays the picture draws over the terrain.
///
/// The original keeps them as bits of one word on the picture widget, and each of the three
/// buttons flips the bit its command names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overlays(u16);

impl Default for Overlays {
    fn default() -> Self {
        Self::new()
    }
}

impl Overlays {
    /// Every other node in its owner's colour.
    pub const TERRITORY: u16 = 1 << 0;
    /// Building nodes in white.
    pub const BUILDINGS: u16 = 1 << 1;
    /// Nodes a road leaves.
    pub const ROADS: u16 = 1 << 2;
    /// Nodes the player has not seen hidden.
    pub const FOG: u16 = 1 << 4;
    /// Each node's area index in place of everything else.
    pub const AREAS: u16 = 1 << 5;

    /// The three the buttons along the bottom carry, in button order.
    pub const BUTTONS: [u16; 3] = [Self::TERRITORY, Self::BUILDINGS, Self::ROADS];

    /// What a new picture starts with: the three buttons on and the fog with them.
    #[must_use]
    pub const fn new() -> Self {
        Self(Self::TERRITORY | Self::BUILDINGS | Self::ROADS | Self::FOG)
    }

    /// The set the word `bits` names.
    #[must_use]
    pub const fn of(bits: u16) -> Self {
        Self(bits)
    }

    /// The word behind it, which is what the original keeps.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Whether `overlay` is on.
    #[must_use]
    pub const fn shows(self, overlay: u16) -> bool {
        self.0 & overlay != 0
    }

    /// Turns `overlay` over.
    pub const fn toggle(&mut self, overlay: u16) {
        self.0 ^= overlay;
    }

    /// The same set without `overlay`.
    #[must_use]
    pub const fn without(self, overlay: u16) -> Self {
        Self(self.0 & !overlay)
    }

    /// The same set with `overlay`.
    #[must_use]
    pub const fn with(self, overlay: u16) -> Self {
        Self(self.0 | overlay)
    }
}

/// What the picture reads about one node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MapNode {
    /// Which player owns it, where one does.
    pub owner: Option<Player>,
    /// Whether the player the window belongs to has seen it.
    pub seen: bool,
    /// Whether a road leaves it.
    pub road: bool,
    /// Whether it holds a building.
    pub building: bool,
    /// The terrain's own colour, shaded as the main view shades it.
    pub terrain: u8,
    /// The node's area index, which only the overlay no button reaches shows.
    pub area: u8,
}

/// The colour one node draws in.
///
/// `odd` is whether the column and the row add up odd, which is where the territory overlay
/// draws. The steps run in the original's order, so a road draws over territory and a building
/// over both.
#[must_use]
pub fn colour(node: MapNode, overlays: Overlays, odd: bool) -> u8 {
    if overlays.shows(Overlays::AREAS) {
        return node.area;
    }
    if overlays.shows(Overlays::FOG) && !node.seen {
        return UNSEEN;
    }
    let territory = overlays.shows(Overlays::TERRITORY) && odd;
    let mut index = match node.owner.filter(|_| territory) {
        Some(owner) => colour_base(owner),
        None => node.terrain,
    };
    if overlays.shows(Overlays::ROADS) && node.road {
        index = ROAD;
    }
    if overlays.shows(Overlays::BUILDINGS) && node.building {
        index = BUILDING;
    }
    index
}

/// What a press on the map screen asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    /// One of the three overlays went on or off.
    Overlay(usize),
    /// The window swapped between its two sizes.
    Swap,
}

/// The map screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapWindow {
    /// How many nodes the world is across and down.
    pub nodes: (u16, u16),
    /// Which overlays are on.
    pub overlays: Overlays,
    /// The size the window had before it swapped, where it has swapped.
    pub kept: Option<(u32, u32)>,
    /// The node under the middle of the main view, where the marker goes.
    pub centre: Option<(u16, u16)>,
    /// The picture's pixels, one per node, filled by [`MapWindow::read`].
    pixels: Vec<u8>,
}

impl MapWindow {
    /// Which window kind it opens as.
    pub const KIND: WindowId = kind::MAP;

    /// A window for a world `nodes` across and down, with nothing drawn yet.
    #[must_use]
    pub fn new(nodes: (u16, u16)) -> Self {
        Self {
            nodes,
            overlays: Overlays::new(),
            kept: None,
            centre: None,
            pixels: Vec::new(),
        }
    }

    /// How many nodes the picture covers. A world under [`LEAST_NODES`] wide is drawn at that
    /// count in both directions, whatever its height.
    #[must_use]
    pub fn covered(&self) -> (u32, u32) {
        let (across, down) = self.nodes;
        if across < LEAST_NODES {
            (u32::from(LEAST_NODES), u32::from(LEAST_NODES))
        } else {
            (u32::from(across), u32::from(down))
        }
    }

    /// How big the window is. A swapped window takes the fixed size instead.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        if self.kept.is_some() {
            return SWAPPED;
        }
        let (across, down) = self.covered();
        (across + MARGIN.0, down + MARGIN.1)
    }

    /// Where the picture sits inside a window of `area`.
    #[must_use]
    pub fn picture(&self, area: Rect) -> Rect {
        let (across, down) = self.covered();
        let (left, top) = PICTURE_AT;
        Rect::new(area.x + left, area.y + top, across, down)
    }

    /// Where the marker's origin goes over `picture`: the centre node scaled from the world's
    /// node count to the picture's size, rounding down.
    #[must_use]
    pub fn marker(&self, picture: Rect) -> Option<Point> {
        let (x, y) = self.centre?;
        let scale = |node: u16, size: u32, nodes: u16| {
            let nodes = u32::from(nodes.max(1));
            i32::try_from(u32::from(node) * size / nodes).unwrap_or_default()
        };
        Some(Point::new(
            picture.x + scale(x, picture.width, self.nodes.0),
            picture.y + scale(y, picture.height, self.nodes.1),
        ))
    }

    /// Where one of the three overlay buttons sits inside a window of `area`.
    #[must_use]
    pub fn button(place: usize, area: Rect) -> Rect {
        let (up, left, step, side) = BUTTONS;
        let across = left + step * i32::try_from(place.min(OVERLAY_PICTURES.len() - 1)).unwrap_or_default();
        Rect::new(area.x + across, area.bottom() - up, side, side)
    }

    /// Where the button that swaps the size sits inside a window of `area`.
    #[must_use]
    pub fn swap(area: Rect) -> Rect {
        let (up, _, _, side) = BUTTONS;
        let right = i32::try_from(area.width).unwrap_or_default() - SWAP_FROM_RIGHT;
        Rect::new(area.x + right, area.bottom() - up, side, side)
    }

    /// Swaps the window between its map-sized shape and the fixed one, giving back the size it
    /// now has.
    pub fn swapped(&mut self, area: Rect) -> (u32, u32) {
        if let Some(kept) = self.kept.take() {
            return kept;
        }
        self.kept = Some((area.width, area.height));
        SWAPPED
    }

    /// Colours every pixel of the picture, taking each node from `node`.
    ///
    /// A picture as wide as the world is one pixel per node. A world under [`LEAST_NODES`] wide
    /// is drawn into a bigger picture, and the original takes the node from the column times the
    /// world's width over the picture's, and the row times its height over the picture's
    /// (`0x4a435`). Whether the territory shows is decided by the picture's own column and row.
    pub fn read<F: FnMut(u16, u16) -> MapNode>(&mut self, mut node: F) {
        let (across, down) = self.covered();
        let (width, height) = (u32::from(self.nodes.0), u32::from(self.nodes.1));
        self.pixels.clear();
        for row in 0..down {
            let y = u16::try_from(row * height / down.max(1)).unwrap_or_default();
            for column in 0..across {
                let x = u16::try_from(column * width / across.max(1)).unwrap_or_default();
                let odd = (column + row) % 2 == 1;
                self.pixels.push(colour(node(x, y), self.overlays, odd));
            }
        }
    }

    /// The picture's pixels, one per node, row by row.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Takes `mouse`, draws the window and gives back what a release asks for.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        mouse: Pointer,
        swappable: bool,
    ) -> Option<Outcome> {
        let window = *ui.window(Self::KIND)?;
        let area = window.area;
        painter.push_clip(area);
        if window.rolled() {
            painter.pop_clip();
            return None;
        }
        let picture = self.picture(area);
        if !self.pixels.is_empty() {
            let image = painter.picture(&self.pixels, picture.width, picture.height);
            painter.push_clip(picture);
            painter.image(image, Point::new(picture.x, picture.y));
            if let Some(at) = self.marker(picture) {
                painter.game(Sheet::Landscape, MARKER, at);
            }
            painter.pop_clip();
        }
        let mut asked = None;
        for (place, &overlay) in Overlays::BUTTONS.iter().enumerate() {
            let rect = Self::button(place, area);
            let widget = WidgetId::new(Self::KIND, u16::try_from(place).unwrap_or_default());
            let acted = ui.press(widget, rect, mouse, true);
            // The button turns before it draws, so the frame the click lands on already shows
            // the new state rather than the old one.
            if acted && asked.is_none() {
                self.overlays.toggle(overlay);
                asked = Some(Outcome::Map(Shown::Overlay(place)));
            }
            // The buttons show no on state: S2 draws the same face whichever overlays are on.
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.face(OVERLAY_STYLE, look) {
                painter.frame_edges(item, skin.edge(OVERLAY_STYLE), rect, look.edges());
            }
            if let Some(item) = skin.picture(OVERLAY_PICTURES[place]) {
                painter.item_centred(item, rect);
            }
        }
        if swappable {
            let rect = Self::swap(area);
            let widget = WidgetId::new(Self::KIND, SWAP_WIDGET);
            let acted = ui.press(widget, rect, mouse, true);
            if acted && asked.is_none() {
                self.swapped(area);
                asked = Some(Outcome::Map(Shown::Swap));
            }
            let look = ui.button(widget).look(true, self.kept.is_some());
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), rect, look.edges());
            }
            if let Some(item) = skin.picture(SWAP_PICTURE) {
                painter.item_centred(item, rect);
            }
        }
        painter.pop_clip();
        asked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ItemId;

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Tick, Look::Idle, ItemId(2));
        skin.set(Control::Button, Look::Idle, ItemId(3));
        skin
    }

    fn plain(x: u16, y: u16) -> MapNode {
        MapNode {
            seen: true,
            terrain: u8::try_from((x + y) % 7).unwrap_or_default() + 1,
            ..MapNode::default()
        }
    }

    #[test]
    fn the_window_is_the_world_plus_a_margin() {
        let window = MapWindow::new((176, 80));
        assert_eq!(window.covered(), (176, 80));
        assert_eq!(window.size(), (176 + 38, 80 + 88));
        let narrow = MapWindow::new((64, 32));
        assert_eq!(
            narrow.covered(),
            (144, 144),
            "a world under 144 wide is drawn at 144 both ways"
        );
        assert_eq!(narrow.size(), (144 + 38, 144 + 88));
    }

    #[test]
    fn the_picture_and_the_buttons_sit_where_the_original_puts_them() {
        let window = MapWindow::new((176, 80));
        let (width, height) = window.size();
        let area = Rect::new(0, 0, width, height);
        assert_eq!(window.picture(area), Rect::new(19, 29, 176, 80));
        assert_eq!(MapWindow::button(0, area), Rect::new(19, 116, 36, 36));
        assert_eq!(MapWindow::button(1, area), Rect::new(55, 116, 36, 36));
        assert_eq!(MapWindow::button(2, area), Rect::new(91, 116, 36, 36));
        assert_eq!(MapWindow::swap(area), Rect::new(159, 116, 36, 36));
    }

    #[test]
    fn a_small_world_is_stretched_over_the_picture() {
        let mut window = MapWindow::new((64, 32));
        let mut asked = Vec::new();
        window.read(|x, y| {
            asked.push((x, y));
            plain(x, y)
        });
        let (across, down) = window.covered();
        let pixels = (across * down) as usize;
        assert_eq!(asked.len(), pixels);
        assert_eq!(window.pixels().len(), pixels);
        assert_eq!(asked[0], (0, 0));
        assert_eq!(asked[1], (0, 0), "two pixels share the first node");
        assert_eq!(asked[144], (0, 0), "the second row is still node row 0");
        assert_eq!(asked[pixels - 1], (63, 31), "the last pixel is the last node");

        let mut same = MapWindow::new((176, 80));
        let mut nodes = Vec::new();
        same.read(|x, y| {
            nodes.push((x, y));
            plain(x, y)
        });
        assert_eq!(nodes[176], (0, 1), "one pixel to a node at full size");
    }

    #[test]
    fn a_road_draws_over_territory_and_a_building_over_both() {
        let node = MapNode {
            owner: Player::new(2),
            seen: true,
            road: true,
            building: true,
            terrain: 9,
            area: 4,
        };
        let all = Overlays::new();
        assert_eq!(
            all.bits(),
            (1 << 0) | (1 << 1) | (1 << 2) | (1 << 4),
            "the word the original starts the picture with"
        );
        assert_eq!(colour(node, all, true), BUILDING);
        let no_buildings = all.without(Overlays::BUILDINGS);
        assert_eq!(colour(node, no_buildings, true), ROAD);
        let plain_terrain = no_buildings.without(Overlays::ROADS);
        assert_eq!(
            colour(node, plain_terrain, true),
            colour_base(Player::new(2).expect("player 2 exists")),
            "territory draws on the odd nodes"
        );
        assert_eq!(
            colour(node, plain_terrain, false),
            9,
            "and the terrain shows through on the rest"
        );
    }

    #[test]
    fn what_has_not_been_seen_hides_and_the_area_overlay_beats_everything() {
        let hidden = MapNode {
            owner: Player::new(1),
            building: true,
            terrain: 9,
            area: 4,
            ..MapNode::default()
        };
        assert_eq!(colour(hidden, Overlays::new(), true), UNSEEN);
        let no_fog = Overlays::new().without(Overlays::FOG);
        assert_eq!(colour(hidden, no_fog, true), BUILDING);
        let areas = Overlays::new().with(Overlays::AREAS);
        assert_eq!(colour(hidden, areas, true), 4);
    }

    #[test]
    fn the_picture_holds_one_pixel_a_node() {
        let mut window = MapWindow::new((176, 80));
        window.read(plain);
        assert_eq!(window.pixels().len(), 176 * 80);
        assert_eq!(window.pixels()[0], 1, "the terrain at the first node");
    }

    #[test]
    fn a_press_turns_one_overlay_over_and_the_fourth_button_swaps_the_size() {
        let mut window = MapWindow::new((176, 80));
        let (width, height) = window.size();
        let area = Rect::new(0, 0, width, height);
        window.read(plain);
        let mut ui = Ui::new();
        ui.open(MapWindow::KIND, area);
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let rect = MapWindow::button(1, area);
        let on = Pointer::at(Point::new(rect.x + 1, rect.y + 1));
        window.run(&mut ui, &mut painter, &skin(), on.pressing(), true);
        painter.reset();
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), on.releasing(), true),
            Some(Outcome::Map(Shown::Overlay(1)))
        );
        assert!(!window.overlays.shows(Overlays::BUILDINGS));
        assert_eq!(painter.images().len(), 1, "the picture goes in once a frame");

        let rect = MapWindow::swap(area);
        let on = Pointer::at(Point::new(rect.x + 1, rect.y + 1));
        painter.reset();
        window.run(&mut ui, &mut painter, &skin(), on.pressing(), true);
        painter.reset();
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), on.releasing(), true),
            Some(Outcome::Map(Shown::Swap))
        );
        assert_eq!(window.kept, Some((width, height)));
        assert_eq!(window.size(), (262, 312));
    }

    #[test]
    fn a_rolled_up_window_draws_its_frame_and_takes_no_press() {
        let mut window = MapWindow::new((176, 80));
        let (width, height) = window.size();
        let area = Rect::new(0, 0, width, height);
        window.read(plain);
        let mut ui = Ui::new();
        ui.open(MapWindow::KIND, area);
        ui.roll(MapWindow::KIND);
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let rect = MapWindow::button(0, area);
        let on = Pointer::at(Point::new(rect.x + 1, rect.y + 1));
        window.run(&mut ui, &mut painter, &skin(), on.pressing(), true);
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), on.releasing(), true),
            None
        );
        assert!(window.overlays.shows(Overlays::TERRITORY));
        assert!(painter.images().is_empty());
    }

    #[test]
    fn the_marker_scales_the_centre_node_to_the_picture() {
        let mut window = MapWindow::new((64, 64));
        let picture = Rect::new(100, 50, 144, 144);
        assert_eq!(window.marker(picture), None);
        window.centre = Some((32, 10));
        assert_eq!(window.marker(picture), Some(Point::new(100 + 72, 50 + 22)));
    }

    #[test]
    fn a_low_memory_game_has_no_button_to_swap_with() {
        let mut window = MapWindow::new((176, 80));
        let (width, height) = window.size();
        let area = Rect::new(0, 0, width, height);
        let mut ui = Ui::new();
        ui.open(MapWindow::KIND, area);
        let mut painter = Painter::new(Rect::new(0, 0, 640, 480));
        let rect = MapWindow::swap(area);
        let on = Pointer::at(Point::new(rect.x + 1, rect.y + 1));
        window.run(&mut ui, &mut painter, &skin(), on.pressing(), false);
        assert_eq!(
            window.run(&mut ui, &mut painter, &skin(), on.releasing(), false),
            None
        );
        assert_eq!(window.kept, None);
    }
}
