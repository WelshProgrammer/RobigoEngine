//! The mouse pointer: which of its six pictures shows and where it is drawn (`0x8c034`).

use s2_assets::{Palette, Sprite};
use s2_render::terrain::Point;

use crate::Items;

/// Bytes a pixel of [`rgba`] takes: red, green, blue and alpha.
pub const RGBA: usize = 4;

/// One of the pointer's pictures, in the order the pointer keeps them (`+0x12` to `+0x26`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerPicture {
    /// `handa`, shown normally.
    Hand,
    /// `handb`, while the left button is held.
    Pressed,
    /// `handc`, while the right button is held.
    Scroll,
    /// `handw`, while the game saves or loads.
    Wait,
    /// `handk`, while a road is drawn.
    Road,
    /// `handl`, while a road is drawn and the left button is held.
    RoadPressed,
}

impl PointerPicture {
    /// Every picture, in the pointer's order.
    pub const ALL: [Self; 6] = [
        Self::Hand,
        Self::Pressed,
        Self::Scroll,
        Self::Wait,
        Self::Road,
        Self::RoadPressed,
    ];

    /// The name `RESOURCE.DAT` gives the picture.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hand => "handa",
            Self::Pressed => "handb",
            Self::Scroll => "handc",
            Self::Wait => "handw",
            Self::Road => "handk",
            Self::RoadPressed => "handl",
        }
    }

    /// Whether `0x8bc14` moves the picture's origin to its centre.
    const fn centred(self) -> bool {
        !matches!(self, Self::Hand | Self::Pressed)
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// A mouse button the pointer tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerButton {
    /// The left button.
    Left,
    /// The right button, which scrolls the map while held.
    Right,
}

/// The pointer's state: the buttons held, the modes the game has set and where it was last
/// moved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MousePointer {
    origins: [Point; 6],
    plain: bool,
    held: Option<PointerButton>,
    wait: bool,
    road: bool,
    at: Point,
    anchor: Point,
}

impl MousePointer {
    /// A pointer whose pictures have `sizes`, in [`PointerPicture::ALL`] order. A `plain`
    /// pointer shows only [`PointerPicture::Hand`] and [`PointerPicture::Wait`], as the menu
    /// screens' pointer does.
    #[must_use]
    pub fn new(sizes: [(u16, u16); 6], plain: bool) -> Self {
        let origins = std::array::from_fn(|index| {
            let (width, height) = sizes[index];
            if PointerPicture::ALL[index].centred() {
                Point::new(i32::from(width / 2), i32::from(height / 2))
            } else {
                Point::new(0, 0)
            }
        });
        Self {
            origins,
            plain,
            held: None,
            wait: false,
            road: false,
            at: Point::new(0, 0),
            anchor: Point::new(0, 0),
        }
    }

    /// The pointer for the pictures `items` holds; a picture missing from them counts as empty.
    #[must_use]
    pub fn from_items(items: &Items, plain: bool) -> Self {
        Self::new(
            PointerPicture::ALL.map(|picture| size(sprite(items, picture))),
            plain,
        )
    }

    /// The mouse moved to `at`, the point clicks land on. The pointer stays put while the right
    /// button is held.
    pub fn moved(&mut self, at: Point) {
        if self.held != Some(PointerButton::Right) {
            self.at = at;
            self.anchor = self.origin(self.picture());
        }
    }

    /// A button went down at `at`. A button counts only while the other is up, and a left
    /// press also moves the pointer, with the picture showing before the press.
    pub fn press(&mut self, button: PointerButton, at: Point) {
        let before = self.picture();
        if self.held.is_none() {
            self.held = Some(button);
        }
        if button == PointerButton::Left && self.held != Some(PointerButton::Right) {
            self.at = at;
            self.anchor = self.origin(before);
        }
    }

    /// A button came up.
    pub fn release(&mut self, button: PointerButton) {
        if self.held == Some(button) {
            self.held = None;
        }
    }

    /// Whether the game is saving or loading (`0x45004`, `0x45028`).
    pub const fn set_wait(&mut self, wait: bool) {
        self.wait = wait;
    }

    /// Whether a road is being drawn (`0x4504c`, `0x45070`).
    pub const fn set_road(&mut self, road: bool) {
        self.road = road;
    }

    /// The picture showing.
    #[must_use]
    pub const fn picture(&self) -> PointerPicture {
        if self.wait {
            PointerPicture::Wait
        } else if self.plain {
            PointerPicture::Hand
        } else {
            match (self.held, self.road) {
                (Some(PointerButton::Right), _) => PointerPicture::Scroll,
                (Some(PointerButton::Left), true) => PointerPicture::RoadPressed,
                (Some(PointerButton::Left), false) => PointerPicture::Pressed,
                (None, true) => PointerPicture::Road,
                (None, false) => PointerPicture::Hand,
            }
        }
    }

    /// The point clicks land on.
    #[must_use]
    pub const fn at(&self) -> Point {
        self.at
    }

    /// Where the showing picture's top-left corner goes: on the click point after a move, and
    /// shifted by the difference of the two pictures' origins when the picture changed since.
    #[must_use]
    pub fn top_left(&self) -> Point {
        let origin = self.origin(self.picture());
        Point::new(
            self.at.x + self.anchor.x - origin.x,
            self.at.y + self.anchor.y - origin.y,
        )
    }

    /// Where the click point sits within the showing picture: its hotspot.
    #[must_use]
    pub fn hotspot(&self) -> Point {
        let top_left = self.top_left();
        Point::new(self.at.x - top_left.x, self.at.y - top_left.y)
    }

    const fn origin(&self, picture: PointerPicture) -> Point {
        self.origins[picture.index()]
    }
}

/// The sprite `items` holds for `picture`.
#[must_use]
pub fn sprite(items: &Items, picture: PointerPicture) -> Option<&Sprite> {
    items
        .id(picture.name())
        .and_then(|id| items.sheet().get(usize::from(id.0)))
}

/// `sprite` as rows of red, green, blue and alpha bytes, coloured through `palette`, with each
/// pixel repeated `scale` times across and down. The picture's empty pixels are clear.
#[must_use]
pub fn rgba(sprite: &Sprite, palette: &Palette, scale: u32) -> Vec<u8> {
    let scale = usize::try_from(scale.max(1)).unwrap_or(1);
    let width = usize::from(sprite.width()) * scale;
    let mut pixels = vec![0; width * usize::from(sprite.height()) * scale * RGBA];
    for y in 0..sprite.height() {
        for run in sprite.row(y) {
            for (step, &index) in sprite.values(run).iter().enumerate() {
                let [red, green, blue] = palette.0[usize::from(index)];
                let x = usize::from(run.x) + step;
                for row in 0..scale {
                    let line = (usize::from(y) * scale + row) * width;
                    for column in 0..scale {
                        let at = (line + x * scale + column) * RGBA;
                        pixels[at..at + RGBA].copy_from_slice(&[red, green, blue, u8::MAX]);
                    }
                }
            }
        }
    }
    pixels
}

fn size(sprite: Option<&Sprite>) -> (u16, u16) {
    sprite.map_or((0, 0), |sprite| (sprite.width(), sprite.height()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sizes `RESOURCE.DAT` gives the six pictures.
    const SIZES: [(u16, u16); 6] = [(32, 32), (32, 32), (30, 21), (32, 32), (20, 14), (20, 12)];

    fn pointer() -> MousePointer {
        MousePointer::new(SIZES, false)
    }

    #[test]
    fn the_buttons_and_modes_choose_the_picture() {
        let mut pointer = pointer();
        let at = Point::new(100, 100);
        assert_eq!(pointer.picture(), PointerPicture::Hand);
        pointer.press(PointerButton::Left, at);
        assert_eq!(pointer.picture(), PointerPicture::Pressed);
        pointer.press(PointerButton::Right, at);
        assert_eq!(
            pointer.picture(),
            PointerPicture::Pressed,
            "the left button came first"
        );
        pointer.release(PointerButton::Left);
        pointer.release(PointerButton::Right);
        pointer.press(PointerButton::Right, at);
        assert_eq!(pointer.picture(), PointerPicture::Scroll);
        pointer.release(PointerButton::Right);
        pointer.set_road(true);
        assert_eq!(pointer.picture(), PointerPicture::Road);
        pointer.press(PointerButton::Left, at);
        assert_eq!(pointer.picture(), PointerPicture::RoadPressed);
        pointer.set_wait(true);
        assert_eq!(pointer.picture(), PointerPicture::Wait);
    }

    #[test]
    fn a_plain_pointer_keeps_the_hand() {
        let mut pointer = MousePointer::new(SIZES, true);
        pointer.set_road(true);
        pointer.press(PointerButton::Right, Point::new(0, 0));
        assert_eq!(pointer.picture(), PointerPicture::Hand);
        pointer.set_wait(true);
        assert_eq!(pointer.picture(), PointerPicture::Wait);
    }

    /// The placements S2 captures show: `handk` with its corner on the click point at
    /// (460, 300), `handc` centred on (300, 150) after a right press, and `handl` a row below
    /// `handk`.
    #[test]
    fn the_picture_lands_where_the_original_draws_it() {
        let mut pointer = pointer();
        pointer.set_road(true);
        pointer.moved(Point::new(460, 300));
        assert_eq!(pointer.top_left(), Point::new(460, 300));
        pointer.moved(Point::new(300, 150));
        pointer.press(PointerButton::Left, Point::new(300, 150));
        assert_eq!(pointer.top_left(), Point::new(300, 151));
        pointer.release(PointerButton::Left);
        pointer.set_road(false);
        pointer.moved(Point::new(300, 150));
        pointer.press(PointerButton::Right, Point::new(300, 150));
        assert_eq!(pointer.top_left(), Point::new(285, 140));
    }

    #[test]
    fn the_pointer_stays_put_while_the_right_button_scrolls() {
        let mut pointer = pointer();
        pointer.moved(Point::new(10, 10));
        pointer.press(PointerButton::Right, Point::new(10, 10));
        pointer.moved(Point::new(50, 60));
        assert_eq!(pointer.at(), Point::new(10, 10));
        pointer.release(PointerButton::Right);
        pointer.moved(Point::new(50, 60));
        assert_eq!(pointer.at(), Point::new(50, 60));
        assert_eq!(pointer.hotspot(), Point::new(0, 0));
    }
}
