//! A world to play or look at: the game, how it started and how it is drawn, loaded from a map,
//! a save or a replay. The game and the instrument both start one through [`Start`].

pub(crate) mod source;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use s2_assets::{
    CARRIER_BOB, JOBS_BOB, Landscape, LandscapeAssets, Nation, Palette, ROM_ANIM, ROM_BOBS, Remap,
    SHIP_SPRITES, Settlers, SpriteSheet,
};
use s2_data::{DataSet, Layer};
use s2_formats::bob::RomAnim;
use s2_formats::map::Map;
use s2_formats::rtx::Script;
use s2_mods::{Sheets, is_gold};
use s2_render::terrain::{Fog, Point, ScreenNode, Terrain};
use s2_render::{Layers, MapView, ObjectGraphics};
use s2_sim::replay::{MapSource, Replay, content_hash};
use s2_sim::save::{Restored, Save};
use s2_sim::{
    Alliances, AnimationScript, BeyondRepair, Game, MessageTypes, Node, Objective, Player, PlayerSlots,
    Portraits, Repaired, Setup, SimTables, StartGoods, StartOrder,
};
use s2_ui::screens::world::Continents;

use crate::error::{Error, Result};
use crate::game::desktop::Desktop;
use crate::game::record::Queue;

/// What the fog shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum FogMode {
    /// Everything visible.
    #[default]
    None,
    /// What the player has seen in the game, from the hexagon round their headquarters on.
    Game,
}

/// Which animals are on the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum AnimalMode {
    /// No animals.
    None,
    /// The map's animal list, as the first frame of a game shows it.
    #[default]
    Start,
}

/// How the players are allied as the game starts, the setting Unlimited Play's setup screen
/// cycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum AllianceMode {
    /// Nobody allied, which is where the setting starts.
    #[default]
    None,
    /// People with people and the computer's players with each other.
    Controllers,
    /// The players of each nation with each other.
    Nations,
    /// Every player but the first with every other, the World Campaign's.
    #[value(skip)]
    AgainstFirst,
}

impl From<AllianceMode> for Alliances {
    fn from(mode: AllianceMode) -> Self {
        match mode {
            AllianceMode::None => Self::None,
            AllianceMode::Controllers => Self::Controllers,
            AllianceMode::Nations => Self::Nations,
            AllianceMode::AgainstFirst => Self::AgainstFirst,
        }
    }
}

/// Which world to start and how to draw it.
#[derive(Debug, Clone)]
pub(crate) struct Start {
    /// Game data: an install folder, the GOG folder, a CD sheet or an image. Without it the game
    /// plays from the baked mods alone.
    pub(crate) data: Option<PathBuf>,
    /// A folder layered over the data.
    pub(crate) patch: Option<PathBuf>,
    /// The map, as a data path or a file on disk. Not needed with a save.
    pub(crate) map: Option<String>,
    /// A saved game to start from.
    pub(crate) load: Option<PathBuf>,
    /// A replay to play into the game.
    pub(crate) replay: Option<PathBuf>,
    /// The landscape to draw with, over the map header's.
    pub(crate) landscape: Option<Landscape>,
    /// The player the view starts on and plays as.
    pub(crate) player: usize,
    /// The node to centre on instead of that player's headquarters.
    pub(crate) node: Option<(i32, i32)>,
    /// What the fog shows.
    pub(crate) fog: FogMode,
    /// Whether the player sees only the land they have explored, the setting Unlimited Play's
    /// settings screen turns off to show the whole map.
    pub(crate) exploration: bool,
    /// How many headquarters to place, one per start position.
    pub(crate) players: Option<usize>,
    /// Each player's nation, in player order.
    pub(crate) nations: Vec<Nation>,
    /// How the players are allied as the game starts.
    pub(crate) alliances: AllianceMode,
    /// Who plays each player and with which face, as the settings screen left them; `None` for
    /// what the screen offers for the map.
    pub(crate) slots: Option<PlayerSlots>,
    /// What each headquarters starts with.
    pub(crate) goods: StartGoods,
    /// How the players are given the start positions.
    pub(crate) starts: StartOrder,
    /// What wins the game.
    pub(crate) objective: Objective,
    /// Whether a new game fixes the engine bugs copied from S2. A save or a replay keeps the
    /// setting it was played with.
    pub(crate) bug_fixes: s2_sim::BugFixes,
    /// The campaign mission a new game plays, which names its map and script. A save or a
    /// replay keeps the mission it was played with.
    pub(crate) mission: Option<u16>,
    /// The World Campaign continent a new game plays, which names its map and seats its players.
    /// A save or a replay keeps the set-up it was played with.
    pub(crate) continent: Option<u8>,
    /// The animals on the map.
    pub(crate) animals: AnimalMode,
    /// Simulation steps to run before the first frame.
    pub(crate) steps: u32,
    /// The layers the view draws.
    pub(crate) layers: Layers,
    /// Whether to load the interface.
    pub(crate) interface: bool,
    /// The base mod and the player's mods, which the sheets and sounds come from when present.
    pub(crate) mods: Option<s2_mods::ModStack>,
}

impl Start {
    /// This start with the World Campaign's rules when it names a continent (`0x5efcd`): the
    /// computers allied with each other, the headquarters objective, the default start goods and
    /// exploration on.
    fn with_continent_rules(&self) -> Cow<'_, Self> {
        if self.continent.is_none() {
            return Cow::Borrowed(self);
        }
        Cow::Owned(Self {
            alliances: AllianceMode::AgainstFirst,
            objective: Objective::Headquarters,
            goods: StartGoods::Normal,
            exploration: true,
            ..self.clone()
        })
    }
}

/// A world: the running game and how it is drawn.
pub(crate) struct Scene {
    pub(crate) title: String,
    pub(crate) palette: Palette,
    pub(crate) view: MapView,
    pub(crate) game: Game,
    /// The node the view starts centred on.
    pub(crate) start: Node,
    /// The replay's commands being played in.
    pub(crate) replay: Queue,
    /// How the game started, for writing a replay of it.
    pub(crate) origin: Origin,
    /// The interface, with `--ui`.
    pub(crate) desktop: Option<Desktop>,
    /// The table a paused view draws through.
    pub(crate) dim: Remap,
    /// The list each kind of message goes on and the effect it plays.
    pub(crate) message_types: MessageTypes,
}

/// How a game started: enough to start it again and to write a replay of it.
pub(crate) struct Origin {
    name: String,
    map: Map,
    /// The bytes of a map file from disk, which a save keeps.
    file: Option<Vec<u8>>,
    script: Option<AnimationScript>,
    /// The campaign mission's script, for a campaign game.
    mission: Option<Script>,
    tables: SimTables,
    /// Whether the save being loaded was made with other tables.
    tables_changed: bool,
    replay: Replay,
}

impl Origin {
    /// The map as the command line or the save named it.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// The game as it started.
    fn start(&self) -> Result<Game> {
        let unscripted = Script::default();
        let mission = self.mission.as_ref().unwrap_or(&unscripted);
        let mut game = Game::start_mission(&self.map, &self.replay.setup, mission, &self.tables)
            .map_err(|e| Error::Terrain(self.name.clone(), e))?;
        if let Some(script) = &self.script {
            game.set_animations(script.clone());
        }
        self.replay.apply_players(&mut game);
        Ok(game)
    }
}

impl Start {
    /// The data this start names with its patch over it, or an empty set when it names none,
    /// which leaves every file to the mods.
    ///
    /// # Errors
    ///
    /// [`s2_data::Error`] for data or a patch that cannot be opened.
    pub(crate) fn data_set(&self) -> std::result::Result<DataSet, s2_data::Error> {
        let mut data = match &self.data {
            Some(path) => DataSet::discover(path)?,
            None => DataSet::new([]),
        };
        if let Some(patch) = &self.patch {
            data.push_layer(Layer::folder(patch)?);
        }
        Ok(data)
    }
}

impl Scene {
    /// The scroll position a `size` view starts at: `at` when given, otherwise centred on the
    /// start node.
    pub(crate) fn start_scroll(&self, at: Option<(i32, i32)>, size: (u32, u32)) -> Point {
        if let Some((x, y)) = at {
            return Point::new(x, y);
        }
        let height = self.view.terrain().map().get(self.start).altitude;
        self.start.start_scroll(height, size.0, size.1)
    }
}

/// Loads the world `how` names.
///
/// # Errors
///
/// Any [`Error`] from loading the data, the map, a save or replay, or the graphics.
pub(crate) fn load(how: &Start) -> Result<Scene> {
    let how = &*how.with_continent_rules();
    let data = how.data_set()?;
    let sheets = Sheets {
        stack: how.mods.as_ref(),
        data: &data,
    };
    let save = how.load.as_deref().map(read_save).transpose()?;
    check_mods(how, save.as_ref())?;
    let name = match (&save, &how.map) {
        (Some(save), _) => match &save.replay.map {
            MapSource::Data(path) | MapSource::File(path) => path.clone(),
        },
        (None, Some(map)) => map.clone(),
        (None, None) => match (how.mission, how.continent) {
            (Some(mission), _) => s2_sim::map_path(mission),
            (None, Some(continent)) => Continents::original().map_path(continent).unwrap_or_default(),
            (None, None) => String::new(),
        },
    };
    let loaded = match &save {
        Some(save) => saved_map(how, &sheets, save)?,
        None => source::load(&sheets, &name)?,
    };
    let landscape = landscape(how, &name, &loaded)?;
    let map = &loaded.map;
    log::info!(
        "{}: {}x{} nodes, {landscape}",
        loaded.title,
        map.width(),
        map.height()
    );
    let assets = sheets.landscape(landscape)?;
    let (rules, script) = rules(&sheets)?;
    let settlers = Arc::new(settlers(&sheets, &script)?);
    let origin = origin(how, (rules, &sheets), name, &loaded, &settlers, save.as_ref())?;
    let (game, replay, repaired) = start_game(how, &origin, save.as_ref())?;
    let nations = nations(how, &origin.replay);
    let mut terrain =
        Terrain::with_assets(game.nodes().clone(), &assets).with_tables(Arc::new(sheets.terrain_tables()?));
    let hq = headquarters(map, how.player);
    let start = match how.node {
        Some((x, y)) => Node::wrapped(x, y, map.width(), map.height()),
        None => hq.unwrap_or_else(|| {
            log::warn!("player {} has no headquarters; starting mid-map", how.player);
            Node {
                x: map.width() / 2,
                y: map.height() / 2,
            }
        }),
    };
    if how.fog == FogMode::Game && origin.replay.exploration {
        terrain.set_fog(game_fog(&game, how.player));
    }
    let graphics = graphics(&sheets, &assets, landscape, &nations)?.with_settlers(Some(settlers));
    let mut view = MapView::new(terrain, graphics);
    view.set_layers(how.layers);
    log::info!(
        "{} headquarters, {} animals after {} steps; layers {}",
        game.world().buildings().count(),
        game.world().figures().count(),
        game.clock().step(),
        view.layers()
    );
    for (_, building) in game.world().buildings() {
        log::info!(
            "player {} headquarters at {},{}",
            building.owner.index(),
            building.node.x,
            building.node.y
        );
    }
    let nation = nations.get(how.player).copied().unwrap_or_default();
    let mut desktop = how
        .interface
        .then(|| desktop(&sheets, (nation, landscape), &assets, &game, save.as_ref()))
        .transpose()?;
    if let Some(desktop) = desktop.as_mut() {
        desktop.show_repairs(&repaired, how.player);
    }
    Ok(Scene {
        title: loaded.title,
        palette: assets.palette().clone(),
        view,
        game,
        start,
        replay,
        origin,
        desktop,
        dim: assets.paused().clone(),
        message_types: sheets.message_types()?,
    })
}

/// The desktop for `game` as `nation` on `landscape`, with the mission's texts and, from a save,
/// what the desktop kept beside the game.
fn desktop(
    sheets: &Sheets<'_>,
    (nation, landscape): (Nation, Landscape),
    assets: &LandscapeAssets,
    game: &Game,
    save: Option<&Save>,
) -> Result<Desktop> {
    let nodes = (game.nodes().width(), game.nodes().height());
    let tables = (
        assets.highlight().clone(),
        assets.shadow().clone(),
        assets.disabled().clone(),
    );
    let mut desktop = Desktop::load(sheets, (nation, landscape), tables, nodes)?
        .with_mission_texts(sheets, game.mission().and_then(s2_sim::Mission::texts));
    if let Some(kept) = save.and_then(|save| save.desktop.as_deref()) {
        desktop.restore(kept);
    }
    Ok(desktop)
}

/// What settlers are drawn from: the BOB files, the Roman animation bitmaps and the animation
/// script `script`, which the sim reads as well.
fn settlers(sheets: &Sheets<'_>, script: &[u8]) -> Result<Settlers> {
    Ok(Settlers {
        carrier: sheets.bob(CARRIER_BOB)?,
        jobs: sheets.bob(JOBS_BOB)?,
        roman: sheets.sheet(ROM_BOBS)?,
        script: RomAnim::parse(script).map_err(|source| s2_assets::Error::Bob {
            path: ROM_ANIM,
            source,
        })?,
    })
}

/// What a game is played by: the sim's tables and the animation script, and their hashes.
struct Rules {
    tables: SimTables,
    tables_hash: u64,
    script_hash: u64,
}

/// The rules the mods give, and the animation script's bytes.
fn rules(sheets: &Sheets<'_>) -> Result<(Rules, Vec<u8>)> {
    let tables = sheets.sim_tables()?;
    let script = sheets.animation_script()?;
    let rules = Rules {
        tables_hash: s2_mods::tables_hash(&tables)?,
        tables,
        script_hash: content_hash(&script),
    };
    Ok((rules, script))
}

/// The landscape `--landscape` names, otherwise the one the map's header names, otherwise
/// greenland for a map with no header.
fn landscape(how: &Start, name: &str, loaded: &source::LoadedMap) -> Result<Landscape> {
    match (how.landscape, loaded.landscape) {
        (Some(chosen), _) => Ok(chosen),
        (None, Ok(Some(from_header))) => Ok(from_header),
        (None, Ok(None)) => {
            log::warn!("{name} has no header; drawing it as greenland (see --landscape)");
            Ok(Landscape::Greenland)
        }
        (None, Err(byte)) => Err(Error::UnknownLandscape(byte)),
    }
}

/// The game as `origin` starts it, or as `save` left it, run on by `--steps`, and the commands
/// still to play into it.
fn start_game(how: &Start, origin: &Origin, save: Option<&Save>) -> Result<(Game, Queue, Vec<Repaired>)> {
    let mut game = origin.start()?;
    let mut repaired = Vec::new();
    if let Some(save) = save
        && origin.tables_changed
    {
        let path = how.load.clone().unwrap_or_default();
        game = save.state(&game).ok_or(Error::SaveRulesChanged(path.clone()))?;
        log::warn!(
            "{}: the mods' rules have changed since this game was saved, which may damage it; check \
             it carefully before overwriting the save",
            path.display()
        );
        let saved = save
            .building_types()
            .unwrap_or_else(|| game.world().building_types().clone());
        if let Some(why) = game.beyond_repair(&saved) {
            let (what, kind) = match why {
                BeyondRepair::Size(kind) => ("size", kind),
                BeyondRepair::Strength(kind) => ("strength", kind),
            };
            return Err(Error::SaveBeyondRepair {
                path,
                what,
                building: kind.name().unwrap_or("building"),
            });
        }
        repaired = game.repair(&saved);
        for repaired in &repaired {
            log::warn!("{}: repaired {repaired:?}", path.display());
        }
    } else if let Some(save) = save {
        let how;
        (game, how) = save.restore(game);
        match how {
            Restored::State => log::info!("restored the saved state at step {}", save.replay.end),
            Restored::Replayed => log::warn!(
                "the save is from another version; replayed {} commands to step {}",
                save.replay.commands.len(),
                save.replay.end
            ),
        }
    }
    let commands = origin.replay.commands.clone();
    let mut replay = if save.is_some() {
        Queue::resumed(commands, &game)
    } else {
        Queue::new(commands)
    };
    for _ in 0..how.steps {
        replay.catch_up(&mut game);
        game.step();
    }
    replay.catch_up(&mut game);
    Ok((game, replay, repaired))
}

/// How the game starts: from the options, or as the replay given with `--replay` or the save
/// given with `--load` says.
fn origin(
    how: &Start,
    (rules, sheets): (Rules, &Sheets<'_>),
    name: String,
    loaded: &source::LoadedMap,
    settlers: &Settlers,
    save: Option<&Save>,
) -> Result<Origin> {
    let script_hash = rules.script_hash;
    let source = if loaded.in_data {
        MapSource::Data(name.clone())
    } else {
        MapSource::File(name.clone())
    };
    let slots = match (how.slots, how.continent) {
        (Some(chosen), _) => chosen.on_map(&loaded.map),
        (None, Some(_)) => PlayerSlots::world_campaign(&loaded.map),
        (None, None) => PlayerSlots::new_game(&loaded.map, how.players, &Portraits::ORIGINAL),
    };
    let campaign = how
        .mission
        .map(|number| mission_script(sheets, number, None))
        .transpose()?;
    let setup = Setup {
        players: how.players,
        animals: how.animals == AnimalMode::Start,
        goods: how.goods,
        slots: campaign.is_none().then_some(slots),
        starts: how.starts,
        objective: how.objective,
        bug_fixes: how.bug_fixes,
        mission: how.mission,
    };
    let mut start = Replay::new(source, loaded.hash, Some(script_hash), setup);
    start.mods = Some(loaded_mods(how));
    start.tables = Some(rules.tables_hash);
    let mut tables_changed = false;
    start.waypoints = if let Some((_, script)) = &campaign {
        faces(&PlayerSlots::campaign(&loaded.map, script))
    } else if how.nations.is_empty() {
        faces(&slots)
    } else {
        how.nations
            .iter()
            .map(|nation| nation_waypoints(*nation))
            .collect()
    };
    start.nations.clone_from(&start.waypoints);
    start.alliances = how.alliances.into();
    start.exploration = how.exploration;
    let played = match (save, &how.replay, &how.load) {
        (Some(save), _, Some(path)) => Some((save.replay.clone(), path.clone())),
        (None, Some(path), _) => Some((read_replay(path)?, path.clone())),
        _ => None,
    };
    let mut wanted_mission = None;
    if let Some((played, path)) = played {
        wanted_mission = played.mission_script.map(|hash| (hash, path.clone()));
        if played.map_hash != loaded.hash {
            return Err(Error::ReplayMismatch { path, what: "map" });
        }
        if played.script.is_some_and(|hash| hash != script_hash) {
            return Err(Error::ReplayMismatch {
                path,
                what: "animation script",
            });
        }
        if played.tables.is_some_and(|hash| hash != rules.tables_hash) {
            if save.is_none() {
                return Err(Error::ReplayMismatch {
                    path,
                    what: "set of sim tables",
                });
            }
            tables_changed = true;
        }
        start.setup = played.setup;
        start.script = played.script;
        start.waypoints = played.waypoints;
        start.nations = played.nations;
        start.alliances = played.alliances;
        start.exploration = played.exploration;
        start.commands = played.commands;
    }
    let mission = start
        .setup
        .mission
        .map(|number| mission_script(sheets, number, wanted_mission))
        .transpose()?;
    start.mission_script = mission.as_ref().map(|&(hash, _)| hash);
    Ok(Origin {
        name,
        map: loaded.map.clone(),
        file: loaded.file.clone(),
        script: start
            .script
            .is_some()
            .then(|| AnimationScript::new(&settlers.script)),
        mission: mission.map(|(_, script)| script),
        tables: rules.tables,
        tables_changed,
        replay: start,
    })
}

/// Mission `number`'s script and its hash, which must be `wanted`'s hash when a replay or save
/// gives one.
fn mission_script(sheets: &Sheets<'_>, number: u16, wanted: Option<(u64, PathBuf)>) -> Result<(u64, Script)> {
    let bytes = sheets.mission_script(number)?;
    let hash = content_hash(&bytes);
    if let Some((expected, path)) = wanted
        && expected != hash
    {
        return Err(Error::ReplayMismatch {
            path,
            what: "mission script",
        });
    }
    let script = Script::parse(&bytes).map_err(|e| Error::Mission(s2_sim::script_path(number), e))?;
    Ok((hash, script))
}

/// The nation each player's face gives it as the game starts (`0x16f7c`), up to the last
/// player in the game.
fn faces(slots: &PlayerSlots) -> Vec<u8> {
    let nations = slots.nations(&Portraits::ORIGINAL);
    let count = slots.players().last().map_or(0, |player| player.index() + 1);
    nations[..count].to_vec()
}

/// The nations to draw: those given, or the game's own nations, or failing those the nation of
/// each player's waypoint set.
fn nations(how: &Start, replay: &Replay) -> Vec<Nation> {
    if !how.nations.is_empty() {
        return how.nations.clone();
    }
    let indices = if replay.nations.is_empty() {
        &replay.waypoints
    } else {
        &replay.nations
    };
    indices
        .iter()
        .map(|&set| Nation::ALL.get(usize::from(set)).copied().unwrap_or_default())
        .collect()
}

/// The ids of the mods the game plays with, in load order: none when it reads the original files.
fn loaded_mods(how: &Start) -> Vec<String> {
    how.mods
        .as_ref()
        .map(|stack| stack.mods().map(|manifest| manifest.id.clone()).collect())
        .unwrap_or_default()
}

/// Refuses a save or a `--replay` recorded with other mods than the ones loaded, or with the
/// same mods in another order. One that names no mods plays with any.
fn check_mods(how: &Start, save: Option<&Save>) -> Result<()> {
    let (recorded, doing) = match (save, &how.replay) {
        (Some(save), _) => (save.replay.mods.clone(), "load this save"),
        (None, Some(path)) => (read_replay(path)?.mods, "play this replay"),
        (None, None) => return Ok(()),
    };
    let Some(recorded) = recorded else {
        return Ok(());
    };
    match crate::mods::differ(&recorded, &loaded_mods(how)) {
        Some(reason) => Err(Error::ModsDiffer(format!("Can't {doing}, {reason}"))),
        None => Ok(()),
    }
}

/// Reads the save at `path`.
fn read_save(path: &Path) -> Result<Save> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    Save::from_bytes(&bytes).map_err(|source| Error::Save {
        path: path.to_owned(),
        source,
    })
}

/// The map `save` was played on: from the game's data, or the file the save holds.
fn saved_map(how: &Start, sheets: &Sheets<'_>, save: &Save) -> Result<source::LoadedMap> {
    match (&save.replay.map, &save.map) {
        (MapSource::Data(path), _) => source::load(sheets, path),
        (MapSource::File(name), Some(bytes)) => source::wld(name, bytes.clone(), false),
        (MapSource::File(_), None) => Err(Error::SaveWithoutMap(how.load.clone().unwrap_or_default())),
    }
}

/// Saves `game`, started as `origin` says and given the commands in `log`, to `path`, with what
/// the desktop keeps beside it when there is a desktop.
pub(crate) fn write_save(
    path: &Path,
    origin: &Origin,
    log: &[s2_sim::Stamped],
    game: &Game,
    desktop: Option<Vec<u8>>,
) -> Result<()> {
    let mut replay = origin.replay.clone();
    replay.commands = log.to_vec();
    if let MapSource::File(map) = &replay.map {
        if origin.file.is_none() {
            log::warn!("{map} is not a .WLD file; the save cannot hold it");
        }
        let name = Path::new(map)
            .file_name()
            .map_or_else(|| map.clone(), |n| n.to_string_lossy().into_owned());
        replay.map = MapSource::File(name);
    }
    let failed = |source| Error::Save {
        path: path.to_owned(),
        source,
    };
    let bytes = Save::new(game, &replay, origin.file.as_deref())
        .and_then(|save| Save { desktop, ..save }.to_bytes())
        .map_err(failed)?;
    std::fs::write(path, &bytes).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    log::info!(
        "saved step {} to {} ({} bytes)",
        game.clock().step(),
        path.display(),
        bytes.len()
    );
    Ok(())
}

/// The waypoint set the original gives `nation`'s builders, its own, which has the nation's
/// number.
fn nation_waypoints(nation: Nation) -> u8 {
    u8::try_from(nation.index()).unwrap_or(0)
}

fn read_replay(path: &Path) -> Result<Replay> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    Replay::parse(&text).map_err(|source| Error::Log {
        path: path.to_owned(),
        source,
    })
}

/// How often a written replay records the state hash.
const CHECK_EVERY: u32 = 250;

/// Writes a replay to `path` of the game `origin` started, with `log` carried out and run to
/// step `end`. The game is run again from its start to record the hashes.
pub(crate) fn write_replay(path: &Path, origin: &Origin, log: &[s2_sim::Stamped], end: u32) -> Result<()> {
    let mut replay = origin.replay.clone();
    if let MapSource::File(map) = &replay.map {
        replay.map = MapSource::File(relative_to(Path::new(map), path));
    }
    replay.commands = log.to_vec();
    replay.end = end;
    replay.record_checks(&mut origin.start()?, CHECK_EVERY);
    std::fs::write(path, replay.to_text()).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    log::info!(
        "wrote {} commands and {} checks over {end} steps to {}",
        log.len(),
        replay.checks.len(),
        path.display()
    );
    Ok(())
}

/// `map` as the replay at `replay` names it: its file name when the two share a folder,
/// otherwise its full path.
fn relative_to(map: &Path, replay: &Path) -> String {
    let folder = |p: &Path| {
        p.canonicalize()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
    };
    let replay_folder = replay
        .parent()
        .map(|p| {
            if p.as_os_str().is_empty() {
                Path::new(".")
            } else {
                p
            }
        })
        .and_then(|p| p.canonicalize().ok());
    match (folder(map), replay_folder, map.file_name()) {
        (Some(a), Some(b), Some(name)) if a == b => name.to_string_lossy().into_owned(),
        _ => map
            .canonicalize()
            .unwrap_or_else(|_| map.to_path_buf())
            .to_string_lossy()
            .into_owned(),
    }
}

/// The object graphics: the landscape's sheet, the free-play mission set, the ship sheet when
/// the data has one, and each player's nation sheet (loaded once per nation).
fn graphics(
    sheets: &Sheets<'_>,
    assets: &LandscapeAssets,
    landscape: Landscape,
    nations: &[Nation],
) -> Result<ObjectGraphics> {
    let objects = landscape.objects_path();
    if is_gold(objects) && !sheets.exists(objects) {
        return Err(Error::NeedsGold(format!("the {landscape} landscape")));
    }
    let map = sheets.sheet(objects)?;
    let mission = landscape
        .free_play_mission_set()
        .filter(|path| sheets.exists(path))
        .map(|path| sheets.sheet(path).map(Arc::new))
        .transpose()?;
    let ships = sheets
        .exists(SHIP_SPRITES)
        .then(|| sheets.sheet(SHIP_SPRITES).map(Arc::new))
        .transpose()?;
    let mut graphics = ObjectGraphics::new(
        Arc::new(map),
        assets.shadow().clone(),
        assets.grey_shadow().clone(),
    )
    .with_mission(mission)
    .with_ships(ships)
    .with_work_table(assets.work().clone())
    .with_tables(sheets.object_tables()?);
    let mut loaded: BTreeMap<Nation, Arc<SpriteSheet>> = BTreeMap::new();
    for index in 0..Player::COUNT {
        let Some(player) = u8::try_from(index).ok().and_then(Player::new) else {
            continue;
        };
        let nation = nations.get(index).copied().unwrap_or_default();
        let sheet = match loaded.entry(nation) {
            Entry::Occupied(known) => Arc::clone(known.get()),
            Entry::Vacant(new) => {
                let sheet = sheets.sheet(nation.sprites_path(landscape))?;
                Arc::clone(new.insert(Arc::new(sheet)))
            }
        };
        graphics.set_nation(player, nation, sheet);
    }
    Ok(graphics)
}

/// Player `player`'s headquarters: start position `player` of the header.
fn headquarters(map: &Map, player: usize) -> Option<Node> {
    let hq = (*map.header()?.headquarters().get(player)?)?;
    Some(Node { x: hq.x, y: hq.y })
}

/// The fog `player` sees in `game`, as its seen bits say. Players above 1 have no fog in the
/// game.
fn game_fog(game: &Game, player: usize) -> Option<Fog> {
    let fog = u8::try_from(player)
        .ok()
        .and_then(Player::new)
        .and_then(|player| Fog::of(game.nodes(), player));
    if fog.is_none() {
        log::warn!("player {player} sees the whole map in the game; drawing no fog");
    }
    fog
}
