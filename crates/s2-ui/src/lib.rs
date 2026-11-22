//! The interface: the painter, the skin and the game's windows.
//!
//! The structure is Robigo's own. `S2.EXE` keeps a retained widget tree with a class and a vtable
//! per control, and its window set is a dense table whose spare slots cannot be filled from
//! outside the build. Robigo needs the opposite property, because multiplayer, settings, key
//! bindings and its own diagnostic screens all arrive after the original's window set is
//! complete. What has to match is the pixels and the result of a click. See `docs/design/ui.md`
//! for the reasoning.
//!
//! A window emits [`Op`] values into a [`Painter`] rather than writing pixels, and [`paint`]
//! turns them into palette indices in the same frame the map view draws into.

#![forbid(unsafe_code)]

pub mod bar;
pub mod border;
pub mod chrome;
pub mod geometry;
pub mod group;
pub mod icons;
pub mod image;
pub mod items;
pub mod keys;
pub mod layout;
pub mod mouse_pointer;
pub mod paint;
pub mod panel;
pub mod picture;
pub mod screens;
pub mod skin;
pub mod store;
pub mod style;
pub mod text;
pub mod toolbar;
pub mod widget;
pub mod windows;

pub use geometry::Rect;
pub use image::{Image, ImageId, Images};
pub use items::{FONT_NAMES, Items, Texts, fonts, skin, skin_with};
pub use keys::{Action, Bound, Cheat, Scancode};
pub use layout::{Axis, Flow, Grid};
pub use mouse_pointer::{MousePointer, PointerButton, PointerPicture};
pub use paint::{Canvas, ItemId, Op, OpKind, Painter, paint};
pub use panel::{Gutter, MOST_LINES, TextPanel};
pub use picture::Picture;
pub use skin::{Control, Look, Skin};
pub use store::{Drag, ROLLED_HEIGHT, Ui, Window};
pub use style::{Lettering, NO_FRAME, Style, Styles};
pub use text::{FontId, Fonts, Glyphs, LAST_MEASURED, Label, Labels, NoLabels, Pen, Runs, WithRuns};
pub use widget::{Button, Pointer, Press, WidgetId, WindowId};
pub use windows::{Outcome, kind};
