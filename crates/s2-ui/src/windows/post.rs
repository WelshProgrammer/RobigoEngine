//! The post office: the letters a player has been sent.
//!
//! The letters live in the sim, so taking one off the list has to arrive as a command rather than
//! as a press this window acts on by itself. Until that command exists the window shows the
//! letters, filters them and walks them.

use s2_render::terrain::Point;
use s2_sim::{Command, MESSAGE_IDS, Message, MessageTypes, Player, World};

use crate::geometry::Rect;
use crate::paint::{Align, Painter};
use crate::picture::Picture;
use crate::skin::{Control, Look, Skin};
use crate::store::Ui;
use crate::text::{FontId, Label, Pen};
use crate::widget::{Pointer, WidgetId, WindowId};
use crate::windows::{Outcome, kind};

/// The help page the window's own button opens (`0x64de5`).
const HELP_PAGE: u16 = 1007;

/// The text slot the scroll shows while the list is empty (`0x65a7c`).
const NO_LETTERS: u16 = 469;

/// The palette index that line draws in, which `0x508fc` puts on the canvas before the draw.
const SCROLL_TEXT: u8 = 26;

/// How far apart the lines of an open letter sit.
const LINE: i32 = 14;

/// The slot the branch for each letter id loads, in id order. Ids 1 and 3 load more than one.
const WRITTEN: [&[u16]; MESSAGE_IDS] = [
    &[462],
    &[463],
    &[470],
    &[471, 472, 476],
    &[473],
    &[474],
    &[475],
    &[477],
    &[478],
    &[479],
    &[480],
    &[481],
    &[482],
    &[483],
    &[484],
    &[487],
    &[486],
    &[488],
    &[489],
    &[490],
    &[491],
    &[492],
    &[494],
    &[495],
    &[470],
];

/// The first of the five slots a geologist's find picks from with the record's own value.
const FOUND_SLOT: u16 = 464;

/// How many finds those five slots cover.
const FINDS: u32 = 5;

/// The lines `letter` is written as. A geologist's find is one line, the slot the record's
/// value picks put through the `%s` of the slot the branch loads (`0x654f5`).
#[must_use]
fn written(letter: Message) -> Vec<Label> {
    let slots: &[u16] = WRITTEN.get(usize::from(letter.id.get())).copied().unwrap_or(&[]);
    if letter.id == s2_sim::MessageId::GEOLOGIST && letter.value < FINDS {
        let found = u16::try_from(letter.value).unwrap_or_default();
        if let [format] = slots {
            return vec![Label::Filled(*format, FOUND_SLOT + found)];
        }
    }
    slots.iter().map(|&slot| Label::Slot(slot)).collect()
}

/// How many categories a letter can fall in.
pub const CATEGORIES: usize = 4;

/// Which letters the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shown {
    /// Every letter.
    #[default]
    All,
    /// The letters of one category.
    Category(u8),
}

impl Shown {
    /// The text slot the line of help shows while the pointer is over its button. The first
    /// stands for every letter and the four categories follow it (`0x1fd53c`).
    #[must_use]
    pub const fn help(self) -> u16 {
        match self {
            Self::All => 450,
            Self::Category(0) => 451,
            Self::Category(1) => 452,
            Self::Category(2) => 453,
            Self::Category(_) => 454,
        }
    }

    /// Every choice the window's buttons offer, in button order.
    pub const ALL: [Self; CATEGORIES + 1] = [
        Self::All,
        Self::Category(0),
        Self::Category(1),
        Self::Category(2),
        Self::Category(3),
    ];

    /// The `IO.DAT` item that draws its button.
    #[must_use]
    pub const fn picture(self) -> Picture {
        match self {
            Self::All => Picture::LETTERS_ALL,
            Self::Category(0) => Picture::LETTERS_MILITARY,
            Self::Category(1) => Picture::LETTERS_GEOLOGICAL,
            Self::Category(2) => Picture::LETTERS_ECONOMY,
            Self::Category(_) => Picture::LETTERS_VARIOUS,
        }
    }

    /// Whether it shows `letter`, whose list `types` gives.
    #[must_use]
    pub fn shows(self, letter: &Message, types: &MessageTypes) -> bool {
        match self {
            Self::All => true,
            Self::Category(category) => types.get(letter.id).category == category,
        }
    }

    /// Its place in the row of buttons.
    const fn place(self) -> usize {
        match self {
            Self::All => 0,
            Self::Category(category) => category as usize + 1,
        }
    }
}

/// One of the buttons the open letter carries, which the window builds only while a letter is
/// open (`0x65028`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carried {
    /// Takes the letter off the list.
    Forget,
    /// Moves the view to the node the letter is about.
    GoTo,
    /// Opens the screen that writes a game, which the two reminders carry.
    ToSave,
}

impl Carried {
    /// All three, right to left, in the order the original builds them.
    pub const ALL: [Self; 3] = [Self::Forget, Self::GoTo, Self::ToSave];

    /// The text slot the line of help shows while the pointer is over its button.
    #[must_use]
    pub const fn help(self) -> u16 {
        match self {
            Self::Forget => 461,
            Self::GoTo => 460,
            Self::ToSave => 485,
        }
    }

    /// The `IO.DAT` item that draws it.
    #[must_use]
    pub const fn picture(self) -> Picture {
        match self {
            Self::Forget => Picture::FORGET_LETTER,
            Self::GoTo => Picture::GO_TO,
            Self::ToSave => Picture::TO_SAVE,
        }
    }

    /// Whether `letter` carries it.
    #[must_use]
    pub const fn suits(self, letter: &Message) -> bool {
        match self {
            Self::Forget => true,
            Self::GoTo => letter.node.is_some(),
            Self::ToSave => letter.asks_to_save(),
        }
    }
}

/// One of the four buttons that walk the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Walk {
    /// The first letter.
    First,
    /// The letter before the chosen one.
    Back,
    /// The letter after it.
    On,
    /// The newest letter, which is also where a new choice of category lands.
    Last,
}

impl Walk {
    /// The text slot the line of help shows while the pointer is over its button.
    #[must_use]
    pub const fn help(self) -> u16 {
        match self {
            Self::First => 456,
            Self::Back => 457,
            Self::On => 458,
            Self::Last => 459,
        }
    }

    /// The four, left to right.
    pub const ALL: [Self; 4] = [Self::First, Self::Back, Self::On, Self::Last];

    /// The `IO.DAT` item that draws it.
    #[must_use]
    pub const fn picture(self) -> Picture {
        match self {
            Self::First => Picture::FIRST,
            Self::Back => Picture::PREVIOUS,
            Self::On => Picture::NEXT,
            Self::Last => Picture::LAST,
        }
    }

    /// Where it lands in a list of `count` letters with `chosen` chosen, or `None` when there is
    /// nowhere to go.
    #[must_use]
    pub fn landing(self, chosen: Option<usize>, count: usize) -> Option<usize> {
        let last = count.checked_sub(1)?;
        match self {
            Self::First => Some(0),
            Self::Back => chosen.unwrap_or(0).checked_sub(1),
            Self::On => chosen.map_or(Some(0), |at| (at < last).then_some(at + 1)),
            Self::Last => Some(last),
        }
    }
}

/// What the post office shows, read without a renderer so a test can assert it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostView {
    /// The letters the window shows, oldest first.
    pub letters: Vec<Message>,
}

impl PostView {
    /// `player`'s letters that `shown` keeps, whose lists `types` gives.
    #[must_use]
    pub fn read(world: &World, player: Player, shown: Shown, types: &MessageTypes) -> Self {
        let letters = world
            .messages(player)
            .list()
            .iter()
            .filter(|letter| shown.shows(letter, types))
            .copied()
            .collect();
        Self { letters }
    }
}

/// The post office.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostWindow {
    /// Which window kind it opens as.
    pub id: WindowId,
    /// Which letters it shows.
    pub shown: Shown,
    /// Which letter is open, as a place in the shown letters.
    pub chosen: Option<usize>,
    /// Whether the campaign's own button is built, which `0x1f78ea` decides.
    pub campaign: bool,
    /// Which list each kind of letter goes on.
    pub types: MessageTypes,
}

impl PostWindow {
    /// How wide the original's window is.
    pub const WIDTH: u32 = 252;
    /// How tall the original's window is.
    pub const HEIGHT: u32 = 284;
    /// Where the first filter button sits, where the next four start, how far apart they sit and
    /// how big one is.
    const FILTER: (i32, i32, i32, i32, u32) = (15, 55, 25, 36, 36);
    /// Where the campaign's own button sits.
    const MISSION: i32 = 201;
    /// Where the letters sit and how big the control is.
    const LETTERS: (i32, i32, u32, u32) = (11, 69, 230, 162);
    /// Where the walk buttons start, how far apart they sit and how big they are.
    const WALK: (i32, i32, i32, u32, u32) = (50, 240, 29, 28, 24);
    /// Where the help button sits above the bottom edge and how big it is.
    const HELP: (i32, i32, u32, u32) = (18, 48, 30, 32);
    /// Where the rightmost of the buttons the open letter carries sits, how far left the next
    /// one goes and how big one is (`0x65032`).
    const CARRIED: (i32, i32, u32) = (213, 24, 24);

    /// The window at `id`, showing every letter outside a campaign.
    #[must_use]
    pub const fn new(id: WindowId) -> Self {
        Self {
            id,
            shown: Shown::All,
            chosen: None,
            campaign: false,
            types: MessageTypes::ORIGINAL,
        }
    }

    /// The window, putting letters on the lists `types` gives.
    #[must_use]
    pub const fn with_types(mut self, types: MessageTypes) -> Self {
        self.types = types;
        self
    }

    /// How big the original's window is.
    #[must_use]
    pub const fn size() -> (u32, u32) {
        (Self::WIDTH, Self::HEIGHT)
    }

    /// Where the button for `shown` sits in a window whose top-left corner is `at`.
    #[must_use]
    pub fn filter(shown: Shown, at: Point) -> Rect {
        let (first, start, y, step, size) = Self::FILTER;
        let place = shown.place();
        let x = match place.checked_sub(1) {
            None => first,
            Some(after) => start + step * i32::try_from(after).unwrap_or_default(),
        };
        Rect::new(at.x + x, at.y + y, size, size)
    }

    /// Where the campaign's own button sits.
    #[must_use]
    pub fn mission(at: Point) -> Rect {
        let (_, _, y, _, size) = Self::FILTER;
        Rect::new(at.x + Self::MISSION, at.y + y, size, size)
    }

    /// Where the letters sit.
    #[must_use]
    pub fn letters(at: Point) -> Rect {
        let (x, y, width, height) = Self::LETTERS;
        Rect::new(at.x + x, at.y + y, width, height)
    }

    /// Where the button for `walk` sits.
    #[must_use]
    pub fn walk(walk: Walk, at: Point) -> Rect {
        let (x, y, step, width, height) = Self::WALK;
        let place = Walk::ALL.iter().position(|&one| one == walk).unwrap_or(0);
        let across = step * i32::try_from(place).unwrap_or_default();
        Rect::new(at.x + x + across, at.y + y, width, height)
    }

    /// The buttons `letter` carries, with where each one sits in a window whose top-left corner
    /// is `at`.
    ///
    /// They are laid out right to left, each one built taking a step off the x the next would
    /// use, so a letter about nowhere leaves its gap at the end of the row rather than in it
    /// (`0x650a7`, `0x6511d`).
    pub fn carried(letter: Message, at: Point) -> impl Iterator<Item = (Carried, Rect)> {
        let (right, step, size) = Self::CARRIED;
        let (_, y, _, _, _) = Self::WALK;
        Carried::ALL
            .into_iter()
            .filter(move |carried| carried.suits(&letter))
            .enumerate()
            .map(move |(place, carried)| {
                let across = step * i32::try_from(place).unwrap_or_default();
                (carried, Rect::new(at.x + right - across, at.y + y, size, size))
            })
    }

    /// Where the help button sits in a window of `area`.
    #[must_use]
    pub fn help(area: Rect) -> Rect {
        let (x, up, width, height) = Self::HELP;
        Rect::new(area.x + x, area.bottom() - up, width, height)
    }

    /// Takes `mouse`, draws the window into `painter` and gives back what a release does.
    pub fn run(
        &mut self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        view: &PostView,
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
        self.keep_one_open(view);
        let mut shown = None;
        for one in Shown::ALL {
            let button = Self::filter(one, at);
            let slot = Self::FILTER_SLOT + u16::try_from(one.place()).unwrap_or_default();
            let widget = WidgetId::new(self.id, slot);
            ui.note_help(button, mouse, Label::Slot(one.help()));
            let acted = ui.press(widget, button, mouse, true);
            // Nothing in the original marks the chosen category. Marking it is one of the
            // departures in docs/design/ui.md.
            let look = ui.button(widget).look(true, one == self.shown);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), button, look.edges());
            }
            if let Some(item) = skin.picture(one.picture()) {
                painter.item_centred(item, button);
            }
            if acted && shown.is_none() {
                shown = Some(one);
            }
        }
        let mut noted = false;
        if self.campaign {
            let button = Self::mission(at);
            let widget = WidgetId::new(self.id, Self::MISSION_SLOT);
            noted = ui.press(widget, button, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), button, look.edges());
            }
            if let Some(item) = skin.picture(Picture::CAMPAIGN_TASK) {
                painter.item_centred(item, button);
            }
        }
        let letters = Self::letters(at);
        if let Some(item) = skin.picture(Picture::SCROLL) {
            painter.item_centred(item, letters);
        }
        if view.letters.is_empty() {
            painter.text_aligned(
                Point::new(letters.x, letters.y),
                FontId::Medium,
                SCROLL_TEXT,
                Label::Slot(NO_LETTERS),
                Pen::TIGHT,
                Align::CentredInBox(letters.width, letters.height),
            );
        } else if let Some(letter) = self.chosen.and_then(|place| view.letters.get(place)) {
            Self::write(painter, letters, *letter);
        }
        let mut walked = None;
        for walk in Walk::ALL {
            let button = Self::walk(walk, at);
            let landing = walk.landing(self.chosen, view.letters.len());
            let slot = Self::WALK_SLOT + u16::try_from(walk as usize).unwrap_or_default();
            let widget = WidgetId::new(self.id, slot);
            ui.note_help(button, mouse, Label::Slot(walk.help()));
            let acted = ui.press(widget, button, mouse, landing.is_some());
            let look = ui.button(widget).look(landing.is_some(), false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), button, look.edges());
            }
            if let Some(item) = skin.picture(walk.picture()) {
                painter.item_centred(item, button);
            }
            if acted && walked.is_none() {
                walked = landing;
            }
        }
        let open = self.chosen.and_then(|at| view.letters.get(at)).copied();
        let carried_ask = open.and_then(|letter| self.carries(ui, painter, skin, (letter, at), mouse));
        let help = Self::help(area);
        let widget = WidgetId::new(self.id, Self::HELP_SLOT);
        ui.note_help(help, mouse, Label::Slot(crate::windows::HELP_BUTTON));
        let asked = ui.press(widget, help, mouse, true);
        let look = ui.button(widget).look(true, false);
        if let Some(item) = skin.item(Control::Button, look) {
            painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), help, look.edges());
        }
        if let Some(item) = skin.picture(Picture::HELP) {
            painter.item_centred(item, help);
        }
        painter.pop_clip();
        if let Some(shown) = shown {
            self.shown = shown;
            self.chosen = view.letters.len().checked_sub(1);
        }
        if let Some(landing) = walked {
            self.chosen = Some(landing);
        }
        carried_ask
            .or_else(|| asked.then_some(Outcome::Help(HELP_PAGE)))
            .or_else(|| noted.then_some(Outcome::Ask(kind::NOTE)))
    }

    /// Keeps a letter open while the list holds one: the newest, which is where a change of
    /// filter also leaves the window.
    fn keep_one_open(&mut self, view: &PostView) {
        let last = view.letters.len().checked_sub(1);
        match self.chosen {
            None => self.chosen = last,
            Some(place) if Some(place) > last => self.chosen = last,
            Some(_) => {}
        }
    }

    /// Writes the open letter across the scroll, one slot a line.
    ///
    /// A letter the original writes from several slots takes a line per slot here, since
    /// joining them would need the text table a window does not hold.
    fn write(painter: &mut Painter, letters: Rect, letter: Message) {
        let written = written(letter);
        let lines = i32::try_from(written.len()).unwrap_or_default();
        let height = i32::try_from(letters.height).unwrap_or_default();
        let top = letters.y + (height - lines * LINE) / 2;
        for (line, label) in written.into_iter().enumerate() {
            let down = i32::try_from(line).unwrap_or_default() * LINE;
            painter.text_aligned(
                Point::new(letters.x, top + down),
                FontId::Medium,
                SCROLL_TEXT,
                label,
                Pen::TIGHT,
                Align::CentredIn(letters.width),
            );
        }
    }

    /// Draws the buttons the open `letter` carries and gives back what a release asks for.
    fn carries(
        &self,
        ui: &mut Ui,
        painter: &mut Painter,
        skin: &Skin,
        (letter, at): (Message, Point),
        mouse: Pointer,
    ) -> Option<Outcome> {
        let mut asked = None;
        for (carried, button) in Self::carried(letter, at) {
            let slot = Self::CARRIED_SLOT + u16::try_from(carried as usize).unwrap_or_default();
            let widget = WidgetId::new(self.id, slot);
            ui.note_help(button, mouse, Label::Slot(carried.help()));
            let acted = ui.press(widget, button, mouse, true);
            let look = ui.button(widget).look(true, false);
            if let Some(item) = skin.item(Control::Button, look) {
                painter.frame_edges(item, skin.item(Control::Edge, Look::Idle), button, look.edges());
            }
            if let Some(item) = skin.picture(carried.picture()) {
                painter.item_centred(item, button);
            }
            if acted && asked.is_none() {
                asked = match carried {
                    Carried::Forget => Some(Outcome::Command(Command::ForgetMessage {
                        serial: letter.serial,
                    })),
                    Carried::GoTo => letter.node.map(Outcome::Centre),
                    Carried::ToSave => Some(Outcome::Open(kind::SAVE_GAME)),
                };
            }
        }
        asked
    }

    /// The widget slot the first filter button takes.
    const FILTER_SLOT: u16 = 0;
    /// The widget slot the campaign's own button takes.
    const MISSION_SLOT: u16 = 8;
    /// The widget slot the first walk button takes.
    const WALK_SLOT: u16 = 9;
    /// The widget slot the help button takes.
    const HELP_SLOT: u16 = 16;
    /// The widget slot the first of the open letter's own buttons takes.
    const CARRIED_SLOT: u16 = 17;
}

/// The kind the window opens as.
pub const KIND: WindowId = kind::POST;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ItemId;
    use s2_sim::{MessageId, Node};

    fn skin() -> Skin {
        let mut skin = Skin::new();
        skin.set(Control::Frame, Look::Idle, ItemId(1));
        skin.set(Control::Button, Look::Idle, ItemId(2));
        skin.set(Control::Tick, Look::Idle, ItemId(3));
        skin.set(Control::Tick, Look::On, ItemId(4));
        skin
    }

    fn open() -> (PostWindow, Ui, Painter) {
        let mut ui = Ui::new();
        let (width, height) = PostWindow::size();
        ui.open(KIND, Rect::new(0, 0, width, height));
        (PostWindow::new(KIND), ui, Painter::new(Rect::new(0, 0, 640, 480)))
    }

    fn letter(id: MessageId, serial: u32) -> Message {
        Message {
            id,
            owner: Player::new(0).expect("player 0"),
            step: serial,
            serial,
            node: None,
            building: None,
            value: 0,
        }
    }

    fn view(count: u32) -> PostView {
        PostView {
            letters: (0..count)
                .map(|serial| letter(MessageId::BUILDING_LOST, serial))
                .collect(),
        }
    }

    fn click(
        window: &mut PostWindow,
        ui: &mut Ui,
        painter: &mut Painter,
        view: &PostView,
        place: Rect,
    ) -> Option<Outcome> {
        let on = Pointer::at(Point::new(place.x + 1, place.y + 1));
        window.run(ui, painter, &skin(), view, on.pressing());
        painter.reset();
        let outcome = window.run(ui, painter, &skin(), view, on.releasing());
        painter.reset();
        outcome
    }

    #[test]
    fn the_controls_sit_where_the_original_puts_them() {
        let at = Point::new(0, 0);
        assert_eq!(PostWindow::filter(Shown::All, at), Rect::new(15, 25, 36, 36));
        assert_eq!(
            PostWindow::filter(Shown::Category(0), at),
            Rect::new(55, 25, 36, 36)
        );
        assert_eq!(
            PostWindow::filter(Shown::Category(3), at),
            Rect::new(163, 25, 36, 36)
        );
        assert_eq!(PostWindow::mission(at), Rect::new(201, 25, 36, 36));
        assert_eq!(PostWindow::letters(at), Rect::new(11, 69, 230, 162));
        assert_eq!(PostWindow::walk(Walk::First, at), Rect::new(50, 240, 28, 24));
        assert_eq!(PostWindow::walk(Walk::Last, at), Rect::new(137, 240, 28, 24));
        let area = Rect::new(0, 0, PostWindow::WIDTH, PostWindow::HEIGHT);
        assert_eq!(PostWindow::help(area), Rect::new(18, 236, 30, 32));
        let placed = |letter| PostWindow::carried(letter, at).collect::<Vec<_>>();
        assert_eq!(
            placed(letter(MessageId::BUILDING_LOST, 0)),
            [(Carried::Forget, Rect::new(213, 240, 24, 24))],
            "a letter about nowhere carries only the one that forgets it"
        );
        let mut somewhere = letter(MessageId::BUILDING_LOST, 0);
        somewhere.node = Some(Node { x: 1, y: 2 });
        assert_eq!(
            placed(somewhere),
            [
                (Carried::Forget, Rect::new(213, 240, 24, 24)),
                (Carried::GoTo, Rect::new(189, 240, 24, 24)),
            ]
        );
        assert_eq!(
            placed(letter(MessageId::UNSAVED_HOUR, 0)),
            [
                (Carried::Forget, Rect::new(213, 240, 24, 24)),
                (Carried::ToSave, Rect::new(189, 240, 24, 24)),
            ],
            "a reminder about nowhere closes the gap the go-to button leaves"
        );
    }

    #[test]
    fn an_open_letter_carries_the_buttons_that_suit_it() {
        let at = Point::new(0, 0);
        let (mut window, mut ui, mut painter) = open();
        let mut view = view(2);
        let forget = PostWindow::carried(letter(MessageId::BUILDING_LOST, 0), at)
            .next()
            .expect("the button that forgets a letter")
            .1;
        let second = Rect::new(forget.x - 24, forget.y, forget.width, forget.height);
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, &view, second),
            None,
            "a letter about nowhere leaves the button that goes there out"
        );
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, &view, forget),
            Some(Outcome::Command(Command::ForgetMessage { serial: 1 })),
        );
        let node = Node { x: 7, y: 9 };
        view.letters[1].node = Some(node);
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, &view, second),
            Some(Outcome::Centre(node)),
        );
        view.letters[1] = letter(MessageId::UNSAVED_HALF_HOUR, 1);
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, &view, second),
            Some(Outcome::Open(kind::SAVE_GAME)),
            "a reminder puts the button that writes a game in the same place"
        );
    }

    #[test]
    fn the_newest_letter_opens_by_itself_and_stays_inside_the_list() {
        let (mut window, mut ui, mut painter) = open();
        assert_eq!(window.chosen, None);
        window.run(&mut ui, &mut painter, &skin(), &view(3), Pointer::default());
        assert_eq!(window.chosen, Some(2), "the newest letter opens");
        window.run(&mut ui, &mut painter, &skin(), &view(1), Pointer::default());
        assert_eq!(window.chosen, Some(0), "a shorter list pulls the choice back");
        window.run(
            &mut ui,
            &mut painter,
            &skin(),
            &PostView { letters: Vec::new() },
            Pointer::default(),
        );
        assert_eq!(window.chosen, None, "an empty list leaves none open");
    }

    #[test]
    fn the_slots_a_letter_is_written_from_follow_its_id() {
        assert_eq!(
            written(letter(MessageId::UNSAVED_HALF_HOUR, 0)),
            vec![Label::Slot(473)]
        );
        let mut found = letter(MessageId::GEOLOGIST, 1);
        found.value = 3;
        assert_eq!(
            written(found),
            vec![Label::Filled(463, 464 + 3)],
            "a find goes through the %s of the slot the branch loads"
        );
        found.value = 9;
        assert_eq!(
            written(found),
            vec![Label::Slot(463)],
            "a value past the five finds fills nothing in"
        );
        assert_eq!(
            written(letter(MessageId::CATAPULT_HIT, 2)),
            vec![Label::Slot(495)],
            "the last two ids take the slots their branches load, skipping 493"
        );
        assert_eq!(
            written(letter(MessageId::UNDER_ATTACK, 3)),
            vec![Label::Slot(470)],
            "the last branch loads the same slot as id 2"
        );
    }

    #[test]
    fn a_filter_keeps_the_letters_of_its_category() {
        let military = letter(MessageId::BUILDING_LOST, 1);
        let found = letter(MessageId::GEOLOGIST, 2);
        let other = letter(MessageId::MINE_DRY, 3);
        let types = MessageTypes::ORIGINAL;
        let category = |letter: &Message| types.get(letter.id).category;
        assert_eq!(category(&military), 0);
        assert_eq!(category(&found), 1);
        assert_eq!(category(&other), 3, "a mine that gives up is one of the others");

        assert!(Shown::All.shows(&other, &types));
        assert!(Shown::Category(0).shows(&military, &types));
        assert!(!Shown::Category(0).shows(&found, &types));
        assert!(Shown::Category(1).shows(&found, &types));
        assert!(!Shown::Category(2).shows(&other, &types));
        assert!(Shown::Category(3).shows(&other, &types));
    }

    #[test]
    fn a_player_with_no_letters_has_none_to_show() {
        let world = World::empty(32, 32);
        let player = Player::new(0).expect("player 0");
        let letters = PostView::read(&world, player, Shown::All, &MessageTypes::ORIGINAL).letters;
        assert!(letters.is_empty(), "{letters:?}");
    }

    #[test]
    fn the_walk_buttons_land_where_they_should() {
        assert_eq!(Walk::First.landing(None, 0), None, "nothing to walk");
        assert_eq!(Walk::Last.landing(None, 0), None);
        assert_eq!(Walk::First.landing(Some(2), 5), Some(0));
        assert_eq!(Walk::Last.landing(Some(2), 5), Some(4));
        assert_eq!(Walk::Back.landing(Some(2), 5), Some(1));
        assert_eq!(Walk::Back.landing(Some(0), 5), None);
        assert_eq!(Walk::On.landing(Some(4), 5), None);
        assert_eq!(Walk::On.landing(Some(3), 5), Some(4));
        assert_eq!(Walk::On.landing(None, 5), Some(0));
    }

    #[test]
    fn a_press_walks_the_list_and_a_filter_lands_on_the_newest() {
        let (mut window, mut ui, mut painter) = open();
        let at = Point::new(0, 0);
        let view = view(3);
        assert_eq!(window.chosen, None);
        click(
            &mut window,
            &mut ui,
            &mut painter,
            &view,
            PostWindow::walk(Walk::Last, at),
        );
        assert_eq!(window.chosen, Some(2));
        click(
            &mut window,
            &mut ui,
            &mut painter,
            &view,
            PostWindow::walk(Walk::Back, at),
        );
        assert_eq!(window.chosen, Some(1));
        click(
            &mut window,
            &mut ui,
            &mut painter,
            &view,
            PostWindow::filter(Shown::Category(2), at),
        );
        assert_eq!(window.shown, Shown::Category(2));
        assert_eq!(window.chosen, Some(2), "the newest of what it was given");
    }

    #[test]
    fn the_help_button_asks_for_its_page() {
        let (mut window, mut ui, mut painter) = open();
        let area = Rect::new(0, 0, PostWindow::WIDTH, PostWindow::HEIGHT);
        let view = view(1);
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, &view, PostWindow::help(area)),
            Some(Outcome::Help(HELP_PAGE))
        );
    }

    #[test]
    fn the_campaigns_own_button_is_built_only_in_a_campaign() {
        let (mut window, mut ui, mut painter) = open();
        let view = view(1);
        let place = PostWindow::mission(Point::new(0, 0));
        let on = Pointer::at(Point::new(place.x + 1, place.y + 1));
        window.run(&mut ui, &mut painter, &skin(), &view, on.pressing());
        let without = painter.ops().len();
        painter.reset();
        window.campaign = true;
        window.run(&mut ui, &mut painter, &skin(), &view, on.pressing());
        assert_eq!(painter.ops().len(), without + 1);
        assert_eq!(
            click(&mut window, &mut ui, &mut painter, &view, place),
            Some(Outcome::Ask(kind::NOTE)),
            "it asks for the small diary (`0x45500`)"
        );
    }
}
