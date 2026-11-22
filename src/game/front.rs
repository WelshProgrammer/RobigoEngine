//! The screens before a game: the intro film, the title picture, which a left click passes, the
//! main menu, the campaign's chapter screens and Unlimited play's settings, with Robigo's options
//! window opening over the menu. A screen is 640 by 480 at
//! the largest whole scale the window holds, centred with black around it, through the picture's
//! own palette.

use s2_assets::{Remap, Tint, dac_palette};
use s2_audio::EFFECT_IDS;
use s2_audio::Frames;
use s2_mods::{Backdrop, Sheets};
use s2_platform::{Cursor, Event, Key, MouseButton, Platform, Window};
use s2_render::terrain::Point;
use s2_render::{Frame, Overlay, Paint, Patch, Renderer, Zoom, blit, overlay};
use s2_ui::chrome::Chrome;
use s2_ui::geometry::Rect;
use s2_ui::mouse_pointer::{self, MousePointer, PointerButton, PointerPicture};
use s2_ui::paint::{Canvas, ItemId, Painter, paint};
use s2_ui::screens::chapters::{self, ChapterPick, Chapters};
use s2_ui::screens::maps::MapList;
use s2_ui::screens::settings::{self as setup, Items, SettingsScreen};
use s2_ui::screens::world::{self as world_screen, Continents, WorldPick, WorldScreen, WorldView};
use s2_ui::screens::{self, Choice, MainMenu};
use s2_ui::store::Ui;
use s2_ui::text::{FontId, Label, WithRuns};
use s2_ui::widget::{Pointer, WindowId};
use s2_ui::windows::music::Play;
use s2_ui::windows::notice::Line;
use s2_ui::windows::{
    Change, NoticeWindow, OptionsView, OptionsWindow, Outcome, SavesWindow, Slot, SoundTestWindow,
    TestEffect, kind, saves,
};

use super::campaign::Progress;
use super::credits::Credits;
use super::desktop::{CLICK, Kit, ROLL, kit, pointer_cursor, pointer_patches};
use super::intro::{self, Intro};
use super::loading;
use super::slots::Slots;
use super::unlimited::{FIRST_MAP, Leads, Maps, Unlimited};
use super::world::WorldPicture;
use crate::error::Result;
use crate::scene::Start;

/// Which screen shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Showing {
    Title,
    Menu,
    Chapter,
    ChapterList,
    /// The picture a won chapter or continent leads to, which a left click passes.
    Won,
    Settings,
    /// The World Campaign's world screen.
    World,
    /// The picture a game shows while it starts.
    Loading,
    /// The credits, which a key leaves.
    Credits,
}

/// The picture a game shows while it starts, and the game it leads to.
struct Loading {
    picture: Backdrop,
    frame: Frame,
    since: std::time::Instant,
    game: Start,
    /// Whether the player passed it with a click or a key.
    passed: bool,
}

/// The line the won picture shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WonLine {
    /// A Roman chapter's, from 0.
    Chapter(u8),
    /// A continent's, or the world's once `all` are won.
    Continent { all: bool },
}

/// What the intro film asks of the sound.
pub(crate) enum FilmCue {
    /// Play the film's sound in place of the music.
    Start(Box<dyn Frames>),
    /// The film is over: the music comes back.
    Stop,
}

/// What the screens asked for on a frame.
#[derive(Debug, Default)]
pub(crate) struct Asked {
    /// The main menu's button the player released on.
    pub(crate) choice: Option<Choice>,
    /// What the player asked to change on the options window, in order.
    pub(crate) changes: Vec<Change>,
    /// Whether the player pressed Start game on the settings screen.
    pub(crate) start_game: bool,
    /// The campaign chapter, from 0, whose Play chapter the player pressed.
    pub(crate) chapter: Option<u8>,
    /// The World Campaign continent whose Start the player pressed.
    pub(crate) continent: Option<u8>,
    /// The saved game the load screen asked for.
    pub(crate) load: Option<std::path::PathBuf>,
    /// The game whose loading picture has shown its time or been passed.
    pub(crate) ready: Option<Start>,
    /// The effects the sound test asked to play, in order.
    pub(crate) tests: Vec<TestEffect>,
    /// The songs the sound test asked for, in order.
    pub(crate) songs: Vec<Play>,
}

/// The title and the main menu, and the windows that open over them.
pub(crate) struct FrontEnd {
    kit: Kit,
    /// The tables a marked control lightens through, a disabled one greys through and a label
    /// darkens through, made from the pictures' palette.
    remaps: (Remap, Remap, Remap),
    hand: MousePointer,
    software_pointer: bool,
    ui: Ui,
    pointer: Pointer,
    title: Backdrop,
    menu: Backdrop,
    settings: Backdrop,
    /// The three pictures as frames, ready to paste.
    title_frame: Frame,
    menu_frame: Frame,
    settings_frame: Frame,
    logo: Option<ItemId>,
    showing: Showing,
    main: MainMenu,
    setup: SettingsScreen,
    setup_items: Items,
    /// The settings screen's map, players and settings, while it is open.
    unlimited: Option<Unlimited>,
    /// The map list over the settings, while it is open.
    maps: Option<Maps>,
    map_list: MapList,
    chapters: Chapters,
    /// The chapter the chapter screen shows, from 0, which a game keeps (`0x1f78ee`).
    chapter: u8,
    /// Which chapters the chapter list lets the player start, read as the screens are built
    /// (`0x5e10e`).
    progress: Progress,
    /// Whether there is a saved game for Resume last game to load, as the screens are built.
    resumable: bool,
    /// The won picture, the picture as a frame and its line, while it shows.
    won: Option<(Backdrop, Frame, WonLine)>,
    world_screen: WorldScreen,
    continents: Continents,
    /// The world screen's pictures, while it shows.
    world: Option<WorldPicture>,
    /// The world screen's cross and headquarters markers.
    markers: (Option<ItemId>, Option<ItemId>),
    /// What the screens were loaded from, which the map list reads its maps from.
    start: Start,
    options: OptionsWindow,
    view: Option<OptionsView>,
    sound_test: SoundTestWindow,
    /// Which effects have a sample, by id, which the sound test greys the rest by.
    sampled: [bool; EFFECT_IDS],
    notice: NoticeWindow,
    noticed: (&'static str, Vec<Line>),
    /// The loading picture, while it shows.
    loading_picture: Option<Loading>,
    /// The load screen over the menu, and the saved games it lists.
    loading: SavesWindow,
    slots: Option<Slots>,
    effects: Vec<u8>,
    /// The screen's layer and how many output pixels one of its pixels covers: the largest whole
    /// scale the screen fits the window at.
    layer: Frame,
    scale: u8,
    output: (u32, u32),
    /// The interface scale the settings hold, which the game takes.
    ui_scale: u8,
    /// The layer the windows over the screens and the pointer draw into, and its scale: the
    /// interface scale, held to the screen's so a window always fits.
    over: Frame,
    over_scale: u8,
    /// Where the pointer is in the windows' layer.
    over_at: Point,
    /// The screen whose palette the renderer holds.
    shown: Option<Showing>,
    /// The picture SDL shows for the pointer, its scale and the cursor made from it.
    cursor: Option<(PointerPicture, u32, Cursor)>,
    /// The intro film, while it plays.
    intro: Option<Intro>,
    /// What the film asked of the sound since the last call.
    film_cues: Vec<FilmCue>,
    /// The credits while they show.
    credits: Option<Credits>,
}

impl FrontEnd {
    /// The screens for the data `start` names, on an `output` window, starting at the title.
    ///
    /// # Errors
    ///
    /// Any [`crate::error::Error`] from reading the data, the interface or the pictures.
    pub(crate) fn load(
        start: &Start,
        output: (u32, u32),
        (software_pointer, ui_scale): (bool, u8),
    ) -> Result<Self> {
        let data = start.data_set()?;
        let sheets = Sheets {
            stack: start.mods.as_ref(),
            data: &data,
        };
        let kit = kit(&sheets)?;
        let title = sheets.backdrop(screens::TITLE_PICTURE)?;
        let menu = sheets.backdrop(screens::MENU_PICTURE)?;
        let settings = sheets.backdrop(setup::PICTURE)?;
        let mut main = MainMenu::new();
        if WorldPicture::available(&sheets) {
            main.title = Some(Label::Slot(screens::GOLD_TITLE));
        } else {
            main.world = Label::Text("World Campaign");
        }
        let palette = &menu.palette;
        let shadow = Remap::nearest(palette, Tint::SHADOW);
        let lighter = Remap::nearest(palette, Tint::HIGHLIGHT);
        let twice = lighter.then(&lighter);
        let greyed = Remap::nearest(palette, Tint::SCREEN_DISABLED);
        let remaps = (twice.then(&twice), greyed.then(&greyed), shadow);
        let mut front = Self {
            hand: MousePointer::from_items(&kit.items, true),
            logo: kit.items.id(screens::LOGO),
            markers: (
                kit.items.id(world_screen::CROSS),
                kit.items.id(world_screen::HEADQUARTERS),
            ),
            setup_items: Items {
                in_game: kit.items.id(setup::IN_GAME),
                out_of_game: kit.items.id(setup::OUT_OF_GAME),
            },
            kit,
            remaps,
            software_pointer,
            ui: Ui::new(),
            pointer: Pointer::default(),
            title_frame: framed(&title),
            menu_frame: framed(&menu),
            settings_frame: framed(&settings),
            title,
            menu,
            settings,
            showing: Showing::Title,
            main,
            setup: SettingsScreen::new(),
            unlimited: None,
            maps: None,
            map_list: MapList::new(),
            chapters: Chapters::new(),
            chapter: 0,
            progress: Progress::read(Progress::file().as_deref(), start),
            resumable: Slots::open().is_some_and(|slots| slots.latest().is_some()),
            won: None,
            world_screen: WorldScreen::new(),
            continents: Continents::original(),
            world: None,
            start: Start {
                mission: None,
                ..start.clone()
            },
            options: OptionsWindow::new(),
            view: None,
            sound_test: SoundTestWindow::new(),
            sampled: [false; EFFECT_IDS],
            notice: NoticeWindow::new(),
            noticed: ("", Vec::new()),
            loading_picture: None,
            loading: SavesWindow::new(saves::Mode::Load),
            slots: None,
            effects: Vec::new(),
            layer: Frame::new(1, 1, 0),
            scale: 1,
            output: (1, 1),
            ui_scale,
            over: Frame::new(1, 1, 0),
            over_scale: 1,
            over_at: Point::new(0, 0),
            shown: None,
            cursor: None,
            intro: None,
            film_cues: Vec::new(),
            credits: None,
        };
        front.resize(output);
        Ok(front)
    }

    /// Follows the window to an `output` of this many pixels.
    fn resize(&mut self, output: (u32, u32)) {
        self.output = output;
        let fits = (output.0 / screens::SIZE.0).min(output.1 / screens::SIZE.1);
        self.scale = u8::try_from(fits.clamp(1, u32::from(overlay::LARGEST_SCALE))).unwrap_or(1);
        let (width, height) = overlay::size(output, self.scale);
        self.layer = Frame::new(width, height, 0);
        self.lay_over();
    }

    /// Sizes the windows' layer for the output and the interface scale, and puts every open
    /// window back in the middle of the layer it draws into.
    fn lay_over(&mut self) {
        self.over_scale = self.ui_scale.clamp(1, self.scale);
        let (width, height) = overlay::size(self.output, self.over_scale);
        self.over = Frame::new(width, height, 0);
        self.cursor = None;
        let open: Vec<(WindowId, Rect)> = self
            .ui
            .back_to_front()
            .map(|window| (window.id, window.area))
            .collect();
        for (id, area) in open {
            let at = if id == self.map_list.id {
                MapList::area(self.corner())
            } else {
                self.centred(area.width, area.height)
            };
            self.ui.move_to(id, Point::new(at.x, at.y));
        }
    }

    /// Where a window of this size sits in the middle of the windows' layer.
    fn centred(&self, width: u32, height: u32) -> Rect {
        let middle = |room: u32, used: u32| i32::try_from(room.saturating_sub(used) / 2).unwrap_or_default();
        Rect::new(
            middle(self.over.width(), width),
            middle(self.over.height(), height),
            width,
            height,
        )
    }

    /// Where the screen's top-left corner sits in the layer.
    fn corner(&self) -> Point {
        let half = |room: u32, used: u32| i32::try_from(room.saturating_sub(used) / 2).unwrap_or_default();
        Point::new(
            half(self.layer.width(), screens::SIZE.0),
            half(self.layer.height(), screens::SIZE.1),
        )
    }

    /// Plays the intro film from the data `start` names, the first of [`intro::FILMS`] it holds,
    /// then the title. Without one the title shows, with a warning.
    pub(crate) fn play_intro(&mut self, start: &Start) {
        let film = start
            .data_set()
            .map_err(|error| error.to_string())
            .and_then(|data| {
                let sheets = Sheets {
                    stack: start.mods.as_ref(),
                    data: &data,
                };
                intro::FILMS
                    .iter()
                    .find_map(|path| sheets.video(path).ok())
                    .ok_or_else(|| "the data holds no intro film".to_owned())
            });
        match film.and_then(|bytes| Intro::new(bytes).map_err(|error| error.to_string())) {
            Ok(film) => {
                self.ui.close(kind::NOTICE);
                self.ui.close(kind::OPTIONS);
                self.intro = Some(film);
            }
            Err(error) => log::warn!("no intro: {error}"),
        }
    }

    /// Ends the film for the title.
    fn end_intro(&mut self) {
        if self.intro.take().is_some() {
            self.film_cues.push(FilmCue::Stop);
            self.showing = Showing::Title;
            self.shown = None;
            self.cursor = None;
        }
    }

    /// What the film asked of the sound since the last call.
    pub(crate) fn take_film_cues(&mut self) -> Vec<FilmCue> {
        std::mem::take(&mut self.film_cues)
    }

    /// Opens Unlimited play's settings on the first map of the data `start` names. A map that
    /// cannot be read leaves the menu showing, with an error in the log.
    pub(crate) fn open_unlimited(&mut self, start: &Start) {
        match Unlimited::open(start, FIRST_MAP) {
            Ok(unlimited) => {
                self.ui.close(kind::NOTICE);
                self.ui.close(kind::OPTIONS);
                self.unlimited = Some(unlimited);
                self.showing = Showing::Settings;
            }
            Err(error) => log::error!("Unlimited play: {error}"),
        }
    }

    /// Makes `chapter`, from 0, the one the chapter screen shows, as the chapter word keeps the
    /// chapter played last (`0x1f78ee`).
    pub(crate) const fn set_chapter(&mut self, chapter: u8) {
        self.chapter = chapter;
    }

    /// Shows the campaign's chapter screen on the chapter played last.
    pub(crate) fn open_campaign(&mut self) {
        self.ui.close(kind::NOTICE);
        self.ui.close(kind::OPTIONS);
        self.showing = Showing::Chapter;
    }

    /// Shows the picture a won chapter leads to, with the line naming `chapter`, from 0, which a
    /// left click passes for the main menu (`0x1058f`). A picture that cannot be read leaves the
    /// main menu showing, with an error in the log.
    pub(crate) fn show_won(&mut self, chapter: u8) {
        self.chapter = chapter;
        self.show_won_picture(chapters::won_picture(chapter), WonLine::Chapter(chapter));
    }

    /// Shows the picture a won continent leads to, with the line for one continent or, once
    /// every continent is won, for the world (`0x5f532`, `0x60c5a`). A left click passes it for
    /// the main menu.
    pub(crate) fn show_continent_won(&mut self) {
        let all = self.progress.won_continents().iter().all(|&won| won);
        self.show_won_picture(world_screen::won_picture(all), WonLine::Continent { all });
    }

    fn show_won_picture(&mut self, path: &str, line: WonLine) {
        match self.read_backdrop(path) {
            Ok(picture) => {
                let frame = framed(&picture);
                self.won = Some((picture, frame, line));
                self.showing = Showing::Won;
                self.shown = None;
            }
            Err(error) => log::error!("{path}: {error}"),
        }
    }

    /// Shows the credits, with their crowd taking `pace` steps a second. Without what they
    /// read the menu stays.
    pub(crate) fn open_credits(&mut self, pace: u16) {
        let opened = self
            .start
            .data_set()
            .map_err(|error| error.to_string())
            .and_then(|data| {
                let sheets = Sheets {
                    stack: self.start.mods.as_ref(),
                    data: &data,
                };
                Credits::open(&sheets, pace)
            });
        match opened {
            Ok(credits) => {
                self.credits = Some(credits);
                self.showing = Showing::Credits;
                self.shown = None;
            }
            Err(error) => log::error!("the credits: {error}"),
        }
    }

    /// Leaves the credits for the main menu.
    fn close_credits(&mut self) {
        self.credits = None;
        self.showing = Showing::Menu;
        self.shown = None;
    }

    /// The picture at data path `path`, from the mods or the data the screens were loaded from.
    fn read_backdrop(&self, path: &str) -> std::result::Result<Backdrop, String> {
        let data = self.start.data_set().map_err(|error| error.to_string())?;
        Sheets {
            stack: self.start.mods.as_ref(),
            data: &data,
        }
        .backdrop(path)
        .map_err(|error| error.to_string())
    }

    /// Shows the World Campaign's world screen, or, without the Gold Edition's mod, a notice
    /// saying so.
    pub(crate) fn open_world(&mut self) {
        self.ui.close(kind::NOTICE);
        self.ui.close(kind::OPTIONS);
        let loaded = self
            .start
            .data_set()
            .map_err(|error| error.to_string())
            .and_then(|data| {
                let sheets = Sheets {
                    stack: self.start.mods.as_ref(),
                    data: &data,
                };
                if !WorldPicture::available(&sheets) {
                    return Ok(None);
                }
                WorldPicture::load(&sheets, &self.continents, &self.progress.won_continents()).map(Some)
            });
        match loaded {
            Ok(Some(picture)) => {
                self.world = Some(picture);
                self.showing = Showing::World;
                self.shown = None;
            }
            Ok(None) => self.notify(
                "World Campaign",
                [
                    "The World Campaign needs the mod",
                    s2_mods::GOLD_NAME,
                    "enabled. It comes with the",
                    "Gold Edition.",
                ]
                .map(|text| Line::from(Label::Text(text)))
                .to_vec(),
            ),
            Err(error) => log::error!("the World Campaign: {error}"),
        }
    }

    /// Shows the world screen with a notice that the player was beaten on the continent.
    pub(crate) fn show_defeat(&mut self) {
        self.open_world();
        if self.showing == Showing::World {
            self.notify(
                "Defeated",
                ["You have been defeated.", "Pick a continent and try again."]
                    .map(|text| Line::from(Label::Text(text)))
                    .to_vec(),
            );
        }
    }

    /// The game the settings screen describes, over `start`, if it is open.
    pub(crate) fn unlimited_game(&self, start: &Start) -> Option<Start> {
        self.unlimited.as_ref().map(|unlimited| unlimited.game(start))
    }

    /// Goes past the title to the main menu, as leaving a game does.
    pub(crate) fn skip_title(&mut self) {
        self.showing = Showing::Menu;
        self.loading_picture = None;
        self.shown = None;
    }

    /// Whether there is a saved game for Resume last game to load.
    pub(crate) const fn can_resume(&self) -> bool {
        self.resumable
    }

    /// Whether the loading picture shows.
    pub(crate) fn is_loading(&self) -> bool {
        self.showing == Showing::Loading
    }

    /// The interface scale the settings hold.
    pub(crate) const fn ui_scale(&self) -> u8 {
        self.ui_scale
    }

    /// Takes the interface scale the settings now hold, for the windows over the screens and
    /// for the next game.
    pub(crate) fn set_ui_scale(&mut self, scale: u8) {
        self.ui_scale = scale;
        self.lay_over();
    }

    /// Draws the pointer into the layer when `software` is set, as S2 does, and otherwise leaves
    /// it to SDL.
    pub(crate) fn set_software_pointer(&mut self, software: bool) {
        self.software_pointer = software;
        self.cursor = None;
    }

    /// Shows the settings on the options window.
    pub(crate) fn show_options(&mut self, view: OptionsView) {
        self.view = Some(view);
    }

    /// Opens the options window, or closes it when it is open.
    pub(crate) fn toggle_options(&mut self) {
        if self.ui.is_open(kind::OPTIONS) {
            self.ui.close(kind::OPTIONS);
        } else {
            self.open(kind::OPTIONS, OptionsWindow::size());
        }
    }

    /// Opens the sound test, with `sampled` saying which effects have a sample, by id.
    pub(crate) fn open_sound_test(&mut self, sampled: [bool; EFFECT_IDS]) {
        self.sampled = sampled;
        if !self.ui.is_open(kind::SOUND_TEST) {
            self.open(kind::SOUND_TEST, SoundTestWindow::size());
        }
    }

    /// Shows the picture `game` shows while it starts (`0x232ae`), which then leads to it. Gives
    /// back false, showing nothing, when there is no picture for it.
    pub(crate) fn show_loading(&mut self, game: Start) -> bool {
        let random = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| {
                usize::try_from(since.subsec_nanos()).unwrap_or_default()
            });
        let path = self.start.data_set().ok().and_then(|data| {
            let sheets = Sheets {
                stack: self.start.mods.as_ref(),
                data: &data,
            };
            loading::picture(&game, &sheets, random)
        });
        let Some(picture) = path.and_then(|path| self.read_backdrop(&path).ok()) else {
            return false;
        };
        let frame = framed(&picture);
        self.loading_picture = Some(Loading {
            picture,
            frame,
            since: std::time::Instant::now(),
            game,
            passed: false,
        });
        self.showing = Showing::Loading;
        self.shown = None;
        true
    }

    /// Opens the load screen over the menu on the saved games in the preferences folder
    /// (`0x5e7a3`).
    pub(crate) fn open_load(&mut self) {
        self.slots = Slots::open();
        if !self.ui.is_open(kind::LOAD_GAME) {
            self.open(kind::LOAD_GAME, SavesWindow::size(saves::Mode::Load));
        }
    }

    /// Opens a notice titled `title` showing `lines`, in place of any notice already open.
    pub(crate) fn notify(&mut self, title: &'static str, lines: Vec<Line>) {
        self.noticed = (title, lines);
        self.ui.close(kind::NOTICE);
        self.open(kind::NOTICE, NoticeWindow::size());
    }

    /// Opens the window of `id`, `size` big, in the middle of the screen.
    fn open(&mut self, id: WindowId, (width, height): (u32, u32)) {
        let area = self.centred(width, height);
        self.ui.open(id, area);
        self.ui.bring_to_front(id);
    }

    /// The effects the screens played since the last call.
    pub(crate) fn take_effects(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.effects)
    }

    /// Acts on `event`, with `density` output pixels to a window unit. Returns false to quit.
    ///
    /// # Errors
    ///
    /// SDL's error when the window will not go fullscreen.
    pub(crate) fn handle(
        &mut self,
        event: Event,
        window: &mut Window,
        renderer: &mut Renderer,
        density: f64,
    ) -> Result<bool> {
        if self.intro.is_some() {
            match event {
                Event::KeyDown {
                    key: Key::F11,
                    repeat: false,
                    ..
                }
                | Event::PixelSizeChanged { .. }
                | Event::Quit => {}
                Event::KeyDown { repeat: false, .. } | Event::MouseDown { .. } => {
                    self.end_intro();
                    return Ok(true);
                }
                _ => return Ok(true),
            }
        }
        if let Some(loading) = &mut self.loading_picture
            && matches!(
                event,
                Event::KeyDown { repeat: false, .. } | Event::MouseDown { .. }
            )
            && !matches!(event, Event::KeyDown { key: Key::F11, .. })
        {
            loading.passed = true;
            return Ok(true);
        }
        match event {
            Event::Quit => return Ok(false),
            Event::KeyDown {
                key: Key::F11,
                repeat: false,
                ..
            } => window.set_fullscreen(!window.is_fullscreen())?,
            Event::KeyDown { repeat: false, .. } if self.showing == Showing::Credits => self.close_credits(),
            Event::KeyDown {
                key: Key::ESCAPE,
                repeat: false,
                ..
            } => {
                self.ui.close(kind::NOTICE);
                self.ui.close(kind::OPTIONS);
                self.ui.close(kind::SOUND_TEST);
                self.ui.close(kind::LOAD_GAME);
            }
            Event::MouseMotion { x, y, .. } => {
                self.point(x, y, density);
                self.hand.moved(self.over_at);
            }
            Event::MouseDown { button, x, y } => {
                self.point(x, y, density);
                if let Some(which) = pointer_button(button) {
                    self.hand.press(which, self.over_at);
                }
                if button == MouseButton::Left {
                    self.press();
                }
            }
            Event::MouseUp { button, x, y } => {
                self.point(x, y, density);
                if let Some(which) = pointer_button(button) {
                    self.hand.release(which);
                }
                if button == MouseButton::Left {
                    self.pointer.released = true;
                    self.pointer.held = false;
                }
            }
            Event::PixelSizeChanged { width, height } => {
                if let Err(error) = renderer.resize(width, height) {
                    log::warn!("{error}; closing the game");
                    return Ok(false);
                }
                self.resize((width, height));
            }
            _ => {}
        }
        Ok(true)
    }

    /// Takes a press of the left button where the pointer is: the title gives way to the menu,
    /// and on the menu the press goes to the controls.
    fn press(&mut self) {
        if matches!(self.showing, Showing::Title | Showing::Won) {
            self.showing = Showing::Menu;
            self.won = None;
            self.shown = None;
            return;
        }
        self.pointer.pressed = true;
        self.pointer.held = true;
        if let Some(id) = self.ui.hit(self.over_at) {
            self.ui.bring_to_front(id);
        }
        self.effects.push(CLICK);
    }

    /// Moves the pointer to the window point (`x`, `y`), in the screen's layer and in the
    /// windows'.
    fn point(&mut self, x: f32, y: f32, density: f64) {
        self.pointer.at = layer_point((x, y), density, self.scale);
        self.over_at = layer_point((x, y), density, self.over_scale);
    }

    /// The pointer as the windows over the screen see it.
    const fn over_pointer(&self) -> Pointer {
        Pointer {
            at: self.over_at,
            ..self.pointer
        }
    }

    /// The parts of the windows' layer that hold something: the open windows, where the screen
    /// shows them, and the pointer when the game draws it.
    fn over_patches(&self) -> Vec<Patch> {
        let mut patches = Vec::new();
        if self.shows_windows() {
            patches.extend(self.ui.back_to_front().map(|window| Patch {
                x: window.area.x,
                y: window.area.y,
                width: window.area.width,
                height: window.area.height,
            }));
        }
        if self.draws_pointer() {
            patches.extend(pointer_patches(&self.kit.items, &self.hand));
        }
        patches
    }

    /// Whether the screen that shows has the windows over it.
    fn shows_windows(&self) -> bool {
        matches!(self.showing, Showing::Menu | Showing::World)
    }

    /// Whether the game draws the pointer itself now. No pointer shows over the loading
    /// picture, whose palette is none of the pointer's.
    fn draws_pointer(&self) -> bool {
        self.software_pointer && self.showing != Showing::Loading
    }

    /// Draws the screen and the windows over it, presents them, keeps SDL's pointer on the
    /// hand, and gives back what the player asked for.
    ///
    /// # Errors
    ///
    /// The renderer's error when the frame cannot be presented.
    pub(crate) fn show(
        &mut self,
        renderer: &mut Renderer,
        platform: &Platform,
        enabled: &dyn Fn(Choice) -> bool,
    ) -> s2_render::Result<Asked> {
        if let Some(film) = &mut self.intro {
            platform.show_cursor(false);
            if let Some(sound) = film.take_sound() {
                self.film_cues.push(FilmCue::Start(sound));
            }
            if film.show(renderer)? {
                return Ok(Asked::default());
            }
            self.end_intro();
        }
        if self.shown != Some(self.showing) {
            renderer.set_palette(&dac_palette(&self.backdrop().palette));
            self.shown = Some(self.showing);
        }
        let asked = self.draw(enabled);
        let patches = self.over_patches();
        let windows = Overlay {
            frame: &self.over,
            scale: self.over_scale,
            patches: &patches,
        };
        let zoom = Zoom::from_quarters(self.scale.saturating_mul(4)).unwrap_or(Zoom::ONE);
        renderer.render(&self.layer, zoom, Some(windows))?;
        self.follow_pointer(platform);
        Ok(asked)
    }

    /// The picture of the screen that shows.
    const fn backdrop(&self) -> &Backdrop {
        if let (Showing::Loading, Some(loading)) = (self.showing, &self.loading_picture) {
            return &loading.picture;
        }
        if let (Showing::Credits, Some(credits)) = (self.showing, &self.credits) {
            return &credits.picture;
        }
        match (self.showing, &self.won, &self.world) {
            (Showing::Title, _, _) => &self.title,
            (Showing::Won, Some((picture, _, _)), _) => picture,
            (Showing::World, _, Some(world)) => &world.backdrop,
            (
                Showing::Menu
                | Showing::Chapter
                | Showing::ChapterList
                | Showing::Won
                | Showing::World
                | Showing::Loading
                | Showing::Credits,
                _,
                _,
            ) => &self.menu,
            (Showing::Settings, _, _) => &self.settings,
        }
    }

    /// Draws the frame into the layer and gives back what it asked for.
    fn draw(&mut self, enabled: &dyn Fn(Choice) -> bool) -> Asked {
        let corner = self.corner();
        self.layer.clear(0);
        if let Some(credits) = &mut self.credits {
            credits.pass();
        }
        self.lay_backdrop(corner);
        let bounds = Rect::new(0, 0, self.layer.width(), self.layer.height());
        let mut painter = Painter::new(bounds).with_styles(std::sync::Arc::clone(&self.kit.styles));
        let over = Rect::new(0, 0, self.over.width(), self.over.height());
        let mut windows = Painter::new(over).with_styles(std::sync::Arc::clone(&self.kit.styles));
        let mut asked = Asked::default();
        if self.showing == Showing::Menu {
            let covered = self.ui.hit(self.over_at).is_some();
            let mouse = if covered { Pointer::away() } else { self.pointer };
            asked.choice = self.main.run(
                &mut self.ui,
                &mut painter,
                &self.kit.skin,
                self.logo,
                corner,
                enabled,
                mouse,
            );
            self.open_windows(&mut windows, &mut asked);
        } else if let (Showing::Credits, Some(credits)) = (self.showing, &self.credits) {
            screens::credits::draw_text(&mut painter, credits.text(), corner);
        } else if self.showing == Showing::Won {
            match self.won.as_ref().map(|(_, _, line)| *line) {
                Some(WonLine::Chapter(chapter)) => chapters::draw_won(&mut painter, chapter, corner),
                Some(WonLine::Continent { all }) => world_screen::draw_won(&mut painter, all, corner),
                None => {}
            }
        } else if self.showing == Showing::World {
            self.world_screen(&mut painter, corner, &mut asked);
            self.open_windows(&mut windows, &mut asked);
        } else if self.showing == Showing::Loading
            && self
                .loading_picture
                .as_ref()
                .is_some_and(|loading| loading.passed || loading.since.elapsed() >= loading::HOLD)
        {
            asked.ready = self.loading_picture.take().map(|loading| loading.game);
        } else if matches!(self.showing, Showing::Chapter | Showing::ChapterList) {
            self.chapter_screens(&mut painter, corner, &mut asked);
        } else if self.showing == Showing::Settings {
            if self.maps.is_some() {
                self.map_list(&mut painter);
            } else {
                self.settings_screen(&mut painter, corner, &mut asked);
            }
        }
        Self::paint(&self.kit, &self.remaps, &painter, &mut self.layer);
        Self::paint(&self.kit, &self.remaps, &windows, &mut self.over);
        if self.draws_pointer()
            && let Some(sprite) = mouse_pointer::sprite(&self.kit.items, self.hand.picture())
        {
            blit(
                &mut self.over.surface(),
                0,
                sprite,
                self.hand.top_left(),
                Paint::PLAIN,
            );
        }
        self.pointer.pressed = false;
        self.pointer.released = false;
        asked
    }

    /// Draws the windows open over the menu, the frontmost last, and keeps what they ask for.
    fn open_windows(&mut self, painter: &mut Painter, asked: &mut Asked) {
        let open: Vec<WindowId> = self.ui.back_to_front().map(|window| window.id).collect();
        let front = open.last().copied();
        for id in open {
            self.window(id, front == Some(id), painter, asked);
        }
    }

    /// Draws the settings screen with its corner at `corner` and acts on a pick.
    fn settings_screen(&mut self, painter: &mut Painter, corner: Point, asked: &mut Asked) {
        let Some(unlimited) = &mut self.unlimited else {
            self.showing = Showing::Menu;
            return;
        };
        let rows = unlimited.rows();
        let view = unlimited.view(&rows);
        let picked = self.setup.run(
            &mut self.ui,
            painter,
            &self.kit.skin,
            self.setup_items,
            &view,
            corner,
            self.pointer,
        );
        match picked.map(|pick| unlimited.pick(pick)) {
            Some(Leads::Back) => {
                self.unlimited = None;
                self.showing = Showing::Menu;
            }
            Some(Leads::Play) => asked.start_game = true,
            Some(Leads::Maps) => {
                self.maps = Some(Maps::open(&self.start, unlimited.map(), &self.kit.texts));
                self.ui.open(self.map_list.id, MapList::area(corner));
            }
            Some(Leads::Stay) | None => {}
        }
    }

    /// Draws the chapter screen or the chapter list with its corner at `corner` and acts on a
    /// pick.
    fn chapter_screens(&mut self, painter: &mut Painter, corner: Point, asked: &mut Asked) {
        let skin = &self.kit.skin;
        let picked = if self.showing == Showing::Chapter {
            self.chapters
                .run_chapter(&mut self.ui, painter, skin, self.chapter, corner, self.pointer)
        } else {
            let open = self.progress.open();
            self.chapters
                .run_list(&mut self.ui, painter, skin, &open, corner, self.pointer)
        };
        match picked {
            Some(ChapterPick::Choose) => self.showing = Showing::ChapterList,
            Some(ChapterPick::Play) => asked.chapter = Some(self.chapter),
            Some(ChapterPick::Back) => self.showing = Showing::Menu,
            Some(ChapterPick::Chapter(chapter)) => {
                self.chapter = chapter;
                self.showing = Showing::Chapter;
            }
            None => {}
        }
    }

    /// Draws the world screen's markers and buttons with its corner at `corner` and acts on a
    /// pick.
    fn world_screen(&mut self, painter: &mut Painter, corner: Point, asked: &mut Asked) {
        let Some(world) = &self.world else {
            self.showing = Showing::Menu;
            return;
        };
        let won = self.progress.won_continents();
        let mask = |x, y| world.mask(x, y);
        let view = WorldView {
            continents: &self.continents,
            picture: world.size,
            mask: &mask,
            won: &won,
            chosen: self.progress.chosen_continent(),
            markers: self.markers,
        };
        let covered = self.ui.hit(self.over_at).is_some();
        let mouse = if covered { Pointer::away() } else { self.pointer };
        let picked = self
            .world_screen
            .run(&mut self.ui, painter, &self.kit.skin, &view, corner, mouse);
        match picked {
            Some(WorldPick::Continent(number)) => {
                self.progress.choose_continent(number);
                if let Some(path) = Progress::file() {
                    self.progress.write(&path);
                }
            }
            Some(WorldPick::Start) => asked.continent = Some(self.progress.chosen_continent()),
            Some(WorldPick::Return) => {
                self.world = None;
                self.showing = Showing::Menu;
                self.shown = None;
            }
            None => {}
        }
    }

    /// Draws the map list over the settings' picture and acts on a pick.
    fn map_list(&mut self, painter: &mut Painter) {
        let Some(maps) = &mut self.maps else {
            return;
        };
        let line = self.kit.fonts.get(FontId::Medium).line_height();
        let view = maps.view(line);
        let (picked, scrolled) =
            self.map_list
                .run(&mut self.ui, painter, &self.kit.skin, &view, self.pointer);
        maps.scroll(scrolled);
        let Some(pick) = picked else {
            return;
        };
        let (take, close) = maps.pick(pick, &self.start, &self.kit.texts);
        if let (Some(path), Some(unlimited)) = (take, &mut self.unlimited)
            && let Err(error) = unlimited.choose_map(&self.start, &path)
        {
            log::error!("{path}: {error}");
        }
        if close {
            self.maps = None;
            self.ui.close(self.map_list.id);
        }
    }

    /// Copies the screen's picture into the layer with its corner at `corner`.
    fn lay_backdrop(&mut self, corner: Point) {
        if let (Showing::Loading, Some(loading)) = (self.showing, &self.loading_picture) {
            let place = |at: i32| u32::try_from(at).unwrap_or_default();
            self.layer.paste(&loading.frame, place(corner.x), place(corner.y));
            return;
        }
        if let (Showing::Credits, Some(credits)) = (self.showing, &self.credits) {
            let place = |at: i32| u32::try_from(at).unwrap_or_default();
            self.layer.paste(&credits.frame, place(corner.x), place(corner.y));
            credits.draw(&mut self.layer, corner);
            return;
        }
        let picture = match (self.showing, &self.won, &self.world) {
            (Showing::Title, _, _) => &self.title_frame,
            (Showing::Won, Some((_, frame, _)), _) => frame,
            (Showing::World, _, Some(world)) => &world.frame,
            (
                Showing::Menu
                | Showing::Chapter
                | Showing::ChapterList
                | Showing::Won
                | Showing::World
                | Showing::Loading
                | Showing::Credits,
                _,
                _,
            ) => &self.menu_frame,
            (Showing::Settings, _, _) => &self.settings_frame,
        };
        let place = |at: i32| u32::try_from(at).unwrap_or_default();
        self.layer.paste(picture, place(corner.x), place(corner.y));
    }

    /// Draws the window of `id`, `front` when it is the frontmost, in its frame, and keeps what
    /// it asks for.
    fn window(&mut self, id: WindowId, front: bool, painter: &mut Painter, asked: &mut Asked) {
        let Some(area) = self.ui.window(id).map(|window| window.area) else {
            return;
        };
        let title = match id {
            kind::OPTIONS => Label::Text("Options"),
            kind::SOUND_TEST => Label::Text("Sound test"),
            kind::LOAD_GAME => Label::Slot(saves::Mode::Load.title()),
            _ => Label::Text(self.noticed.0),
        };
        let chrome = Chrome::new(id, Some(title));
        chrome.under(painter, &self.kit.skin, (area, front));
        let mouse = if self.ui.hit(self.over_at) == Some(id) {
            self.over_pointer()
        } else {
            Pointer::away()
        };
        let framed = chrome.over(&mut self.ui, painter, &self.kit.skin, area, mouse);
        let inside = match id {
            kind::OPTIONS => self.view.and_then(|view| {
                self.options
                    .run(&mut self.ui, painter, &self.kit.skin, &view, mouse)
            }),
            kind::SOUND_TEST => {
                let sampled = self.sampled;
                let has = move |id: u8| sampled.get(usize::from(id)).copied().unwrap_or(false);
                self.sound_test
                    .run(&mut self.ui, painter, &self.kit.skin, &has, mouse)
            }
            kind::LOAD_GAME => {
                let entries = self.slots.as_ref().map(Slots::entries).unwrap_or_default();
                let listed: Vec<Option<Slot<'_>>> = (0..saves::SLOTS)
                    .map(|slot| {
                        entries
                            .get(slot)
                            .and_then(Option::as_ref)
                            .map(|(number, name)| Slot {
                                number: *number,
                                name,
                            })
                    })
                    .collect();
                self.loading
                    .run(&mut self.ui, painter, &self.kit.skin, &listed, mouse)
            }
            _ => {
                let lines: Vec<Label> = self.noticed.1.iter().map(|line| line.label(painter)).collect();
                self.notice
                    .run(&mut self.ui, painter, &self.kit.skin, &lines, mouse)
            }
        };
        for outcome in framed.into_iter().chain(inside) {
            match outcome {
                Outcome::Options(change) => asked.changes.push(change),
                Outcome::TestEffect(effect) => asked.tests.push(effect),
                Outcome::Play(play) => asked.songs.push(play),
                Outcome::Saves(ask) => {
                    if let (saves::Ask::Load(slot), Some(slots)) = (ask, &self.slots)
                        && slots.entries().get(slot).is_some_and(Option::is_some)
                    {
                        asked.load = Some(slots.path(slot));
                    }
                    if !matches!(ask, saves::Ask::Automatic(_)) {
                        self.ui.close(kind::LOAD_GAME);
                    }
                }
                Outcome::Close(id) => {
                    self.ui.close(id);
                }
                Outcome::Roll(id) => {
                    self.ui.roll(id);
                    self.effects.push(ROLL);
                }
                other => log::debug!("{other:?} has nothing to do before a game"),
            }
        }
    }

    /// Runs `painter`'s operations into `layer` through the kit's art and text.
    fn paint(kit: &Kit, remaps: &(Remap, Remap, Remap), painter: &Painter, layer: &mut Frame) {
        let labels = WithRuns {
            labels: &kit.texts,
            runs: painter.runs(),
        };
        let canvas = Canvas {
            sheet: kit.items.sheet(),
            fonts: &kit.fonts,
            labels: &labels,
            images: painter.images(),
            sheets: s2_ui::paint::Sheets {
                buildings: None,
                landscape: None,
                nation: None,
            },
            lighten: Some(&remaps.0),
            disabled: Some(&remaps.1),
            darken: Some(&remaps.2),
        };
        paint(painter.ops(), &mut layer.surface(), &canvas);
    }

    /// Keeps SDL's cursor on the hand at the windows' scale, or hides it while the hand is
    /// drawn into the layer and while the loading picture shows.
    fn follow_pointer(&mut self, platform: &Platform) {
        if self.software_pointer || self.showing == Showing::Loading {
            platform.show_cursor(false);
            self.cursor = None;
            return;
        }
        let scale = u32::from(self.over_scale);
        let palette = dac_palette(&self.backdrop().palette);
        let Some(made) = pointer_cursor(&self.kit.items, &self.hand, &palette, scale) else {
            return;
        };
        if self
            .cursor
            .as_ref()
            .is_some_and(|(picture, at, _)| (*picture, *at) == (made.picture, scale))
        {
            return;
        }
        match platform.create_cursor(&made.pixels, made.size, made.hotspot) {
            Ok(cursor) => {
                platform.set_cursor(&cursor);
                platform.show_cursor(true);
                self.cursor = Some((made.picture, scale, cursor));
            }
            Err(error) => log::warn!("pointer {:?}: {error}", made.picture),
        }
    }
}

/// `picture` as a frame of palette indices.
pub(crate) fn framed(picture: &Backdrop) -> Frame {
    let mut frame = Frame::new(picture.width, picture.height, 0);
    let width = usize::try_from(picture.width).unwrap_or(1).max(1);
    for (row, line) in picture.pixels.chunks_exact(width).enumerate() {
        for (column, &index) in line.iter().enumerate() {
            let (Ok(x), Ok(y)) = (i32::try_from(column), i32::try_from(row)) else {
                continue;
            };
            frame.set(x, y, index);
        }
    }
    frame
}

/// The pixel of a layer at `scale` under the window point (`x`, `y`).
#[allow(
    clippy::cast_possible_truncation,
    reason = "output pixels over the scale stay far inside i32"
)]
fn layer_point((x, y): (f32, f32), density: f64, scale: u8) -> Point {
    let scale = f64::from(scale.max(1));
    Point::new(
        (f64::from(x) * density / scale).floor() as i32,
        (f64::from(y) * density / scale).floor() as i32,
    )
}

/// The pointer's button for SDL's `button`, for the two the pointer follows.
fn pointer_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Left),
        MouseButton::Right => Some(PointerButton::Right),
        _ => None,
    }
}
