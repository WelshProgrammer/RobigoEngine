//! The painter: indexed-colour operations and the pixels they draw.
//!
//! A window emits operations rather than touching pixels, so the same list can be drawn into a
//! frame or asserted in a test. Coordinates stay on the original's pixel grid and the scaling
//! happens later over the whole frame, which keeps the arithmetic here in integers.

use std::sync::Arc;

use s2_assets::{Remap, Sprite, SpriteSheet};
use s2_render::terrain::{Point, Surface};
use s2_render::{Paint, blit};

use crate::geometry::Rect;
use crate::image::{Image, ImageId, Images};
use crate::style::{Lettering, Styles};
use crate::text::{FontId, Fonts, Glyphs, Label, Labels, Pen, Runs};

/// An item in the interface sheet loaded from `IO.DAT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemId(pub u16);

/// Where an item sits in the rectangle it is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Place {
    /// In the middle of it, which is where a control's picture sits.
    Centre,
    /// In its top-left corner.
    TopLeft,
    /// In its top-right corner, with the item's own right edge on the rectangle's.
    TopRight,
}

/// Which pair of edges one call to the original's frame routine draws, where the other two are
/// left as the face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Edges {
    /// The top and the left, which is what a control at rest shows.
    #[default]
    Lit,
    /// The right and the bottom, which is what a bar shows.
    Shaded,
}

/// How a run of text sits against the point it is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// The point is the first glyph's origin.
    Left,
    /// The point is the middle of the run, which starts half the run's width to its left
    /// (`0x22f6d`).
    Centre,
    /// The point is one past the run's last pixel.
    Right,
    /// Half the width less half the run, each halved on its own, which is the arithmetic
    /// `0x22f6d` does for a message and the window titles do for their own bars.
    CentredIn(u32),
    /// Half of what the run leaves over in a rectangle of that size, across and down, and one
    /// pixel further down, which is the arithmetic a label or a button does for its one line
    /// (`0x4deae`, `0x4f412`).
    CentredInBox(u32, u32),
    /// Half of what the run leaves over across a span that wide, rounded toward zero, which is
    /// what a drawing context's centred line does (`0x509cc`).
    Across(u32),
}

/// What one operation draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    /// An `IO.DAT` item with its origin at `at`. Player-colour shades draw as `base` plus the
    /// shade, which leaves `base` at 0 for the chrome.
    Item {
        /// Which item.
        item: ItemId,
        /// Where its origin lands.
        at: Point,
        /// The first index of the colour ramp player-colour shades add to.
        base: u8,
    },
    /// An item placed against a rectangle, which is how a control's picture sits inside the
    /// rectangle its record gives it and how a window's title bar ends sit against its edges.
    /// The sizes are only known once the sheet is to hand, so the position is worked out when
    /// the op is drawn.
    ItemIn {
        /// Which item.
        item: ItemId,
        /// The rectangle it sits against.
        within: Rect,
        /// Where in that rectangle it lands.
        place: Place,
        /// The first index of the colour ramp player-colour shades add to.
        base: u8,
    },
    /// An item repeated across a rectangle, which is how a control's face and a window's frame
    /// are drawn. The first copy sits at the rectangle's top-left corner and the rest follow at the
    /// item's own size.
    Tile {
        /// Which item.
        item: ItemId,
        /// The rectangle it fills.
        within: Rect,
    },
    /// A picture from one of the game's own archives rather than from `IO.DAT`, drawn with its
    /// origin at `at` ([`crate::icons`]).
    Game {
        /// Which archive.
        sheet: Sheet,
        /// Which item of it.
        slot: u16,
        /// Where its origin lands.
        at: Point,
    },
    /// A shadow from one of the game's own archives, drawn through the canvas's darkening table
    /// the way the world draws one.
    Shadow {
        /// Which archive.
        sheet: Sheet,
        /// Which item of it.
        slot: u16,
        /// Where its origin lands.
        at: Point,
    },
    /// A background with a strip of another item around it, which is how a window and a control
    /// with a frame are drawn. Both items repeat the way [`OpKind::Tile`] does.
    Frame {
        /// The item the background repeats.
        face: ItemId,
        /// The item the strip around it repeats, where the skin has one.
        edge: Option<ItemId>,
        /// The rectangle the two cover.
        within: Rect,
        /// Which pair of edges the strip covers.
        pair: Edges,
    },
    /// A rectangle of one palette index.
    Fill {
        /// Which pixels.
        rect: Rect,
        /// The index to write.
        index: u8,
    },
    /// A rectangle sent through the table a marked control's background takes, which the
    /// original applies over the background and under the picture (`0x51898`).
    Lighten {
        /// Which pixels.
        rect: Rect,
    },
    /// A rectangle sent through the table a disabled control goes through once it is drawn
    /// (`0x4d8c0`).
    Disable {
        /// Which pixels.
        rect: Rect,
    },
    /// A rectangle sent through the shadow table, which is how a label with the fill flag darkens
    /// what is under it before its text goes on.
    Darken {
        /// Which pixels.
        rect: Rect,
    },
    /// A run of text with its first glyph's origin at `at`.
    Text {
        /// Where the run starts, and the column a line break goes back to.
        at: Point,
        /// Which font.
        font: FontId,
        /// The first index of the colour ramp the glyph shades add to.
        colour: u8,
        /// What it says.
        label: Label,
        /// How the glyphs are spaced.
        pen: Pen,
        /// Where the run sits against `at`.
        align: Align,
    },
    /// A picture a window worked out itself, with its top left corner at `at`.
    Image {
        /// Which picture.
        image: ImageId,
        /// Where its top left corner lands.
        at: Point,
    },
    /// A line of one palette index, drawn as `0x83cf4` draws it.
    Line {
        /// Where it starts.
        from: Point,
        /// Where it ends, the last pixel included.
        to: Point,
        /// The index to write.
        index: u8,
    },
}

/// One drawing operation, with the clip rectangle that was in force when it was pushed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Op {
    /// Pixels outside this rectangle are left alone.
    pub clip: Rect,
    /// What to draw.
    pub kind: OpKind,
}

/// Collects the operations a frame's windows draw.
///
/// The clip stack is resolved as operations are pushed, so each operation carries the rectangle
/// it is held inside and the list can be drawn in any order.
#[derive(Debug, Clone)]
pub struct Painter {
    ops: Vec<Op>,
    clips: Vec<Rect>,
    runs: Runs,
    images: Images,
    styles: Arc<Styles>,
}

impl Painter {
    /// A painter that clips everything to `bounds`.
    #[must_use]
    pub fn new(bounds: Rect) -> Self {
        Self {
            ops: Vec::new(),
            clips: vec![bounds],
            runs: Runs::new(),
            images: Images::new(),
            styles: Arc::new(Styles::ORIGINAL),
        }
    }

    /// The painter, giving text the fonts and colours of `styles`.
    #[must_use]
    pub fn with_styles(mut self, styles: Arc<Styles>) -> Self {
        self.styles = styles;
        self
    }

    /// What window style `style`'s text draws in, which is nothing for [`crate::NO_FRAME`] and for
    /// anything past the table.
    #[must_use]
    pub fn lettering(&self, style: u16) -> Option<Lettering> {
        self.styles.lettering(style)
    }

    /// What the text of style 0, the one most controls take, draws in.
    #[must_use]
    pub fn first_lettering(&self) -> Lettering {
        self.styles.first()
    }

    /// Drops every operation and every pushed clip, keeping the bounds the painter was made
    /// with so the buffers can be reused between frames.
    pub fn reset(&mut self) {
        self.ops.clear();
        self.clips.truncate(1);
        self.runs.clear();
        self.images.clear();
    }

    /// Keeps `text` for this frame and gives back the label that draws it.
    pub fn run(&mut self, text: &[u8]) -> Label {
        self.runs.add(text)
    }

    /// The runs pushed this frame, which [`paint`] reads a [`Label::Run`] from.
    #[must_use]
    pub fn runs(&self) -> &Runs {
        &self.runs
    }

    /// Keeps a `width` by `height` picture for this frame and gives back what draws it.
    pub fn picture(&mut self, pixels: &[u8], width: u32, height: u32) -> ImageId {
        self.images.add(pixels, width, height)
    }

    /// The pictures pushed this frame, which [`paint`] reads an [`OpKind::Image`] from.
    #[must_use]
    pub fn images(&self) -> &Images {
        &self.images
    }

    /// The clip rectangle in force.
    #[must_use]
    pub fn clip(&self) -> Rect {
        self.clips.last().copied().unwrap_or(Rect::EMPTY)
    }

    /// Clips the operations that follow to `rect` as well, which can only shrink the area.
    pub fn push_clip(&mut self, rect: Rect) {
        let clip = self.clip().intersect(rect);
        self.clips.push(clip);
    }

    /// Returns to the clip in force before the last [`Painter::push_clip`]. The bounds the
    /// painter was made with always remain.
    pub fn pop_clip(&mut self) {
        if self.clips.len() > 1 {
            self.clips.pop();
        }
    }

    /// Draws `item` with its origin at `at`.
    pub fn item(&mut self, item: ItemId, at: Point) {
        self.push(OpKind::Item { item, at, base: 0 });
    }

    /// Draws `item` with player-colour shades added to `base`.
    pub fn item_coloured(&mut self, item: ItemId, at: Point, base: u8) {
        self.push(OpKind::Item { item, at, base });
    }

    /// Fills `within` with `item`, repeating it from the rectangle's top-left corner.
    pub fn tile(&mut self, item: ItemId, within: Rect) {
        let clip = self.clip().intersect(within);
        if !clip.is_empty() {
            self.ops.push(Op {
                clip,
                kind: OpKind::Tile { item, within },
            });
        }
    }

    /// Draws item `slot` of `sheet` with its origin at `at`.
    pub fn game(&mut self, sheet: Sheet, slot: u16, at: Point) {
        self.push(OpKind::Game { sheet, slot, at });
    }

    /// Draws shadow item `slot` of `sheet` with its origin at `at`, darkening what is under it.
    pub fn shadow(&mut self, sheet: Sheet, slot: u16, at: Point) {
        self.push(OpKind::Shadow { sheet, slot, at });
    }

    /// Draws the building icon of graphics group `group` with its origin at `at`.
    pub fn building(&mut self, group: u16, at: Point) {
        self.game(Sheet::Buildings, group, at);
    }

    /// Fills `within` with the index `face` and lays `edge` along the pair of edges `pair`
    /// names, which is what a control in the style that names no items draws (`0x515c2`).
    pub fn flat_frame(&mut self, (face, edge): (u8, u8), within: Rect, pair: Edges) {
        self.fill(within, UNDER);
        for piece in edges(within, pair) {
            self.fill(piece, edge);
        }
        self.fill(inside(within), face);
    }

    /// Fills `within` with `face` and draws `edge` along the pair of edges a control at rest
    /// shows.
    pub fn frame(&mut self, face: ItemId, edge: Option<ItemId>, within: Rect) {
        self.frame_edges(face, edge, within, Edges::Lit);
    }

    /// The same with the pair of edges `pair` names.
    pub fn frame_edges(&mut self, face: ItemId, edge: Option<ItemId>, within: Rect, pair: Edges) {
        let clip = self.clip().intersect(within);
        if !clip.is_empty() {
            self.ops.push(Op {
                clip,
                kind: OpKind::Frame {
                    face,
                    edge,
                    within,
                    pair,
                },
            });
        }
    }

    /// Draws `item` centred in `within`, which is where a control's picture sits.
    pub fn item_centred(&mut self, item: ItemId, within: Rect) {
        self.item_placed(item, within, Place::Centre);
    }

    /// Draws `item` against `within` the way `place` says.
    pub fn item_placed(&mut self, item: ItemId, within: Rect, place: Place) {
        self.push(OpKind::ItemIn {
            item,
            within,
            place,
            base: 0,
        });
    }

    /// Fills `rect` with `index`.
    pub fn fill(&mut self, rect: Rect, index: u8) {
        self.push(OpKind::Fill { rect, index });
    }

    /// Lightens the pixels of `rect`, which is how a marked control draws its background.
    pub fn lighten(&mut self, rect: Rect) {
        self.push(OpKind::Lighten { rect });
    }

    /// Greys the pixels of `rect` the way a disabled control is drawn over once it is down.
    pub fn disable(&mut self, rect: Rect) {
        self.push(OpKind::Disable { rect });
    }

    /// Darkens the pixels of `rect`, which is how a label with the fill flag lays its strip.
    pub fn darken(&mut self, rect: Rect) {
        self.push(OpKind::Darken { rect });
    }

    /// Draws `label` starting at `at`, with each glyph at its own width.
    pub fn text(&mut self, at: Point, font: FontId, colour: u8, label: Label) {
        self.text_spaced(at, font, colour, label, Pen::TIGHT);
    }

    /// Draws `label` starting at `at`, spaced by `pen`.
    pub fn text_spaced(&mut self, at: Point, font: FontId, colour: u8, label: Label, pen: Pen) {
        self.text_aligned(at, font, colour, label, pen, Align::Left);
    }

    /// Draws `label` at `at`, sitting against it as `align` says.
    pub fn text_aligned(
        &mut self,
        at: Point,
        font: FontId,
        colour: u8,
        label: Label,
        pen: Pen,
        align: Align,
    ) {
        self.push(OpKind::Text {
            at,
            font,
            colour,
            label,
            pen,
            align,
        });
    }

    /// Draws the picture `image` with its top left corner at `at`.
    pub fn image(&mut self, image: ImageId, at: Point) {
        self.push(OpKind::Image { image, at });
    }

    /// Draws a line of `index` from `from` to `to`.
    pub fn line(&mut self, from: Point, to: Point, index: u8) {
        self.push(OpKind::Line { from, to, index });
    }

    /// What has been pushed, in order.
    #[must_use]
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    /// Adds `kind` under the clip in force, dropping it when nothing could show.
    fn push(&mut self, kind: OpKind) {
        let clip = self.clip();
        if !clip.is_empty() {
            self.ops.push(Op { clip, kind });
        }
    }
}

/// Everything the painter needs to turn operations into pixels.
#[derive(Debug)]
pub struct Canvas<'a, L> {
    /// The interface sheet an [`ItemId`] indexes.
    pub sheet: &'a SpriteSheet,
    /// The fonts a run of text draws in.
    pub fonts: &'a Fonts,
    /// Where a [`Label::Slot`] gets its characters.
    pub labels: &'a L,
    /// The pictures an [`OpKind::Image`] names, which [`Painter::images`] gives.
    pub images: &'a Images,
    /// The game's own archives, which an [`OpKind::Game`] draws from.
    pub sheets: Sheets<'a>,
    /// The table an [`OpKind::Lighten`] sends its pixels through. Without one it draws nothing.
    pub lighten: Option<&'a Remap>,
    /// The table an [`OpKind::Darken`] sends its pixels through. Without one it draws nothing.
    pub darken: Option<&'a Remap>,
    /// The table an [`OpKind::Disable`] sends its pixels through. Without one it draws nothing.
    pub disabled: Option<&'a Remap>,
}

/// One of the game's own sprite archives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Sheet {
    /// The player's nation icons (`<NATION>_ICON.LST`), indexed by a building's graphics group.
    Buildings,
    /// The landscape's own archive (`MAP_<set>_Y.LST`), which holds the ware and person pictures.
    Landscape,
    /// The player's nation archive (`<NATION>_Z.LST`), which holds the buildings as the world
    /// draws them.
    Nation,
}

/// The game's archives a canvas can draw from. A sheet left out draws nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct Sheets<'a> {
    /// The player's nation icons.
    pub buildings: Option<&'a SpriteSheet>,
    /// The landscape's own archive.
    pub landscape: Option<&'a SpriteSheet>,
    /// The player's nation archive.
    pub nation: Option<&'a SpriteSheet>,
}

impl<'a> Sheets<'a> {
    /// The sheet `sheet` names, where the canvas carries it.
    #[must_use]
    pub const fn get(self, sheet: Sheet) -> Option<&'a SpriteSheet> {
        match sheet {
            Sheet::Buildings => self.buildings,
            Sheet::Landscape => self.landscape,
            Sheet::Nation => self.nation,
        }
    }
}

/// Draws `ops` into `surface`, cutting each one to its clip rectangle and to the surface.
pub fn paint<L: Labels>(ops: &[Op], surface: &mut Surface<'_>, canvas: &Canvas<'_, L>) {
    let bounds = Rect::new(0, 0, surface.width(), surface.height());
    for op in ops {
        let clip = op.clip.intersect(bounds);
        let (Ok(left), Ok(top)) = (u32::try_from(clip.x), u32::try_from(clip.y)) else {
            continue;
        };
        if clip.is_empty() {
            continue;
        }
        let mut view = surface.window(left, top, clip.width, clip.height);
        match op.kind {
            OpKind::Item { item, at, base } => {
                if let Some(sprite) = canvas.sheet.get(usize::from(item.0)) {
                    let paint = Paint { base, shadow: None };
                    blit(&mut view, 0, sprite, local(at, clip), paint);
                }
            }
            OpKind::ItemIn {
                item,
                within,
                place,
                base,
            } => {
                if let Some(sprite) = canvas.sheet.get(usize::from(item.0)) {
                    let paint = Paint { base, shadow: None };
                    blit(
                        &mut view,
                        0,
                        sprite,
                        local(placed(within, sprite, place), clip),
                        paint,
                    );
                }
            }
            OpKind::Game { sheet, slot, at } => {
                if let Some(sprite) = canvas
                    .sheets
                    .get(sheet)
                    .and_then(|sheet| sheet.get(usize::from(slot)))
                {
                    blit(&mut view, 0, sprite, local(at, clip), Paint::PLAIN);
                }
            }
            OpKind::Shadow { sheet, slot, at } => {
                if let Some(sprite) = canvas
                    .sheets
                    .get(sheet)
                    .and_then(|sheet| sheet.get(usize::from(slot)))
                {
                    let paint = Paint {
                        base: 0,
                        shadow: canvas.darken,
                    };
                    blit(&mut view, 0, sprite, local(at, clip), paint);
                }
            }
            OpKind::Tile { item, within } => {
                if let Some(sprite) = canvas.sheet.get(usize::from(item.0)) {
                    tile(&mut view, sprite, within, clip);
                }
            }
            OpKind::Frame {
                face,
                edge,
                within,
                pair,
            } => {
                frame(surface, canvas.sheet, (face, edge), (within, pair), clip);
            }
            OpKind::Fill { rect, index } => {
                let area = rect.intersect(clip);
                if !area.is_empty() {
                    let at = local(Point::new(area.x, area.y), clip);
                    let (Ok(x), Ok(y)) = (u32::try_from(at.x), u32::try_from(at.y)) else {
                        continue;
                    };
                    view.window(x, y, area.width, area.height).clear(index);
                }
            }
            OpKind::Lighten { rect } => through(&mut view, clip, rect, canvas.lighten),
            OpKind::Disable { rect } => through(&mut view, clip, rect, canvas.disabled),
            OpKind::Darken { rect } => through(&mut view, clip, rect, canvas.darken),
            OpKind::Image { image, at } => {
                if let Some(picture) = canvas.images.get(image) {
                    draw_image(&mut view, local(at, clip), picture);
                }
            }
            OpKind::Line { from, to, index } => {
                draw_line(&mut view, local(from, clip), local(to, clip), index);
            }
            OpKind::Text {
                at,
                font,
                colour,
                label,
                pen,
                align,
            } => draw_text(&mut view, clip, canvas, (at, font, colour), (label, pen, align)),
        }
    }
}

/// Draws `label` into `view`, starting from the point `align` puts it at and breaking its lines
/// the way the original's text routine does.
fn draw_text<L: Labels>(
    view: &mut Surface<'_>,
    clip: Rect,
    canvas: &Canvas<'_, L>,
    (at, font, colour): (Point, FontId, u8),
    (label, pen, align): (Label, Pen, Align),
) {
    let glyphs = canvas.fonts.get(font);
    let paint = Paint {
        base: colour,
        shadow: None,
    };
    let at = align_run(at, align, glyphs, pen, label, canvas.labels);
    let start = local(at, clip);
    let mut cursor = start;
    label.with_text(canvas.labels, |text| {
        let mut rest = text;
        while let Some((&code, next)) = rest.split_first() {
            rest = next;
            match code {
                0 => break,
                NEW_LINE | CARRIAGE_RETURN => {
                    if let Some((&again, after)) = rest.split_first()
                        && matches!(again, NEW_LINE | CARRIAGE_RETURN)
                    {
                        rest = after;
                    }
                    let down = i32::try_from(glyphs.line_advance(pen)).unwrap_or_default();
                    cursor = Point::new(start.x, cursor.y.saturating_add(down));
                }
                TAB => draw_glyph(view, glyphs, SPACE, pen, &mut cursor, paint),
                SPACE.. => draw_glyph(view, glyphs, code, pen, &mut cursor, paint),
                _ => {}
            }
        }
    });
}

/// `at` moved so a run of `label` sits against it as `align` says.
fn align_run<L: Labels>(
    at: Point,
    align: Align,
    glyphs: &Glyphs,
    pen: Pen,
    label: Label,
    labels: &L,
) -> Point {
    if matches!(align, Align::Left) {
        return at;
    }
    let width = label.with_text(labels, |text| glyphs.measure(text, pen));
    let width = i32::try_from(width).unwrap_or_default();
    let span = |across: u32| i32::try_from(across).unwrap_or_default();
    let left = match align {
        Align::Left => at.x,
        Align::Centre => at.x - width / 2,
        Align::Right => at.x - width,
        Align::CentredIn(across) => at.x + span(across) / 2 - width / 2,
        Align::CentredInBox(across, _) | Align::Across(across) => at.x + (span(across) - width) / 2,
    };
    let down = match align {
        Align::CentredInBox(_, height) => {
            let line = i32::try_from(glyphs.line_height()).unwrap_or_default();
            at.y + (span(height) - line) / 2 + 1
        }
        _ => at.y,
    };
    Point::new(left, down)
}

/// Copies a picture's pixels in, one row at a time, with its top left corner at `at`.
fn draw_image(view: &mut Surface<'_>, at: Point, picture: Image<'_>) {
    let width = usize::try_from(picture.width).unwrap_or_default();
    for (row, pixels) in picture.pixels.chunks_exact(width.max(1)).enumerate() {
        let Ok(down) = i32::try_from(row) else { break };
        for (column, &index) in pixels.iter().enumerate() {
            let Ok(across) = i32::try_from(column) else { break };
            view.pixel(at.x + across, at.y + down, index);
        }
    }
}

/// Draws a line of `index` from `from` to `to` the way `0x83cf4` does, stepping across whenever
/// the error term is not negative and down whenever it is.
fn draw_line(view: &mut Surface<'_>, from: Point, to: Point, index: u8) {
    let across = (to.x - from.x).abs();
    let down = (to.y - from.y).abs();
    let step_x = if from.x < to.x { 1 } else { -1 };
    let step_y = if from.y < to.y { 1 } else { -1 };
    let mut error = across - down;
    let (mut x, mut y) = (from.x, from.y);
    loop {
        view.pixel(x, y, index);
        if x == to.x && y == to.y {
            break;
        }
        if error >= 0 {
            x += step_x;
            error -= down;
        }
        if error < 0 {
            y += step_y;
            error += across;
        }
    }
}

/// The first character code with a glyph, which a tab draws in its place (`0x93850`).
const SPACE: u8 = b' ';

/// The code a tab arrives as.
const TAB: u8 = b'\t';

/// The codes that break a line, either one taking a following one of the pair with it.
const NEW_LINE: u8 = b'\n';
/// The other of the pair.
const CARRIAGE_RETURN: u8 = b'\r';

/// Draws the glyph for `code` at `cursor` and moves `cursor` along, as `0x93a0c` does.
fn draw_glyph(
    view: &mut Surface<'_>,
    glyphs: &Glyphs,
    code: u8,
    pen: Pen,
    cursor: &mut Point,
    paint: Paint<'_>,
) {
    if let Some(glyph) = glyphs.glyph(code) {
        let at = cursor.offset(glyphs.inset(code, pen), 0);
        blit(view, 0, glyph, at, paint);
    }
    let advance = i32::try_from(glyphs.advance(code, pen)).unwrap_or_default();
    *cursor = cursor.offset(advance, 0);
}

/// Where `sprite` lands when it centres in `within`. An odd difference leaves the extra pixel on
/// the right and the bottom.
/// Fills `within` with `face` and draws `edge` as a strip around it, both repeating. Each part
/// takes a window of its own so that nothing reaches past the rectangle it fills.
fn frame(
    surface: &mut Surface<'_>,
    sheet: &SpriteSheet,
    (face, edge): (ItemId, Option<ItemId>),
    (within, pair): (Rect, Edges),
    clip: Rect,
) {
    let Some(edge) = edge else {
        tiles(surface, sheet, face, within, clip);
        return;
    };
    blacken(surface, within, clip);
    tiles(
        surface,
        sheet,
        face,
        inside(within),
        inside(within).intersect(clip),
    );
    for piece in edges(within, pair) {
        tiles(surface, sheet, edge, piece, piece.intersect(clip));
    }
}

/// The part of `within` the face covers: the rectangle taken in by the frame's thickness on every
/// side, which `0x51164` fills after the frame.
#[must_use]
pub fn inside(within: Rect) -> Rect {
    let thick = i32::try_from(EDGE).unwrap_or_default();
    Rect::new(
        within.x + thick,
        within.y + thick,
        within.width.saturating_sub(2 * EDGE),
        within.height.saturating_sub(2 * EDGE),
    )
}

/// The index `0x51270` lays under a control before its frame and face go on, which is the black
/// that shows along the two edges the lit pair and the face between them never cover. The
/// palette holds the same black at 0, and an S2 capture holds this one.
pub const UNDER: u8 = 252;

/// Lays [`UNDER`] over the control's whole rectangle, the way `0x51270` does.
fn blacken(surface: &mut Surface<'_>, within: Rect, clip: Rect) {
    let area = within.intersect(clip);
    if area.is_empty() {
        return;
    }
    let (Ok(x), Ok(y)) = (u32::try_from(area.x), u32::try_from(area.y)) else {
        return;
    };
    surface.window(x, y, area.width, area.height).clear(UNDER);
}

/// Repeats `item` across `within`, drawing only what `clip` covers.
fn tiles(surface: &mut Surface<'_>, sheet: &SpriteSheet, item: ItemId, within: Rect, clip: Rect) {
    let Some(sprite) = sheet.get(usize::from(item.0)) else {
        return;
    };
    let (Ok(left), Ok(top)) = (u32::try_from(clip.x), u32::try_from(clip.y)) else {
        return;
    };
    if clip.is_empty() {
        return;
    }
    let mut view = surface.window(left, top, clip.width, clip.height);
    tile(&mut view, sprite, within, clip);
}

/// How thick the strip around a background is (`0x512a4`).
pub const EDGE: u32 = 2;

/// The four pieces `0x51164` draws for one pair of edges.
///
/// The lit pair is the top strip, one pixel at its right end, the left strip and one pixel at its
/// foot; the shaded pair is their mirror. One call draws one pair, and the caller picks it from
/// the control's state. A control at rest shows the lit pair, so its other two edges are left as
/// its face, which is what reads as the shaded side.
fn edges(within: Rect, pair: Edges) -> [Rect; 4] {
    let thick = i32::try_from(EDGE).unwrap_or_default();
    let (width, height) = (within.width, within.height);
    match pair {
        Edges::Lit => [
            Rect::new(within.x, within.y, width.saturating_sub(EDGE), EDGE.min(height)),
            Rect::new(within.right() - thick, within.y, 1, 1),
            Rect::new(
                within.x,
                within.y + thick,
                EDGE.min(width),
                height.saturating_sub(2 * EDGE),
            ),
            Rect::new(within.x, within.bottom() - thick, 1, 1),
        ],
        Edges::Shaded => [
            Rect::new(
                within.right() - thick,
                within.y + thick,
                EDGE.min(width),
                height.saturating_sub(EDGE),
            ),
            Rect::new(within.right() - 1, within.y + 1, 1, 1),
            Rect::new(
                within.x + thick,
                within.bottom() - thick,
                width.saturating_sub(2 * EDGE),
                EDGE.min(height),
            ),
            Rect::new(within.x + 1, within.bottom() - 1, 1, 1),
        ],
    }
}

/// Repeats `sprite` from `within`'s top-left corner over everything `clip` covers, which is how
/// `0x81b94` fills a control.
fn tile(view: &mut Surface<'_>, sprite: &Sprite, within: Rect, clip: Rect) {
    let paint = Paint {
        base: 0,
        shadow: None,
    };
    let across = i32::from(sprite.width().max(1));
    let down = i32::from(sprite.height().max(1));
    let mut y = within.y;
    while y < clip.bottom() {
        let mut x = within.x;
        while x < clip.right() {
            blit(view, 0, sprite, local(Point::new(x, y), clip), paint);
            x += across;
        }
        y += down;
    }
}

/// Where to hand an item to [`blit`], which takes the point the item's own origin lands on and
/// puts its top-left corner that far up and left. A centred item puts its origin on the middle of
/// `within`, as a control's picture and the picture widget do (`0x4e599`, `0x4fa27`); every `io`
/// item's origin is its own middle. The other places land the item's corner where they say,
/// whatever hotspot it carries.
fn placed(within: Rect, sprite: &Sprite, place: Place) -> Point {
    let across = i32::from(sprite.width());
    let width = i32::try_from(within.width).unwrap_or(across);
    let height = i32::try_from(within.height).unwrap_or_default();
    let corner = match place {
        Place::Centre => return Point::new(within.x + width / 2, within.y + height / 2),
        Place::TopLeft => Point::new(within.x, within.y),
        Place::TopRight => Point::new(within.x + width - across, within.y),
    };
    let origin = sprite.origin();
    Point::new(corner.x + i32::from(origin.x), corner.y + i32::from(origin.y))
}

/// Sends the part of `rect` inside `clip` through `table`. Without a table nothing is drawn.
fn through(view: &mut Surface<'_>, clip: Rect, rect: Rect, table: Option<&Remap>) {
    let area = rect.intersect(clip);
    let Some(table) = table else { return };
    if area.is_empty() {
        return;
    }
    let at = local(Point::new(area.x, area.y), clip);
    let (Ok(x), Ok(y)) = (u32::try_from(at.x), u32::try_from(at.y)) else {
        return;
    };
    view.window(x, y, area.width, area.height).remap(table);
}

/// `at` in the coordinates of a view whose top-left corner is `clip`'s.
fn local(at: Point, clip: Rect) -> Point {
    Point::new(at.x.saturating_sub(clip.x), at.y.saturating_sub(clip.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{Glyphs, NoLabels};

    fn fonts() -> Fonts {
        Fonts::new(Glyphs::fixed(9, 8), Glyphs::fixed(11, 10), Glyphs::fixed(14, 12))
    }

    fn painter() -> Painter {
        Painter::new(Rect::new(0, 0, 100, 50))
    }

    #[test]
    fn a_centred_item_puts_its_origin_on_the_middle_and_a_cornered_one_its_corner() {
        let square = |origin| Sprite::build(origin, 4, 4, (0..16).map(|_| Some((s2_assets::Ink::Colour, 1))));
        let centred = square(s2_formats::lst::Origin { x: 2, y: 2 });
        let hung = square(s2_formats::lst::Origin { x: 3, y: 2 });
        let within = Rect::new(10, 20, 11, 10);
        assert_eq!(
            placed(within, &centred, Place::Centre),
            Point::new(15, 25),
            "the middle of an 11 by 10 box, 5 and 5 in"
        );
        assert_eq!(
            placed(within, &hung, Place::Centre),
            Point::new(15, 25),
            "whatever the origin"
        );
        assert_eq!(
            placed(within, &hung, Place::TopLeft),
            Point::new(13, 22),
            "the origin is added back so blit takes it off again"
        );
    }

    #[test]
    fn a_pushed_clip_shrinks_and_pops_back() {
        let mut p = painter();
        assert_eq!(p.clip(), Rect::new(0, 0, 100, 50));
        p.push_clip(Rect::new(10, 10, 20, 20));
        assert_eq!(p.clip(), Rect::new(10, 10, 20, 20));
        p.push_clip(Rect::new(0, 0, 15, 100));
        assert_eq!(p.clip(), Rect::new(10, 10, 5, 20), "a nested clip only shrinks");
        p.pop_clip();
        assert_eq!(p.clip(), Rect::new(10, 10, 20, 20));
        p.pop_clip();
        p.pop_clip();
        assert_eq!(p.clip(), Rect::new(0, 0, 100, 50), "the bounds always remain");
    }

    #[test]
    fn each_operation_carries_the_clip_it_was_pushed_under() {
        let mut p = painter();
        p.fill(Rect::new(0, 0, 4, 4), 1);
        p.push_clip(Rect::new(10, 10, 20, 20));
        p.item(ItemId(3), Point::new(12, 12));
        p.pop_clip();
        assert_eq!(p.ops().len(), 2);
        assert_eq!(p.ops()[0].clip, Rect::new(0, 0, 100, 50));
        assert_eq!(p.ops()[1].clip, Rect::new(10, 10, 20, 20));
        p.reset();
        let found = p.ops();
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(p.clip(), Rect::new(0, 0, 100, 50));
    }

    #[test]
    fn an_operation_under_an_empty_clip_is_dropped() {
        let mut p = painter();
        p.push_clip(Rect::new(200, 200, 10, 10));
        assert!(p.clip().is_empty());
        p.fill(Rect::new(200, 200, 4, 4), 1);
        p.text(Point::new(0, 0), FontId::Small, 1, Label::Number(5));
        let found = p.ops();
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_fill_writes_its_rectangle_and_nothing_outside_the_clip() {
        let mut pixels = vec![0u8; 8 * 4];
        let mut surface = Surface::new(&mut pixels, 8, 4).expect("size matches");
        let mut p = Painter::new(Rect::new(0, 0, 8, 4));
        p.push_clip(Rect::new(2, 1, 3, 2));
        p.fill(Rect::new(0, 0, 8, 4), 9);
        p.pop_clip();
        let sheet = SpriteSheet::from_slots([]);
        let fonts = fonts();
        let canvas = Canvas {
            sheet: &sheet,
            fonts: &fonts,
            labels: &NoLabels,
            images: p.images(),
            sheets: Sheets::default(),
            lighten: None,
            disabled: None,
            darken: None,
        };
        paint(p.ops(), &mut surface, &canvas);
        let row = |y: usize| &pixels[y * 8..(y + 1) * 8];
        assert_eq!(row(0), [0; 8], "above the clip");
        assert_eq!(row(1), [0, 0, 9, 9, 9, 0, 0, 0]);
        assert_eq!(row(2), [0, 0, 9, 9, 9, 0, 0, 0]);
        assert_eq!(row(3), [0; 8], "below the clip");
    }

    #[test]
    fn a_line_steps_across_and_down_as_the_original_does() {
        let mut pixels = vec![0u8; 8 * 5];
        let mut surface = Surface::new(&mut pixels, 8, 5).expect("size matches");
        let mut p = Painter::new(Rect::new(0, 0, 8, 5));
        p.line(Point::new(0, 0), Point::new(7, 0), 1);
        p.line(Point::new(0, 1), Point::new(0, 4), 2);
        p.line(Point::new(1, 1), Point::new(4, 4), 3);
        p.line(Point::new(-4, 2), Point::new(2, 2), 4);
        let sheet = SpriteSheet::from_slots([]);
        let fonts = fonts();
        let canvas = Canvas {
            sheet: &sheet,
            fonts: &fonts,
            labels: &NoLabels,
            images: p.images(),
            sheets: Sheets::default(),
            lighten: None,
            disabled: None,
            darken: None,
        };
        paint(p.ops(), &mut surface, &canvas);
        let row = |y: usize| &pixels[y * 8..(y + 1) * 8];
        assert_eq!(row(0), [1; 8], "a flat line fills its span");
        assert_eq!(row(1), [2, 3, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            row(2),
            [4, 4, 4, 0, 0, 0, 0, 0],
            "a line from off the left still lands, over the diagonal"
        );
        assert_eq!(row(3), [2, 0, 0, 3, 0, 0, 0, 0]);
        assert_eq!(row(4), [2, 0, 0, 0, 3, 0, 0, 0]);
    }

    #[test]
    fn a_run_sits_where_its_alignment_says() {
        let mut p = painter();
        let runs = [
            (Align::Left, 20),
            (Align::Centre, 20 - 12),
            (Align::Right, 20 - 24),
            (Align::CentredIn(30), 20 + 3),
        ];
        for (align, _) in runs {
            p.text_aligned(
                Point::new(20, 0),
                FontId::Small,
                1,
                Label::Number(123),
                Pen::TIGHT,
                align,
            );
        }
        let glyphs = Glyphs::fixed(9, 8);
        let width = glyphs.measure(b"123", Pen::TIGHT);
        assert_eq!(width, 24, "three glyphs of the fixed test font");
        for (op, (_, left)) in p.ops().iter().zip(runs) {
            let OpKind::Text { at, align, .. } = op.kind else {
                panic!("a run")
            };
            let placed = align_run(at, align, &glyphs, Pen::TIGHT, Label::Number(123), &NoLabels);
            assert_eq!(placed.x, left, "{align:?}");
        }
    }

    #[test]
    fn the_two_centrings_halve_different_things() {
        let glyphs = Glyphs::fixed(9, 8);
        let run = Label::Number(123);
        let width = glyphs.measure(b"123", Pen::TIGHT);
        assert_eq!(width, 24, "three glyphs of the fixed test font");
        let at = Point::new(10, 20);
        let placed = |across, down| {
            align_run(
                at,
                Align::CentredInBox(across, down),
                &glyphs,
                Pen::TIGHT,
                run,
                &NoLabels,
            )
        };
        assert_eq!(
            placed(30, 9),
            Point::new(13, 21),
            "an exact fit lands one pixel down, as `0x4dec8` adds 1"
        );
        assert_eq!(
            placed(31, 10),
            Point::new(13, 21),
            "a label halves what the run leaves over, so the spare pixel goes right and above"
        );
        assert_eq!(
            placed(30, 20),
            Point::new(13, 26),
            "a box 20 tall: (20 - 9) / 2 + 1"
        );
        let halves = |across| align_run(at, Align::CentredIn(across), &glyphs, Pen::TIGHT, run, &NoLabels).x;
        assert_eq!(halves(30), 13);
        assert_eq!(
            halves(31),
            13,
            "a title halves the width and the run on their own, which rounds the other way"
        );
    }

    #[test]
    fn the_shaded_pair_of_edges_mirrors_the_lit_one() {
        let within = Rect::new(4, 6, 20, 12);
        let [top, corner, left, foot] = edges(within, Edges::Lit);
        assert_eq!(top, Rect::new(4, 6, 18, 2));
        assert_eq!(corner, Rect::new(22, 6, 1, 1));
        assert_eq!(left, Rect::new(4, 8, 2, 8));
        assert_eq!(foot, Rect::new(4, 16, 1, 1));
        let [right, head, bottom, toe] = edges(within, Edges::Shaded);
        assert_eq!(right, Rect::new(22, 8, 2, 10));
        assert_eq!(head, Rect::new(23, 7, 1, 1));
        assert_eq!(bottom, Rect::new(6, 16, 16, 2));
        assert_eq!(toe, Rect::new(5, 17, 1, 1));
    }

    #[test]
    fn an_operation_off_the_surface_draws_nothing() {
        let mut pixels = vec![0u8; 4 * 2];
        let mut surface = Surface::new(&mut pixels, 4, 2).expect("size matches");
        let mut p = Painter::new(Rect::new(-20, -20, 10, 10));
        p.fill(Rect::new(-20, -20, 10, 10), 7);
        let sheet = SpriteSheet::from_slots([]);
        let fonts = fonts();
        let canvas = Canvas {
            sheet: &sheet,
            fonts: &fonts,
            labels: &NoLabels,
            images: p.images(),
            sheets: Sheets::default(),
            lighten: None,
            disabled: None,
            darken: None,
        };
        paint(p.ops(), &mut surface, &canvas);
        assert_eq!(pixels, [0; 8]);
    }
}
