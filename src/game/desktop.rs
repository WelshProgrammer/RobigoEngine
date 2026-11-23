//! The interface over the map view: the items, fonts and texts a data set holds, the windows that
//! are open, and what a press on one asks of the game.
//!
//! The painter works in desktop pixels, so the windows draw at 1:1 into a layer of their own that
//! the renderer scales by the interface's own whole scale, apart from the view's zoom. A window
//! the player opens is centred on the layer and stepped down and across for each window already
//! open, which keeps a new one in sight without covering the last.
//!
//! The keys follow the original's table through [`s2_ui::keys`], except where the viewer's own
//! keys already use them ([`LEFT_TO_THE_VIEWER`]).

use s2_assets::{Landscape, Nation, Palette, Remap, SpriteSheet};
use std::collections::BTreeMap;
use std::sync::Arc;

use s2_audio::EFFECT_IDS;
use s2_formats::text::StringTable;
use s2_mods::{Sheets, Text};
use s2_render::terrain::Point;
use s2_render::terrain::Terrain;
use s2_render::{Frame, Paint, Patch, blit};
use s2_sim::{Command, Game, Letter, MapObject, Mission, Node, Player, Repaired, Tree};
use s2_ui::border::Border;
use s2_ui::chrome::Chrome;
use s2_ui::geometry::Rect;
use s2_ui::keys::{self, Action, Cheat, Scancode};
use s2_ui::mouse_pointer::{self, MousePointer, PointerButton, PointerPicture};
use s2_ui::paint::{Align, Canvas, Painter, paint};
use s2_ui::skin::Skin;
use s2_ui::store::Ui;
use s2_ui::text::{FontId, Fonts, Glyphs, Label, Pen, WithRuns};
use s2_ui::toolbar::Toolbar;
use s2_ui::widget::{Pointer, WindowId};
use s2_ui::windows::notice::Line;
use s2_ui::windows::{self, action};
use s2_ui::windows::{
    ActionView, ActionWindow, Answer, BuildingView, BuildingWindow, BuildingsView, BuildingsWindow, CdWindow,
    DemolishView, DemolishWindow, DistributionView, DistributionWindow, HelpLineWindow, MapWindow, MenuView,
    MenuWindow, MerchandiseView, MerchandiseWindow, MidiWindow, MilitaryView, MilitaryWindow, NoticeWindow,
    ObserveWindow, OptionsView, OptionsWindow, Outcome, Pick, Play, PostView, PostWindow, ProductionView,
    ProductionWindow, QuitWindow, Reading, ReadingWindow, SavesWindow, SelectWindow, ShipsView, ShipsWindow,
    StatisticsView, StatisticsWindow, StockView, StockWindow, ToolsView, ToolsWindow, TransportView,
    TransportWindow, WinnerView, WinnerWindow, kind,
};
use s2_ui::windows::{
    DiaryWindow, SoundTestWindow, TestEffect, demolish, diary, helpline, map, menu, observe, options, saves,
};
use s2_ui::{Items, Labels, Styles, Texts, items};

use super::{build, migration};
use crate::error::{Error, Result};

/// What [`Desktop::kept`] writes into a save.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Kept {
    /// The slot of the task the small diary shows.
    task: Option<u16>,
}

/// How many times a window the pause leaves dim goes through the shadow table: twice, as the
/// view does.
const PAUSE_DARKENINGS: usize = 2;

/// The index the interface's items come from.
const INTERFACE: &str = "DATA/IO/IO";

/// The index the fonts come from.
const RESOURCE: &str = "DATA/RESOURCE";

/// The texts an English build shows while a game is on.
const TEXTS: &str = "DATA/TXT/ONGAME.ENG";

/// The names of the buildings, figures and wares.
const NAMES: &str = "DATA/TXT/NAMES.ENG";

/// The scancodes that close the diary when it is the window in front: Enter and Esc
/// (`0x664e7`).
const CLOSES_THE_DIARY: [u8; 2] = [28, 1];

/// The two files the reading screens show.
const PAGES: [(WindowId, &str); 2] = [
    (kind::README, s2_ui::windows::reading::README_FILE),
    (kind::KEYS, "DATA/TXT2/KEYS.ENG"),
];

/// The set 1 scancodes the viewer's own keys keep, which the interface leaves alone: `s` sends a
/// scout, `b` chooses a building type, `F11` goes fullscreen and `F12` takes a screenshot. Every
/// other key the viewer uses is one the original leaves to the view.
const LEFT_TO_THE_VIEWER: [u8; 4] = [31, 48, 87, 88];

/// The screen a window is centred on before a frame has been drawn, which is the size the
/// original ran at.
const FIRST_SCREEN: (u32, u32) = (640, 480);

/// How much of a window stays on the frame when it is dragged towards an edge.
const KEPT: i32 = 32;

/// How far to the side of the pointer a window placed beside it opens and how far above the
/// pointer its middle sits (`0x44f74`).
const MENU_BESIDE: (i32, i32) = (40, 10);

/// The height the action menu is built at (`0x5add8`). `0x44f74` places the menu before its
/// rows resize it, so a menu of any depth opens the same distance above the pointer.
const MENU_BUILT: u32 = 78;

/// The `NAMES.ENG` slot the building names start at, one per building type.
const BUILDING_NAMES: u16 = 50;

/// How far above the building's own point its name sits (`0x1f048`).
const NAME_UP: i32 = 24;

/// How far above it the line saying what it is doing sits (`0x1f08f`).
const STATUS_UP: i32 = 16;

/// The pen the name and most status lines draw in (`0x1f043`).
const LABEL_PEN: u8 = 132;

/// The pen an unoccupied or stopped building's line and a low productivity draw in.
const WARNING_PEN: u8 = 137;

/// The pen a middling productivity draws in, and the one a productivity from
/// [`HEALTHY_FROM`] up draws in (`0x1f4e8`, `0x1f4fd`).
const MIDDLING_PEN: u8 = 5;
const HEALTHY_PEN: u8 = 45;

/// The productivities at which the pen turns middling, then healthy.
const MIDDLING_FROM: u8 = 15;
const HEALTHY_FROM: u8 = 50;

/// The font both lines draw in, the view's font 2 (`0x1c04f`).
const LABEL_FONT: FontId = FontId::Small;

/// The text slots the status line takes.
mod status {
    pub const UNOCCUPIED: u16 = 544;
    pub const STOPPED: u16 = 546;
    pub const BUILDING: u16 = 549;
    pub const ONE_SOLDIER: u16 = 550;
    pub const SOLDIERS: u16 = 551;
}

/// A window being dragged by its background.
#[derive(Debug, Clone, Copy)]
struct Grab {
    /// Which window.
    id: WindowId,
    /// Where the pointer was when the press landed.
    from: Point,
    /// Where the window's corner was then.
    origin: Point,
}

/// What the open windows ask of the game once they have been drawn.
#[derive(Debug, Default)]
pub(crate) struct Asks {
    /// Commands for the sim, in the order the windows gave them.
    pub(crate) commands: Vec<Command>,
    /// The node a window asked the view to move to.
    pub(crate) centre: Option<Node>,
    /// The game menu's own way out.
    pub(crate) leave: bool,
    /// Whether a window asked for the view to go to the player's own headquarters.
    pub(crate) home: bool,
    /// Where the pointer goes back to, in desktop pixels, now the action menu has closed.
    pub(crate) pointer: Option<Point>,
    /// Effects the interface played, in order.
    pub(crate) effects: Vec<u8>,
    /// What the player asked of the music, in order.
    pub(crate) music: Vec<Music>,
    /// What the player asked to change on the options window, in order.
    pub(crate) options: Vec<options::Change>,
    /// The effects the sound test asked to play, in order.
    pub(crate) tests: Vec<TestEffect>,
    /// Whether the game should now be paused or running: a letter pauses it as it opens and its
    /// Continue lets it run (`0x466c0`, `0x46728`).
    pub(crate) pause: Option<bool>,
    /// The slot the save screen asked to save the game into.
    pub(crate) save: Option<usize>,
    /// The slot the load screen asked to load the game from.
    pub(crate) load: Option<usize>,
    /// The automatic save period the save screen set.
    pub(crate) automatic: Option<usize>,
}

/// What the player asks of the music and the disc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Music {
    /// A press on the CD player or the MIDI player.
    Play(Play),
    /// The game menu's music switch.
    Switch,
}

/// The game menu's switches and volumes as a game starts: everything on and loudest.
const FIRST_SOUND: MenuView = MenuView {
    music: true,
    effects: true,
    music_volume: menu::LOUDEST,
    effect_volume: menu::LOUDEST,
};

/// The effect a press on a window plays: event 0 of the routine `0x23dc5` installs at
/// `0x23da6`.
pub(crate) const CLICK: u8 = 61;

/// The effect a window rolling up or down plays: event 4 of the same routine.
pub(crate) const ROLL: u8 = 63;

/// The effect typing the cheat word plays as it switches the cheat on or off (`0x43cdc`).
const CHEAT: u8 = 58;

/// The mouse pointer's picture ready for SDL.
pub(crate) struct PointerCursor {
    /// Which picture it is.
    pub(crate) picture: PointerPicture,
    /// Rows of red, green, blue and alpha bytes.
    pub(crate) pixels: Vec<u8>,
    /// Width and height in output pixels.
    pub(crate) size: (u32, u32),
    /// The click point within the picture, in output pixels.
    pub(crate) hotspot: (u32, u32),
}

/// The interface and everything it draws from.
pub(crate) struct Desktop {
    items: Items,
    /// Where the pointer was when the action menu opened, which it goes back to when the menu
    /// closes (`0x5af2c`).
    pointer_home: Option<Point>,
    /// The mouse pointer's own state: which of S2's pictures it shows and where.
    hand: MousePointer,
    /// Whether the pointer is drawn into the interface layer rather than shown by SDL.
    software_pointer: bool,
    border: Border,
    fonts: Fonts,
    texts: Texts,
    skin: Skin,
    /// The window styles text draws in.
    styles: Arc<Styles>,
    icons: Option<SpriteSheet>,
    landscape: Option<SpriteSheet>,
    /// The player's nation archive, which the demolish prompt pictures a building from.
    nation: Option<SpriteSheet>,
    highlight: Remap,
    /// The table a disabled control goes through once it is drawn.
    disabled: Remap,
    shadow: Remap,
    ui: Ui,
    cheat: Cheat,
    pointer: Pointer,
    pressed_on: Option<(WindowId, Point)>,
    grab: Option<Grab>,
    /// Where the pointer last was while the right button holds the observation window's
    /// picture.
    picture_held: Option<Point>,
    /// Effects played since the last frame, which the next [`Asks`] carries.
    effects: Vec<u8>,
    kept: BTreeMap<WindowId, Point>,
    letters: usize,
    helping: Option<Label>,
    options: u16,
    screen: Rect,
    sound: MenuView,
    options_window: OptionsWindow,
    options_view: Option<OptionsView>,
    sound_test: SoundTestWindow,
    /// Which effects have a sample, by id, which the sound test greys the rest by.
    sampled: [bool; EFFECT_IDS],
    notice: NoticeWindow,
    /// The open notice's title and lines.
    noticed: (&'static str, Vec<Line>),
    action: ActionWindow,
    map: MapWindow,
    observe: ObserveWindow,
    demolish: DemolishWindow,
    stock: StockWindow,
    help: HelpLineWindow,
    loading: SavesWindow,
    saving: SavesWindow,
    /// Each slot's number and name, which the two save screens list.
    saved: Vec<Option<(i32, Vec<u8>)>>,
    node: Node,
    quit: QuitWindow,
    cd: CdWindow,
    /// The disc's playable tracks, which the CD player shows.
    tracks: Vec<u16>,
    midi: MidiWindow,
    reading: [(ReadingWindow, Vec<u8>); PAGES.len()],
    helped: Option<ReadingWindow>,
    /// The list of what a load took out of the save, once shown.
    repairs: Option<ReadingWindow>,
    /// The mission's own texts, which the diary reads its letters from.
    mission_texts: Option<StringTable>,
    /// The letter on show, and the sim's letter it was opened for.
    letter: Option<(DiaryWindow, Letter)>,
    /// The small diary on show.
    note: Option<DiaryWindow>,
    /// The slot of the last task a letter carried, which the small diary shows (the view's
    /// `+0x49a`).
    task: Option<u16>,
    /// The diary window Enter or Esc closed, which the next frame acts on.
    typed: Option<WindowId>,
    help_text: BTreeMap<u16, Vec<u8>>,
    select: SelectWindow,
    menu: MenuWindow,
    buildings: BuildingsWindow,
    distribution: DistributionWindow,
    military: MilitaryWindow,
    post: PostWindow,
    production: ProductionWindow,
    ships: ShipsWindow,
    statistics: StatisticsWindow,
    winner: WinnerWindow,
    /// The player the game was decided for, once the winner window has been opened for it.
    won: Option<Player>,
    tools: ToolsWindow,
    transport: TransportWindow,
    merchandise: MerchandiseWindow,
    /// The buildings' own windows, one for each building whose window is open.
    houses: Vec<BuildingWindow>,
}

/// The skin the mods give over `items`, and the window styles it was built from.
/// The picture `hand` shows for SDL, coloured by `palette` and `scale` output pixels to an
/// interface pixel, from `items`.
pub(crate) fn pointer_cursor(
    items: &Items,
    hand: &MousePointer,
    palette: &Palette,
    scale: u32,
) -> Option<PointerCursor> {
    let picture = hand.picture();
    let sprite = mouse_pointer::sprite(items, picture)?;
    let hotspot = hand.hotspot();
    let inside = |value: i32, size: u16| {
        u32::try_from(value)
            .unwrap_or(0)
            .min(u32::from(size).saturating_sub(1))
    };
    Some(PointerCursor {
        picture,
        pixels: mouse_pointer::rgba(sprite, palette, scale),
        size: (
            u32::from(sprite.width()) * scale,
            u32::from(sprite.height()) * scale,
        ),
        hotspot: (
            inside(hotspot.x, sprite.width()) * scale,
            inside(hotspot.y, sprite.height()) * scale,
        ),
    })
}

/// What an open observation window asks the game to draw.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Observing {
    /// The node the window stands on.
    pub(crate) node: Node,
    /// What its picture keeps in its middle.
    pub(crate) watch: observe::Watch,
    /// The world pixel in the middle of the picture once it has followed a figure.
    pub(crate) middle: Option<Point>,
    /// How far the picture has been dragged since it was last drawn, in world pixels.
    pub(crate) dragged: Point,
    /// How big the picture is.
    pub(crate) size: (u32, u32),
    /// How many times the picture is doubled.
    pub(crate) zoom: u8,
}

/// The patches the picture `hand` shows covers where it is, from `items`: one for each run of
/// pixels it draws.
pub(crate) fn pointer_patches(items: &Items, hand: &MousePointer) -> Vec<Patch> {
    let Some(sprite) = mouse_pointer::sprite(items, hand.picture()) else {
        return Vec::new();
    };
    let at = hand.top_left();
    (0..sprite.height())
        .flat_map(|y| {
            sprite.row(y).iter().map(move |run| Patch {
                x: at.x + i32::from(run.x),
                y: at.y + i32::from(y),
                width: u32::from(run.len),
                height: 1,
            })
        })
        .collect()
}

/// The art and the text every screen of the interface draws with.
pub(crate) struct Kit {
    /// The interface's items, by id and by name.
    pub(crate) items: Items,
    /// The three fonts.
    pub(crate) fonts: Fonts,
    /// The game's texts, with the names table when the data has one.
    pub(crate) texts: Texts,
    /// The faces, edges and pictures of the window styles.
    pub(crate) skin: Skin,
    /// What each style's letters draw in.
    pub(crate) styles: Arc<Styles>,
}

/// Reads the interface's items, fonts, texts and styles out of `sheets`.
pub(crate) fn kit(sheets: &Sheets<'_>) -> Result<Kit> {
    let items = Items::from_named(
        sheets
            .named(INTERFACE)?
            .into_iter()
            .chain(sheets.named(RESOURCE)?),
    );
    let [small, medium, large] = items::FONT_NAMES.map(|name| {
        sheets
            .font(name)
            .map(|font| Glyphs::from_sprites(font.line_height, font.glyphs))
    });
    let fonts = Fonts::new(small?, medium?, large?);
    let Text::Table(table) = sheets.text(TEXTS)? else {
        return Err(Error::NotATable(TEXTS));
    };
    let texts = match sheets.text(NAMES) {
        Ok(Text::Table(names)) => Texts::new(table).with_names(names),
        _ => Texts::new(table),
    };
    let (skin, styles) = skin(sheets, &items)?;
    Ok(Kit {
        items,
        fonts,
        texts,
        skin,
        styles,
    })
}

fn skin(sheets: &Sheets<'_>, items: &Items) -> Result<(Skin, Arc<Styles>)> {
    let styles = sheets.styles()?;
    let skin = items::skin_with(items, &styles, &sheets.pictures()?);
    Ok((skin, Arc::new(styles)))
}

impl Desktop {
    /// Reads the interface through `sheets`, with the building icons of `nation`, the ware and
    /// person pictures of `landscape`, the tables a marked control lightens its background
    /// through, a label darkens its strip through and a disabled control is greyed through, and
    /// a map screen the size of a world of `nodes` nodes.
    pub(crate) fn load(
        sheets: &Sheets<'_>,
        (nation, landscape): (Nation, Landscape),
        (highlight, shadow, disabled): (Remap, Remap, Remap),
        nodes: (u16, u16),
    ) -> Result<Self> {
        let Kit {
            items,
            fonts,
            texts,
            skin,
            styles,
        } = kit(sheets)?;
        let icons = sheets
            .sheet(nation.icons_path())
            .inspect_err(|error| log::warn!("no building icons for the {nation}: {error}"))
            .ok();
        let buildings = sheets
            .sheet(nation.sprites_path(landscape))
            .inspect_err(|error| log::warn!("no buildings for the {nation}: {error}"))
            .ok();
        let pictures = sheets
            .sheet(landscape.icons_path())
            .inspect_err(|error| log::warn!("no ware pictures for {landscape}: {error}"))
            .ok();
        let colour = styles.first().colour;
        Ok(Self {
            pointer_home: None,
            hand: MousePointer::from_items(&items, false),
            software_pointer: false,
            border: Border::from_items(&items),
            items,
            fonts,
            texts,
            skin,
            styles,
            icons,
            landscape: pictures,
            nation: buildings,
            highlight,
            disabled,
            shadow,
            ui: Ui::new(),
            cheat: Cheat::new(),
            pointer: Pointer::default(),
            pressed_on: None,
            grab: None,
            picture_held: None,
            effects: Vec::new(),
            kept: BTreeMap::new(),
            letters: 0,
            helping: None,
            options: 0,
            screen: Rect::new(0, 0, FIRST_SCREEN.0, FIRST_SCREEN.1),
            sound: FIRST_SOUND,
            options_window: OptionsWindow::new(),
            options_view: None,
            sound_test: SoundTestWindow::new(),
            sampled: [false; EFFECT_IDS],
            notice: NoticeWindow::new(),
            noticed: ("", Vec::new()),
            action: ActionWindow::new(kind::ACTION),
            map: MapWindow::new(nodes),
            observe: ObserveWindow::new(Node { x: 0, y: 0 }),
            demolish: DemolishWindow::new(kind::DEMOLISH),
            stock: StockWindow::new(kind::STOCK),
            houses: Vec::new(),
            help: HelpLineWindow::new(),
            loading: SavesWindow::new(saves::Mode::Load),
            saving: SavesWindow::new(saves::Mode::Save),
            saved: vec![None; saves::SLOTS],
            node: Node { x: 0, y: 0 },
            quit: QuitWindow::new(false),
            cd: CdWindow::new(kind::CD_PLAYER),
            tracks: Vec::new(),
            midi: MidiWindow::new(kind::MIDI_PLAYER),
            reading: pages(sheets),
            helped: None,
            repairs: None,
            mission_texts: None,
            letter: None,
            note: None,
            task: None,
            typed: None,
            help_text: help_pages(sheets),
            select: SelectWindow::new(kind::SELECT),
            menu: MenuWindow::new(),
            buildings: BuildingsWindow::new(kind::BUILDINGS, colour),
            distribution: DistributionWindow::new(kind::DISTRIBUTION, colour),
            military: MilitaryWindow::new(kind::MILITARY, colour),
            post: PostWindow::new(kind::POST).with_types(sheets.message_types()?),
            production: ProductionWindow::new(kind::PRODUCTION, colour),
            ships: ShipsWindow::new(kind::SHIPS),
            statistics: StatisticsWindow::new(kind::STATISTICS),
            winner: WinnerWindow::new(kind::WINNER),
            won: None,
            tools: ToolsWindow::new(kind::TOOLS, colour),
            transport: TransportWindow::new(kind::TRANSPORT),
            merchandise: MerchandiseWindow::new(kind::MERCHANDISE),
        })
    }

    /// What the desktop keeps in a save beside the game, as S2 keeps the view's `+0x49a` in
    /// `DESKTnnn.DAT`: the slot of the task the small diary shows.
    pub(crate) fn kept(&self) -> Vec<u8> {
        serde_json::to_vec(&Kept { task: self.task }).unwrap_or_default()
    }

    /// Takes back what [`Self::kept`] wrote. Bytes it cannot read leave the desktop as it is,
    /// with a warning.
    pub(crate) fn restore(&mut self, bytes: &[u8]) {
        match serde_json::from_slice::<Kept>(bytes) {
            Ok(kept) => self.task = kept.task,
            Err(error) => log::warn!("the save's desktop: {error}; left as it starts"),
        }
    }

    /// Reads the text table `!LOAD_MISSION_TEXTS` chose, `DATA/TXT/MISS_nnn.ENG` for `number`,
    /// which the diary's letters come from.
    pub(crate) fn with_mission_texts(mut self, sheets: &Sheets<'_>, number: Option<u16>) -> Self {
        let Some(number) = number else {
            return self;
        };
        let path = format!("DATA/TXT/MISS_{number:03}.ENG");
        match sheets.text(&path) {
            Ok(Text::Table(table)) => self.mission_texts = Some(table),
            Ok(Text::Page(_)) => log::warn!("{path} is not a string table"),
            Err(error) => log::warn!("{path}: {error}; the diary has no letters"),
        }
        self
    }

    /// Takes the key at set 1 scancode `scan` with `alt` held. Returns true when the interface
    /// used it.
    pub(crate) fn key(&mut self, scan: u8, alt: bool) -> bool {
        if LEFT_TO_THE_VIEWER.contains(&scan) {
            return false;
        }
        if CLOSES_THE_DIARY.contains(&scan)
            && let Some(id) = self.front().filter(|&id| id == kind::DIARY || id == kind::NOTE)
        {
            self.typed = Some(id);
            return true;
        }
        let code = Scancode(scan);
        if self.cheat.press(code) {
            self.effects.push(CHEAT);
        }
        let Some(bound) = keys::bound(code, alt, self.cheat.on()) else {
            return false;
        };
        if self.ui.paused() && !Self::reaches_while_paused(bound.action) {
            return true;
        }
        match bound.action {
            Action::Open(id) => self.open(id),
            Action::Ask(id) => self.ask(id),
            Action::Swap { close, open } => {
                self.ui.close(close);
                self.open(open);
            }
            // The viewer owns the pause, so the key goes on to it.
            Action::Flag(mask) => self.options ^= mask,
            Action::Pause => return false,
            other => {
                log::debug!("{other:?} is not wired to the viewer yet");
                return false;
            }
        }
        true
    }

    /// Whether a key bound to `action` works while the game is paused: one that opens a window
    /// the pause leaves live, or the pause key itself.
    fn reaches_while_paused(action: Action) -> bool {
        match action {
            Action::Open(id) | Action::Ask(id) | Action::Swap { open: id, .. } => {
                windows::live_while_paused(id)
            }
            Action::Pause => true,
            _ => false,
        }
    }

    /// Whether the window of `id` takes input and draws at full brightness: every window while
    /// the game runs, and while it is paused the toolbar and the ones
    /// [`windows::live_while_paused`] names.
    ///
    /// The letter opens over the pause its event put on, so it stays live too.
    fn live(&self, id: WindowId) -> bool {
        !self.ui.paused() || id == Toolbar::KIND || id == kind::DIARY || windows::live_while_paused(id)
    }

    /// The frontmost window that carries a frame.
    fn front(&self) -> Option<WindowId> {
        self.ui
            .back_to_front()
            .map(|window| window.id)
            .filter(|&id| windows::framed(id))
            .last()
    }

    /// Follows the pointer to desktop pixel `at`.
    pub(crate) fn moved(&mut self, at: Point) {
        self.pointer.at = at;
        self.hand.moved(at);
    }

    /// Tells the mouse pointer a button went down or came up at desktop pixel `at`.
    pub(crate) fn pointer_button(&mut self, button: PointerButton, down: bool, at: Point) {
        if down {
            self.hand.press(button, at);
        } else {
            self.hand.release(button);
        }
    }

    /// Tells the mouse pointer whether a road is being drawn.
    pub(crate) const fn set_drawing(&mut self, drawing: bool) {
        self.hand.set_road(drawing);
    }

    /// Draws the mouse pointer into the interface layer, or leaves it to SDL.
    pub(crate) const fn set_software_pointer(&mut self, software: bool) {
        self.software_pointer = software;
    }

    /// Whether the mouse pointer is drawn into the interface layer.
    pub(crate) const fn software_pointer(&self) -> bool {
        self.software_pointer
    }

    /// The pointer's picture for SDL, coloured by `palette` and `scale` output pixels to a
    /// desktop pixel: the picture, its RGBA pixels, their size and the hotspot.
    pub(crate) fn pointer_cursor(&self, palette: &Palette, scale: u32) -> Option<PointerCursor> {
        pointer_cursor(&self.items, &self.hand, palette, scale)
    }

    /// The patches the software pointer covers: one for each run of pixels it draws, so the
    /// view shows through around its edges.
    fn pointer_patches(&self) -> Vec<Patch> {
        if !self.software_pointer {
            return Vec::new();
        }
        pointer_patches(&self.items, &self.hand)
    }

    /// Shows the disc's playable tracks on the CD player and whether anything plays on the game
    /// menu's music switch.
    pub(crate) fn hear_music(&mut self, tracks: &[u16], on: bool) {
        if self.tracks != tracks {
            tracks.clone_into(&mut self.tracks);
        }
        self.sound.music = on;
    }

    /// Opens the sound test, with `sampled` saying which effects have a sample, by id.
    pub(crate) fn open_sound_test(&mut self, sampled: [bool; EFFECT_IDS]) {
        self.sampled = sampled;
        self.open(kind::SOUND_TEST);
    }

    /// Shows the settings on the options window. Without them the game menu has no use for it.
    pub(crate) fn show_options(&mut self, view: OptionsView) {
        self.options_view = Some(view);
    }

    /// Opens the list of what a load took out of a save made by other rules, when any of it
    /// was player `player`'s. What other players lost is left out.
    pub(crate) fn show_repairs(&mut self, repaired: &[Repaired], player: usize) {
        let own: Vec<Repaired> = repaired
            .iter()
            .filter(|found| found.owner.index() == player)
            .copied()
            .collect();
        if own.is_empty() {
            return;
        }
        let text = migration::log(&own, |name| {
            String::from_utf8_lossy(self.texts.named(name.slot())).into_owned()
        });
        let reading = Reading::Repairs;
        let (width, height) = reading.size();
        let mut window = ReadingWindow::new(reading);
        window.read(text.as_bytes(), Rect::new(0, 0, width, height));
        self.repairs = Some(window);
        self.open(kind::REPAIRS);
        self.ui.bring_to_front(kind::REPAIRS);
    }

    /// Opens a notice titled `title` showing `lines`, in place of any notice already open.
    pub(crate) fn notify(&mut self, title: &'static str, lines: Vec<Line>) {
        self.noticed = (title, lines);
        self.ui.close(kind::NOTICE);
        self.open(kind::NOTICE);
        self.ui.bring_to_front(kind::NOTICE);
    }

    /// The game menu's music volume, 0 to 127.
    pub(crate) fn music_volume(&self) -> u8 {
        u8::try_from(self.sound.music_volume).unwrap_or(u8::MAX)
    }

    /// Whether the game menu has effects on, and their volume, 0 to 127.
    pub(crate) fn effect_settings(&self) -> (bool, u8) {
        let volume = u8::try_from(self.sound.effect_volume).unwrap_or(u8::MAX);
        (self.sound.effects, volume)
    }

    /// Takes a press of the left button.
    pub(crate) fn press(&mut self, at: Point) {
        self.pointer.at = at;
        self.pointer.pressed = true;
        self.pointer.held = true;
        if let Some(id) = self.ui.hit(at)
            && self.live(id)
        {
            if id != Toolbar::KIND {
                self.ui.bring_to_front(id);
            }
            self.pressed_on = Some((id, at));
            self.effects.push(CLICK);
        }
    }

    /// Takes a release of the left button.
    pub(crate) fn release(&mut self, at: Point) {
        self.pointer.at = at;
        self.pointer.released = true;
        self.pointer.held = false;
        self.pressed_on = None;
        self.grab = None;
    }

    /// Takes the window a press landed on, once the controls have had their say: a press no
    /// control took is a press on the window's background, which drags it.
    fn start_grab(&mut self) {
        if let Some((id, from)) = self.pressed_on.take()
            && id != Toolbar::KIND
            && !self.ui.holding()
            && let Some(window) = self.ui.window(id)
        {
            self.grab = Some(Grab {
                id,
                from,
                origin: Point::new(window.area.x, window.area.y),
            });
        }
    }

    /// Moves every open window back inside `bounds`, which a smaller window or a larger interface
    /// scale leaves them hanging off.
    fn hold_on_screen(&mut self, bounds: Rect) {
        let moved: Vec<(WindowId, Rect)> = self
            .ui
            .back_to_front()
            .map(|window| (window.id, within(bounds, window.area)))
            .collect();
        for (id, area) in moved {
            self.ui.move_to(id, Point::new(area.x, area.y));
        }
    }

    /// Moves the window being dragged to where the pointer has taken it, keeping [`KEPT`] pixels
    /// of it on the frame.
    fn move_grabbed(&mut self, bounds: Rect) {
        let Some(grab) = self.grab else { return };
        let at = Point::new(
            grab.origin.x + self.pointer.at.x - grab.from.x,
            grab.origin.y + self.pointer.at.y - grab.from.y,
        );
        let Some(window) = self.ui.window(grab.id) else {
            self.grab = None;
            return;
        };
        let across = i32::try_from(window.area.width).unwrap_or_default();
        let right = i32::try_from(bounds.width).unwrap_or_default();
        let down = i32::try_from(bounds.height).unwrap_or_default();
        let corner = Point::new(
            at.x.clamp(KEPT - across, right - KEPT),
            at.y.clamp(0, down - KEPT),
        );
        self.ui.move_to(grab.id, corner);
        // The original keeps the corner of a window that has been moved and opens it there next
        // time, which the view holds per kind at `+0x10e` (`0x46179`).
        self.kept.insert(grab.id, corner);
    }

    /// The pointer as the window of `id` sees it: the real one for the window that takes it
    /// and [`Pointer::away`] for the rest, so a control on a window behind never answers a press
    /// meant for the window in front.
    fn pointer_for(&self, id: WindowId) -> Pointer {
        if self.live(id) && self.taking_pointer() == Some(id) {
            self.pointer
        } else {
            Pointer::away()
        }
    }

    /// The window the pointer belongs to: the one being dragged by its background, the one
    /// holding a control the pointer started on, or the frontmost one the pointer covers.
    fn taking_pointer(&self) -> Option<WindowId> {
        if let Some(grab) = self.grab {
            return Some(grab.id);
        }
        if let Some(drag) = self.ui.drag() {
            return Some(drag.widget.window);
        }
        if let Some(widget) = self.ui.holder() {
            return Some(widget.window);
        }
        self.ui.hit(self.pointer.at)
    }

    /// The view's draw options, the word every key and the window menu turn bits of on and off.
    pub(crate) const fn options(&self) -> u16 {
        self.options
    }

    /// Opens the window that names `winner`, the first time the game is decided for anyone
    /// (`0x2a031`). A game already decided when it is loaded opens nothing.
    pub(crate) fn decided(&mut self, winner: Option<Player>, loaded: bool) {
        if self.won.is_some() || winner.is_none() {
            return;
        }
        self.won = winner;
        if !loaded {
            self.open(kind::WINNER);
        }
    }

    /// Whether the cheat word has been typed and the cheat is on (`0x200c8e`).
    pub(crate) fn cheating(&self) -> bool {
        self.cheat.on()
    }

    /// Points the interface at `node`, which is what opening the action menu on it does.
    pub(crate) const fn set_node(&mut self, node: Node) {
        self.node = node;
    }

    /// The node the action menu is open on, if it is open.
    pub(crate) fn menu_node(&self) -> Option<Node> {
        self.ui.is_open(kind::ACTION).then_some(self.node)
    }

    /// Tells the map window which node sits under the middle of the main view.
    pub(crate) const fn set_view_centre(&mut self, node: Node) {
        self.map.centre = Some((node.x, node.y));
    }

    /// What an open observation window watches, how big a picture it wants and how many times
    /// that picture is doubled. A rolled window asks for nothing.
    pub(crate) fn observing(&self) -> Option<Observing> {
        let window = self.ui.window(kind::OBSERVE)?;
        if window.rolled() {
            return None;
        }
        let picture = ObserveWindow::picture(window.area);
        Some(Observing {
            node: self.observe.node,
            watch: self.observe.watch,
            middle: self.observe.middle,
            dragged: self.observe.dragged,
            size: (picture.width, picture.height),
            zoom: self.observe.zoom(),
        })
    }

    /// Takes what the observation window watches now: `watch`, the world pixel in the middle of
    /// its picture once it has followed a figure, and the node that is on.
    pub(crate) const fn watched(&mut self, watch: observe::Watch, middle: Option<Point>, node: Node) {
        self.observe.watch = watch;
        self.observe.middle = middle;
        self.observe.node = node;
        self.observe.dragged = Point::new(0, 0);
    }

    /// Takes a press of the right button at `at`. Returns true when it took hold of the
    /// observation window's picture, which the pointer then drags until the button comes up.
    pub(crate) fn grab_picture(&mut self, at: Point) -> bool {
        let held = self.ui.hit(at) == Some(kind::OBSERVE)
            && self
                .ui
                .window(kind::OBSERVE)
                .is_some_and(|window| !window.rolled() && ObserveWindow::grabs(window.area, at));
        if held {
            self.observe.grab();
            self.picture_held = Some(at);
        }
        held
    }

    /// Drags a held picture after the pointer to `at`. With `with_pointer` the view goes the
    /// pointer's way, as the original's does, and without it the land does.
    pub(crate) const fn drag_picture(&mut self, at: Point, with_pointer: bool) {
        if let Some(from) = self.picture_held {
            let way = if with_pointer { 1 } else { -1 };
            self.observe
                .drag(Point::new(way * (at.x - from.x), way * (at.y - from.y)));
            self.picture_held = Some(at);
        }
    }

    /// Lets go of a held picture.
    pub(crate) const fn release_picture(&mut self) {
        self.picture_held = None;
    }

    /// Hands the observation window the picture it asked for, one byte a pixel, row by row.
    pub(crate) fn observed(&mut self, pixels: &[u8]) {
        self.observe.read(pixels);
    }

    /// Puts the draw options in the state `options` names, for a screenshot of a view a key
    /// would otherwise have to turn on.
    pub(crate) const fn set_options(&mut self, options: u16) {
        self.options = options;
    }

    /// Turns the house names on or off. `0x5bc28` clears both bits when either is set and sets
    /// both when neither is, so the one command covers the two keys.
    fn name_houses(&mut self) {
        let both = keys::flag::C | keys::flag::S;
        if self.options & both == 0 {
            self.options |= both;
        } else {
            self.options &= !both;
        }
    }

    /// Whether a window or the border round the view covers desktop pixel `at`, which keeps a
    /// click on the interface out of the world below it.
    pub(crate) fn covers(&self, at: Point) -> bool {
        self.ui.hit(at).is_some() || !Border::inner(self.screen).contains(at.x, at.y)
    }

    /// The parts of the layer the frame round the view and the open windows draw, back to
    /// front. Every window fills its own rectangle, so the renderer draws the layer over the
    /// view inside these and leaves the rest of the view as it is.
    pub(crate) fn patches(&self) -> impl Iterator<Item = Patch> + '_ {
        let windows = self.ui.back_to_front().map(|window| window.area);
        self.border
            .cover(self.screen)
            .into_iter()
            .chain(windows)
            .map(|area| Patch {
                x: area.x,
                y: area.y,
                width: area.width,
                height: area.height,
            })
            .chain(self.pointer_patches())
    }

    /// Where a window of this size opens: the middle of the screen. `0x7ec0c` takes half the
    /// screen and steps back half the window, which rounds the same way for every even screen
    /// width the original offered.
    fn centred(&self, width: u32, height: u32) -> Rect {
        let middle = |screen: u32, window: u32| {
            i32::try_from(screen.saturating_sub(window) + 1).unwrap_or_default() / 2
        };
        Rect::new(
            self.screen.x + middle(self.screen.width, width),
            self.screen.y + middle(self.screen.height, height),
            width,
            height,
        )
    }

    /// Where a window of `id` opens the first time: the observation window stays at the corner
    /// it is built at, since its constructor never centres it (`0x49258`), the demolish prompt
    /// opens beside the pointer, and every other window is centred.
    fn first_place(&self, id: WindowId, width: u32, height: u32) -> Rect {
        match id {
            kind::OBSERVE => within(
                self.screen,
                Rect::new(self.screen.x, self.screen.y, width, height),
            ),
            kind::DEMOLISH => {
                let (x, y) = beside(self.screen, self.pointer.at, (width, height));
                within(self.screen, Rect::new(x, y, width, height))
            }
            _ => self.centred(width, height),
        }
    }

    /// Opens a window of `id` where the original opens one: at the corner it was last left at,
    /// or where [`Self::first_place`] puts it. Nothing opens while a letter is open (`0x456f3`).
    fn open(&mut self, id: WindowId) {
        if self.ui.is_open(kind::DIARY) {
            log::debug!("window {:#04x} stays shut while a letter is open", id.0);
            return;
        }
        if self.ui.paused() && !windows::opens_while_paused(id) {
            log::debug!("window {:#04x} stays shut while the game is paused", id.0);
            return;
        }
        if id == kind::NOTE && !self.fill_note() {
            return;
        }
        self.place(id);
    }

    /// Puts a window of `id` on the screen, paused or not.
    fn place(&mut self, id: WindowId) {
        let Some((width, height)) = self.size(id) else {
            log::debug!("window {:#04x} has no size yet", id.0);
            return;
        };
        if id == kind::OBSERVE {
            self.observe = ObserveWindow::new(self.node);
        }
        if let Some(gone) = windows::replaces(id) {
            self.ui.close(gone);
        }
        let area = match self.kept.get(&id) {
            Some(&corner) => within(self.screen, Rect::new(corner.x, corner.y, width, height)),
            None => self.first_place(id, width, height),
        };
        self.ui.open(id, area);
        self.ui.bring_to_front(id);
        if let Some((window, text)) = self
            .reading
            .iter_mut()
            .find(|(window, _)| window.reading.kind() == id)
        {
            window.panel = s2_ui::panel::TextPanel::new();
            window.read(text, area);
        }
    }

    /// Opens the diary on the letter the sim's mission holds, once for each letter, and forgets a
    /// letter the sim no longer holds.
    ///
    /// The letter shuts every window first (`0x85578`) and pauses the game (`0x45468`).
    fn follow_letter(&mut self, game: &Game, asks: &mut Asks) {
        let letter = game.mission().and_then(Mission::letter);
        let shown = self.letter.as_ref().map(|(_, shown)| *shown);
        if letter == shown {
            return;
        }
        let Some(letter) = letter else {
            self.letter = None;
            self.ui.close(kind::DIARY);
            return;
        };
        self.close_all();
        asks.pause = Some(true);
        let page = self.mission_text(letter.text);
        let task = letter.text.checked_add(1);
        let task_text = task.map(|slot| self.mission_text(slot)).unwrap_or_default();
        if !task_text.is_empty() {
            self.task = task;
        }
        let portrait = self.item_size(diary::Shape::Letter.portrait());
        let window = DiaryWindow::letter(&page, &task_text, letter.closing, portrait);
        self.letter = Some((window, letter));
        self.place(kind::DIARY);
    }

    /// Builds the small diary on the last task a letter carried, and says whether there is one
    /// (`0x45500`).
    fn fill_note(&mut self) -> bool {
        let Some(task) = self.task else {
            return false;
        };
        let text = self.mission_text(task);
        let portrait = self.item_size(diary::Shape::Note.portrait());
        self.note = Some(DiaryWindow::note(&text, portrait));
        true
    }

    /// Slot `slot` of the mission's text table, or nothing for a slot it does not fill.
    fn mission_text(&self, slot: u16) -> Vec<u8> {
        self.mission_texts
            .as_ref()
            .and_then(|table| table.raw(usize::from(slot)))
            .map(<[u8]>::to_vec)
            .unwrap_or_default()
    }

    /// The width and height of the item the skin draws `picture` with.
    fn item_size(&self, picture: s2_ui::picture::Picture) -> (u32, u32) {
        self.skin
            .picture(picture)
            .and_then(|item| self.items.sheet().get(usize::from(item.0)))
            .map_or((0, 0), |sprite| {
                (u32::from(sprite.width()), u32::from(sprite.height()))
            })
    }

    /// Closes the letter with its Continue: the sim is told, so a replay keeps it, and the pause
    /// comes off (`0x6655c`).
    fn close_letter(&mut self, asks: &mut Asks) {
        self.ui.close(kind::DIARY);
        if self.letter.is_some() {
            asks.commands.push(Command::CloseLetter);
        }
        asks.pause = Some(false);
    }

    /// How big a window of `id` opens, with the map taking its size from the world.
    fn size(&self, id: WindowId) -> Option<(u32, u32)> {
        match id {
            kind::MAP => Some(self.map.size()),
            kind::OBSERVE => Some(self.observe.size()),
            kind::HELP_PAGE => self.helped.as_ref().map(|window| window.reading.size()),
            kind::REPAIRS => self.repairs.as_ref().map(|window| window.reading.size()),
            kind::CD_PLAYER => Some(CdWindow::size(self.tracks.len())),
            kind::DIARY => Some(diary::Shape::Letter.size()),
            kind::NOTE => Some(diary::Shape::Note.size()),
            kind::OPTIONS => Some(OptionsWindow::size()),
            kind::SOUND_TEST => Some(SoundTestWindow::size()),
            kind::NOTICE => Some(NoticeWindow::size()),
            other => size(other),
        }
    }

    /// Follows the game's pause flag, which the store keeps and which decides what may open
    /// ([`windows::opens_while_paused`]).
    pub(crate) fn pause(&mut self, paused: bool) {
        self.ui.set_paused(paused);
    }

    /// Opens the action menu for `node` beside the press at `at`.
    ///
    /// Gives back where the pointer moves to, in desktop pixels: onto the button the menu offers
    /// first (`0x5ae3b`). The pointer's own place is kept for when the menu closes. A menu
    /// replacing one already open keeps the new place, since the press that opened it moved the
    /// pointer there.
    pub(crate) fn open_action(
        &mut self,
        game: &Game,
        player: Player,
        node: Node,
        at: Point,
    ) -> Option<Point> {
        if self.ui.paused() {
            log::debug!("the action menu stays shut while the game is paused");
            return None;
        }
        self.node = node;
        self.action = ActionWindow::new(kind::ACTION);
        // The menu opens with its first tabs already chosen, so it opens at the size they need.
        let (view, types) = action_menu(&mut self.action, node, game, player);
        let (width, height) = self.action.size(types.len());
        self.ui.close(kind::ACTION);
        let (x, y) = beside(self.screen, at, (width, MENU_BUILT));
        let area = within(self.screen, Rect::new(x, y, width, height));
        self.ui.open(kind::ACTION, area);
        self.ui.bring_to_front(kind::ACTION);
        self.pointer_home = Some(at);
        self.action.pointer_to(&view, Point::new(area.x, area.y))
    }

    /// Opens a window of `id` if it is not open already.
    pub(crate) fn show(&mut self, id: WindowId) {
        if !self.ui.is_open(id) {
            self.open(id);
        }
    }

    /// Opens a window of `id` that is not there and closes one that is.
    ///
    /// `0x46254` rolls an open window up and closes it on the next press. The roll button in the
    /// window's own corner does that, so the key stays a plain open and close.
    fn ask(&mut self, id: WindowId) {
        if self.ui.is_open(id) {
            self.ui.close(id);
        } else {
            self.open(id);
        }
    }

    /// Closes every window the player has open, leaving the toolbar. A letter closes on the next
    /// frame the way its Continue closes it, so the sim hears of it and the pause comes off.
    pub(crate) fn close_all(&mut self) {
        let open: Vec<WindowId> = self
            .ui
            .back_to_front()
            .map(|window| window.id)
            .filter(|&id| id != Toolbar::KIND)
            .collect();
        for id in open {
            if id == kind::DIARY {
                self.typed = Some(id);
            } else {
                self.ui.close(id);
            }
        }
    }

    /// Draws every open window into `frame` and gives back what they asked for.
    pub(crate) fn run(&mut self, game: &Game, player: Player, terrain: &Terrain, frame: &mut Frame) -> Asks {
        let bounds = Rect::new(0, 0, frame.width(), frame.height());
        if bounds != self.screen {
            self.hold_on_screen(bounds);
        }
        self.screen = bounds;
        let mut painter = Painter::new(bounds).with_styles(Arc::clone(&self.styles));
        let mut asks = Asks::default();
        self.move_grabbed(bounds);
        self.stand_toolbar(bounds);
        self.border.draw_border(&mut painter, bounds);
        // The controls report as they draw, so the line the pointer was over is the one the
        // frame starts with, and this frame's reports fill it in for the next.
        self.helping = self.ui.help();
        self.ui.forget_help();
        self.keep_houses(game, player);
        self.post.campaign = game.mission().is_some();
        self.follow_letter(game, &mut asks);
        if let Some(id) = self.typed.take() {
            self.act(Outcome::Close(id), &mut asks);
        }
        let open: Vec<WindowId> = self.ui.back_to_front().map(|window| window.id).collect();
        // The toolbar sits in the same list but is not one of the factory's windows, so the one
        // in front is the frontmost that carries a frame.
        let front = open.iter().rev().copied().find(|&id| windows::framed(id));
        for id in open {
            // The frame carries the window's own background, so it goes down before the window
            // draws into it, the way the base class draws itself and then its children.
            self.frame_under(id, front == Some(id), &mut painter);
            // The frame builds its client widget, then the corner buttons, then the window adds
            // its own controls, so the corners draw over a panel of text and under everything
            // else, as S2 captures of the help pages and a building window's foot show.
            let client = windows::content_is_client(id);
            if !client && let Some(outcome) = self.frame_over(id, &mut painter) {
                self.act(outcome, &mut asks);
            }
            if let Some(outcome) = self.window(id, game, player, terrain, &mut painter) {
                self.act(outcome, &mut asks);
            }
            self.help_line(id, &mut painter);
            if client && let Some(outcome) = self.frame_over(id, &mut painter) {
                self.act(outcome, &mut asks);
            }
            if id == Toolbar::KIND {
                self.border.draw_pieces(&mut painter, bounds);
            }
            if !self.live(id)
                && let Some(area) = self.ui.window(id).map(|window| window.area)
            {
                for _ in 0..PAUSE_DARKENINGS {
                    painter.darken(area);
                }
            }
        }
        self.start_grab();
        asks.effects.append(&mut self.effects);
        if !self.ui.is_open(kind::ACTION) {
            asks.pointer = self.pointer_home.take();
        }
        self.paint(&painter, frame);
        if self.software_pointer
            && let Some(sprite) = mouse_pointer::sprite(&self.items, self.hand.picture())
        {
            blit(
                &mut frame.surface(),
                0,
                sprite,
                self.hand.top_left(),
                Paint::PLAIN,
            );
        }
        self.pointer.pressed = false;
        self.pointer.released = false;
        asks
    }

    /// Runs `painter`'s operations into `frame` through this desktop's own art and text.
    fn paint(&self, painter: &Painter, frame: &mut Frame) {
        let labels = WithRuns {
            labels: &self.texts,
            runs: painter.runs(),
        };
        let canvas = Canvas {
            sheet: self.items.sheet(),
            fonts: &self.fonts,
            labels: &labels,
            images: painter.images(),
            sheets: s2_ui::paint::Sheets {
                buildings: self.icons.as_ref(),
                landscape: self.landscape.as_ref(),
                nation: self.nation.as_ref(),
            },
            lighten: Some(&self.highlight),
            disabled: Some(&self.disabled),
            darken: Some(&self.shadow),
        };
        paint(painter.ops(), &mut frame.surface(), &canvas);
    }

    /// Writes the name of each of `player`'s buildings over the world frame, and what each is
    /// doing, which the `c` and `s` keys turn on. With the cheat word typed every player's
    /// buildings in sight are written over. `scroll` is the frame's top-left corner in world
    /// pixels and `period` the map's size in them.
    pub(crate) fn label_buildings(
        &mut self,
        game: &Game,
        terrain: &Terrain,
        player: Player,
        (scroll, period): (Point, (u32, u32)),
        frame: &mut Frame,
    ) {
        let names = self.options & keys::flag::C != 0;
        let states = self.options & keys::flag::S != 0;
        if !names && !states {
            return;
        }
        let everyone = self.cheat.on();
        let bounds = Rect::new(0, 0, frame.width(), frame.height());
        let size = (frame.width(), frame.height());
        let mut painter = Painter::new(bounds).with_styles(Arc::clone(&self.styles));
        let world = game.world();
        for (record, building) in world.buildings() {
            let own = building.owner == player;
            if !(own || everyone && terrain.node_visible(building.node)) {
                continue;
            }
            let at = build::on_frame(game, building.node, scroll, period, size);
            if !bounds.contains(at.x, at.y) {
                continue;
            }
            if names {
                let slot = BUILDING_NAMES + u16::from(building.kind.0);
                let label = Label::Named(slot);
                painter.text_aligned(
                    Point::new(at.x, at.y - NAME_UP),
                    LABEL_FONT,
                    LABEL_PEN,
                    label,
                    Pen::TIGHT,
                    Align::Centre,
                );
            }
            if states && let Some((pen, line)) = self.status(world, record, building) {
                let label = painter.run(&line);
                painter.text_aligned(
                    Point::new(at.x, at.y - STATUS_UP),
                    LABEL_FONT,
                    pen,
                    label,
                    Pen::TIGHT,
                    Align::Centre,
                );
            }
        }
        self.paint(&painter, frame);
    }

    /// The line the `s` key writes over `building` and its pen (`0x1f069`): a workplace's
    /// productivity or why it makes nothing, a military building's soldiers, and "(building)"
    /// on a site. A storehouse, and a workplace whose type makes no ware, get none.
    fn status(
        &self,
        world: &s2_sim::World,
        record: u16,
        building: &s2_sim::Building,
    ) -> Option<(u8, Vec<u8>)> {
        use s2_sim::BuildingState;
        let slot = |slot: u16| Label::Slot(slot).with_text(&self.texts, <[u8]>::to_vec);
        let state = building.state;
        if !state.contains(BuildingState::FINISHED) {
            return state
                .intersects(BuildingState::BUILDING | BuildingState::LEVELLING)
                .then(|| (LABEL_PEN, slot(status::BUILDING)));
        }
        let row = world.building_types().get(building.kind)?;
        let manned = state.contains(BuildingState::MANNED);
        match row.category {
            s2_sim::Category::Workplace | s2_sim::Category::Military if !manned => {
                Some((WARNING_PEN, slot(status::UNOCCUPIED)))
            }
            s2_sim::Category::Workplace if state.contains(BuildingState::STOPPED) => {
                Some((WARNING_PEN, slot(status::STOPPED)))
            }
            s2_sim::Category::Workplace => row.output.map(|_| {
                let productivity = building.productivity;
                let pen = if productivity < MIDDLING_FROM {
                    WARNING_PEN
                } else if productivity < HEALTHY_FROM {
                    MIDDLING_PEN
                } else {
                    HEALTHY_PEN
                };
                (pen, format!("({productivity}%)").into_bytes())
            }),
            s2_sim::Category::Military => {
                let inside = world
                    .military_of(record)
                    .map_or(0, |(_, military)| military.inside);
                let line = if inside == 1 {
                    slot(status::ONE_SOLDIER)
                } else {
                    fill_number(&slot(status::SOLDIERS), inside)
                };
                Some((LABEL_PEN, line))
            }
            s2_sim::Category::Storehouse => None,
        }
    }

    /// The title a window of `id` shows: its kind's slot, or a building's name.
    fn title(&self, id: WindowId) -> Option<Label> {
        if kind::is_building(id) {
            self.houses
                .iter()
                .find(|house| house.id == id)
                .map(BuildingWindow::title)
        } else if id == kind::OPTIONS {
            Some(Label::Text("Options"))
        } else if id == kind::SOUND_TEST {
            Some(Label::Text("Sound test"))
        } else if id == kind::NOTICE {
            Some(Label::Text(self.noticed.0))
        } else if id == kind::REPAIRS {
            Some(Reading::Repairs.title())
        } else {
            windows::title(id).map(Label::Slot)
        }
    }

    /// Opens the window of `player`'s building on `node` beside the press at `at`, the way a
    /// click on the map does (`0x44da0`), and says whether there is one to open. A window the
    /// building already has is unrolled when rolled up and otherwise left as it is.
    pub(crate) fn open_building(&mut self, game: &Game, player: Player, node: Node, at: Point) -> bool {
        let Some(view) = BuildingView::read(game.world(), player, node) else {
            return false;
        };
        if self.ui.paused() {
            log::debug!("a building's window stays shut while the game is paused");
            return true;
        }
        if let Some(house) = self.houses.iter().find(|house| house.record == view.record) {
            let id = house.id;
            if self.ui.window(id).is_some_and(s2_ui::Window::rolled) {
                self.ui.roll(id);
            }
            return true;
        }
        let id = (kind::FIRST_BUILDING.0..=u16::MAX)
            .map(WindowId)
            .find(|&id| self.houses.iter().all(|house| house.id != id))
            .unwrap_or(kind::FIRST_BUILDING);
        let (width, height) = view.page.size();
        let (x, y) = beside(self.screen, at, (width, height));
        self.ui
            .open(id, within(self.screen, Rect::new(x, y, width, height)));
        self.ui.bring_to_front(id);
        self.houses.push(BuildingWindow::new(id, &view, false));
        true
    }

    /// Closes the buildings' windows whose building finished, went or changed hands, and
    /// forgets the ones the player closed (`0x13ad4`).
    fn keep_houses(&mut self, game: &Game, player: Player) {
        let world = game.world();
        let ui = &mut self.ui;
        self.houses.retain(|house| {
            let open = ui.is_open(house.id);
            let current = house.current(BuildingView::read(world, player, house.node).as_ref());
            if open && !current {
                ui.close(house.id);
            }
            open && current
        });
    }

    /// Draws the part of the frame around a window of `id` that goes under its own controls.
    /// The toolbar and the action menu carry no frame.
    fn frame_under(&mut self, id: WindowId, front: bool, painter: &mut Painter) {
        if !windows::framed(id) {
            return;
        }
        let Some(area) = self.ui.window(id).map(|window| window.area) else {
            return;
        };
        let mut chrome = Chrome::new(id, self.title(id));
        if id == kind::HELP_PAGE
            && let Some(window) = self.helped.as_ref()
        {
            chrome.client = Some(ReadingWindow::panel(window.reading, area));
        }
        chrome.under(painter, &self.skin, (area, front));
    }

    /// Draws the corner buttons, under the window's own controls, and gives back what a release
    /// on one asks for.
    fn frame_over(&mut self, id: WindowId, painter: &mut Painter) -> Option<Outcome> {
        if !windows::framed(id) {
            return None;
        }
        let area = self.ui.window(id)?.area;
        let chrome = Chrome::new(id, self.title(id));
        let mouse = self.pointer_for(id);
        chrome.over(&mut self.ui, painter, &self.skin, area, mouse)
    }

    /// Draws the line of help the pointer is over into the strip the window of `id` carries.
    fn help_line(&mut self, id: WindowId, painter: &mut Painter) {
        let Some(help) = self.helping else {
            return;
        };
        let Some(window) = self.ui.window(id).copied() else {
            return;
        };
        if window.rolled() {
            return;
        }
        let strip = if kind::is_building(id) {
            self.houses
                .iter()
                .find(|house| house.id == id)
                .and_then(|house| house.help_strip(window.area))
        } else {
            windows::help_strip(id, window.area)
        };
        if let Some(rect) = strip {
            help.with_text(&self.texts, |line| helpline::strip(painter, rect, line));
        }
    }

    /// Puts the toolbar along the bottom of `bounds`, opening it on the first frame behind any
    /// window already open. It belongs to the desktop, so every window draws over it.
    fn stand_toolbar(&mut self, bounds: Rect) {
        let area = Toolbar::area(bounds);
        if self.ui.is_open(Toolbar::KIND) {
            self.ui.move_to(Toolbar::KIND, Point::new(area.x, area.y));
        } else {
            self.ui.open_behind(Toolbar::KIND, area);
        }
    }

    /// How many letters the player has not seen, which the toolbar prints on its messages button.
    ///
    /// The original keeps the count on the button itself and the sim holds no read mark, so the
    /// letters that arrive while the post office is shut are the ones that count.
    fn waiting(&mut self, game: &Game, player: Player) -> u16 {
        let letters = game.world().messages(player).list().len();
        if self.ui.is_open(kind::POST) {
            self.letters = letters;
        }
        u16::try_from(letters.saturating_sub(self.letters)).unwrap_or(u16::MAX)
    }

    /// Draws the window of `id` and gives back what a release on it does.
    fn window(
        &mut self,
        id: WindowId,
        game: &Game,
        player: Player,
        terrain: &Terrain,
        painter: &mut Painter,
    ) -> Option<Outcome> {
        let world = game.world();
        let mouse = self.pointer_for(id);
        let waiting = self.waiting(game, player);
        let (ui, skin) = (&mut self.ui, &self.skin);
        match id {
            id if kind::is_building(id) => {
                let house = self.houses.iter_mut().find(|house| house.id == id)?;
                let view = BuildingView::read(world, player, house.node)?;
                house.run(ui, painter, skin, &view, mouse)
            }
            Toolbar::KIND => Toolbar::run(ui, painter, skin, waiting, mouse),
            kind::ACTION => {
                let (view, types) = action_menu(&mut self.action, self.node, game, player);
                let outcome = self.action.run(ui, painter, skin, &view, &types, mouse);
                let (width, height) = self.action.size(types.len());
                ui.resize(kind::ACTION, width, height);
                // A resize holds the window inside its parent (`0x84a8c`), which pulls a menu
                // opened low on the screen back up as its rows deepen it.
                if let Some(area) = ui.window(kind::ACTION).map(|window| window.area) {
                    let held = within(self.screen, area);
                    ui.move_to(kind::ACTION, Point::new(held.x, held.y));
                }
                outcome
            }
            kind::MAP => {
                let map = &mut self.map;
                map.read(|x, y| node_colours(game, player, terrain, Node { x, y }));
                let outcome = map.run(ui, painter, skin, mouse, true);
                if outcome == Some(Outcome::Map(map::Shown::Swap))
                    && let Some(window) = ui.window(kind::MAP)
                {
                    let (width, height) = map.swapped(window.area);
                    ui.resize(kind::MAP, width, height);
                }
                outcome
            }
            kind::OBSERVE => {
                let outcome = self.observe.run(ui, painter, skin, mouse);
                if outcome == Some(Outcome::Observe(observe::Asked::Size)) {
                    let (width, height) = self.observe.resize();
                    ui.resize(kind::OBSERVE, width, height);
                }
                outcome
            }
            kind::DEMOLISH => {
                let subject = match world.object(self.node) {
                    MapObject::Building(_) => demolish::Subject::Building,
                    _ => demolish::Subject::Flag,
                };
                let view = DemolishView::read(world, self.node, subject);
                self.demolish.run(ui, painter, skin, &view, mouse)
            }
            kind::QUIT => self.quit.run(ui, painter, skin, mouse),
            kind::CD_PLAYER => self.cd.run(ui, painter, skin, &self.tracks, mouse),
            kind::MIDI_PLAYER => self.midi.run(ui, painter, skin, mouse),
            kind::README | kind::KEYS | kind::HELP_PAGE | kind::REPAIRS => {
                self.read(id, painter, mouse);
                None
            }
            kind::DIARY | kind::NOTE => self.diary(id, painter, mouse),
            kind::HELP_LINE => {
                let mut window = self.help;
                match self.helping {
                    Some(help) => help.with_text(&self.texts, |line| window.run(ui, painter, line)),
                    None => window.run(ui, painter, &[]),
                }
                self.help = window;
                None
            }
            kind::LOAD_GAME | kind::SAVE_GAME => {
                let slots: Vec<Option<s2_ui::windows::Slot<'_>>> = self
                    .saved
                    .iter()
                    .map(|held| {
                        held.as_ref().map(|(number, name)| s2_ui::windows::Slot {
                            number: *number,
                            name,
                        })
                    })
                    .collect();
                let window = if id == kind::LOAD_GAME {
                    &mut self.loading
                } else {
                    &mut self.saving
                };
                window.run(ui, painter, skin, &slots, mouse)
            }
            kind::SELECT => self.select.run(ui, painter, skin, mouse),
            kind::MENU => self.menu.run(ui, painter, skin, &self.sound, mouse),
            _ => self.settings(id, game, player, painter),
        }
    }

    /// Draws the file viewer or help page `id` names.
    fn read(&mut self, id: WindowId, painter: &mut Painter, mouse: Pointer) {
        let window = if id == kind::HELP_PAGE {
            self.helped.as_mut()
        } else if id == kind::REPAIRS {
            self.repairs.as_mut()
        } else {
            self.reading
                .iter_mut()
                .map(|(window, _)| window)
                .find(|window| window.reading.kind() == id)
        };
        if let Some(window) = window {
            window.run(&mut self.ui, painter, &self.skin, mouse);
        }
    }

    /// Draws the letter or the small diary, whichever `id` names, and gives back what a release on
    /// it does.
    fn diary(&mut self, id: WindowId, painter: &mut Painter, mouse: Pointer) -> Option<Outcome> {
        let height = self.fonts.get(FontId::Medium).line_height();
        let window = if id == kind::DIARY {
            self.letter.as_mut().map(|(window, _)| window)
        } else {
            self.note.as_mut()
        };
        window?.run(&mut self.ui, painter, &self.skin, height, mouse)
    }

    /// Draws the screens that only read the world, and the options, and gives back what a release
    /// on one does.
    fn settings(
        &mut self,
        id: WindowId,
        game: &Game,
        player: Player,
        painter: &mut Painter,
    ) -> Option<Outcome> {
        let world = game.world();
        let mouse = self.pointer_for(id);
        let (ui, skin) = (&mut self.ui, &self.skin);
        match id {
            kind::OPTIONS => {
                let view = self.options_view?;
                self.options_window.run(ui, painter, skin, &view, mouse)
            }
            kind::SOUND_TEST => {
                let sampled = self.sampled;
                let has = move |id: u8| sampled.get(usize::from(id)).copied().unwrap_or(false);
                self.sound_test.run(ui, painter, skin, &has, mouse)
            }
            kind::NOTICE => {
                let lines: Vec<Label> = self.noticed.1.iter().map(|line| line.label(painter)).collect();
                self.notice.run(ui, painter, skin, &lines, mouse)
            }
            kind::WINNER => {
                let view = WinnerView::read(world, self.won?);
                self.winner.run(ui, painter, skin, &view, mouse)
            }
            kind::BUILDINGS => {
                let view = BuildingsView::read(world, player, |kind| game.may_build(player, kind));
                self.buildings.run(ui, painter, skin, &view, mouse)
            }
            kind::DISTRIBUTION => {
                let view = DistributionView::read(world, player, self.distribution.page);
                self.distribution.run(ui, painter, skin, &view, mouse)
            }
            kind::MILITARY => {
                let view = MilitaryView::read(world, player);
                self.military.run(ui, painter, skin, &view, mouse)
            }
            kind::POST => {
                let view = PostView::read(world, player, self.post.shown, &self.post.types);
                self.post.run(ui, painter, skin, &view, mouse)
            }
            kind::PRODUCTION => {
                let view = ProductionView::read(world, player, |kind| game.may_build(player, kind));
                self.production.run(ui, painter, skin, &view);
                None
            }
            kind::SHIPS => {
                let view = ShipsView::read(world, player, self.ships.chosen);
                self.ships.run(ui, painter, skin, &view, mouse)
            }
            kind::STATISTICS => {
                let view = StatisticsView::read(world, self.statistics.statistic, self.statistics.span);
                self.statistics.run(ui, painter, skin, &view, mouse)
            }
            kind::TOOLS => {
                let view = ToolsView::read(world, player);
                self.tools.run(ui, painter, skin, &view, mouse)
            }
            kind::TRANSPORT => {
                let view = TransportView::read(world, player);
                self.transport.run(ui, painter, skin, &view, mouse)
            }
            kind::STOCK => {
                let view = StockView::realm(world, player);
                self.stock.run(ui, painter, skin, &view, mouse)
            }
            kind::MERCHANDISE => {
                let view =
                    MerchandiseView::read(world, player, self.merchandise.chosen, self.merchandise.span);
                self.merchandise.run(ui, painter, skin, &view, mouse)
            }
            _ => None,
        }
    }

    /// Acts on what a window asked for, collecting into `asks` what only the game can answer.
    ///
    /// A paused game takes no command at all. The windows that send them are shut while it is
    /// paused, so this only catches one that opened before the pause or asked for something
    /// without being open, and it keeps the rule in one place rather than in each window.
    fn act(&mut self, outcome: Outcome, asks: &mut Asks) {
        if self.ui.paused() {
            match outcome {
                Outcome::Open(id) | Outcome::Ask(id) | Outcome::Roll(id)
                    if !windows::live_while_paused(id) =>
                {
                    log::debug!("window {:#04x} stays as it is while the game is paused", id.0);
                    return;
                }
                Outcome::ConstructionAid | Outcome::HouseNames => return,
                _ => {}
            }
        }
        match outcome {
            Outcome::Command(_) if self.ui.paused() => {
                log::debug!("a paused game takes no command");
            }
            Outcome::Command(command) => asks.commands.push(command),
            Outcome::CommandAndClose(command, id) => {
                if !self.ui.paused() {
                    asks.commands.push(command);
                }
                self.ui.close(id);
            }
            Outcome::Open(id) => self.open(id),
            Outcome::Ask(id) => self.ask(id),
            Outcome::ConstructionAid => self.options ^= keys::flag::SPACE,
            Outcome::Roll(id) => {
                self.ui.roll(id);
                self.effects.push(ROLL);
            }
            Outcome::Close(kind::DIARY) => self.close_letter(asks),
            Outcome::Close(id) => {
                self.ui.close(id);
            }
            Outcome::Centre(node) => asks.centre = Some(node),
            Outcome::Demolish(node, id) => {
                self.ui.close(kind::DEMOLISH);
                self.node = node;
                self.open(kind::DEMOLISH);
                self.ui.close(id);
            }
            Outcome::Headquarters => asks.home = true,
            Outcome::Quit(answer) => {
                self.ui.close(kind::QUIT);
                match answer {
                    Answer::Leave => asks.leave = true,
                    Answer::Stay => {}
                    Answer::Save => {
                        self.ui.close(kind::LOAD_GAME);
                        self.open(kind::SAVE_GAME);
                    }
                }
            }
            Outcome::Help(page) => self.open_help(page),
            Outcome::HouseNames => self.name_houses(),
            Outcome::Map(_) | Outcome::Observe(_) => {}
            Outcome::Saves(ask) => self.asked_of_a_save(ask, asks),
            Outcome::Menu(pick) => self.picked(pick, asks),
            Outcome::EndGame => asks.leave = true,
            Outcome::Play(play) => asks.music.push(Music::Play(play)),
            Outcome::Options(change) => asks.options.push(change),
            Outcome::TestEffect(effect) => asks.tests.push(effect),
            other @ Outcome::Routes(_) => log::debug!("{other:?} is not wired to the viewer yet"),
        }
    }

    /// Opens help page `page`, which closes the page already open and reads the page's own
    /// file, the way `0x452f8` builds a new window of the kind each time.
    fn open_help(&mut self, page: u16) {
        self.ui.close(kind::HELP_PAGE);
        let reading = Reading::Help(page);
        let (width, height) = reading.size();
        let mut window = ReadingWindow::new(reading);
        let area = Rect::new(0, 0, width, height);
        if let Some(text) = self.help_text.get(&page) {
            window.read(text, area);
        } else {
            log::debug!("help page {page} is not in the data set");
        }
        self.helped = Some(window);
        self.open(kind::HELP_PAGE);
        // The page opens at the size its table gives and `0x59b14` then grows it to its text,
        // so the corner it is centred on comes from the table and not from the text.
        if let Some(grown) = self.helped.as_ref().and_then(ReadingWindow::grown)
            && let Some(area) = self.ui.window(kind::HELP_PAGE).map(|window| window.area)
        {
            self.ui.resize(kind::HELP_PAGE, area.width, grown);
            let held = within(self.screen, Rect::new(area.x, area.y, area.width, grown));
            self.ui.move_to(kind::HELP_PAGE, Point::new(held.x, held.y));
        }
    }

    /// Acts on a press on one of the two save screens: a slot closes the screen and asks for the
    /// game to be saved into it or loaded from it, as `0x4c290` does.
    fn asked_of_a_save(&mut self, ask: saves::Ask, asks: &mut Asks) {
        match ask {
            saves::Ask::Cancel => {}
            saves::Ask::Automatic(period) => {
                asks.automatic = Some(period);
                return;
            }
            saves::Ask::Load(place) => asks.load = Some(place),
            saves::Ask::Save(place) => asks.save = Some(place),
        }
        self.ui.close(kind::LOAD_GAME);
        self.ui.close(kind::SAVE_GAME);
    }

    /// Lists `saved`, each slot's number and name, on the two save screens.
    pub(crate) fn show_saved(&mut self, saved: Vec<Option<(i32, Vec<u8>)>>) {
        self.saved = saved;
    }

    /// Marks automatic save period `period` on the save screen.
    pub(crate) const fn show_automatic(&mut self, period: usize) {
        self.saving.automatic = period;
    }

    /// Acts on a press on the game menu.
    fn picked(&mut self, pick: Pick, asks: &mut Asks) {
        match pick {
            Pick::Music => asks.music.push(Music::Switch),
            Pick::Effects => self.sound.effects = !self.sound.effects,
            Pick::MusicVolume(step) => self.sound.music_volume = volume(self.sound.music_volume, step),
            Pick::EffectVolume(step) => {
                self.sound.effect_volume = volume(self.sound.effect_volume, step);
            }
            Pick::Keys => self.ask(kind::KEYS),
            Pick::Readme => self.ask(kind::README),
            Pick::Load => self.ask(kind::LOAD_GAME),
            Pick::Save => self.ask(kind::SAVE_GAME),
            Pick::Midi => self.ask(kind::MIDI_PLAYER),
            Pick::Cd => self.ask(kind::CD_PLAYER),
            Pick::Options if self.options_view.is_some() => self.ask(kind::OPTIONS),
            Pick::Options => {}
            Pick::Leave => asks.leave = true,
        }
    }
}

/// `format` with its `%d` replaced by `number`, as `sprintf` fills slot `0x227` (`0x1f5b6`).
fn fill_number(format: &[u8], number: u8) -> Vec<u8> {
    const MARK: &[u8] = b"%d";
    match format.windows(MARK.len()).position(|window| window == MARK) {
        Some(at) => [
            &format[..at],
            number.to_string().as_bytes(),
            &format[at + MARK.len()..],
        ]
        .concat(),
        None => format.to_vec(),
    }
}

/// What the map screen reads about `node`.
fn node_colours(game: &Game, player: Player, terrain: &Terrain, node: Node) -> map::MapNode {
    let nodes = game.nodes();
    let data = nodes.get(node);
    let object = game.world().object(node);
    let tree = match object {
        MapObject::Tree(tree) if tree.stage == Tree::GROWN && !tree.felling => Some(tree.kind),
        _ => None,
    };
    map::MapNode {
        owner: data.owner,
        seen: nodes.seen(node, player).contains(&true),
        road: data.roads.to_byte() != 0,
        building: matches!(object, MapObject::Building(_)),
        terrain: terrain.map_colour(node, tree),
        area: data.area,
    }
}

/// A volume stepped by `step` and held inside what the bars show.
fn volume(volume: u32, step: i32) -> u32 {
    let wanted = i64::from(volume) + i64::from(step);
    u32::try_from(wanted.clamp(0, i64::from(menu::LOUDEST))).unwrap_or_default()
}

/// What the action menu shows on `node`: the view its records read, and the building types its
/// list row offers. The first tab of each row it opens is chosen here, so the types are gathered
/// for the size that choice lands on.
fn action_menu(
    action: &mut ActionWindow,
    node: Node,
    game: &Game,
    player: Player,
) -> (ActionView, Vec<action::Offer>) {
    let drawing = game.road_draft(player).is_some();
    let view = ActionView::read(game, player, node, drawing);
    action.open_first(&view);
    let types = match action.listing() {
        Some(size) => {
            let room = action::Room::read(game, player, node);
            action::types(game.world().building_types(), size, room, |kind| {
                game.may_build(player, kind)
            })
        }
        None => Vec::new(),
    };
    (view, types)
}

/// How big the original draws a window of `id`, where the kind is wired up.
fn size(id: WindowId) -> Option<(u32, u32)> {
    let size = match id {
        kind::SELECT => SelectWindow::size(),
        kind::MENU => MenuWindow::size(),
        kind::BUILDINGS => BuildingsWindow::size(),
        kind::DISTRIBUTION => DistributionWindow::size(),
        kind::MILITARY => MilitaryWindow::size(),
        kind::POST => PostWindow::size(),
        kind::PRODUCTION => ProductionWindow::size(),
        kind::SHIPS => ShipsWindow::size(),
        kind::STATISTICS => StatisticsWindow::size(),
        kind::WINNER => WinnerWindow::size(),
        kind::TOOLS => ToolsWindow::size(),
        kind::TRANSPORT => TransportWindow::size(),
        kind::MERCHANDISE => MerchandiseWindow::size(),
        kind::STOCK => StockWindow::size(),
        kind::DEMOLISH => DemolishWindow::size(),
        kind::QUIT => QuitWindow::size(),
        kind::CD_PLAYER => CdWindow::size(0),
        kind::MIDI_PLAYER => MidiWindow::size(),
        kind::HELP_LINE => HelpLineWindow::size(),
        kind::LOAD_GAME => SavesWindow::size(saves::Mode::Load),
        kind::SAVE_GAME => SavesWindow::size(saves::Mode::Save),
        kind::README => Reading::Readme.size(),
        kind::KEYS => Reading::Keys.size(),
        kind::ACTION => ActionWindow::new(kind::ACTION).size(0),
        _ => return None,
    };
    Some(size)
}

/// Every help page the data set holds, by its number. The original reads one file as the page
/// opens; reading them once is the same text with no file work in the frame.
fn help_pages(sheets: &Sheets<'_>) -> BTreeMap<u16, Vec<u8>> {
    let folder = s2_ui::windows::reading::HELP_FOLDER;
    let mut pages = BTreeMap::new();
    let Ok(entries) = sheets.data.list_dir(folder) else {
        log::debug!("{folder}: no help pages");
        return pages;
    };
    for entry in entries {
        let Some(page) = help_number(&entry.name) else {
            continue;
        };
        if Reading::help_file(page, false).eq_ignore_ascii_case(&entry.name) {
            match sheets.text(&format!("{folder}/{}", entry.name)) {
                Ok(text) => {
                    pages.insert(page, lines(text));
                }
                Err(error) => log::debug!("{}: {error}", entry.name),
            }
        }
    }
    pages
}

/// The page number a help file's name holds, or nothing for a name of another shape.
fn help_number(name: &str) -> Option<u16> {
    let stem = name.split('.').next()?;
    let digits = stem.strip_prefix("HELP").or_else(|| stem.strip_prefix("help"))?;
    digits.parse().ok()
}

/// The two reading screens with the text each one shows, which stays empty where the data set
/// has no such file.
fn pages(sheets: &Sheets<'_>) -> [(ReadingWindow, Vec<u8>); PAGES.len()] {
    PAGES.map(|(id, path)| {
        let reading = Reading::ALL
            .into_iter()
            .find(|reading| reading.kind() == id)
            .unwrap_or(Reading::Readme);
        let text = sheets.text(path).map_or_else(
            |error| {
                log::debug!("{path}: {error}");
                Vec::new()
            },
            lines,
        );
        (ReadingWindow::new(reading), text)
    })
}

/// The lines of a shipped page: a string table's slots one to a line, or the bytes as they stand
/// for a file that is not one.
pub(crate) fn lines(text: Text) -> Vec<u8> {
    let table = match text {
        Text::Table(table) => table,
        Text::Page(bytes) => return bytes,
    };
    let mut text = Vec::new();
    for (_, line) in table.iter() {
        text.extend_from_slice(line);
        text.extend_from_slice(b"\r\n");
    }
    text
}

/// Where the corner of a window `width` by `height` goes for a pointer at `at`: beside the
/// pointer, on the side with room for it, with its middle one notch above the pointer
/// (`0x44f74`). The action menu and the demolish prompt open this way.
fn beside(screen: Rect, at: Point, (width, height): (u32, u32)) -> (i32, i32) {
    let (gap, lift) = MENU_BESIDE;
    let across = i32::try_from(width).unwrap_or_default();
    let right = screen.x + i32::try_from(screen.width).unwrap_or_default();
    let x = if at.x >= right - across - gap {
        at.x - across - gap
    } else {
        at.x + gap
    };
    let y = at.y - i32::try_from(height / 2).unwrap_or_default() - lift;
    (x, y)
}

/// Moves `area` until it sits inside `screen`, the way `0x7eab8` holds a window in its parent.
fn within(screen: Rect, area: Rect) -> Rect {
    let fit = |at: i32, size: u32, edge: i32, span: u32| {
        let last = edge + i32::try_from(span.saturating_sub(size)).unwrap_or_default();
        at.clamp(edge, last.max(edge))
    };
    Rect::new(
        fit(area.x, area.width, screen.x, screen.width),
        fit(area.y, area.height, screen.y, screen.height),
        area.width,
        area.height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The screen the original ran at, which the captures below were taken on.
    const SCREEN: Rect = Rect::new(0, 0, 640, 480);

    #[test]
    fn a_window_that_has_been_moved_opens_where_it_was_left() {
        let kept = Point::new(600, 400);
        let area = within(SCREEN, Rect::new(kept.x, kept.y, 202, 138));
        assert_eq!(
            (area.x, area.y),
            (438, 342),
            "a corner near the edge is pulled back inside the screen"
        );
        let area = within(SCREEN, Rect::new(20, 30, 202, 138));
        assert_eq!((area.x, area.y), (20, 30), "a corner with room stays put");
    }

    #[test]
    fn the_demolish_prompt_opens_beside_the_pointer_at_its_own_height() {
        // An S2 capture: pulling a flag down from a button at (348, 362) opened the 200 by
        // 200 prompt at (388, 252).
        assert_eq!(beside(SCREEN, Point::new(348, 362), (200, 200)), (388, 252));
    }

    #[test]
    fn the_action_menu_opens_beside_the_press() {
        // An S2 capture: a press at (150, 120) opened a 202 by 138 menu at (190, 71), and one
        // at (300, 200) a menu of the same width at (340, 151).
        assert_eq!(beside(SCREEN, Point::new(150, 120), (202, MENU_BUILT)), (190, 71));
        assert_eq!(
            beside(SCREEN, Point::new(300, 200), (202, MENU_BUILT)),
            (340, 151)
        );
    }

    #[test]
    fn a_press_with_no_room_to_its_right_opens_the_menu_to_its_left() {
        // The same capture: a press at (520, 120) opened the menu at (278, 71).
        assert_eq!(beside(SCREEN, Point::new(520, 120), (202, MENU_BUILT)), (278, 71));
        // The press one pixel short of the turn keeps the menu on the right.
        assert_eq!(beside(SCREEN, Point::new(397, 120), (202, MENU_BUILT)).0, 437);
        assert_eq!(beside(SCREEN, Point::new(398, 120), (202, MENU_BUILT)).0, 156);
    }

    #[test]
    fn a_menu_deeper_than_the_room_below_it_is_held_on_the_screen() {
        // The same capture: a press at (150, 380) opened a 218-deep menu, which left 331 for
        // its corner and ended up at 262.
        let (x, y) = beside(SCREEN, Point::new(150, 380), (202, MENU_BUILT));
        assert_eq!((x, y), (190, 331));
        assert_eq!(
            within(SCREEN, Rect::new(x, y, 202, 218)),
            Rect::new(190, 262, 202, 218)
        );
    }

    #[test]
    fn the_viewers_own_keys_are_left_alone() {
        for scan in LEFT_TO_THE_VIEWER {
            assert!(
                keys::bound(Scancode(scan), false, true).is_some(),
                "{scan:#04x} is only worth listing while the original binds it"
            );
        }
        // The viewer's other keys need no entry: the original leaves them to the view.
        for scan in [33, 34] {
            assert_eq!(keys::bound(Scancode(scan), false, true), None);
        }
        // Except the pause, which the original binds and the viewer owns.
        assert_eq!(
            keys::bound(Scancode(25), false, false).map(|bound| bound.action),
            Some(Action::Pause)
        );
    }

    #[test]
    fn every_wired_kind_has_a_size() {
        for id in [
            kind::SELECT,
            kind::MENU,
            kind::BUILDINGS,
            kind::DISTRIBUTION,
            kind::MILITARY,
            kind::POST,
            kind::PRODUCTION,
            kind::SHIPS,
            kind::STATISTICS,
            kind::TOOLS,
            kind::TRANSPORT,
            kind::MERCHANDISE,
            kind::HELP_LINE,
            kind::LOAD_GAME,
            kind::SAVE_GAME,
        ] {
            let (width, height) = size(id).unwrap_or_else(|| panic!("{:#04x}", id.0));
            assert!(width > 0 && height > 0);
        }
        assert_eq!(size(kind::ROUTES), None, "nothing opens the trading routes");
    }

    #[test]
    fn a_volume_steps_by_eight_and_stays_inside_its_bar() {
        assert_eq!(volume(0, -menu::STEP), 0);
        assert_eq!(volume(0, menu::STEP), 8);
        assert_eq!(volume(3, -menu::STEP), 0);
        assert_eq!(volume(menu::LOUDEST, menu::STEP), menu::LOUDEST);
        let presses =
            std::iter::successors(Some(menu::LOUDEST), |&v| (v > 0).then(|| volume(v, -menu::STEP)));
        assert_eq!(presses.count(), 17);
    }
}
