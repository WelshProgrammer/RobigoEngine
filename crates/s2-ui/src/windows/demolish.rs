//! The prompt that asks before a building or a flag goes.

use s2_render::terrain::Point;
use s2_sim::{BuildingKind, Command, MapObject, Node, World};

use crate::geometry::Rect;
use crate::paint::{Align, Painter, Sheet};
use crate::picture::Picture;
use crate::skin::Skin;
use crate::store::Ui;
use crate::text::{Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{BUILDING_NAMES, Outcome, kind};

/// What the prompt asks about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    /// A building or a site, which the first button demolishes.
    Building,
    /// A flag, which the first button removes with its roads and its building.
    Flag,
}

/// What the prompt shows, read without a renderer so a test can assert it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemolishView {
    /// The node the prompt was opened on.
    pub node: Node,
    /// What goes if the first button is pressed.
    pub subject: Subject,
    /// The building the prompt pictures, which a flag with nothing at it leaves empty.
    pub building: Option<BuildingKind>,
    /// The building's graphics group, which names its picture in the nation archive.
    pub group: Option<u16>,
}

impl DemolishView {
    /// What the prompt shows for `node`: the building standing there, or the building at the flag
    /// when the prompt is for the flag.
    #[must_use]
    pub fn read(world: &World, node: Node, subject: Subject) -> Self {
        let record = match (subject, world.object(node)) {
            (Subject::Building, MapObject::Building(record)) => Some(record),
            (Subject::Flag, MapObject::Flag(flag)) => world.building_at_flag(flag),
            _ => None,
        };
        let building = record
            .and_then(|record| world.building(record))
            .map(|building| building.kind);
        Self {
            node,
            subject,
            building,
            group: building
                .and_then(|kind| world.building_types().get(kind))
                .map(|row| row.group),
        }
    }

    /// The command the first button sends.
    #[must_use]
    pub const fn command(&self) -> Command {
        match self.subject {
            Subject::Building => Command::Demolish { node: self.node },
            Subject::Flag => Command::RemoveFlag { node: self.node },
        }
    }
}

/// The nation archive's first building item. Group `g` stands at `250 + 5·g` with its shadow
/// one after it.
const FIRST_BUILDING_ITEM: u16 = 250;

/// How many nation items each graphics group takes.
const ITEMS_PER_GROUP: u16 = 5;

/// How far above the picture's bottom edge the building's origin goes (`0x58181`).
const BUILDING_UP: i32 = 35;

/// How far down the strip the name draws.
const NAME_ROW: i32 = 5;

/// The text slot all three buttons report as their line of help, which is the one the original
/// passes for each of them.
const BUTTON_HELP: u16 = 219;

/// The demolish prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemolishWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
}

impl DemolishWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 200;
    /// How tall the original's window is.
    pub const HEIGHT: u32 = 200;
    /// Where the picture of the building sits and how big it is.
    const PICTURE: (i32, i32, u32, u32) = (12, 22, 176, 120);
    /// Where the strip under the picture sits and how big it is.
    const STRIP: (i32, i32, u32, u32) = (12, 120, 176, 20);
    /// How far down the three buttons sit, and how tall they are.
    const BUTTON_ROW: (i32, u32) = (140, 40);

    /// The window at `id`.
    #[must_use]
    pub const fn new(id: WindowId) -> Self {
        Self { id }
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where the picture of the building sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn picture(at: Point) -> Rect {
        let (x, y, width, height) = Self::PICTURE;
        Rect::new(at.x + x, at.y + y, width, height)
    }

    /// Where the strip under the picture sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn strip(at: Point) -> Rect {
        let (x, y, width, height) = Self::STRIP;
        Rect::new(at.x + x, at.y + y, width, height)
    }

    /// Where button `press` sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn button(press: Press, at: Point) -> Rect {
        let (y, height) = Self::BUTTON_ROW;
        let (x, width) = press.place();
        Rect::new(at.x + x, at.y + y, width, height)
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release does.
    pub fn run(
        &self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &DemolishView,
        mouse: Pointer,
    ) -> Option<Outcome> {
        let window = *ui.window(self.id)?;
        let area = window.area;
        painter.push_clip(area);
        if window.rolled() {
            painter.pop_clip();
            return None;
        }
        let at = Point::new(area.x, area.y);
        if let Some(group) = view.group {
            let picture = Self::picture(at);
            let body = FIRST_BUILDING_ITEM + ITEMS_PER_GROUP * group;
            let half = i32::try_from(picture.width / 2).unwrap_or_default();
            let origin = Point::new(picture.x + half, picture.bottom() - BUILDING_UP);
            painter.push_clip(picture);
            painter.shadow(Sheet::Nation, body + 1, origin);
            painter.game(Sheet::Nation, body, origin);
            painter.pop_clip();
        }
        let strip = Self::strip(at);
        if let Some(kind) = view.building {
            let look = painter.first_lettering();
            painter.text_aligned(
                Point::new(strip.x, strip.y + NAME_ROW),
                look.font,
                look.colour,
                Label::Named(BUILDING_NAMES + u16::from(kind.0)),
                Pen::TIGHT,
                Align::CentredIn(strip.width),
            );
        }
        let mut outcome = None;
        for press in Press::ALL {
            let place = Self::button(press, at);
            let widget = WidgetId::new(self.id, press.slot());
            ui.note_help(place, mouse, Label::Slot(BUTTON_HELP));
            let acted = ui.press(widget, place, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.face(press.style(), look) {
                painter.frame_edges(item, skin.edge(press.style()), place, look.edges());
            }
            if let Some(item) = skin.picture(press.picture()) {
                painter.item_centred(item, place);
            }
            if acted && outcome.is_none() {
                outcome = Some(match press {
                    Press::Yes => Outcome::CommandAndClose(view.command(), self.id),
                    Press::No => Outcome::Close(self.id),
                    Press::Show => Outcome::Centre(view.node),
                });
            }
        }
        painter.pop_clip();
        outcome
    }
}

/// One of the three buttons along the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// Goes ahead, in style 1 where the other two draw in style 0.
    Yes,
    /// Closes the prompt.
    No,
    /// Moves the view to the node.
    Show,
}

impl Press {
    /// The three buttons, left to right.
    pub const ALL: [Self; 3] = [Self::Yes, Self::No, Self::Show];

    /// The picture that draws it.
    #[must_use]
    pub const fn picture(self) -> Picture {
        match self {
            Self::Yes => Picture::CONFIRM,
            Self::No => Picture::CANCEL,
            Self::Show => Picture::GO_TO,
        }
    }

    /// Where it sits across the window and how wide it is.
    const fn place(self) -> (i32, u32) {
        match self {
            Self::Yes => (14, 66),
            Self::No => (82, 66),
            Self::Show => (150, 36),
        }
    }

    /// The style it draws in: 1 for the button that goes ahead, 0 for the other two.
    const fn style(self) -> u16 {
        match self {
            Self::Yes => 1,
            Self::No | Self::Show => 0,
        }
    }

    /// Its widget slot.
    const fn slot(self) -> u16 {
        self as u16
    }
}

/// The kind the prompt opens as.
pub const KIND: WindowId = kind::DEMOLISH;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ItemId;
    use crate::skin::{Control, Look};

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set_face(0, Look::Idle, ItemId(2));
        skin.set_face(1, Look::Idle, ItemId(4));
        skin
    }

    fn open() -> (DemolishWindow, Ui, Painter) {
        let mut ui = Ui::new();
        let (width, height) = DemolishWindow::size();
        ui.open(KIND, Rect::new(0, 0, width, height));
        (
            DemolishWindow::new(KIND),
            ui,
            Painter::new(Rect::new(0, 0, 640, 480)),
        )
    }

    fn view(subject: Subject) -> DemolishView {
        DemolishView {
            node: Node { x: 7, y: 9 },
            subject,
            building: Some(BuildingKind::SAWMILL),
            group: Some(21),
        }
    }

    fn click(
        window: DemolishWindow,
        ui: &mut Ui,
        painter: &mut Painter,
        view: DemolishView,
        place: Rect,
    ) -> Option<Outcome> {
        let on = Pointer::at(Point::new(place.x + 1, place.y + 1));
        window.run(ui, painter, &skin(), &view, on.pressing());
        painter.reset();
        window.run(ui, painter, &skin(), &view, on.releasing())
    }

    #[test]
    fn the_prompt_pictures_the_building_either_way() {
        use s2_sim::{Building, BuildingState, Flag, FlagKind, Player};

        let mut world = World::empty(32, 32);
        let player = Player::new(0).expect("player 0");
        let at = Node { x: 10, y: 10 };
        let gate = Node { x: 10, y: 11 };
        let flag = world
            .add_flag(Flag::new(FlagKind::Normal, player, gate))
            .expect("a free flag slot");
        let record = world
            .add_building(Building::new(
                BuildingKind::SAWMILL,
                player,
                at,
                flag,
                BuildingState::empty(),
            ))
            .expect("a free building slot");
        world.set_object(at, MapObject::Building(record));
        world.set_object(gate, MapObject::Flag(flag));

        let view = DemolishView::read(&world, at, Subject::Building);
        assert_eq!(view.building, Some(BuildingKind::SAWMILL));
        assert_eq!(view.command(), Command::Demolish { node: at });

        let view = DemolishView::read(&world, gate, Subject::Flag);
        assert_eq!(view.building, Some(BuildingKind::SAWMILL));
        assert_eq!(view.command(), Command::RemoveFlag { node: gate });

        let bare = Node { x: 20, y: 20 };
        assert_eq!(DemolishView::read(&world, bare, Subject::Building).building, None);
        assert_eq!(DemolishView::read(&world, bare, Subject::Flag).building, None);
    }

    #[test]
    fn the_building_stands_over_its_shadow_and_the_name_has_no_frame() {
        use crate::paint::OpKind;

        let (window, mut ui, mut painter) = open();
        window.run(
            &mut ui,
            &mut painter,
            &skin(),
            &view(Subject::Building),
            Pointer::default(),
        );
        let origin = Point::new(12 + 88, 22 + 120 - 35);
        let drawn: Vec<_> = painter
            .ops()
            .iter()
            .filter_map(|op| match op.kind {
                OpKind::Shadow { sheet, slot, at } | OpKind::Game { sheet, slot, at } => {
                    Some((sheet, slot, at))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            drawn,
            [(Sheet::Nation, 356, origin), (Sheet::Nation, 355, origin)]
        );
        let strip = DemolishWindow::strip(Point::new(0, 0));
        assert!(
            !painter
                .ops()
                .iter()
                .any(|op| matches!(op.kind, OpKind::Frame { within, .. } if within == strip))
        );
    }

    #[test]
    fn the_parts_sit_where_the_original_puts_them() {
        let at = Point::new(0, 0);
        assert_eq!(DemolishWindow::picture(at), Rect::new(12, 22, 176, 120));
        assert_eq!(DemolishWindow::strip(at), Rect::new(12, 120, 176, 20));
        assert_eq!(DemolishWindow::button(Press::Yes, at), Rect::new(14, 140, 66, 40));
        assert_eq!(DemolishWindow::button(Press::No, at), Rect::new(82, 140, 66, 40));
        assert_eq!(
            DemolishWindow::button(Press::Show, at),
            Rect::new(150, 140, 36, 40)
        );
        let width = i32::try_from(DemolishWindow::WIDTH).expect("fits");
        for press in Press::ALL {
            let place = DemolishWindow::button(press, at);
            assert!(place.right() <= width, "{place:?}");
        }
        let slots: Vec<_> = Press::ALL.iter().map(|press| press.slot()).collect();
        assert_eq!(slots, [0, 1, 2]);
    }

    #[test]
    fn the_first_button_sends_the_command_the_subject_asks_for_and_closes_the_prompt() {
        let (window, mut ui, mut painter) = open();
        let place = DemolishWindow::button(Press::Yes, Point::new(0, 0));
        let node = Node { x: 7, y: 9 };
        assert_eq!(
            click(window, &mut ui, &mut painter, view(Subject::Building), place),
            Some(Outcome::CommandAndClose(Command::Demolish { node }, KIND))
        );
        assert_eq!(
            click(window, &mut ui, &mut painter, view(Subject::Flag), place),
            Some(Outcome::CommandAndClose(Command::RemoveFlag { node }, KIND))
        );
    }

    #[test]
    fn the_other_two_close_the_prompt_and_move_the_view() {
        let (window, mut ui, mut painter) = open();
        let at = Point::new(0, 0);
        let view = view(Subject::Building);
        assert_eq!(
            click(
                window,
                &mut ui,
                &mut painter,
                view,
                DemolishWindow::button(Press::No, at)
            ),
            Some(Outcome::Close(KIND))
        );
        assert_eq!(
            click(
                window,
                &mut ui,
                &mut painter,
                view,
                DemolishWindow::button(Press::Show, at)
            ),
            Some(Outcome::Centre(Node { x: 7, y: 9 }))
        );
    }

    #[test]
    fn a_rolled_up_prompt_draws_nothing_and_takes_no_press() {
        let (window, mut ui, mut painter) = open();
        ui.roll(KIND);
        let place = DemolishWindow::button(Press::Yes, Point::new(0, 0));
        assert_eq!(
            click(window, &mut ui, &mut painter, view(Subject::Building), place),
            None
        );
        assert!(
            painter.ops().is_empty(),
            "a rolled prompt draws nothing of its own"
        );
    }
}
