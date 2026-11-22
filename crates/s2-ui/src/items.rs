//! The `IO.DAT` items the interface draws from, found by the names the original uses.
//!
//! `S2.EXE` never addresses `IO.DAT` by position. A window style record, a control record and an
//! action menu record all store a name, and `0x50128` hands it to the registry lookup `0x8dee5`.
//! The names a control record carries are short, so `0x5013c` copies the template `iocon_000` and
//! overwrites its last three characters: a record naming `io067` reaches the item named `iocon_067`.
//!
//! [`Items`] keeps the sheet and that lookup together. A slot of the sheet is an entry's place in
//! the index, which is also what an [`ItemId`] carries.

use std::collections::BTreeMap;

use s2_assets::{Sprite, SpriteSheet};
use s2_formats::idx::Index;
use s2_formats::lst::Item;
use s2_formats::text::StringTable;

use crate::paint::ItemId;
use crate::picture::{self, Picture};
use crate::skin::{Control, Look, Skin};
use crate::style::Styles;
use crate::text::{Fonts, Glyphs, Labels};

/// The template name `0x5013c` copies before overwriting its last three characters.
const CONTROL_TEMPLATE: &str = "iocon_";

/// How many characters of a control record's name name the item.
const CONTROL_DIGITS: usize = 3;

/// Which style record the toolbar's buttons draw their face from.
const TOOLBAR_STYLE: u16 = 2;

/// The item the graphs draw their paper from.
const GRAPH_PAPER: &str = "iobob_00";

/// The pieces of a window's own frame, which `0x8e92c` looks up in the `RESOURCE` registry, and
/// the control each one draws.
const CHROME: [(Control, &str); 7] = [
    (Control::TitleLeft, "leftbord"),
    (Control::TitleRight, "rghtbord"),
    (Control::Title, "titlepas"),
    (Control::SideLeft, "leftfram"),
    (Control::SideRight, "rghtfram"),
    (Control::BottomEdge, "bottfram"),
    (Control::Pattern, "patter01"),
];

/// The title bar in the other two states its name ends in: the one a window in front shows, and
/// the one it shows while it is being dragged.
const TITLE_STATES: [(Look, &str); 2] = [(Look::On, "titleact"), (Look::Pressed, "titleslc")];

/// The four corner buttons. A name ends in `up` untouched, `dn` held down and `sl` with the
/// pointer on it.
const CORNERS: [(Control, &str); 4] = [
    (Control::Close, "clos"),
    (Control::Roll, "icon"),
    (Control::Resize, "zoom"),
    (Control::Blank, "dmmy"),
];

/// The three endings a corner button's name takes, and the look each one draws.
const CORNER_STATES: [(Look, &str); 3] = [
    (Look::Idle, "icup"),
    (Look::Pressed, "icdn"),
    (Look::Hover, "icsl"),
];

/// The names of the three fonts in `RESOURCE.DAT`, smallest first.
pub const FONT_NAMES: [&str; 3] = ["font09", "font11", "font14"];

/// The items of one named index, with the names the original finds them by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Items {
    sheet: SpriteSheet,
    names: BTreeMap<Box<str>, ItemId>,
}

impl Items {
    /// Builds the sheet from `index`, one slot per entry in the index's own order.
    ///
    /// An entry holding something other than a picture takes an empty slot, which keeps a slot
    /// number equal to an entry's place. An entry whose name is not plain ASCII is left out of
    /// the lookup and keeps its slot.
    #[must_use]
    pub fn from_index(index: &Index) -> Self {
        let mut names = BTreeMap::new();
        let mut slots = Vec::with_capacity(index.len());
        for (slot, entry) in index.entries().iter().enumerate() {
            slots.push(Sprite::from_item(entry.item()));
            if let (Some(name), Ok(id)) = (entry.name(), u16::try_from(slot)) {
                names.insert(name.into(), ItemId(id));
            }
        }
        Self {
            sheet: SpriteSheet::from_slots(slots),
            names,
        }
    }

    /// Builds the sheet from named pictures, one slot each in the order given. A name already
    /// taken keeps the picture it had.
    #[must_use]
    pub fn from_named(pictures: impl IntoIterator<Item = (String, Sprite)>) -> Self {
        let mut names = BTreeMap::new();
        let mut slots = Vec::new();
        for (name, sprite) in pictures {
            let Ok(id) = u16::try_from(slots.len()) else {
                break;
            };
            slots.push(Some(sprite));
            names.entry(name.into_boxed_str()).or_insert(ItemId(id));
        }
        Self {
            sheet: SpriteSheet::from_slots(slots),
            names,
        }
    }

    /// The item `name` draws from.
    ///
    /// A name in the index is taken as it stands. A control record's `ioNNN` reaches `iocon_NNN`
    /// the way `0x5013c` builds it.
    #[must_use]
    pub fn id(&self, name: &str) -> Option<ItemId> {
        if let Some(id) = self.names.get(name) {
            return Some(*id);
        }
        let digits = name.strip_prefix("io")?;
        if digits.len() != CONTROL_DIGITS || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut full = String::with_capacity(CONTROL_TEMPLATE.len() + CONTROL_DIGITS);
        full.push_str(CONTROL_TEMPLATE);
        full.push_str(digits);
        self.names.get(full.as_str()).copied()
    }

    /// The sheet an [`ItemId`] indexes.
    #[must_use]
    pub fn sheet(&self) -> &SpriteSheet {
        &self.sheet
    }

    /// How many slots the sheet holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sheet.len()
    }

    /// Whether the index held nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sheet.is_empty()
    }

    /// How many slots carry a picture.
    #[must_use]
    pub fn drawn(&self) -> usize {
        self.sheet.iter().count()
    }

    /// Adds the entries of `index` after the ones already held, so one sheet carries the items
    /// of more than one of the game's files. A name already taken keeps the item it had.
    #[must_use]
    pub fn and_index(mut self, index: &Index) -> Self {
        let base = self.sheet.len();
        let mut slots: Vec<_> = (0..base).map(|slot| self.sheet.get(slot).cloned()).collect();
        for (slot, entry) in index.entries().iter().enumerate() {
            slots.push(Sprite::from_item(entry.item()));
            if let (Some(name), Ok(id)) = (entry.name(), u16::try_from(base + slot)) {
                self.names.entry(name.into()).or_insert(ItemId(id));
            }
        }
        self.sheet = SpriteSheet::from_slots(slots);
        self
    }
}

/// The three fonts of a `RESOURCE` index, smallest first.
///
/// Returns nothing unless all three are present and hold a font.
#[must_use]
pub fn fonts(index: &Index) -> Option<Fonts> {
    let mut found = Vec::with_capacity(FONT_NAMES.len());
    for name in FONT_NAMES {
        let Item::Font(font) = index.get(name)?.item() else {
            return None;
        };
        found.push(Glyphs::from_font(font));
    }
    let mut taken = found.into_iter();
    let (small, medium, large) = (taken.next()?, taken.next()?, taken.next()?);
    Some(Fonts::new(small, medium, large))
}

/// The skin the shipped data gives: the chrome every control draws its face from, and the picture
/// each [`Picture`] draws over it.
///
/// Every control here takes style 0, the look all but a handful of the original's controls carry.
/// The style index a particular control asks for sits in its own record, and a window that wants
/// another style sets it over this.
///
/// A picture the shipped data has no art for is left unset.
#[must_use]
pub fn skin(items: &Items) -> Skin {
    skin_with(items, &Styles::ORIGINAL, &picture::ORIGINAL)
}

/// The skin `styles` and `pictures` give over `items`: [`skin`] with each style's items and each
/// picture's item, by [`Picture::index`], taken from them.
#[must_use]
pub fn skin_with(items: &Items, styles: &Styles, pictures: &[impl AsRef<str>]) -> Skin {
    let mut skin = Skin::new();
    let first = &styles.all()[0];
    let toolbar = &styles.all()[usize::from(TOOLBAR_STYLE)];
    let face = items.id(&first.background);
    let other = items.id(&first.alternate);
    for control in Control::ALL.into_iter().filter(|c| !c.is_chrome()) {
        if let Some(face) = face {
            skin.set(control, Look::Idle, face);
        }
        if let Some(other) = other {
            skin.set(control, Look::Hover, other);
            skin.set(control, Look::On, other);
        }
    }
    if let Some(frame) = items.id(&first.frame) {
        skin.set(Control::Edge, Look::Idle, frame);
    }
    for (index, style) in styles.all().iter().enumerate() {
        let Ok(index) = u16::try_from(index) else {
            continue;
        };
        if let Some(face) = items.id(&style.background) {
            skin.set_face(index, Look::Idle, face);
        }
        if let Some(other) = items.id(&style.alternate) {
            skin.set_face(index, Look::Hover, other);
            skin.set_face(index, Look::On, other);
        }
        if let Some(frame) = items.id(&style.frame) {
            skin.set_edge(index, frame);
        }
    }
    if let Some(face) = items.id(&toolbar.background) {
        skin.set(Control::Toolbar, Look::Idle, face);
    }
    if let Some(other) = items.id(&toolbar.alternate) {
        skin.set(Control::Toolbar, Look::Hover, other);
        skin.set(Control::Toolbar, Look::On, other);
    }
    if let Some(paper) = items.id(GRAPH_PAPER) {
        skin.set(Control::GraphPaper, Look::Idle, paper);
    }
    for (control, name) in CHROME {
        if let Some(item) = items.id(name) {
            skin.set(control, Look::Idle, item);
        }
    }
    for (look, name) in TITLE_STATES {
        if let Some(item) = items.id(name) {
            skin.set(Control::Title, look, item);
        }
    }
    for (control, stem) in CORNERS {
        for (look, ending) in CORNER_STATES {
            if let Some(item) = items.id(&format!("{stem}{ending}")) {
                skin.set(control, look, item);
            }
        }
    }
    for (place, name) in pictures.iter().enumerate() {
        if let (Ok(number), Some(item)) = (u16::try_from(place), items.id(name.as_ref())) {
            skin.set_picture(Picture(number), item);
        }
    }
    skin
}

/// A string table read from one of the game's `.ENG` files, as the windows' text slots index it.
#[derive(Debug, Clone)]
pub struct Texts {
    table: StringTable,
    names: Option<StringTable>,
}

impl Texts {
    /// Takes `table` as the slot source.
    #[must_use]
    pub const fn new(table: StringTable) -> Self {
        Self { table, names: None }
    }

    /// The same with `names` as the names table (`DATA/TXT/NAMES`).
    #[must_use]
    pub fn with_names(mut self, names: StringTable) -> Self {
        self.names = Some(names);
        self
    }

    /// The table behind it.
    #[must_use]
    pub const fn table(&self) -> &StringTable {
        &self.table
    }
}

impl Labels for Texts {
    fn slot(&self, slot: u16) -> &[u8] {
        self.table.raw(usize::from(slot)).unwrap_or_default()
    }

    fn named(&self, slot: u16) -> &[u8] {
        self.names
            .as_ref()
            .and_then(|names| names.raw(usize::from(slot)))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An index of `count` entries, named `iocon_000` upwards, each holding a one-pixel bitmap.
    fn index(count: usize) -> Index {
        let mut idx = Vec::new();
        let mut dat = Vec::new();
        idx.extend_from_slice(&u32::try_from(count).unwrap().to_le_bytes());
        for i in 0..count {
            let mut name = [0_u8; 16];
            let text = format!("iocon_{i:03}");
            name[..text.len()].copy_from_slice(text.as_bytes());
            idx.extend_from_slice(&name);
            idx.extend_from_slice(&u32::try_from(dat.len()).unwrap().to_le_bytes());
            idx.extend_from_slice(&[0; 6]);
            idx.extend_from_slice(&2_u16.to_le_bytes());
            // One 1 x 1 bitmap: nx, ny, zero, width, height, then the row offsets and one run.
            dat.extend_from_slice(&0_i16.to_le_bytes());
            dat.extend_from_slice(&0_i16.to_le_bytes());
            dat.extend_from_slice(&0_u32.to_le_bytes());
            dat.extend_from_slice(&1_u16.to_le_bytes());
            dat.extend_from_slice(&1_u16.to_le_bytes());
            dat.extend_from_slice(&6_u32.to_le_bytes());
            dat.extend_from_slice(&[0, 1, 1, 0xff, 0, 0]);
        }
        Index::parse(&idx, &dat).expect("the synthetic index parses")
    }

    #[test]
    fn a_slot_is_an_entrys_place_in_the_index() {
        let items = Items::from_index(&index(4));
        assert_eq!(items.len(), 4);
        assert_eq!(items.id("iocon_000"), Some(ItemId(0)));
        assert_eq!(items.id("iocon_003"), Some(ItemId(3)));
    }

    #[test]
    fn a_control_records_name_reaches_the_long_name() {
        let items = Items::from_index(&index(70));
        assert_eq!(
            items.id("io067"),
            items.id("iocon_067"),
            "`0x5013c` builds one from the other"
        );
        assert_eq!(items.id("io000"), Some(ItemId(0)));
    }

    #[test]
    fn a_name_the_index_does_not_hold_draws_nothing() {
        let items = Items::from_index(&index(4));
        assert_eq!(items.id("io999"), None);
        assert_eq!(items.id("back_01"), None);
        assert_eq!(items.id("io67"), None, "only the three-digit form");
        assert_eq!(items.id("ioabc"), None);
        assert_eq!(items.id("io"), None);
    }

    #[test]
    fn an_empty_index_gives_an_empty_sheet() {
        let items = Items::from_index(&index(0));
        assert!(items.is_empty());
        assert_eq!(items.id("io000"), None);
    }

    /// A table of two slots: the first holds `text`, the second is empty.
    fn texts(text: &[u8]) -> Texts {
        let mut block = Vec::new();
        block.extend_from_slice(&8_u32.to_le_bytes());
        block.extend_from_slice(&0_u32.to_le_bytes());
        block.extend_from_slice(text);
        block.push(0);
        let mut file = Vec::new();
        file.extend_from_slice(&s2_formats::text::MAGIC.to_le_bytes());
        file.extend_from_slice(&2_u16.to_le_bytes());
        file.extend_from_slice(&1_u16.to_le_bytes());
        file.extend_from_slice(&u32::try_from(block.len()).unwrap().to_le_bytes());
        file.extend_from_slice(&block);
        Texts::new(StringTable::parse(&file).expect("the synthetic table parses"))
    }

    #[test]
    fn a_slot_gives_back_its_own_bytes() {
        let texts = texts(b"Help");
        assert_eq!(texts.slot(0), b"Help");
    }

    #[test]
    fn an_empty_or_missing_slot_draws_nothing() {
        let texts = texts(b"Help");
        assert_eq!(texts.slot(1), b"", "offset 0 marks an empty slot");
        assert_eq!(texts.slot(571), b"", "past the end of the table");
    }
}
