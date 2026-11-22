//! Which `IO.DAT` item draws each control in each of its looks.
//!
//! The window frames, buttons and scroll parts all come out of `IO.DAT`, so a control type with
//! no counterpart in the original composes from the same items and matches the chrome around it.
//! A skin starts empty: the original names its items rather than numbering them, which leaves the
//! names in [`style`](crate::style) and the indices to whatever read `IO.IDX`.

use crate::paint::{Edges, ItemId};
use crate::picture::Picture;
use crate::style;

/// A kind of control a skin can draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Control {
    /// The background a window or a control sits on.
    Frame,
    /// The strip of texture around that background, two pixels thick.
    Edge,
    /// The strip along the top that carries the title.
    Title,
    /// The control that rolls a window up and down.
    Roll,
    /// A control that acts when released.
    Button,
    /// A control that carries an on or off state.
    Tick,
    /// The upward end of a scroll bar.
    ScrollUp,
    /// The downward end of a scroll bar.
    ScrollDown,
    /// The part of a scroll bar that moves.
    ScrollGrip,
    /// The part of a scroll bar the grip travels along.
    ScrollTrack,
    /// The paper a graph draws its lines over.
    GraphPaper,
    /// A button of the toolbar, which draws a face and no frame.
    Toolbar,
    /// The left end of a window's title bar.
    TitleLeft,
    /// The right end of it.
    TitleRight,
    /// The strip down a window's left side.
    SideLeft,
    /// The strip down its right side.
    SideRight,
    /// The strip along its bottom.
    BottomEdge,
    /// The pattern a window's own background tiles.
    Pattern,
    /// The corner button that closes a window.
    Close,
    /// The corner button that resizes one.
    Resize,
    /// A corner with no button in it.
    Blank,
}

impl Control {
    /// Every control, in order.
    pub const ALL: [Self; 21] = [
        Self::Frame,
        Self::Edge,
        Self::Title,
        Self::Roll,
        Self::Button,
        Self::Tick,
        Self::ScrollUp,
        Self::ScrollDown,
        Self::ScrollGrip,
        Self::ScrollTrack,
        Self::GraphPaper,
        Self::Toolbar,
        Self::TitleLeft,
        Self::TitleRight,
        Self::SideLeft,
        Self::SideRight,
        Self::BottomEdge,
        Self::Pattern,
        Self::Close,
        Self::Resize,
        Self::Blank,
    ];

    /// The pieces a window's own frame is built from, which take their art from `RESOURCE`
    /// rather than from a style record.
    pub const CHROME: [Self; 11] = [
        Self::Title,
        Self::TitleLeft,
        Self::TitleRight,
        Self::SideLeft,
        Self::SideRight,
        Self::BottomEdge,
        Self::Pattern,
        Self::Close,
        Self::Roll,
        Self::Resize,
        Self::Blank,
    ];

    /// Whether it is one of [`Control::CHROME`].
    #[must_use]
    pub fn is_chrome(self) -> bool {
        Self::CHROME.contains(&self)
    }

    /// How many kinds there are.
    pub const COUNT: usize = Self::ALL.len();

    /// The control's place in [`Control::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// How a control is drawn at the moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Look {
    /// Untouched.
    Idle,
    /// Held down with the pointer inside.
    Pressed,
    /// Carrying an on state, for a control that has one.
    On,
    /// Under the pointer with no press on it: the style's second face (`0x51558`).
    Hover,
    /// Drawn but unable to act, which `S2.EXE` marks with a nonzero `+0x1d` on the widget.
    Disabled,
}

impl Look {
    /// Every look, in order.
    pub const ALL: [Self; 5] = [Self::Idle, Self::Pressed, Self::On, Self::Disabled, Self::Hover];

    /// How many looks there are.
    pub const COUNT: usize = Self::ALL.len();

    /// The look's place in [`Look::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Idle => 0,
            Self::Pressed => 1,
            Self::On => 2,
            Self::Disabled => 3,
            Self::Hover => 4,
        }
    }

    /// The pair of edges a control in this look lights: the right and the bottom while it is
    /// held, the top and the left otherwise (`0x512e2`, `0x513d9`).
    #[must_use]
    pub const fn edges(self) -> Edges {
        match self {
            Self::Pressed => Edges::Shaded,
            _ => Edges::Lit,
        }
    }
}

/// The item that draws each control in each look.
///
/// A look with no item of its own falls back to [`Look::Idle`], which keeps a skin usable while
/// only part of it is filled in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skin {
    items: [[Option<ItemId>; Look::COUNT]; Control::COUNT],
    pictures: [Option<ItemId>; Picture::COUNT],
    faces: [[Option<ItemId>; Look::COUNT]; style::COUNT],
    edges: [Option<ItemId>; style::COUNT],
}

impl Skin {
    /// A skin with nothing set.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            items: [[None; Look::COUNT]; Control::COUNT],
            pictures: [None; Picture::COUNT],
            faces: [[None; Look::COUNT]; style::COUNT],
            edges: [None; style::COUNT],
        }
    }

    /// Sets the item `control` draws from in `look`.
    pub const fn set(&mut self, control: Control, look: Look, item: ItemId) {
        self.items[control.index()][look.index()] = Some(item);
    }

    /// The item `control` draws from in `look`, falling back to [`Look::Idle`].
    #[must_use]
    pub fn item(&self, control: Control, look: Look) -> Option<ItemId> {
        let row = &self.items[control.index()];
        row[look.index()].or(row[Look::Idle.index()])
    }

    /// Sets the item a control of window style `style` draws its face from in `look`.
    pub const fn set_face(&mut self, style: u16, look: Look, item: ItemId) {
        if (style as usize) < style::COUNT {
            self.faces[style as usize][look.index()] = Some(item);
        }
    }

    /// Sets the item its frame draws from.
    pub const fn set_edge(&mut self, style: u16, item: ItemId) {
        if (style as usize) < style::COUNT {
            self.edges[style as usize] = Some(item);
        }
    }

    /// The item a control of window style `style` draws its face from in `look`, falling back to
    /// [`Look::Idle`].
    ///
    /// A control carries its style in its own record, and all but a handful take style 0.
    #[must_use]
    pub fn face(&self, style: u16, look: Look) -> Option<ItemId> {
        let row = self.faces.get(usize::from(style))?;
        row[look.index()].or(row[Look::Idle.index()])
    }

    /// The item its frame draws from.
    #[must_use]
    pub fn edge(&self, style: u16) -> Option<ItemId> {
        *self.edges.get(usize::from(style))?
    }

    /// Sets the item `picture` draws from.
    pub const fn set_picture(&mut self, picture: Picture, item: ItemId) {
        self.pictures[picture.index()] = Some(item);
    }

    /// The item `picture` draws from, or nothing when this skin has no art for it.
    ///
    /// A skin is allowed to be short of pictures. The original's is, because a window Robigo adds
    /// names pictures the 1996 data has no art for.
    #[must_use]
    pub fn picture(&self, picture: Picture) -> Option<ItemId> {
        self.pictures[picture.index()]
    }
}

impl Default for Skin {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_look_falls_back_to_idle() {
        let mut skin = Skin::new();
        assert_eq!(skin.item(Control::Button, Look::Idle), None);
        skin.set(Control::Button, Look::Idle, ItemId(7));
        assert_eq!(skin.item(Control::Button, Look::Pressed), Some(ItemId(7)));
        skin.set(Control::Button, Look::Pressed, ItemId(8));
        assert_eq!(skin.item(Control::Button, Look::Pressed), Some(ItemId(8)));
        assert_eq!(skin.item(Control::Button, Look::Idle), Some(ItemId(7)));
        assert_eq!(
            skin.item(Control::Tick, Look::Idle),
            None,
            "other controls stay unset"
        );
    }

    #[test]
    fn a_style_carries_a_face_of_its_own() {
        let mut skin = Skin::new();
        skin.set_face(1, Look::Idle, ItemId(5));
        skin.set_edge(1, ItemId(6));
        assert_eq!(skin.face(1, Look::Pressed), Some(ItemId(5)), "a look falls back");
        assert_eq!(skin.edge(1), Some(ItemId(6)));
        assert_eq!(skin.face(0, Look::Idle), None, "other styles stay unset");
        let past = u16::try_from(style::COUNT).expect("the table is short");
        skin.set_face(past, Look::Idle, ItemId(7));
        assert_eq!(
            skin.face(past, Look::Idle),
            None,
            "a style past the table has none"
        );
    }

    #[test]
    fn every_control_and_look_has_its_own_place() {
        let indices: Vec<_> = Control::ALL.iter().map(|c| c.index()).collect();
        assert_eq!(indices, (0..Control::COUNT).collect::<Vec<_>>());
        let looks: Vec<_> = Look::ALL.iter().map(|l| l.index()).collect();
        assert_eq!(looks, (0..Look::COUNT).collect::<Vec<_>>());
    }
}
