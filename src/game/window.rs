//! The game's window. The game runs at the original's pace. Arrow keys or a mouse drag scroll,
//! the wheel or +/- zoom, Ctrl with +/- steps the interface's scale and Ctrl+0 resets it, P pauses,
//! F toggles fast speed and Esc closes the interface's windows; closing the window quits. F11 or
//! Alt+Enter toggles fullscreen, and the digit keys toggle layers (1 triangles, 2 borders, 3 roads,
//! 4 stones, 5 objects, 6 shadows, 7 buildings, 8 flags, 9 figures, 0 fog). Terrain comes from
//! cached tiles
//! with the objects drawn over them every frame. Water and lava cycle, and trees, flags and
//! animals move as in the game.
//!
//! Clicks build, as the player Tab moves through. A left click places a flag, or on one of the
//! player's flags starts a road; Shift makes it a water flag or road, Ctrl removes the flag or road
//! under the cursor, or demolishes the building there, instead. While a road is drawn, S2's icons
//! round its head show each step's slope, the step back and a flag to end on, a click next to the
//! head takes that step, a click further away takes the road there along a route the game plans,
//! and a right click cancels. A click on the head, or one that leaves the head where it was, opens
//! the menu to erect a flag or stop the road; with the interface off a click on the head places the
//! flag. B and Shift+B choose a building type to place with a left click, until a right click. G
//! sends a geologist to the flag under the cursor and S a scout. F12 saves what the window shows as
//! a PNG in the `--shots` folder, and Shift+F12 the same view without the marker under the mouse
//! and the road icons. F1 and F2 open the load and save screens, whose slots live in the
//! preferences folder.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use s2_audio::EFFECT_IDS;
use s2_mods::{GENERALUSER_GS, Offered, Rendition};
use s2_platform::{Cursor, Event, Key, MouseButton, Platform, Scan, Tone, Window};
use s2_render::terrain::ScreenNode;
use s2_render::{Frame, HEARING, Heard, Layers, Placement, Renderer, Zoom};
use s2_sim::Job;
use s2_ui::mouse_pointer::{PointerButton, PointerPicture};
use s2_ui::screens::Choice;
use s2_ui::screens::world::Continents;
use s2_ui::windows::notice::{self, Line};
use s2_ui::windows::{self as ui, OptionsView};

use super::Options;
use super::build::{Builder, Modifiers};
use super::camera::{self, Camera};
use super::campaign::Progress;
use super::desktop::{Asks, Desktop, Music};
use super::front::{Asked, FilmCue, FrontEnd};
use super::options::{self, Fonts};
use super::pace::Pace;
use super::picture::{Drawing, Picture};
use super::plain;
use super::setup;
use super::slots::{self, Slots};
use super::sound::Sound;
use crate::error::Result;
use crate::mods::Prompt;
use crate::scene::{self, Scene, Start, write_replay, write_save};
use crate::settings::{self, SMALLEST_WINDOW, Settings};

/// How far, in output pixels, the mouse may move between press and release for a click.
const CLICK_DRIFT: f64 = 6.0;

/// Arrow-key scroll speed in output pixels per second.
const SCROLL_SPEED: f64 = 900.0;

/// The arrow keys, in the order [`Arrows`] holds them.
const ARROW_KEYS: [Key; 4] = [Key::LEFT, Key::RIGHT, Key::UP, Key::DOWN];

/// Which arrow keys are held: left, right, up, down.
#[derive(Debug, Default, Clone, Copy)]
struct Arrows([bool; 4]);

impl Arrows {
    /// Records `key` going down or up. Returns false if it is not an arrow.
    fn set(&mut self, key: Key, held: bool) -> bool {
        let Some(slot) = ARROW_KEYS.iter().position(|&k| k == key) else {
            return false;
        };
        self.0[slot] = held;
        true
    }

    /// The scroll direction, each axis -1, 0 or 1.
    fn direction(self) -> (f64, f64) {
        let [left, right, up, down] = self.0;
        let axis = |neg: bool, pos: bool| f64::from(i8::from(pos) - i8::from(neg));
        (axis(left, right), axis(up, down))
    }
}

/// The layer each digit key toggles, from 0 to 9.
const DIGIT_LAYERS: [Layers; 10] = [
    Layers::FOG,
    Layers::TRIANGLES,
    Layers::BORDERS,
    Layers::ROADS,
    Layers::STONES,
    Layers::OBJECTS,
    Layers::SHADOWS,
    Layers::BUILDINGS,
    Layers::FLAGS,
    Layers::FIGURES,
];

/// What the window holds: nothing yet, or a world being played. A world's pause belongs to the
/// world, so a paused one is still [`State::Playing`].
enum State {
    /// No world and no screens to show: the window waits, drawing nothing.
    Idle,
    /// The title and the main menu.
    Front(Box<FrontEnd>),
    /// A world being played.
    Playing(Box<Playing>),
}

impl State {
    /// The state `start` asks for: a world when it names a map, a save, a campaign mission or a
    /// continent, and otherwise the title, or nothing when the title cannot be read. The window takes the
    /// world's title.
    fn enter(options: &Options, start: &Start, window: &Window) -> Result<Self> {
        if !names_game(start) {
            window.set_title(TITLE);
            let output = window.size_in_pixels();
            return Ok(
                match FrontEnd::load(start, output, (options.software_pointer, options.ui_scale)) {
                    Ok(front) => Self::Front(Box::new(front)),
                    Err(error) => {
                        log::warn!("no title screen: {error}");
                        Self::Idle
                    }
                },
            );
        }
        if options.loading_pictures {
            let screens = Start {
                map: None,
                load: None,
                mission: None,
                continent: None,
                ..start.clone()
            };
            let output = window.size_in_pixels();
            if let Ok(mut front) =
                FrontEnd::load(&screens, output, (options.software_pointer, options.ui_scale))
                && front.show_loading(start.clone())
            {
                return Ok(Self::Front(Box::new(front)));
            }
        }
        Self::enter_game(options, start, window)
    }

    /// The world `start` names, loaded straight away.
    fn enter_game(options: &Options, start: &Start, window: &Window) -> Result<Self> {
        let start = Start {
            bug_fixes: if options.bug_fixes {
                s2_sim::BugFixes::On
            } else {
                s2_sim::BugFixes::Off
            },
            ..start.clone()
        };
        let scene = scene::load(&start)?;
        window.set_title(&format!("{TITLE}: {}", scene.title));
        let output = window.size_in_pixels();
        Ok(Self::Playing(Box::new(Playing::new(
            options, &start, scene, output,
        ))))
    }
}

/// Whether `start` names a world to play: a map, a save, a campaign mission or a continent.
const fn names_game(start: &Start) -> bool {
    start.map.is_some() || start.load.is_some() || start.mission.is_some() || start.continent.is_some()
}

/// The start-up's side of preparing the mods in the game's window: the system's dialog asks
/// before a sound font is fetched, and a screen of the game's own tells how the copy, the bake,
/// the fetch and the rendering go.
struct Starting<'a> {
    platform: &'a mut Platform,
    window: &'a Window,
    renderer: &'a mut Renderer,
    /// The per-cent and the second line the window last showed.
    shown: Option<(u64, String)>,
    /// Why there are no mods to play from, if the start found none.
    unprepared: Option<String>,
}

impl<'a> Starting<'a> {
    fn new(platform: &'a mut Platform, window: &'a Window, renderer: &'a mut Renderer) -> Self {
        Self {
            platform,
            window,
            renderer,
            shown: None,
            unprepared: None,
        }
    }

    /// Shows `text` in the window, over a bar filled to `per_cent` when there is one and
    /// `detail` under that, and keeps the window answering the desktop while the work goes on.
    fn show(&mut self, text: &str, per_cent: Option<u64>, detail: &str) {
        while let Some(event) = self.platform.poll_event() {
            if let Event::PixelSizeChanged { width, height } = event
                && let Err(error) = self.renderer.resize(width, height)
            {
                log::debug!("{error}");
            }
        }
        self.renderer.set_palette(&plain::palette());
        let output = self.renderer.output_size().unwrap_or(plain::SIZE);
        let placed = Placement::fitted(output, plain::SIZE);
        if let Err(error) = self
            .renderer
            .render_placed(&plain::screen(text, per_cent, detail), placed)
        {
            log::debug!("{error}");
        }
    }

    /// Shows `doing` with `per_cent` after it, over a bar filled that far and `detail` under
    /// it, unless that is what the window shows already.
    fn progress(&mut self, doing: &str, per_cent: u64, detail: &str) {
        if self
            .shown
            .as_ref()
            .is_none_or(|(shown, under)| *shown != per_cent || under != detail)
        {
            self.shown = Some((per_cent, detail.to_owned()));
            self.show(&format!("{doing}: {per_cent}%"), Some(per_cent), detail);
        }
    }
}

impl Prompt for Starting<'_> {
    fn fetch(&mut self, offered: &Offered) -> bool {
        let message = format!(
            "The sound font {} is not installed.\n\nDownload it now? It is {} MB.\n\n\
             Without it the General MIDI songs play with {}.",
            offered.title,
            offered.megabytes(),
            GENERALUSER_GS.title,
        );
        self.window
            .ask(Tone::Information, TITLE, &message, "Download", "Not now")
            .unwrap_or_else(|error| {
                log::warn!("no dialog to ask about {}: {error}", offered.name);
                false
            })
    }

    fn fetching(&mut self, offered: &Offered, got: u64) {
        let per_cent = got * 100 / offered.size.max(1);
        self.progress(
            &format!("Fetching the sound font {}", offered.title),
            per_cent,
            "",
        );
    }

    fn failed(&mut self, offered: &Offered, reason: &str) -> bool {
        self.shown = None;
        let message = format!(
            "The sound font {} could not be downloaded.\n\n{reason}\n\n\
             Without it the General MIDI songs play with {}.",
            offered.title, GENERALUSER_GS.title,
        );
        self.window
            .ask(Tone::Warning, TITLE, &message, "Try again", "Not now")
            .unwrap_or_else(|error| {
                log::warn!("no dialog to say {} failed: {error}", offered.name);
                false
            })
    }

    fn rendering(&mut self) {
        self.shown = None;
        self.show("Rendering the songs", None, "");
    }

    fn baking(&mut self, done: u64, work: u64, last: &str) {
        let detail = if work == 0 {
            "Reading your copy of Settlers 2"
        } else {
            last
        };
        self.progress("Preparing game data", done * 100 / work.max(1), detail);
    }

    fn unprepared(&mut self, reason: &str) {
        self.unprepared = Some(reason.to_owned());
    }
}

/// The effect a save the game cannot make plays: `S2.EXE` refuses one with it when the disk has
/// under 500,000 bytes free (`0x14970`).
const REFUSED: u8 = 59;

/// What every window title starts with.
pub(super) const TITLE: &str = "Robigo";

/// The window's icon. Windows takes the one in the executable, which holds every size.
const ICON: &[u8] = include_bytes!("../../assets/icons/robigo-256.png");

/// Gives `window` the game's icon, for a desktop that finds none installed for it.
fn show_icon(window: &Window) {
    if cfg!(windows) {
        return;
    }
    let mut decoder = png::Decoder::new(std::io::Cursor::new(ICON));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let shown = decoder.read_info().ok().and_then(|mut reader| {
        let (width, height) = (reader.info().width, reader.info().height);
        let mut rgba = vec![0; reader.output_buffer_size()?];
        reader.next_frame(&mut rgba).ok()?;
        window.set_icon(width, height, &rgba).ok()
    });
    if shown.is_none() {
        log::warn!("the window has no icon");
    }
}

/// Opens the window on the world `start` names, or on none, and runs until it closes. F3 leaves
/// the world for the one saved in the [`Options::save`] file, in the same window. A size below
/// [`SMALLEST_WINDOW`] opens at that instead.
pub(crate) fn run(options: &Options, start: &Start) -> Result<()> {
    let mut opened = Options {
        size: settings::held_size(options.size),
        ..options.clone()
    };
    let mut platform = Platform::init(crate::settings::APP)?;
    let (width, height) = opened.size;
    let mut window = platform.create_window(TITLE, width, height)?;
    window.set_minimum_size(SMALLEST_WINDOW.0, SMALLEST_WINDOW.1);
    show_icon(&window);
    if opened.fullscreen {
        toggle_fullscreen(&mut window)?;
    }
    let output = window.size_in_pixels();
    // SAFETY: `renderer` is declared after `window`, so it is dropped first.
    let mut renderer = unsafe { Renderer::new(&window, output.0, output.1)? };
    let info = renderer.adapter_info();
    log::info!(
        "video driver {}, GPU {} via {:?}",
        platform.video_driver(),
        info.name,
        info.backend
    );
    let start = &prepared(
        start,
        &mut opened,
        &window,
        &mut Starting::new(&mut platform, &window, &mut renderer),
    )?;
    let options = &opened;
    let mut current = options.clone();
    let mut sound = Sound::open(
        &platform,
        start,
        options.ambient,
        start.mods.as_ref(),
        options.music,
    );
    let fonts = Fonts::offered(start.mods.as_ref(), &options.sound_font);
    let mut state = State::enter(&current, start, &window)?;
    if !current.skip_intro
        && !names_game(start)
        && let State::Front(front) = &mut state
    {
        front.play_intro(start);
    }
    let mut last = Instant::now();
    'running: loop {
        let density = f64::from(window.pixel_density());
        while let Some(event) = platform.poll_event() {
            let carry_on = match &mut state {
                State::Idle => idle(event, &mut renderer),
                State::Front(front) => front.handle(event, &mut window, &mut renderer, density)?,
                State::Playing(playing) => playing.handle(event, &mut window, &mut renderer, density)?,
            };
            if !carry_on {
                break 'running;
            }
        }
        let now = Instant::now();
        let frame_time = now.duration_since(last);
        last = now;
        let mut turn = Turn {
            platform: &platform,
            window: &mut window,
            renderer: &mut renderer,
            current: &mut current,
            sound: sound.as_mut(),
            fonts,
            start,
            record: options.record.as_deref(),
        };
        if !take_turn(&mut state, &mut turn, (density, frame_time))? {
            break;
        }
        if let Some(sound) = &mut sound {
            sound.pump();
        }
        if let State::Playing(playing) = &mut state
            && let Some(path) = playing.load.take()
        {
            reload_saved(&mut state, &mut current, start, &window, &path);
        }
    }
    keep_view(&state, &mut current);
    write_back(options, &current, &window);
    if let (Some(path), State::Playing(playing)) = (&options.record, &state) {
        let end = playing.scene.game.clock().step();
        write_replay(path, &playing.scene.origin, playing.builder.log(), end)?;
    }
    Ok(())
}

/// What one turn of the loop works on besides the state.
struct Turn<'a> {
    platform: &'a Platform,
    window: &'a mut Window,
    renderer: &'a mut Renderer,
    current: &'a mut Options,
    sound: Option<&'a mut Sound>,
    fonts: Fonts,
    /// What the game started from, which the screens start a world from and go back to.
    start: &'a Start,
    /// Where a replay of a game left for the main menu goes.
    record: Option<&'a std::path::Path>,
}

/// `start` with game data and mods to play from: a start with no data and no base mod of this
/// version's has the player pick their Settlers 2 first, and the base mod is baked when it needs
/// it. A start left with no mods to show the title from tells the player why and fails.
fn prepared(
    start: &Start,
    opened: &mut Options,
    window: &Window,
    starting: &mut Starting<'_>,
) -> Result<Start> {
    let mut start = start.clone();
    if start.data.is_none() && !crate::mods::usable() {
        starting.show("Setting up Robigo", None, "");
        let found = setup::find(window, crate::mods::outdated(), &mut |text, per_cent| {
            starting.progress(text, per_cent, "");
        });
        starting.shown = None;
        let Some((data, patch)) = found else {
            return Err(crate::error::Error::NoData);
        };
        start.data = Some(data);
        start.patch = patch;
    }
    let start = super::with_mods(start, opened, starting);
    if start.mods.is_none() && (start.data.is_none() || !names_game(&start)) {
        let reason = starting
            .unprepared
            .take()
            .unwrap_or_else(|| "the mods could not be read".to_owned());
        setup::tell(window, &unbaked(&reason, crate::retail::kept().as_deref()));
        return Err(crate::error::Error::Unbaked(reason));
    }
    Ok(start)
}

/// What a start that could not bake or read its mods tells the player: `reason`, and where the
/// v1.51 patch goes, the `PATCH` folder of the `RETAIL` folder `kept`, since a copy that is not
/// patched is the likeliest cause.
fn unbaked(reason: &str, kept: Option<&Path>) -> String {
    let patch = kept.map_or_else(
        || "the PATCH folder of the RETAIL folder".to_owned(),
        |kept| kept.join("PATCH").display().to_string(),
    );
    format!(
        "Robigo could not prepare your copy of Settlers 2.\n\n{reason}\n\n\
         A copy older than v1.51 needs the patch: put the patch's files in\n\n    {patch}\n\n\
         and start Robigo again."
    )
}

/// The main menu's buttons that do something: the rest draw greyed until what they open exists.
fn usable(choice: Choice) -> bool {
    matches!(
        choice,
        Choice::Campaign
            | Choice::WorldCampaign
            | Choice::Resume
            | Choice::Load
            | Choice::Unlimited
            | Choice::Options
            | Choice::Intro
            | Choice::Credits
            | Choice::Quit
    )
}

/// Draws one frame of `state` with `density` output pixels to a window unit, `frame_time` after
/// the last, and acts on what it asked for, moving between the screens and a world. Gives false
/// to quit.
fn take_turn(state: &mut State, turn: &mut Turn<'_>, (density, frame_time): (f64, Duration)) -> Result<bool> {
    match state {
        State::Idle => Ok(turn
            .renderer
            .render(&Frame::new(1, 1, 0), Zoom::ONE, None)
            .is_ok()),
        State::Front(front) => {
            let resumable = front.can_resume();
            let enabled = |choice: Choice| usable(choice) && (choice != Choice::Resume || resumable);
            let Ok(mut asked) = front.show(turn.renderer, turn.platform, &enabled) else {
                return Ok(false);
            };
            for cue in front.take_film_cues() {
                if let Some(sound) = turn.sound.as_deref_mut() {
                    match cue {
                        FilmCue::Start(frames) => sound.play_film(frames),
                        FilmCue::Stop => sound.end_film(),
                    }
                }
            }
            let sound = turn.sound.as_deref_mut();
            follow_options(
                front.as_mut(),
                std::mem::take(&mut asked.changes),
                turn.current,
                turn.window,
                (sound, turn.fonts),
            );
            if let Some(sound) = turn.sound.as_deref_mut() {
                for id in front.take_effects() {
                    sound.play(id);
                }
                for effect in &asked.tests {
                    sound.test(effect.id, effect.jitter);
                }
                for &play in &asked.songs {
                    sound.ask(Music::Play(play));
                }
                sound.carry_on();
            }
            if let Some(next) = enter_asked(front, &mut asked, turn) {
                *state = next;
                return Ok(true);
            }
            match asked.choice {
                Some(Choice::Quit) => return Ok(false),
                Some(Choice::Options) => front.toggle_options(),
                Some(Choice::Intro) => front.play_intro(turn.start),
                Some(Choice::Unlimited) => front.open_unlimited(turn.start),
                Some(Choice::Campaign) => front.open_campaign(),
                Some(Choice::WorldCampaign) => front.open_world(),
                Some(Choice::Load) => front.open_load(),
                Some(Choice::Credits) => front.open_credits(turn.current.ambient),
                _ => {}
            }
            Ok(true)
        }
        State::Playing(playing) => {
            playing.advance(frame_time);
            turn.current.autosave = playing.autosave.0;
            playing.record_progress();
            playing.watch_continent();
            let changes = playing.take_changes();
            let sound = turn.sound.as_deref_mut();
            follow_options(
                playing.as_mut(),
                changes,
                turn.current,
                turn.window,
                (sound, turn.fonts),
            );
            if !playing.show(turn.renderer, turn.platform) {
                return Ok(false);
            }
            if let Some((x, y)) = playing.take_warp(density) {
                turn.window.warp_pointer(x, y);
            }
            if let Some(sound) = turn.sound.as_deref_mut() {
                feed(sound, playing, frame_time);
            }
            if playing.leaving {
                leave_for_the_menu(state, turn)?;
            }
            Ok(true)
        }
    }
}

/// Ends the world in `state` for the main menu, keeping the view the player left and writing
/// the replay when one was asked for.
fn leave_for_the_menu(state: &mut State, turn: &mut Turn<'_>) -> Result<()> {
    keep_view(state, turn.current);
    if let (Some(path), State::Playing(playing)) = (turn.record, &*state) {
        let end = playing.scene.game.clock().step();
        write_replay(path, &playing.scene.origin, playing.builder.log(), end)?;
    }
    let (played, won) = match &*state {
        State::Playing(playing) => (
            playing
                .start
                .mission
                .or_else(|| playing.scene.game.mission().map(s2_sim::Mission::number)),
            playing.scene.game.mission().is_some_and(s2_sim::Mission::won),
        ),
        State::Idle | State::Front(_) => (None, false),
    };
    let continent = match &*state {
        State::Playing(playing) => playing.continent.map(|_| playing.continent_end()),
        State::Idle | State::Front(_) => None,
    };
    let menu = Start {
        map: None,
        load: None,
        mission: None,
        continent: None,
        ..turn.start.clone()
    };
    *state = State::enter(turn.current, &menu, turn.window)?;
    if let Some(sound) = turn.sound.as_deref_mut() {
        sound.enter_menu();
    }
    if let State::Front(front) = state {
        front.skip_title();
        if let Some(chapter) = played.and_then(|mission| u8::try_from(mission).ok()) {
            front.set_chapter(chapter);
            if won {
                front.show_won(chapter);
            }
        }
        match continent {
            Some(ContinentEnd::Won) => front.show_continent_won(),
            Some(ContinentEnd::Beaten) => front.show_defeat(),
            Some(ContinentEnd::Left) | None => {}
        }
    }
    Ok(())
}

/// How a World Campaign game ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContinentEnd {
    /// The player won the continent.
    Won,
    /// The player's last building went.
    Beaten,
    /// The player left before either.
    Left,
}

/// A new game of World Campaign continent `continent` over `start`: the continent's map with
/// the players the World Campaign seats.
fn continent_game(start: &Start, continent: u8) -> Start {
    Start {
        map: None,
        load: None,
        replay: None,
        mission: None,
        continent: Some(continent),
        slots: None,
        node: None,
        ..start.clone()
    }
}

/// A new game of campaign chapter `chapter`, from 0, over `start`: the chapter's own map and
/// script, with the players its script seats.
fn chapter_game(start: &Start, chapter: u8) -> Start {
    Start {
        map: None,
        load: None,
        replay: None,
        mission: Some(u16::from(chapter)),
        continent: None,
        slots: None,
        node: None,
        ..start.clone()
    }
}

/// Hands `sound` what the frame asked for: the game menu's switches, the interface's effects,
/// then the world's, which a paused game leaves out.
fn feed(sound: &mut Sound, playing: &mut Playing, frame_time: Duration) {
    let (heard, animations) = playing.listen();
    if let Some((on, volume)) = playing.effect_settings() {
        sound.follow(on, volume);
    }
    for id in playing.take_effects() {
        sound.play(id);
    }
    if let Some(volume) = playing.desktop.as_ref().map(Desktop::music_volume) {
        sound.follow_music(volume);
    }
    for asked in playing.take_music() {
        sound.ask(asked);
    }
    for effect in std::mem::take(&mut playing.tests) {
        sound.test(effect.id, effect.jitter);
    }
    sound.carry_on();
    if let Some(desktop) = playing.desktop.as_mut() {
        desktop.hear_music(sound.tracks(), sound.music_on());
    }
    let paused = playing.pace.paused();
    sound.listen(frame_time, paused, &heard);
    if !paused {
        for id in animations {
            sound.play_world(id);
        }
    }
}

/// Leaves the world in `state` for the one saved at `path`, keeping the view the player left. A
/// save that cannot be loaded leaves the world as it was, with a notice saying why.
fn reload_saved(state: &mut State, current: &mut Options, start: &Start, window: &Window, path: &Path) {
    keep_view(state, current);
    let saved = saved_game(playing_start(state, start), path);
    match State::enter(current, &saved, window) {
        Ok(next) => {
            log::info!("loaded {}", path.display());
            *state = next;
        }
        Err(error) => {
            log::error!("{error}");
            if let State::Playing(playing) = state {
                playing.notify("Load game", notice::wrap(&error.to_string()));
            }
        }
    }
}

/// The world the screens asked for, entered: a game whose loading picture is done goes straight
/// in, any other through its loading picture. A world that cannot be entered leaves the screens
/// showing, with a notice for a saved game or a finished loading picture.
fn enter_asked(front: &mut FrontEnd, asked: &mut Asked, turn: &Turn<'_>) -> Option<State> {
    let (entered, game) = if let Some(game) = asked.ready.take() {
        (State::enter_game(turn.current, &game, turn.window), game)
    } else {
        let game = asked_game(front, asked, turn.start)?;
        (State::enter(turn.current, &game, turn.window), game)
    };
    match entered {
        Ok(next) => Some(next),
        Err(error) => {
            log::error!("{error}");
            if game.load.is_some() || front.is_loading() {
                front.skip_title();
                front.notify("Load game", notice::wrap(&error.to_string()));
            }
            None
        }
    }
}

/// The game the screens asked for over `start`, if any: the settings screen's, a continent, a
/// saved game or a chapter.
fn asked_game(front: &FrontEnd, asked: &Asked, start: &Start) -> Option<Start> {
    if asked.start_game {
        front.unlimited_game(start)
    } else if let Some(continent) = asked.continent {
        Some(continent_game(start, continent))
    } else if let Some(path) = &asked.load {
        Some(saved_game(start, path))
    } else if asked.choice == Some(Choice::Resume) {
        latest_save(start)
    } else {
        asked.chapter.map(|chapter| chapter_game(start, chapter))
    }
}

/// The saved game with the highest number, over `start`, which Resume last game loads
/// (`0x5e76f`); `None` with no saved game.
fn latest_save(start: &Start) -> Option<Start> {
    let slots = Slots::open()?;
    let slot = slots.latest()?;
    Some(saved_game(start, &slots.path(slot)))
}

/// The game saved at `path`, over `start`.
fn saved_game(start: &Start, path: &Path) -> Start {
    Start {
        map: None,
        load: Some(path.to_owned()),
        replay: None,
        mission: None,
        continent: None,
        slots: None,
        node: None,
        steps: 0,
        ..start.clone()
    }
}

/// What the options window changes besides the settings, on whatever shows it: the world being
/// played or the screens before one.
trait Adjust {
    /// The interface scale the options show.
    fn scale(&self) -> u8;
    /// Takes the interface scale `scale`.
    fn set_scale(&mut self, scale: u8);
    /// Draws the pointer into the interface when `software` is set.
    fn set_software_pointer(&mut self, software: bool);
    /// Moves the view the way a drag goes when `moves_view` is set.
    fn set_drag(&mut self, moves_view: bool);
    /// Shows the settings on the options window.
    fn show_options(&mut self, view: OptionsView);
    /// Opens a notice titled `title` showing `lines`.
    fn notify(&mut self, title: &'static str, lines: Vec<Line>);
    /// Opens the sound test, with `sampled` saying which effects have a sample, by id.
    fn open_sound_test(&mut self, sampled: [bool; EFFECT_IDS]);
}

impl Adjust for Playing {
    fn scale(&self) -> u8 {
        self.picture.scale()
    }

    fn set_scale(&mut self, scale: u8) {
        self.picture.set_scale(scale);
    }

    fn set_software_pointer(&mut self, software: bool) {
        if let Some(desktop) = self.desktop.as_mut() {
            desktop.set_software_pointer(software);
        }
        self.cursor = None;
    }

    fn set_drag(&mut self, moves_view: bool) {
        self.drag_moves_view = moves_view;
    }

    fn show_options(&mut self, view: OptionsView) {
        if let Some(desktop) = self.desktop.as_mut() {
            desktop.show_options(view);
        }
    }

    fn notify(&mut self, title: &'static str, lines: Vec<Line>) {
        if let Some(desktop) = self.desktop.as_mut() {
            desktop.notify(title, lines);
        }
    }

    fn open_sound_test(&mut self, sampled: [bool; EFFECT_IDS]) {
        if let Some(desktop) = self.desktop.as_mut() {
            desktop.open_sound_test(sampled);
        }
    }
}

impl Adjust for FrontEnd {
    fn scale(&self) -> u8 {
        self.ui_scale()
    }

    fn set_scale(&mut self, scale: u8) {
        self.set_ui_scale(scale.clamp(1, s2_render::overlay::LARGEST_SCALE));
    }

    fn set_software_pointer(&mut self, software: bool) {
        FrontEnd::set_software_pointer(self, software);
    }

    fn set_drag(&mut self, _moves_view: bool) {}

    fn show_options(&mut self, view: OptionsView) {
        FrontEnd::show_options(self, view);
    }

    fn notify(&mut self, title: &'static str, lines: Vec<Line>) {
        FrontEnd::notify(self, title, lines);
    }

    fn open_sound_test(&mut self, sampled: [bool; EFFECT_IDS]) {
        FrontEnd::open_sound_test(self, sampled);
    }
}

/// Carries out `changes` from the options window on `target`, then shows it the settings as
/// they now are, with the window's size as the player left it.
fn follow_options(
    target: &mut dyn Adjust,
    changes: Vec<ui::Change>,
    current: &mut Options,
    window: &mut Window,
    (mut sound, fonts): (Option<&mut Sound>, Fonts),
) {
    for change in changes {
        change_option(change, current, window, target, sound.as_deref_mut(), fonts);
    }
    if !window.is_fullscreen() {
        current.size = window.size();
    }
    let view = options_view(current, window, target.scale(), fonts);
    target.show_options(view);
}

/// What the options window shows: `current`'s settings, with the window's own state and the
/// interface at `scale`.
fn options_view(current: &Options, window: &Window, scale: u8, fonts: Fonts) -> OptionsView {
    OptionsView {
        fullscreen: window.is_fullscreen(),
        size: current.size,
        sizes: &options::SIZES,
        scale,
        software_pointer: current.software_pointer,
        drag_moves_view: current.drag_moves_view,
        skip_intro: current.skip_intro,
        loading_pictures: current.loading_pictures,
        bug_fixes: current.bug_fixes,
        ambient: current.ambient,
        general_midi: current.music == Rendition::GeneralMidi,
        sound_fonts: fonts.titles,
        sound_font: fonts.place(&current.sound_font),
    }
}

/// Carries out `change` from the options window on the window, `target` and `sound`, and keeps
/// it in `current` for the settings file. A new sound font is only kept, since the songs are
/// rendered with it when the game starts.
fn change_option(
    change: ui::Change,
    current: &mut Options,
    window: &mut Window,
    target: &mut dyn Adjust,
    sound: Option<&mut Sound>,
    fonts: Fonts,
) {
    match change {
        ui::Change::Fullscreen => {
            let fullscreen = !window.is_fullscreen();
            match window.set_fullscreen(fullscreen) {
                Ok(()) if !fullscreen => window.set_size(current.size.0, current.size.1),
                Ok(()) => {}
                Err(error) => log::warn!("fullscreen {fullscreen}: {error}"),
            }
        }
        ui::Change::Size(place) => {
            let Some(&size) = options::SIZES.get(place) else {
                return;
            };
            current.size = size;
            if !window.is_fullscreen() {
                window.set_size(current.size.0, current.size.1);
            }
        }
        ui::Change::Scale(step) => {
            let scale = target.scale().saturating_add_signed(step);
            target.set_scale(scale);
            current.ui_scale = target.scale();
        }
        ui::Change::Pointer(software) => {
            current.software_pointer = software;
            target.set_software_pointer(current.software_pointer);
        }
        ui::Change::Drag(moves_view) => {
            current.drag_moves_view = moves_view;
            target.set_drag(current.drag_moves_view);
        }
        ui::Change::Intro => current.skip_intro = !current.skip_intro,
        ui::Change::LoadingPictures => current.loading_pictures = !current.loading_pictures,
        ui::Change::BugFixes => {
            current.bug_fixes = !current.bug_fixes;
            log::info!(
                "bug fixes {} from the next new game",
                if current.bug_fixes { "on" } else { "off" }
            );
        }
        ui::Change::Ambient(step) => {
            current.ambient = options::stepped_ambient(current.ambient, step);
            if let Some(sound) = sound {
                sound.set_passes(current.ambient);
            }
        }
        ui::Change::Music(general_midi) => {
            current.music = if general_midi {
                Rendition::GeneralMidi
            } else {
                Rendition::Opl3
            };
            if let Some(sound) = sound {
                sound.play_rendition(current.music);
            }
        }
        ui::Change::SoundTest => target.open_sound_test(sampled(sound.as_deref())),
        ui::Change::SoundFont(place) => {
            let Some(setting) = fonts.setting(place) else {
                return;
            };
            if current.sound_font.eq_ignore_ascii_case(setting) {
                return;
            }
            setting.clone_into(&mut current.sound_font);
            log::info!("the sound font {} plays from the next start", current.sound_font);
            if let Some(offered) = crate::mods::needs_fetching(&current.sound_font) {
                target.notify(
                    "Sound font",
                    options::not_installed(&offered)
                        .into_iter()
                        .map(Line::from)
                        .collect(),
                );
            }
        }
    }
}

/// Which effects `sound` has a sample for, by id; none without a sound card.
fn sampled(sound: Option<&Sound>) -> [bool; EFFECT_IDS] {
    let mut sampled = [false; EFFECT_IDS];
    if let Some(sound) = sound {
        for (id, has) in (0u8..).zip(sampled.iter_mut()) {
            *has = sound.sampled(id);
        }
    }
    sampled
}

/// Takes the interface scale and the zoom the player left the world at into `current`, so the
/// next world opens the same way. A zoom the player never touched stays as it was.
fn keep_view(state: &State, current: &mut Options) {
    if let State::Playing(playing) = state {
        current.ui_scale = playing.picture.scale();
        if playing.camera.zoom() != playing.first_zoom {
            current.zoom = Some(playing.camera.zoom());
        }
    }
}

/// Writes what the player changed in the window or on the options window over `opened`, the
/// way it opened, back to the settings file. What the window opened with is left as the file has
/// it, so a flag given for one run is never written.
fn write_back(opened: &Options, current: &Options, window: &Window) {
    let Some(path) = &opened.settings else {
        return;
    };
    let mut settings = Settings::read(path);
    let before = settings.clone();
    let fullscreen = window.is_fullscreen();
    if fullscreen != opened.fullscreen {
        settings.fullscreen = fullscreen;
    }
    let size = if fullscreen { current.size } else { window.size() };
    if size != opened.size {
        settings.size = size;
    }
    if current.ui_scale != opened.ui_scale {
        settings.ui_scale = current.ui_scale;
    }
    if current.zoom != opened.zoom {
        settings.zoom = current.zoom.map(Zoom::quarters);
    }
    if current.ambient != opened.ambient {
        settings.ambient = current.ambient;
    }
    if current.software_pointer != opened.software_pointer {
        settings.software_pointer = current.software_pointer;
    }
    if current.drag_moves_view != opened.drag_moves_view {
        settings.drag_moves_view = current.drag_moves_view;
    }
    if current.skip_intro != opened.skip_intro {
        settings.skip_intro = current.skip_intro;
    }
    if current.loading_pictures != opened.loading_pictures {
        settings.loading_pictures = current.loading_pictures;
    }
    if current.autosave != opened.autosave {
        settings.autosave = current.autosave;
    }
    if current.bug_fixes != opened.bug_fixes {
        settings.bug_fixes = current.bug_fixes;
    }
    if current.music != opened.music {
        settings.set_rendition(current.music);
    }
    if current.sound_font != opened.sound_font {
        settings.sound_font.clone_from(&current.sound_font);
    }
    if settings != before {
        settings.write(path);
    }
}

/// The start the world being played came from, or `first` with no world.
fn playing_start<'a>(state: &'a State, first: &'a Start) -> &'a Start {
    match state {
        State::Playing(playing) => &playing.start,
        State::Idle | State::Front(_) => first,
    }
}

/// Acts on `event` with no world. Gives false to quit.
fn idle(event: Event, renderer: &mut Renderer) -> bool {
    match event {
        Event::Quit => false,
        Event::PixelSizeChanged { width, height } => renderer.resize(width, height).is_ok(),
        _ => true,
    }
}

/// The keys that zoom in, or with Ctrl step the interface's scale up.
const PLUS_KEYS: [Key; 3] = [Key::PLUS, Key::EQUALS, Key::KP_PLUS];

/// The keys that zoom out, or with Ctrl step the interface's scale down.
const MINUS_KEYS: [Key; 2] = [Key::MINUS, Key::KP_MINUS];

/// The modifier keys held down.
#[derive(Debug, Default, Clone, Copy)]
struct Held {
    alt: bool,
    modifiers: Modifiers,
}

impl Held {
    /// Records `key` going down or up. Returns false if it is not Alt, Shift or Ctrl.
    fn set(&mut self, key: Key, down: bool) -> bool {
        let slot = if key == Key::LALT || key == Key::RALT {
            &mut self.alt
        } else if key == Key::LSHIFT || key == Key::RSHIFT {
            &mut self.modifiers.shift
        } else if key == Key::LCTRL || key == Key::RCTRL {
            &mut self.modifiers.ctrl
        } else {
            return false;
        };
        *slot = down;
        true
    }
}

/// Which way a mouse button went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    /// It went down.
    Down,
    /// It came up.
    Up,
}

/// The mouse, in output pixels: where it is, and whether the button held down drags or clicks.
#[derive(Debug, Default, Clone, Copy)]
struct Mouse {
    at: Option<(f64, f64)>,
    held: bool,
    travelled: f64,
}

impl Mouse {
    fn press(&mut self) {
        self.held = true;
        self.travelled = 0.0;
    }

    /// Ends a press. Returns true when it was a click: the mouse moved at most [`CLICK_DRIFT`].
    fn release(&mut self) -> bool {
        let click = self.held && self.travelled <= CLICK_DRIFT;
        self.held = false;
        click
    }

    /// Follows the cursor to `at`, moved by `by`. Returns the drag to scroll by while held.
    fn motion(&mut self, at: (f64, f64), by: (f64, f64)) -> Option<(f64, f64)> {
        self.at = Some(at);
        self.held.then(|| {
            self.travelled += by.0.abs() + by.1.abs();
            by
        })
    }
}

/// A world being played: the scene, how it is shown, and the input that moves it.
struct Playing {
    /// What the world was started from, which a reload keeps its data and players from.
    start: Start,
    scene: Scene,
    desktop: Option<Desktop>,
    picture: Picture,
    camera: Camera,
    builder: Builder,
    arrows: Arrows,
    held: Held,
    mouse: Mouse,
    pace: Pace,
    played: Duration,
    output: (u32, u32),
    shots: std::path::PathBuf,
    /// The saved games the two save screens list.
    slots: Option<Slots>,
    /// A save the load screen asked for, which the window loads after the frame.
    load: Option<PathBuf>,
    /// Which automatic save period is set and the steps run since the last automatic save
    /// (`0x1f791d`, `0x1f7921`).
    autosave: (usize, u32),
    /// Where the pointer is to be moved next, in desktop pixels.
    warp: Option<s2_render::terrain::Point>,
    /// The zoom the world opened at, which tells a zoom the player chose from one they left.
    first_zoom: Zoom,
    /// Effects asked for since the window last took them.
    effects: Vec<u8>,
    /// What the player asked of the music since the window last took it.
    music: Vec<Music>,
    /// The effects the sound test asked to play since the window last took them.
    tests: Vec<ui::TestEffect>,
    /// What the player asked to change on the options window since the window last took it.
    changes: Vec<ui::Change>,
    /// How many messages the player had been posted when the window last looked.
    posts: u32,
    /// The picture SDL shows for the pointer, its hotspot and scale, and the cursor made from it.
    cursor: Option<(PointerPicture, (u32, u32), u32, Cursor)>,
    /// Whether the player asked to leave the game, which ends the run as closing the window does.
    leaving: bool,
    /// Whether a drag moves the view the way the pointer goes, as S2 does, or pulls the map along.
    drag_moves_view: bool,
    /// How many of the chapters a won chapter opened are already in the campaign's progress.
    opened: usize,
    /// The World Campaign continent the game is played on, from the start or the map.
    continent: Option<u8>,
    /// Whether a won continent is already in the campaign's progress.
    continent_recorded: bool,
}

impl Playing {
    fn new(options: &Options, from: &Start, mut scene: Scene, output: (u32, u32)) -> Self {
        let mut desktop = scene.desktop.take();
        if let Some(desktop) = &mut desktop {
            desktop.set_software_pointer(options.software_pointer);
            desktop.decided(scene.game.winner(), true);
        }
        let picture = Picture::new(&scene, Drawing::Cached, options.ui_scale);
        let zoom = options.zoom.unwrap_or_else(|| camera::fitting_zoom(output));
        let mut camera = Camera::new(scene.view.terrain().world_size(), zoom);
        let start = scene.start_scroll(options.at, zoom.world_size(output.0, output.1));
        camera.place(start.x, start.y);
        let builder = Builder::new(&scene.game, options.player, &scene.replay);
        let posts = scene.game.world().messages(builder.player()).posts();
        let slots = Slots::open();
        if let Some(desktop) = &mut desktop {
            desktop.show_saved(slots.as_ref().map(Slots::entries).unwrap_or_default());
            desktop.show_automatic(options.autosave);
        }
        let continent = from.continent.or_else(|| {
            (scene.game.objective() == s2_sim::Objective::Headquarters)
                .then(|| Continents::original().by_map(scene.origin.name()))
                .flatten()
        });
        Self {
            start: from.clone(),
            scene,
            desktop,
            picture,
            camera,
            builder,
            arrows: Arrows::default(),
            held: Held::default(),
            mouse: Mouse::default(),
            pace: Pace::new(),
            played: Duration::ZERO,
            output,
            shots: options.shots.clone(),
            slots,
            load: None,
            autosave: (options.autosave, 0),
            warp: None,
            first_zoom: zoom,
            effects: Vec::new(),
            music: Vec::new(),
            tests: Vec::new(),
            changes: Vec::new(),
            posts,
            cursor: None,
            leaving: false,
            drag_moves_view: options.drag_moves_view,
            opened: 0,
            continent,
            continent_recorded: false,
        }
    }

    /// Records a won continent in the campaign's progress as soon as the game names the player
    /// its winner, as S2 writes `MISSION2.DAT` from the check (`0x1734a`), and leaves for the
    /// world screen once the player's last building has gone.
    fn watch_continent(&mut self) {
        let Some(continent) = self.continent else {
            return;
        };
        if !self.continent_recorded && self.scene.game.winner() == Some(self.builder.player()) {
            self.continent_recorded = true;
            if let Some(path) = Progress::file() {
                let mut progress = Progress::read(Some(&path), &self.start);
                progress.choose_continent(continent);
                if progress.win_continent(continent) {
                    progress.write(&path);
                }
            }
        }
        if !self.leaving && self.continent_end() == ContinentEnd::Beaten {
            log::info!("the player was beaten on continent {continent}");
            self.leaving = true;
        }
    }

    /// How the World Campaign game stands for the player.
    fn continent_end(&self) -> ContinentEnd {
        let player = self.builder.player();
        if self.scene.game.winner() == Some(player) {
            ContinentEnd::Won
        } else if self
            .scene
            .game
            .world()
            .slots()
            .get(player)
            .is_some_and(|slot| slot.controller == s2_sim::Controller::Beaten)
        {
            ContinentEnd::Beaten
        } else {
            ContinentEnd::Left
        }
    }

    /// Adds the chapters a won chapter opened to the campaign's progress as soon as the final
    /// event names them, as S2 writes `MISSION.DAT` from the event check (`0x321ea`).
    fn record_progress(&mut self) {
        let Some(opened) = self.scene.game.mission().map(s2_sim::Mission::opened) else {
            return;
        };
        if opened.len() == self.opened {
            return;
        }
        self.opened = opened.len();
        let Some(path) = Progress::file() else {
            return;
        };
        let mut progress = Progress::read(Some(&path), &self.start);
        if progress.open_chapters(opened) {
            progress.write(&path);
        }
    }

    /// Keeps SDL's cursor on the pointer's picture at the interface's scale, or hides it while
    /// the game draws the pointer itself. Without an interface the system's own cursor stays.
    fn follow_pointer(&mut self, platform: &Platform) {
        let Some(desktop) = &self.desktop else {
            return;
        };
        if desktop.software_pointer() {
            platform.show_cursor(false);
            return;
        }
        let scale = u32::from(self.picture.scale().max(1));
        let Some(made) = desktop.pointer_cursor(&self.scene.palette, scale) else {
            return;
        };
        let same = self.cursor.as_ref().is_some_and(|(picture, hotspot, at, _)| {
            (*picture, *hotspot, *at) == (made.picture, made.hotspot, scale)
        });
        if same {
            return;
        }
        match platform.create_cursor(&made.pixels, made.size, made.hotspot) {
            Ok(cursor) => {
                platform.set_cursor(&cursor);
                platform.show_cursor(true);
                self.cursor = Some((made.picture, made.hotspot, scale, cursor));
            }
            Err(error) => log::warn!("pointer {:?}: {error}", made.picture),
        }
    }

    /// Acts on `event`, with `density` output pixels to a window unit. Returns false to quit.
    fn handle(
        &mut self,
        event: Event,
        window: &mut Window,
        renderer: &mut Renderer,
        density: f64,
    ) -> Result<bool> {
        let scaled = |x: f32, y: f32| (f64::from(x) * density, f64::from(y) * density);
        match event {
            Event::Quit => return Ok(false),
            Event::KeyDown {
                key: Key::ESCAPE,
                repeat: false,
                ..
            } => {
                if let Some(desktop) = self.desktop.as_mut() {
                    desktop.close_all();
                }
            }
            Event::KeyDown {
                key: Key::F11,
                repeat: false,
                ..
            } => toggle_fullscreen(window)?,
            Event::KeyDown {
                key: Key::RETURN,
                repeat: false,
                ..
            } if self.held.alt => toggle_fullscreen(window)?,
            Event::KeyDown { key, .. } if self.held.set(key, true) || self.arrows.set(key, true) => {}
            Event::KeyDown { key, .. } if self.held.modifiers.ctrl && self.scale_key(key) => {}
            Event::KeyDown {
                scan, repeat: false, ..
            } if self.interface_key(scan) => {}
            Event::KeyUp { key, .. } => {
                self.held.set(key, false);
                self.arrows.set(key, false);
            }
            Event::KeyDown {
                key: Key::TAB,
                repeat: false,
                ..
            } => self.builder.next_player(&self.scene.game),
            Event::KeyDown {
                key: Key::F12,
                repeat: false,
                ..
            } => self.screenshot(renderer, !self.held.modifiers.shift),
            Event::KeyDown {
                key: Key::B,
                repeat: false,
                ..
            } => self
                .builder
                .next_building(&self.scene.game, self.held.modifiers.shift),
            Event::KeyDown {
                key: Key::G,
                repeat: false,
                ..
            } => self.builder.send(&mut self.scene.game, Job::GEOLOGIST),
            Event::KeyDown {
                key: Key::S,
                repeat: false,
                ..
            } => self.builder.send(&mut self.scene.game, Job::SCOUT),
            Event::KeyDown {
                key, repeat: false, ..
            } if toggle(key, &mut self.scene, &mut self.pace) => {}
            Event::KeyDown { key, .. } => {
                let centre = (f64::from(self.output.0) / 2.0, f64::from(self.output.1) / 2.0);
                if PLUS_KEYS.contains(&key) {
                    step(&mut self.camera, renderer, self.output, centre, camera::zoom_in);
                } else if MINUS_KEYS.contains(&key) {
                    step(&mut self.camera, renderer, self.output, centre, camera::zoom_out);
                }
            }
            Event::MouseDown { .. } | Event::MouseUp { .. } | Event::MouseMotion { .. } => {
                self.pointer(event, density);
            }
            Event::MouseWheel { amount, x, y } => {
                let next = if amount > 0.0 {
                    camera::zoom_in
                } else {
                    camera::zoom_out
                };
                if amount != 0.0 {
                    step(&mut self.camera, renderer, self.output, scaled(x, y), next);
                }
            }
            Event::PixelSizeChanged { width, height } => return Ok(self.resized(renderer, (width, height))),
            Event::Other => {}
        }
        Ok(true)
    }

    /// Follows the window to its new drawable size. Gives false when the window's surface is
    /// gone, which ends the game the way closing the window does.
    fn resized(&mut self, renderer: &mut Renderer, size: (u32, u32)) -> bool {
        if let Err(error) = renderer.resize(size.0, size.1) {
            log::warn!("{error}; closing the game");
            return false;
        }
        self.camera.resize(self.output, size);
        self.output = size;
        true
    }

    /// Acts on a mouse button or a movement, with `density` output pixels to a window unit. The
    /// interface sees the pointer first, and a press on one of its windows leaves the world
    /// below it alone.
    fn pointer(&mut self, event: Event, density: f64) {
        let scaled = |x: f32, y: f32| (f64::from(x) * density, f64::from(y) * density);
        let (at, button, change) = match event {
            Event::MouseDown { button, x, y } => (scaled(x, y), Some(button), Change::Down),
            Event::MouseUp { button, x, y } => (scaled(x, y), Some(button), Change::Up),
            Event::MouseMotion { x, y, dx, dy } => {
                let at = scaled(x, y);
                let on_desktop = self.desktop_point(at);
                if let Some(desktop) = &mut self.desktop {
                    desktop.moved(on_desktop);
                    desktop.drag_picture(on_desktop, self.drag_moves_view);
                }
                if let Some((dx, dy)) = self.mouse.motion(at, scaled(dx, dy)) {
                    let way = if self.drag_moves_view { 1.0 } else { -1.0 };
                    self.camera.scroll(way * dx, way * dy);
                }
                return;
            }
            _ => return,
        };
        let Some(button @ (MouseButton::Left | MouseButton::Right)) = button else {
            return;
        };
        let on_interface = self.on_interface(at);
        let point = self.desktop_point(at);
        if let Some(desktop) = &mut self.desktop {
            let which = if button == MouseButton::Left {
                PointerButton::Left
            } else {
                PointerButton::Right
            };
            desktop.pointer_button(which, change == Change::Down, point);
            match change {
                Change::Down if which == PointerButton::Right && desktop.grab_picture(point) => {}
                Change::Down => desktop.press(point),
                Change::Up => {
                    if which == PointerButton::Right {
                        desktop.release_picture();
                    }
                    desktop.release(point);
                }
            }
        }
        match change {
            Change::Down if !on_interface => self.mouse.press(),
            Change::Down => {}
            Change::Up => {
                if self.mouse.release() && !on_interface {
                    self.click(button, at);
                }
            }
        }
    }

    /// Acts on a key pressed with Ctrl that changes the interface's scale: + steps it up, - down
    /// and 0 back to the one the viewer opened at. Returns false for any other key.
    fn scale_key(&mut self, key: Key) -> bool {
        let picture = &mut self.picture;
        let scale = if PLUS_KEYS.contains(&key) {
            picture.scale().saturating_add(1)
        } else if MINUS_KEYS.contains(&key) {
            picture.scale().saturating_sub(1)
        } else if key.digit() == Some(0) {
            picture.first_scale()
        } else {
            return false;
        };
        picture.set_scale(scale);
        true
    }

    /// Offers the key at `scan` to the interface. Returns true when a window took it.
    fn interface_key(&mut self, scan: Scan) -> bool {
        let Some(code) = scan.set1() else { return false };
        let Some(desktop) = &mut self.desktop else {
            return false;
        };
        desktop.key(code, self.held.alt)
    }

    /// Whether a window of the interface covers output pixel `at`.
    fn on_interface(&self, at: (f64, f64)) -> bool {
        let point = self.desktop_point(at);
        self.desktop.as_ref().is_some_and(|desktop| desktop.covers(point))
    }

    /// The desktop pixel under output pixel `at`. The interface sees nothing but these, so the
    /// view's zoom never reaches it.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "output pixels over the interface scale stay far inside i32"
    )]
    fn desktop_point(&self, at: (f64, f64)) -> s2_render::terrain::Point {
        let scale = f64::from(self.picture.scale());
        s2_render::terrain::Point::new((at.0 / scale).floor() as i32, (at.1 / scale).floor() as i32)
    }

    /// Saves the view as the window shows it, at its zoom and with the palette as it cycles now,
    /// to the next free `robigo-step<N>.png` name in the shots folder; with `controls`, the marker
    /// under the mouse and the road icons too.
    fn screenshot(&mut self, renderer: &mut Renderer, controls: bool) {
        let step = self.scene.game.clock().step();
        let path = (1..u32::MAX)
            .map(|n| {
                let name = if n == 1 {
                    format!("robigo-step{step}.png")
                } else {
                    format!("robigo-step{step}-{n}.png")
                };
                self.shots.join(name)
            })
            .find(|path| !path.exists())
            .unwrap_or_default();
        let anim = self.pace.anim();
        let frame = self.picture.compose(
            &mut self.scene,
            controls.then(|| (&self.builder, self.desktop.as_ref().and_then(Desktop::menu_node))),
            &self.camera,
            self.output,
            anim,
        );
        let saved = renderer
            .capture(frame, self.camera.zoom(), self.output.0, self.output.1, None)
            .map_err(|error| error.to_string())
            .and_then(|image| super::write_png(&path, &image).map_err(|error| error.to_string()));
        match saved {
            Ok(()) => log::info!("saved {}", path.display()),
            Err(error) => log::warn!("screenshot {}: {error}", path.display()),
        }
    }

    /// A left click opens the node menu on the node under output pixel `at`, or builds there
    /// when the click is part of the road being drawn, the site of a building being placed, or
    /// the interface is off; a right click stops placing buildings or cancels the road being
    /// drawn. While a road is drawn, a click on its head, or one that leaves the head where it
    /// was, opens the menu on the head instead (`0x486f2`). With the fog on and the cheat off, a
    /// click on a node the player has not seen opens nothing unless a road is being drawn
    /// (`0x488dd`).
    fn click(&mut self, button: MouseButton, at: (f64, f64)) {
        let node = self.scene.view.terrain().node_at(self.camera.world_point(at));
        if button != MouseButton::Left {
            self.builder.cancel(&mut self.scene.game);
            return;
        }
        let point = self.desktop_point(at);
        let player = self.builder.player();
        let head = self.builder.head(&self.scene.game);
        let taken = self.builder.takes(&self.scene.game, node) && head != Some(node);
        if let Some(desktop) = &mut self.desktop
            && !taken
        {
            if head.is_none() {
                let hidden = self.scene.view.terrain().fog().is_some()
                    && !desktop.cheating()
                    && !self.scene.game.nodes().visible(node, player);
                if hidden || desktop.open_building(&self.scene.game, player, node, point) {
                    return;
                }
            }
            self.warp = desktop.open_action(&self.scene.game, player, node, point);
            return;
        }
        self.builder
            .click(&mut self.scene.game, node, self.held.modifiers);
        if let Some(desktop) = &mut self.desktop
            && let Some(head) = head
            && self.builder.head(&self.scene.game) == Some(head)
        {
            self.warp = desktop.open_action(&self.scene.game, player, head, point);
        }
    }

    /// Scrolls by the arrows held and runs the game for `frame_time` of wall-clock time.
    fn advance(&mut self, frame_time: Duration) {
        if let Some(desktop) = &mut self.desktop {
            desktop.pause(self.pace.paused());
        }
        let (dx, dy) = self.arrows.direction();
        let seconds = frame_time.as_secs_f64();
        self.camera
            .scroll(dx * SCROLL_SPEED * seconds, dy * SCROLL_SPEED * seconds);
        if !self.pace.paused() {
            self.played += frame_time;
        }
        for _ in 0..self.pace.pass(self.played) {
            self.builder.advance(&mut self.scene.game, &mut self.scene.replay);
            self.count_autosave();
        }
        if let Some(desktop) = &mut self.desktop {
            desktop.decided(self.scene.game.winner(), false);
        }
        let terrain = self.scene.view.terrain();
        let hovered = self
            .mouse
            .at
            .map(|at| terrain.node_at(self.camera.world_point(at)));
        self.builder.hover(hovered);
    }

    /// Saves the game into slot `slot` and names it, as `0x4c290` does, or plays the refusal
    /// when it cannot be written.
    fn save_into(&mut self, slot: usize) {
        let Some(slots) = &mut self.slots else {
            self.effects.push(REFUSED);
            return;
        };
        let path = slots.path(slot);
        let scene = &self.scene;
        let kept = self.desktop.as_ref().map(Desktop::kept);
        let written = slots
            .make_folder()
            .map_err(|source| crate::error::Error::Io {
                path: path.clone(),
                source,
            })
            .and_then(|()| write_save(&path, &scene.origin, self.builder.log(), &scene.game, kept));
        match written {
            Ok(()) => {
                slots.record(slot, slots::name(&scene.title, scene.game.clock().step()));
                log::info!("saved into slot {slot}");
                if let Some(desktop) = &mut self.desktop {
                    desktop.show_saved(slots.entries());
                }
            }
            Err(error) => {
                log::error!("{error}");
                self.effects.push(REFUSED);
            }
        }
    }

    /// Counts a step towards the automatic save and saves into its slot once the period has run
    /// (`0x4c7bb`).
    fn count_autosave(&mut self) {
        let (period, since) = self.autosave;
        let Some(&steps) = slots::PERIODS.get(period).filter(|&&steps| steps > 0) else {
            return;
        };
        let since = since + 1;
        if since >= steps {
            self.autosave = (period, 0);
            self.save_into(slots::AUTOMATIC);
        } else {
            self.autosave = (period, since);
        }
    }

    /// Carries out what the interface's windows asked for: commands go to the game through the
    /// builder, so a replay keeps them, a letter pauses the game or lets it run, and a centring
    /// request moves the view. A replay still playing in keeps the game running through its
    /// letters, since it closes them itself.
    fn answer(&mut self, asks: &Asks) {
        for &command in &asks.commands {
            self.builder.execute(&mut self.scene.game, command);
        }
        if let Some(slot) = asks.save {
            self.save_into(slot);
        }
        if let Some(slot) = asks.load
            && let Some(slots) = &self.slots
            && slots.entries().get(slot).is_some_and(Option::is_some)
        {
            self.load = Some(slots.path(slot));
        }
        if let Some(period) = asks.automatic {
            self.autosave = (period, 0);
        }
        let replaying = !self.scene.replay.finished();
        if !replaying && asks.pause.is_some_and(|paused| paused != self.pace.paused()) {
            self.pace.toggle_pause();
        }
        if let Some(node) = asks.centre {
            let size = self.camera.zoom().world_size(self.output.0, self.output.1);
            let height = self.scene.view.terrain().map().get(node).altitude;
            let at = node.start_scroll(height, size.0, size.1);
            self.camera.place(at.x, at.y);
        }
        if asks.home
            && let Some(node) = self.headquarters()
        {
            let size = self.camera.zoom().world_size(self.output.0, self.output.1);
            let height = self.scene.view.terrain().map().get(node).altitude;
            let at = node.start_scroll(height, size.0, size.1);
            self.camera.place(at.x, at.y);
        }
        if asks.leave {
            log::info!("the player left the game");
            self.leaving = true;
        }
        if asks.pointer.is_some() {
            self.warp = asks.pointer;
        }
        self.effects.extend_from_slice(&asks.effects);
        self.music.extend_from_slice(&asks.music);
        self.tests.extend_from_slice(&asks.tests);
        self.changes.extend_from_slice(&asks.options);
    }

    /// What the player hears: an area the size of the original's map view at the middle of this
    /// window's view, so a larger window or a further zoom never brings in more of the map. Gives
    /// back what that area lets the ambient sounds hear, and the effects the simulation's
    /// animations asked for since the last call whose node lies inside it (`0x18678`).
    fn listen(&mut self) -> (Heard, Vec<u8>) {
        let view = self.camera.zoom().world_size(self.output.0, self.output.1);
        let centre = super::middle(self.camera.position(), view);
        let half = |n: u32| i32::try_from(n / 2).unwrap_or_default();
        let scroll = s2_render::terrain::Point::new(centre.x - half(HEARING.0), centre.y - half(HEARING.1));
        let terrain = self.scene.view.terrain();
        let heard = s2_render::heard(
            terrain,
            self.scene.game.world(),
            scroll,
            HEARING,
            self.pace.anim(),
        );
        let period = terrain.world_size();
        let inside = |at: i32, from: i32, size: u32, period: u32| {
            let period = i32::try_from(period.max(1)).unwrap_or(i32::MAX);
            u32::try_from((at - from).rem_euclid(period)).is_ok_and(|offset| offset < size)
        };
        let animations = self
            .scene
            .game
            .take_sounds()
            .into_iter()
            .filter(|sound| {
                let at = sound.node.world_position(terrain.map().get(sound.node).altitude);
                inside(at.x, scroll.x, HEARING.0, period.0) && inside(at.y, scroll.y, HEARING.1, period.1)
            })
            .filter_map(|sound| u8::try_from(sound.effect).ok())
            .collect::<Vec<_>>();
        let mut effects = animations;
        effects.extend(self.message_sound());
        (heard, effects)
    }

    /// The sound of the newest message posted to the player since the last look, as the view's
    /// update plays it once and clears the holder's flag (`0x44335`). Two posted at once play the
    /// later one's alone.
    fn message_sound(&mut self) -> Option<u8> {
        let messages = self.scene.game.world().messages(self.builder.player());
        let posts = messages.posts();
        if posts == self.posts {
            return None;
        }
        self.posts = posts;
        let newest = messages.list().last()?;
        u8::try_from(self.scene.message_types.get(newest.id).effect).ok()
    }

    /// The effects asked for since the last call, in order.
    fn take_effects(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.effects)
    }

    /// What the player asked of the music since the last call, in order.
    fn take_music(&mut self) -> Vec<Music> {
        std::mem::take(&mut self.music)
    }

    /// What the player asked to change on the options window since the last call, in order.
    fn take_changes(&mut self) -> Vec<ui::Change> {
        std::mem::take(&mut self.changes)
    }

    /// Whether the game menu has effects on and their volume, where there is an interface.
    fn effect_settings(&self) -> Option<(bool, u8)> {
        self.desktop.as_ref().map(Desktop::effect_settings)
    }

    /// Takes the pointer move waiting to be made, in window coordinates for a window with
    /// `density` output pixels to a unit. A desktop pixel covers a square of output pixels at the
    /// interface's scale, and the pointer lands in the middle of it.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a window coordinate inside the output fits an f32 to well under a pixel"
    )]
    fn take_warp(&mut self, density: f64) -> Option<(f32, f32)> {
        let at = self.warp.take()?;
        let scale = f64::from(self.picture.scale());
        let to_window = |pixel: i32| ((f64::from(pixel) + 0.5) * scale / density) as f32;
        Some((to_window(at.x), to_window(at.y)))
    }

    /// Where the player the controls build as has their headquarters.
    fn headquarters(&self) -> Option<s2_sim::Node> {
        let player = self.builder.player();
        self.scene
            .game
            .world()
            .buildings()
            .find(|(_, building)| {
                building.owner == player && building.kind == s2_sim::BuildingKind::HEADQUARTERS
            })
            .map(|(_, building)| building.node)
    }

    /// Presents a frame and brings the pointer into step with it, zooming in when the GPU
    /// cannot hold the frame this zoom needs. Returns false when even the closest zoom is too
    /// large.
    fn show(&mut self, renderer: &mut Renderer, platform: &Platform) -> bool {
        let player = self.builder.player();
        let drawing = self.scene.game.road_draft(player).is_some();
        if let Some(desktop) = &mut self.desktop {
            desktop.set_drawing(drawing);
        }
        let shown = self.picture.show(
            renderer,
            &mut self.scene,
            &self.builder,
            &self.camera,
            self.output,
            (self.played, self.pace.anim()),
            self.pace.paused(),
            self.desktop.as_mut().map(|desktop| (desktop, player)),
        );
        match &shown {
            Ok(asks) => self.answer(asks),
            Err(error @ s2_render::Error::SurfaceLost(_)) => {
                log::warn!("{error}; closing the game");
                return false;
            }
            Err(error) => log::debug!("frame not shown: {error}"),
        }
        if let Err(s2_render::Error::ViewTooLarge { .. }) = shown {
            let centre = (f64::from(self.output.0) / 2.0, f64::from(self.output.1) / 2.0);
            match camera::zoom_in(self.camera.zoom()) {
                Some(closer) => self.camera.zoom_to(closer, centre),
                None => return false,
            }
        }
        self.follow_pointer(platform);
        true
    }
}

/// Acts on a key that toggles something: P pauses, F switches speed, a digit flips a layer.
/// Returns false for any other key.
fn toggle(key: Key, scene: &mut Scene, pace: &mut Pace) -> bool {
    if key == Key::P {
        pace.toggle_pause();
        log::info!("{}", if pace.paused() { "paused" } else { "running" });
    } else if key == Key::F {
        pace.toggle_speed();
        log::info!("speed {}", pace.speed());
    } else if let Some(digit) = key.digit() {
        let layers = scene.view.layers() ^ DIGIT_LAYERS[usize::from(digit)];
        scene.view.set_layers(layers);
        log::info!("layers {layers}");
    } else {
        return false;
    }
    true
}

/// Moves one zoom step with `next`, unless the GPU cannot hold the frame that step needs.
fn step(
    camera: &mut Camera,
    renderer: &Renderer,
    output: (u32, u32),
    anchor: (f64, f64),
    next: fn(Zoom) -> Option<Zoom>,
) {
    let Some(zoom) = next(camera.zoom()) else { return };
    let (w, h) = zoom.world_size(output.0, output.1);
    if w.max(h) <= renderer.max_world_size() {
        camera.zoom_to(zoom, anchor);
        log::debug!("zoom {zoom}");
    }
}

fn toggle_fullscreen(window: &mut Window) -> std::result::Result<(), s2_platform::Error> {
    window.set_fullscreen(!window.is_fullscreen())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_combine_into_a_direction() {
        let mut arrows = Arrows::default();
        assert!(arrows.set(Key::LEFT, true));
        assert!(arrows.set(Key::DOWN, true));
        assert!(!arrows.set(Key::ESCAPE, true));
        assert_eq!(arrows.direction(), (-1.0, 1.0));
        arrows.set(Key::RIGHT, true);
        assert_eq!(arrows.direction(), (0.0, 1.0));
        arrows.set(Key::DOWN, false);
        assert_eq!(arrows.direction(), (0.0, 0.0));
    }

    #[test]
    fn a_press_that_barely_moves_is_a_click() {
        let mut mouse = Mouse::default();
        assert_eq!(mouse.motion((10.0, 10.0), (1.0, 0.0)), None, "nothing held");
        mouse.press();
        assert_eq!(mouse.motion((12.0, 11.0), (2.0, 1.0)), Some((2.0, 1.0)));
        assert!(mouse.release());
        mouse.press();
        mouse.motion((30.0, 11.0), (18.0, 0.0));
        assert!(!mouse.release(), "a drag");
        assert!(!mouse.release(), "nothing held");
        assert_eq!(mouse.at, Some((30.0, 11.0)));
    }

    #[test]
    fn modifiers_follow_their_keys() {
        let mut held = Held::default();
        assert!(held.set(Key::RSHIFT, true));
        assert!(held.set(Key::LCTRL, true));
        assert!(!held.set(Key::F, true));
        assert_eq!(
            held.modifiers,
            Modifiers {
                shift: true,
                ctrl: true
            }
        );
        held.set(Key::LCTRL, false);
        assert!(!held.modifiers.ctrl);
    }
}
