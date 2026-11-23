//! The instrument: any map, scrolled and zoomed in the game's window, or one frame of it
//! rendered to a PNG without a window. Its flags turn into a [`Start`] for the world and the
//! game's [`Options`], so the window it opens is the game's own, and a screenshot is the game's
//! own [`Picture`] of the frame.

use std::path::{Path, PathBuf};

use s2_assets::{Landscape, Nation, Palette};
use s2_render::terrain::Point;
use s2_render::{Frame, Image, Layers, Renderer, Zoom, overlay};
use s2_sim::Player;
use s2_ui::widget::WindowId;

use crate::error::{Error, Result};
use crate::game::camera::Camera;
use crate::game::desktop::Desktop;
use crate::game::picture::{Drawing, Picture};
use crate::game::{self, Options, write_png};
use crate::mods::Unattended;
use crate::scene::{self, AllianceMode, AnimalMode, FogMode, Scene, Start};
use crate::settings::Settings;

/// The window a screenshot is drawn in without `--size`: the original's screen at 640 × 480.
const SCREEN: (u32, u32) = (640, 480);

/// Where a screenshot pretends the press that opens the action menu landed.
const MENU_AT: Point = Point::new(220, 150);

/// Arguments of `robigo view`.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Game data: an install folder, the GOG folder, a .cue/.ins sheet or an .iso.
    data: PathBuf,
    /// The map: a data path such as DATA/MAPS3/OMAP00.WLD, a v1.02 DATA/MAPS/WORLD100.DAT or a
    /// savegame's SAVE/WORLD900.DAT (each with its CONTInnn.DAT beside it), or a file on disk.
    /// Not needed with --load.
    #[arg(required_unless_present_any = ["load", "mission", "continent"])]
    map: Option<String>,
    /// Play this Roman campaign mission, from 0 for the first chapter: its map and script from
    /// the data, its players seated as the campaign seats them.
    #[arg(long, value_name = "N", conflicts_with_all = ["map", "load"])]
    mission: Option<u16>,
    /// Play this World Campaign continent, from 0 for Europe: its map from the Gold Edition's
    /// data, its players seated as the World Campaign seats them.
    #[arg(long, value_name = "N", conflicts_with_all = ["map", "load", "mission"])]
    continent: Option<u8>,
    /// A folder layered over the data, such as the v1.51 patch over a CD image.
    #[arg(long, value_name = "DIR")]
    patch: Option<PathBuf>,
    /// Mods to leave out for this run, by id, over the settings' list: `gold` shows the game as a
    /// player without the Gold Edition sees it.
    #[arg(long, value_name = "MOD,...", value_delimiter = ',')]
    without: Vec<String>,
    /// The landscape to draw with: greenland, wasteland or winter. Overrides the map header;
    /// needed for v1.02 worlds and savegames, which have none (greenland otherwise).
    #[arg(long)]
    landscape: Option<Landscape>,
    /// Zoom, a multiple of 0.25. Default: 1 for screenshots; in a window, the settings' zoom,
    /// or the largest whole zoom at which 640x480 world pixels still fill it.
    #[arg(long)]
    zoom: Option<Zoom>,
    /// Start where this player starts a game: centred on their headquarters as the game
    /// centres it. Maps without one start on the middle of the map.
    #[arg(long, value_name = "PLAYER", default_value_t = 0)]
    start: usize,
    /// Centre on this node instead, given as X,Y, the way the game centres on a headquarters.
    #[arg(long, value_name = "X,Y", value_parser = parse_pair, conflicts_with = "at")]
    node: Option<(i32, i32)>,
    /// Scroll so this world pixel is at the window's top-left corner, given as X,Y.
    #[arg(long, value_name = "X,Y", value_parser = parse_pair)]
    pub(crate) at: Option<(i32, i32)>,
    /// The fog of war of the --start player.
    #[arg(long, value_enum, default_value_t = FogMode::None)]
    fog: FogMode,
    /// Headquarters to place, one per start position from player 0 up. Default: every start
    /// position of the map's player count. Maps without a header place none.
    #[arg(long, value_name = "N")]
    players: Option<usize>,
    /// Each player's nation, in player order (romans, vikings, japanese, africans or their
    /// first three letters); players past the list are Romans.
    #[arg(long, value_name = "NATION,...", value_delimiter = ',')]
    nations: Vec<Nation>,
    /// How the players are allied as the game starts.
    #[arg(long, value_enum, default_value_t = AllianceMode::None)]
    alliances: AllianceMode,
    /// The animals on the map.
    #[arg(long, value_enum, default_value_t = AnimalMode::Start)]
    animals: AnimalMode,
    /// Simulation steps to run before the first frame, 7.5 a second at normal speed. 0 shows
    /// the game as it starts.
    #[arg(long, value_name = "STEPS", default_value_t = 0)]
    steps: u32,
    /// The animation counter for a screenshot, in steps of 8 ticks of 60 Hz since the map
    /// loaded. Default: the steps the game has run, as at normal speed the two advance together.
    #[arg(long, value_name = "STEPS")]
    anim: Option<u32>,
    /// Draw only these layers, comma-separated: triangles, borders, roads, stones, objects,
    /// shadows, buildings, flags, figures, fog; or all or none.
    #[arg(long, value_name = "LAYER,...", default_value = "all")]
    layers: Layers,
    /// Leave out these layers, comma-separated.
    #[arg(long, value_name = "LAYER,...", default_value = "none")]
    hide: Layers,
    /// The window's size in pixels, as `WIDTHxHEIGHT`, over the settings. A screenshot is the
    /// frame the game draws in a window of this size, 640x480 without it.
    #[arg(long, value_name = "WxH", value_parser = parse_size)]
    size: Option<(u32, u32)>,
    /// Render one frame without a window and write it to this PNG, with the map view redrawn
    /// whole as the original draws its first frame.
    #[arg(long, value_name = "PNG")]
    screenshot: Option<PathBuf>,
    /// With --screenshot, also write the figure mask to this PNG: white where an animal's body
    /// or shadow touched a pixel, black elsewhere, at the screenshot's size before zooming.
    #[arg(long, value_name = "PNG", requires = "screenshot")]
    mask: Option<PathBuf>,
    /// Write the screenshot as an 8-bit indexed PNG carrying palette indices, which is what a
    /// raw `DOSBox` capture carries. Only at zoom 1.
    #[arg(long, requires = "screenshot")]
    indexed: bool,
    /// Play this replay into the game: each command runs when the game reaches its step, before
    /// --steps are counted and on through the window. The replay's players, animals, goods and
    /// waypoint sets replace the options given here, and it must name the same map, animation
    /// script and, for a campaign game, mission script. A campaign replay needs --mission.
    #[arg(long, value_name = "REPLAY")]
    replay: Option<PathBuf>,
    /// On closing the window, write a replay of the game to this file: the commands given in
    /// it and any replayed, with a state hash every 250 steps.
    #[arg(long, value_name = "REPLAY", conflicts_with = "screenshot")]
    record: Option<PathBuf>,
    /// Start from this saved game, on the map it names, with its players, animals, goods and
    /// waypoint sets. --steps run on from the step it was saved at.
    #[arg(long, value_name = "SAVE", conflicts_with_all = ["map", "replay"])]
    load: Option<PathBuf>,
    /// The folder F12 in the window saves screenshots to.
    #[arg(long, value_name = "DIR", default_value = ".")]
    shots: PathBuf,
    /// Draw the game's interface over the view. The game's own keys open its windows, except
    /// the ones the viewer uses itself; `m` opens the screen the others open from.
    #[arg(long)]
    ui: bool,
    /// With --ui, the window kinds to open, comma-separated, each decimal or `0x` hex: the
    /// number S2's window factory gives the kind, as `s2_ui::windows::kind` lists them. A
    /// screenshot draws them where they open. 64 opens the window of the building on the node
    /// the view starts on.
    #[arg(long, value_name = "KIND,...", value_delimiter = ',', value_parser = parse_kind, requires = "ui")]
    windows: Vec<u16>,
    /// With --ui, rest the pointer on this frame pixel, given as X,Y, which shows the controls
    /// it is over the way they look under it.
    #[arg(long, value_name = "X,Y", value_parser = parse_pair, requires = "ui")]
    pointer: Option<(i32, i32)>,
    /// With --ui, click this frame pixel before the screenshot, which opens whatever the
    /// control under it opens.
    #[arg(long, value_name = "X,Y", value_parser = parse_pair, requires = "ui")]
    press: Option<(i32, i32)>,
    /// With --ui, how many screen pixels one interface pixel covers: in a window when it opens,
    /// over the settings, and in a screenshot 1 without it. Resizing the window leaves it alone;
    /// Ctrl with + or - steps it and Ctrl+0 goes back to this.
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u8).range(1..=i64::from(overlay::LARGEST_SCALE)), requires = "ui")]
    ui_scale: Option<u8>,
    /// The settings file a window opens from and writes back to, over the one in the user's
    /// preferences folder. A screenshot never reads it.
    #[arg(long, value_name = "FILE")]
    settings: Option<PathBuf>,
    /// Draw the view as a paused game shows it, dimmed under the interface.
    #[arg(long)]
    paused: bool,
    /// Draw options to turn on over --start's own land, as their keys do in a game. Repeat the
    /// flag for more than one.
    #[arg(long, value_enum, value_name = "WHAT")]
    draw: Vec<Draw>,
}

/// A draw option the viewer can turn on from the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum Draw {
    /// The construction aid, which the space bar turns on.
    Aid,
    /// Each building's name, which the `c` key turns on.
    Names,
    /// What each building is doing, which the `s` key turns on.
    States,
}

impl Draw {
    /// The bit this option takes in the draw options word.
    const fn flag(self) -> u16 {
        match self {
            Self::Aid => s2_ui::keys::flag::SPACE,
            Self::Names => s2_ui::keys::flag::C,
            Self::States => s2_ui::keys::flag::S,
        }
    }
}

/// Runs `robigo view`.
///
/// # Errors
///
/// Any [`Error`] from loading the data, map or graphics, or from the GPU or window.
pub fn run(args: &Args) -> Result<()> {
    let mut options = args.options();
    match &args.screenshot {
        Some(path) => {
            let start = game::with_mods(args.start(), &mut options, &mut Unattended);
            screenshot(args, &mut scene::load(&start)?, path)
        }
        None => game::run(&options, &args.start()),
    }
}

impl Args {
    /// The world these flags name.
    pub(crate) fn start(&self) -> Start {
        Start {
            data: Some(self.data.clone()),
            patch: self.patch.clone(),
            map: self.map.clone(),
            load: self.load.clone(),
            replay: self.replay.clone(),
            mission: self.mission,
            continent: self.continent,
            landscape: self.landscape,
            player: self.start,
            node: self.node,
            fog: self.fog,
            players: self.players,
            nations: self.nations.clone(),
            alliances: self.alliances,
            animals: self.animals,
            steps: self.steps,
            layers: self.layers - self.hide,
            interface: self.ui,
            mods: None,
            slots: None,
            goods: s2_sim::StartGoods::Normal,
            starts: s2_sim::StartOrder::InSequence,
            objective: s2_sim::Objective::None,
            bug_fixes: s2_sim::BugFixes::On,
            exploration: true,
        }
    }

    /// How the game's window opens: these flags, then the settings for what they leave out.
    fn options(&self) -> Options {
        let file = self.settings.clone().or_else(Settings::file);
        let settings = file.as_deref().map(Settings::read).unwrap_or_default();
        Options {
            size: self.size.unwrap_or(settings.size),
            fullscreen: settings.fullscreen,
            zoom: self.zoom.or_else(|| settings.zoom()),
            ui_scale: self.ui_scale.unwrap_or(settings.ui_scale),
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
            switched_off: settings
                .switched_off
                .into_iter()
                .chain(self.without.clone())
                .collect(),
            settings: file,
            player: self.start,
            at: self.at,
            shots: self.shots.clone(),
            record: self.record.clone(),
        }
    }
}

/// The frame the game draws in a window of `--size` at `--zoom`, with the interface at
/// `--ui-scale`, after the windows, press and pointer the flags ask for. Gives back the picture
/// and where the view was scrolled to.
///
/// `--paused` puts the interface in the state a paused game leaves it, so a screenshot shows the
/// controls a paused game refuses.
fn screenshot_picture(args: &Args, scene: &mut Scene) -> (Picture, Point) {
    let owner = u8::try_from(args.start).ok().and_then(Player::new);
    let options = args.draw.iter().fold(0, |options, draw| options | draw.flag());
    let output = args.size.unwrap_or(SCREEN);
    let zoom = args.zoom.unwrap_or(Zoom::ONE);
    let mut camera = Camera::new(scene.view.terrain().world_size(), zoom);
    let scroll = scene.start_scroll(args.at, zoom.world_size(output.0, output.1));
    camera.place(scroll.x, scroll.y);
    let anim = args.anim.unwrap_or_else(|| scene.game.clock().step());
    let mut picture = Picture::new(scene, Drawing::Full, args.ui_scale.unwrap_or(1));
    let mut desktop = scene.desktop.take();
    let mut player = None;
    match (&mut desktop, owner) {
        (Some(desktop), Some(owner)) => {
            desktop.set_options(options);
            player = Some(owner);
        }
        (Some(_), None) => log::warn!("player {} is past the last one; drawing no interface", args.start),
        (None, _) => {
            let aid = options & s2_ui::keys::flag::SPACE != 0;
            scene.view.set_aid(aid.then_some(owner).flatten());
        }
    }
    let mut frame = |scene: &mut Scene, desktop: &mut Option<Desktop>| {
        let shown = desktop.as_mut().zip(player);
        picture.draw(scene, None, &camera, output, anim, args.paused, shown)
    };
    frame(scene, &mut desktop);
    if let (Some(desktop), Some(player)) = (&mut desktop, player) {
        let start = scene.start;
        for &kind in &args.windows {
            let id = WindowId(kind);
            if id == s2_ui::windows::kind::ACTION {
                desktop.open_action(&scene.game, player, start, MENU_AT);
            } else if s2_ui::windows::kind::is_building(id) {
                desktop.open_building(&scene.game, player, start, MENU_AT);
            } else {
                desktop.set_node(start);
                desktop.show(id);
            }
        }
        desktop.pause(args.paused);
    }
    if let Some((x, y)) = args.press {
        // A control takes the press on one frame and acts on the release on the next, so a
        // click is two frames whatever the screenshot shows.
        let at = Point::new(x, y);
        if let Some(desktop) = &mut desktop {
            desktop.press(at);
        }
        frame(scene, &mut desktop);
        if let Some(desktop) = &mut desktop {
            desktop.release(at);
        }
        frame(scene, &mut desktop);
    }
    if let Some((x, y)) = args.pointer {
        if let Some(desktop) = &mut desktop {
            desktop.moved(Point::new(x, y));
        }
        // A control reports the line of help it carries as it draws, so the window that shows
        // the line needs the frame after the one the pointer arrived on.
        frame(scene, &mut desktop);
    }
    let asks = frame(scene, &mut desktop);
    if desktop.is_some() {
        log::info!(
            "the interface drew {} window(s) and asked for {} command(s)",
            args.windows.len(),
            asks.commands.len()
        );
    }
    scene.desktop = desktop;
    (picture, scroll)
}

fn screenshot(args: &Args, scene: &mut Scene, path: &Path) -> Result<()> {
    let (picture, scroll) = screenshot_picture(args, scene);
    let zoom = args.zoom.unwrap_or(Zoom::ONE);
    let png_error = |path: &Path| {
        let path = path.to_owned();
        move |source| Error::Png { path, source }
    };
    let (width, height) = if zoom == Zoom::ONE {
        let frame = picture.flattened();
        if args.indexed {
            write_indexed(path, &frame, &scene.palette).map_err(png_error(path))?;
        } else {
            write_png(path, &frame.to_image(&scene.palette)).map_err(png_error(path))?;
        }
        (frame.width(), frame.height())
    } else {
        if args.indexed {
            log::warn!("--indexed needs zoom 1; writing the scaled picture in colour");
        }
        let output = args.size.unwrap_or(SCREEN);
        let image = scaled(&picture, zoom, output, &scene.palette)?;
        write_png(path, &image).map_err(png_error(path))?;
        (image.width(), image.height())
    };
    log::info!(
        "wrote {} ({width}x{height}, scroll {},{} zoom {zoom})",
        path.display(),
        scroll.x,
        scroll.y,
    );
    if let (Some(mask_path), Some(provenance)) = (&args.mask, picture.provenance()) {
        let mask = provenance.mask(Layers::FIGURES);
        let covered = mask.iter().filter(|&&m| m).count();
        write_mask(mask_path, provenance.width(), provenance.height(), &mask)
            .map_err(png_error(mask_path))?;
        log::info!("wrote {} ({covered} figure pixels)", mask_path.display());
    }
    Ok(())
}

/// `picture` on an `output` window at `zoom`, presented on the GPU as the window presents it.
fn scaled(picture: &Picture, zoom: Zoom, output: (u32, u32), palette: &Palette) -> Result<Image> {
    let mut renderer = Renderer::headless()?;
    let info = renderer.adapter_info();
    log::info!("GPU {} via {:?}", info.name, info.backend);
    renderer.set_palette(palette);
    let image = renderer.capture(picture.world(), zoom, output.0, output.1, picture.layer())?;
    Ok(image)
}

/// Writes `frame`'s palette indices as an indexed PNG, with `palette` as its table.
fn write_indexed(path: &Path, frame: &Frame, palette: &Palette) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, frame.width(), frame.height());
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    let table: Vec<u8> = (0..=u8::MAX).flat_map(|i| palette.colour(i)).collect();
    encoder.set_palette(table);
    let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
    writer
        .write_image_data(frame.pixels())
        .map_err(std::io::Error::other)?;
    writer.finish().map_err(std::io::Error::other)
}

fn write_mask(path: &Path, width: u32, height: u32, mask: &[bool]) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
    let bytes: Vec<u8> = mask.iter().map(|&m| if m { u8::MAX } else { 0 }).collect();
    writer.write_image_data(&bytes).map_err(std::io::Error::other)?;
    writer.finish().map_err(std::io::Error::other)
}

fn parse_pair(text: &str) -> std::result::Result<(i32, i32), String> {
    let (x, y) = text
        .split_once(',')
        .ok_or_else(|| format!("expected X,Y, got '{text}'"))?;
    let number = |s: &str| s.trim().parse::<i32>().map_err(|e| format!("'{s}': {e}"));
    Ok((number(x)?, number(y)?))
}

/// A window kind, decimal or `0x` hex.
fn parse_kind(text: &str) -> std::result::Result<u16, String> {
    let text = text.trim();
    let parsed = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u16::from_str_radix(hex, 16),
        None => text.parse(),
    };
    parsed.map_err(|error| format!("'{text}': {error}"))
}

fn parse_size(text: &str) -> std::result::Result<(u32, u32), String> {
    let (w, h) = text
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("expected WIDTHxHEIGHT, got '{text}'"))?;
    let number = |s: &str| match s.trim().parse::<u32>() {
        Ok(0) => Err("sizes must be at least 1".to_owned()),
        Ok(n) => Ok(n),
        Err(e) => Err(format!("'{s}': {e}")),
    };
    Ok((number(w)?, number(h)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_and_sizes_parse() {
        assert_eq!(parse_pair("12,-3"), Ok((12, -3)));
        assert!(parse_pair("12").is_err());
        assert_eq!(parse_size("640x480"), Ok((640, 480)));
        assert_eq!(parse_size("800X600"), Ok((800, 600)));
        assert!(parse_size("0x480").is_err());
        assert!(parse_size("640").is_err());
    }
}
