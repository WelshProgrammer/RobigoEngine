//! The game: its window and loop, the camera, the pace, the build controls and the interface.
//! [`run`] opens the window on the world a [`Start`] names, or on none, and plays until it
//! closes.

pub(crate) mod build;
pub(crate) mod camera;
mod campaign;
mod credits;
pub(crate) mod desktop;
mod front;
mod intro;
mod loading;
pub(crate) mod migration;
mod options;
pub(crate) mod pace;
pub(crate) mod picture;
mod plain;
pub(crate) mod record;
mod setup;
mod slots;
pub(crate) mod sound;
mod unlimited;
mod window;
mod world;

use std::path::{Path, PathBuf};

use s2_render::terrain::{Fog, Point, ScreenNode, world_size};
use s2_render::{Clocks, Frame, Image, Layers, MapView, Zoom, figure_position};
use s2_sim::{Figure, Game, MapObject, Node, NodeMap, World};
use s2_ui::windows::observe::Watch;

use crate::error::Result;
use crate::mods::Prompt;
use crate::scene::{AllianceMode, AnimalMode, FogMode, Start};
use crate::settings::Settings;
use desktop::Desktop;

/// Arguments of `robigo play`: the game's own way in, with the defaults a player gets.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Game data: an install folder, the GOG folder, a .cue/.ins sheet or an .iso. Without it the
    /// game reads the RETAIL folder beside the binary or in the preferences folder.
    data: Option<PathBuf>,
    /// The map to play, as a data path such as DATA/MAPS3/OMAP00.WLD or a file on disk. Without
    /// it and without --load the window opens on no world.
    map: Option<String>,
    /// A folder layered over the data, such as the v1.51 patch over a CD image.
    #[arg(long, value_name = "DIR")]
    patch: Option<PathBuf>,
    /// Start from this saved game.
    #[arg(long, value_name = "SAVE")]
    load: Option<PathBuf>,
    /// The folder F12 saves screenshots to.
    #[arg(long, value_name = "DIR", default_value = ".")]
    shots: PathBuf,
    /// The settings file, over the one in the user's preferences folder.
    #[arg(long, value_name = "FILE")]
    settings: Option<PathBuf>,
}

impl Args {
    /// The world a player starts from `data` with `patch` over it: the map or save given, as the
    /// first player, with the fog of war and the interface on.
    fn start(&self, (data, patch): (Option<PathBuf>, Option<PathBuf>)) -> Start {
        Start {
            data,
            patch,
            map: self.map.clone(),
            load: self.load.clone(),
            replay: None,
            mission: None,
            continent: None,
            landscape: None,
            player: 0,
            node: None,
            fog: FogMode::Game,
            players: None,
            nations: Vec::new(),
            alliances: AllianceMode::None,
            animals: AnimalMode::Start,
            steps: 0,
            layers: Layers::ALL,
            interface: true,
            mods: None,
            slots: None,
            goods: s2_sim::StartGoods::Normal,
            starts: s2_sim::StartOrder::InSequence,
            objective: s2_sim::Objective::None,
            bug_fixes: s2_sim::BugFixes::On,
            exploration: true,
        }
    }

    /// The window a player gets: the size, fullscreen, zoom and interface scale the settings
    /// hold.
    fn options(&self) -> Options {
        let file = self.settings.clone().or_else(Settings::file);
        let settings = file.as_deref().map(Settings::read).unwrap_or_default();
        Options {
            size: settings.size,
            fullscreen: settings.fullscreen,
            zoom: settings.zoom(),
            ui_scale: settings.ui_scale,
            ambient: settings.ambient,
            software_pointer: settings.software_pointer,
            drag_moves_view: settings.drag_moves_view,
            skip_intro: settings.skip_intro,
            loading_pictures: settings.loading_pictures,
            autosave: settings.autosave,
            bug_fixes: settings.bug_fixes,
            music: settings.rendition(),
            sound_font: settings.sound_font.clone(),
            language: settings.language.clone(),
            mods: settings.mods,
            switched_off: settings.switched_off,
            settings: file,
            player: 0,
            at: None,
            shots: self.shots.clone(),
            record: None,
        }
    }
}

/// Runs `robigo play`.
///
/// # Errors
///
/// Any [`crate::error::Error`] from loading the world, the window or the GPU.
pub fn play(args: &Args) -> Result<()> {
    let options = args.options();
    run(&options, &args.start(args.data()))
}

impl Args {
    /// The data to play and the folder over it: the ones given, or else what the `RETAIL`
    /// folder holds, or none, which plays from the baked mods.
    fn data(&self) -> (Option<PathBuf>, Option<PathBuf>) {
        match &self.data {
            Some(data) => (Some(data.clone()), self.patch.clone()),
            None => crate::retail::find().map_or((None, None), |(data, patch)| (Some(data), patch)),
        }
    }
}

/// `start` with the mods `options` names, baking the base mod first when it needs it. A sound
/// font the player declines to fetch gives way to the one Robigo ships, in `options` and in the
/// settings file.
pub(crate) fn with_mods(start: Start, options: &mut Options, prompt: &mut dyn Prompt) -> Start {
    let asked = options.sound_font.clone();
    let mods = crate::mods::prepare(
        start.data.as_deref(),
        start.patch.as_deref(),
        &options.mods,
        &options.switched_off,
        &options.language,
        &mut options.sound_font,
        prompt,
    );
    if options.sound_font != asked
        && let Some(path) = &options.settings
    {
        let mut settings = Settings::read(path);
        settings.sound_font.clone_from(&options.sound_font);
        settings.write(path);
    }
    Start { mods, ..start }
}

/// How the window opens and where it writes what the player saves.
#[derive(Debug, Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is a setting of its own the player flips"
)]
pub(crate) struct Options {
    /// The window's size in window coordinates.
    pub(crate) size: (u32, u32),
    /// Whether the window opens fullscreen.
    pub(crate) fullscreen: bool,
    /// The view's zoom, over the largest whole zoom that fills the window.
    pub(crate) zoom: Option<Zoom>,
    /// How many output pixels one interface pixel covers.
    pub(crate) ui_scale: u8,
    /// Ambient-sound passes a second.
    pub(crate) ambient: u16,
    /// Whether the game draws the mouse pointer itself.
    pub(crate) software_pointer: bool,
    /// Whether a drag moves the view the way the pointer goes, as S2 does.
    pub(crate) drag_moves_view: bool,
    /// Whether the game starts on the title without the intro.
    pub(crate) skip_intro: bool,
    /// Whether a picture shows for a few seconds while a game starts.
    pub(crate) loading_pictures: bool,
    /// Which automatic save period is set, 0 being off.
    pub(crate) autosave: usize,
    /// Whether a new game fixes the engine bugs copied from S2.
    pub(crate) bug_fixes: bool,
    /// The mods to load over the base mod, in order.
    pub(crate) mods: Vec<String>,
    /// The mods left out, by id, though installed.
    pub(crate) switched_off: Vec<String>,
    /// Which rendering of the songs plays.
    pub(crate) music: s2_mods::Rendition,
    /// The sound font setting the General MIDI songs are rendered with.
    pub(crate) sound_font: String,
    /// The language the texts are read in, a folder under a mod's `texts/`.
    pub(crate) language: String,
    /// The settings file the window writes what the player changes back to, if any.
    pub(crate) settings: Option<PathBuf>,
    /// The player the controls build as.
    pub(crate) player: usize,
    /// The world pixel at the window's top-left corner to start on, over the start node.
    pub(crate) at: Option<(i32, i32)>,
    /// The folder screenshots go to.
    pub(crate) shots: PathBuf,
    /// Where to write a replay of the game when the window closes.
    pub(crate) record: Option<PathBuf>,
}

/// Opens the window, prepares the mods `options` names over `start`'s data, and plays the world
/// `start` names, or none when it names no map or save, until the window closes.
///
/// # Errors
///
/// Any [`crate::error::Error`] from loading the first world, the window, the GPU or writing a
/// save or replay.
pub(crate) fn run(options: &Options, start: &Start) -> Result<()> {
    window::run(options, start)
}

/// The world pixel in the middle of a view of `size` scrolled to `scroll`, which `0x2026c`
/// takes as the view's position.
pub(crate) fn middle(scroll: Point, (width, height): (u32, u32)) -> Point {
    let half = |length: u32| i32::try_from(length / 2).unwrap_or(i32::MAX);
    Point::new(scroll.x + half(width), scroll.y + half(height))
}

/// Draws the world around the node an open observation window watches into that window.
///
/// The original doubles a view's picture once for each step of its zoom factor (`0x201bb`), so
/// the patch is rendered at the picture's size over the factor and then expanded.
pub(crate) fn observe(desktop: &mut Desktop, game: &Game, view: &MapView, clocks: Clocks) {
    let Some(asked) = desktop.observing() else {
        return;
    };
    let (width, height) = asked.size;
    let factor = 1 << u32::from(asked.zoom);
    let (across, down) = (width.div_ceil(factor).max(1), height.div_ceil(factor).max(1));
    let mut patch = Frame::new(across, down, 0);
    let map = view.terrain().map();
    let world = game.world();
    let fog = view.terrain().active_fog();
    let on_node = |node: Node| node.start_scroll(map.get(node).altitude, 0, 0);
    let (mut watch, mut middle, mut node) = (asked.watch, asked.middle, asked.node);
    if watch == Watch::Opened {
        watch = if matches!(world.object(node), MapObject::Building(_)) {
            Watch::Still
        } else {
            Watch::Seeking
        };
    }
    if watch == Watch::Seeking {
        let from = middle.unwrap_or_else(|| on_node(node));
        watch = nearest_figure(world, map, fog, from).map_or(Watch::Still, Watch::Following);
    }
    if let Watch::Following(record) = watch {
        match world.figure(record).filter(|figure| shows(fog, figure)) {
            Some(figure) => {
                middle = Some(figure_position(map, figure));
                node = figure.node();
            }
            None => watch = Watch::Still,
        }
    }
    if asked.dragged != Point::new(0, 0) {
        let (width, height) = world_size(map.width(), map.height());
        let wrapped = |at: i32, period: u32| at.rem_euclid(i32::try_from(period.max(1)).unwrap_or(i32::MAX));
        let from = middle.unwrap_or_else(|| on_node(node));
        let to = Point::new(
            wrapped(from.x + asked.dragged.x, width),
            wrapped(from.y + asked.dragged.y, height),
        );
        middle = Some(to);
        node = view.terrain().node_at(to);
    }
    desktop.watched(watch, middle, node);
    let half = |length: u32| i32::try_from(length / 2).unwrap_or(i32::MAX);
    let middle = middle.unwrap_or_else(|| on_node(node));
    let scroll = Point::new(middle.x - half(across), middle.y - half(down));
    view.draw(game.world(), scroll, clocks, &mut patch, None);
    desktop.observed(&magnified(&patch, factor, (width, height)));
}

/// Whether the view draws `figure` under `fog`: with no fog every figure shows.
fn shows(fog: Option<&Fog>, figure: &Figure) -> bool {
    fog.is_none_or(|fog| fog.node_visible(figure.node()))
}

/// The record of the figure nearest the world pixel `from` on `map`, which the follow button
/// picks (`0x48db0`): settlers and animals alike, the distance is measured the short way round
/// the map each way and squared (`0x1ec7c`), and of two as near the lower record wins.
///
/// `S2.EXE` counts every record in use, and a window that picks a figure on land the player has
/// not seen shows nothing but the dark and gives away where that figure walks. Only the figures
/// the view draws under `fog` count here, and the window lets a figure go once it walks out of
/// sight.
fn nearest_figure(world: &World, map: &NodeMap, fog: Option<&Fog>, from: Point) -> Option<u16> {
    let (width, height) = world_size(map.width(), map.height());
    let apart = |a: i32, b: i32, period: u32| {
        let period = i64::from(period.max(1));
        let off = (i64::from(a) - i64::from(b)).rem_euclid(period);
        off.min(period - off)
    };
    world
        .figures()
        .filter(|(_, figure)| shows(fog, figure))
        .map(|(record, figure)| {
            let at = figure_position(map, figure);
            let (dx, dy) = (apart(at.x, from.x, width), apart(at.y, from.y, height));
            (dx * dx + dy * dy, record)
        })
        .min()
        .map(|(_, record)| record)
}

/// `patch` with every pixel repeated `factor` times each way, cut to `width` by `height`.
fn magnified(patch: &Frame, factor: u32, (width, height): (u32, u32)) -> Vec<u8> {
    let (across, down) = (patch.width(), patch.height());
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for row in 0..height {
        let y = (row / factor).min(down.saturating_sub(1));
        let line = &patch.pixels()[(y * across) as usize..][..across as usize];
        for column in 0..width {
            let x = (column / factor).min(across.saturating_sub(1));
            pixels.push(line[x as usize]);
        }
    }
    pixels
}

/// Writes `image` to `path` as an RGBA PNG.
pub(crate) fn write_png(path: &Path, image: &Image) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, image.width(), image.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
    writer
        .write_image_data(image.rgba())
        .map_err(std::io::Error::other)?;
    writer.finish().map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Debug, Parser)]
    struct Command {
        #[command(flatten)]
        play: Args,
    }

    fn parse(line: &[&str]) -> Args {
        Command::parse_from(std::iter::once("play").chain(line.iter().copied())).play
    }

    #[test]
    fn the_follow_button_picks_only_a_figure_the_view_draws() {
        use s2_sim::{NodeData, Species};

        let map = NodeMap::filled(24, 20, NodeData::default()).unwrap();
        let mut world = World::empty(24, 20);
        let near = world.add_figure(Node { x: 5, y: 5 }, Figure::animal(Species::Fox));
        let far = world.add_figure(Node { x: 14, y: 12 }, Figure::animal(Species::Stag));
        let from = Node { x: 5, y: 6 }.start_scroll(0, 0, 0);
        assert_eq!(nearest_figure(&world, &map, None, from), Some(near));
        let mut fog = Fog::new(24, 20);
        assert_eq!(nearest_figure(&world, &map, Some(&fog), from), None);
        fog.reveal(Node { x: 14, y: 12 }, 2);
        assert_eq!(nearest_figure(&world, &map, Some(&fog), from), Some(far));
        assert!(!shows(Some(&fog), world.figure(near).unwrap()));
    }

    #[test]
    fn data_alone_opens_on_no_world() {
        let args = parse(&["/data"]);
        let start = args.start((args.data.clone(), None));
        assert!(start.map.is_none() && start.load.is_none());
        assert_eq!(start.data.as_deref(), Some(std::path::Path::new("/data")));
    }

    #[test]
    fn nothing_given_leaves_the_data_to_the_settings() {
        assert_eq!(parse(&[]).data, None);
    }

    #[test]
    fn a_player_gets_the_fog_and_the_interface() {
        let args = parse(&["/data", "DATA/MAPS3/OMAP00.WLD"]);
        let start = args.start((args.data.clone(), None));
        assert_eq!(start.map.as_deref(), Some("DATA/MAPS3/OMAP00.WLD"));
        assert_eq!(start.fog, FogMode::Game);
        assert!(start.interface);
        assert_eq!(start.layers, Layers::ALL);
    }
}
