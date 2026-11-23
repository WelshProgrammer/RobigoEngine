//! Replays: what a game was started on, the commands players gave, and state hashes along the
//! way, as text. Playing one back and comparing the hashes shows that the simulation still does
//! what it did, on any machine.
//!
//! A replay is a list of lines. Blank lines and lines starting with `#` are skipped. A line
//! starting with a number is a command, `STEP PLAYER VERB ARGS`, given before step STEP runs:
//!
//! | Verb | Arguments | Command |
//! |---|---|---|
//! | `flag`, `water-flag` | X Y | [`Command::PlaceFlag`] |
//! | `remove-flag` | X Y | [`Command::RemoveFlag`] |
//! | `road`, `water-road` | X Y | [`Command::StartRoad`] |
//! | `step` | a direction: `east`, `south-east`, `south-west`, `west`, `north-west`, `north-east` | [`Command::ExtendRoad`] |
//! | `route` | X Y | [`Command::RouteRoad`] |
//! | `cancel` | | [`Command::CancelRoad`] |
//! | `remove-road` | X Y | [`Command::RemoveRoad`] |
//! | `build` | X Y TYPE | [`Command::PlaceBuilding`] |
//! | `demolish` | X Y | [`Command::Demolish`] |
//! | `stock` | X Y `ware`/`job` N `stop`/`send` `on`/`off` | [`Command::SetStock`] |
//! | `stock-all` | X Y `wares`/`people` `stop`/`send` `on`/`off` | [`Command::SetAllStock`] |
//! | `stop-production` | X Y `on`/`off` | [`Command::StopProduction`] |
//! | `shipyard` | X Y `ships`/`boats` | [`Command::ShipyardMode`] |
//! | `reserve` | X Y RANK N | [`Command::SetReserve`] |
//! | `distribution` | PAGE SLIDER WEIGHT | [`Command::SetDistribution`] |
//! | `distribution-default` | PAGE | [`Command::ResetDistribution`] |
//! | `tool` | WARE WEIGHT | [`Command::SetToolWeight`] |
//! | `tools-default` | | [`Command::ResetToolWeights`] |
//! | `transport` | GROUP PLACE | [`Command::MoveTransport`] |
//! | `transport-default` | | [`Command::ResetTransport`] |
//! | `geologist` | X Y | [`Command::SendGeologist`] |
//! | `scout` | X Y | [`Command::SendScout`] |
//! | `military` | SETTING VALUE | [`Command::SetMilitary`] |
//! | `military-default` | | [`Command::ResetMilitary`] |
//! | `attack` | X Y N `strongest`/`weakest` | [`Command::Attack`] |
//! | `expedition` | X Y | [`Command::PrepareExpedition`] |
//! | `expedition-sail` | SHIP SECTOR | [`Command::SailExpedition`] |
//! | `expedition-found` | SHIP | [`Command::FoundHarbour`] |
//! | `expedition-cancel` | SHIP | [`Command::CancelExpedition`] |
//! | `letter-forget` | SERIAL | [`Command::ForgetMessage`] |
//! | `letter-close` | | [`Command::CloseLetter`] |
//!
//! Any other line is a setting, a keyword and its values:
//!
//! | Keyword | Values | Meaning |
//! |---|---|---|
//! | `map-file` | PATH | the map, a file beside the replay |
//! | `map-data` | PATH | the map, a path inside the game's data |
//! | `map-hash` | HEX | [`content_hash`] of the map file's bytes |
//! | `script` | HEX or `none` | [`content_hash`] of `ROM_ANIM.DAT`, or no animation script |
//! | `players` | N or `all` | [`Setup::players`] |
//! | `animals` | `yes` or `no` | [`Setup::animals`] |
//! | `goods` | `low`, `normal` or `high` | [`Setup::goods`] |
//! | `slot` | PLAYER, `first`, `second`, `computer` or `beaten`, PORTRAIT | one player of [`Setup::slots`]; a replay without the line takes what the setup screen offers |
//! | `alliances` | `none`, `controllers`, `nations` or `against-first` | the [`Alliances`] setting |
//! | `starts` | `in-sequence` or `random` | [`Setup::starts`] |
//! | `objective` | `none`, `land`, `domination` or `headquarters` | [`Setup::objective`] |
//! | `mission` | N, then HEX | [`Setup::mission`], the campaign mission the game plays, and [`content_hash`] of its script; a replay without the hash plays by any script |
//! | `bug-fixes` | `on` or `off` | [`Setup::bug_fixes`]; a replay without the line plays with them off, as every replay did before the setting |
//! | `exploration` | `on` or `off` | [`Replay::exploration`] |
//! | `waypoints` | an index into [`crate::SimTables::waypoints`] per player, from player 0 | builders' waypoint sets |
//! | `nations` | a nation per player, from player 0: 0 Romans, 1 Vikings, 2 Japanese, 3 Africans | [`Game::set_nation`] |
//! | `mods` | mod ids, in load order | the mods the game was played with; a replay without the line plays with any |
//! | `tables` | HEX | a hash of the [`crate::SimTables`] the game was played by; a replay without the line plays by any |
//! | `end` | STEP | how many steps the replay runs |
//! | `check` | STEP HEX | the [`Game::state_hash`] after STEP steps |
//!
//! Commands and checks may come in any order; commands with the same step keep theirs.

use std::fmt::Write as _;

use crate::combat::Alliances;
use crate::commands::{Command, Stamped, StockItem};
use crate::game::{BugFixes, Game, Setup};
use crate::grid::{Direction, Node};
use crate::hash::StateHasher;
use crate::job::Job;
use crate::objectives::Objective;
use crate::players::{Controller, NATIONS, PlayerSlot, PlayerSlots, Portrait, StartOrder};
use crate::ware::WareKind;
use crate::world::{BuildingKind, Player, StartGoods, StockSetting};

/// Names of the six directions, in `Direction::ALL` order.
const DIRECTIONS: [&str; 6] = [
    "east",
    "south-east",
    "south-west",
    "west",
    "north-west",
    "north-east",
];

/// Names of the start goods settings, low first.
const GOODS: [(&str, StartGoods); 3] = [
    ("low", StartGoods::Low),
    ("normal", StartGoods::Normal),
    ("high", StartGoods::High),
];

/// Names of the controllers, in [`Controller::ALL`] order.
const CONTROLLERS: [(&str, Controller); 4] = [
    ("first", Controller::First),
    ("second", Controller::Second),
    ("computer", Controller::Computer),
    ("beaten", Controller::Beaten),
];

/// Names of the alliance settings.
const ALLIANCES: [(&str, Alliances); 4] = [
    ("none", Alliances::None),
    ("controllers", Alliances::Controllers),
    ("nations", Alliances::Nations),
    ("against-first", Alliances::AgainstFirst),
];

/// Names of the start orders.
const STARTS: [(&str, StartOrder); 2] = [
    ("in-sequence", StartOrder::InSequence),
    ("random", StartOrder::Random),
];

/// Names of the bug fix settings.
const BUG_FIXES: [(&str, BugFixes); 2] = [("on", BugFixes::On), ("off", BugFixes::Off)];

/// Names of the objectives.
const OBJECTIVES: [(&str, Objective); 4] = [
    ("none", Objective::None),
    ("land", Objective::Land),
    ("domination", Objective::Domination),
    ("headquarters", Objective::Headquarters),
];

/// The hash replays record for a file's bytes: 64-bit FNV-1a, as [`Game::state_hash`] uses.
#[must_use]
pub fn content_hash(bytes: &[u8]) -> u64 {
    StateHasher::of(bytes)
}

/// Where a replay's map comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MapSource {
    /// A file beside the replay, such as a generated map.
    File(String),
    /// A path inside the player's game data, such as `DATA/MAPS3/OMAP00.WLD`.
    Data(String),
}

/// A replay: how the game started, what was done in it and the hashes it passed through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replay {
    /// The map.
    pub map: MapSource,
    /// [`content_hash`] of the map file.
    pub map_hash: u64,
    /// [`content_hash`] of the animation script, or `None` for a game without one.
    pub script: Option<u64>,
    /// How the game was started.
    pub setup: Setup,
    /// Each player's waypoint set, an index into [`crate::SimTables::waypoints`], from player 0.
    pub waypoints: Vec<u8>,
    /// Each player's nation, from player 0; players past the list are Romans.
    pub nations: Vec<u8>,
    /// How the players were allied as the game started.
    pub alliances: Alliances,
    /// Whether the player sees only the land they have explored. The simulation never reads
    /// it: it decides what the view draws and where a click opens the menu.
    pub exploration: bool,
    /// The ids of the mods the game was played with, in load order, or `None` for a replay that
    /// does not say, which is played whatever mods are loaded.
    pub mods: Option<Vec<String>>,
    /// A hash of the tables the game was played by, or `None` for a replay that does not say,
    /// which is played by any.
    pub tables: Option<u64>,
    /// [`content_hash`] of the mission script a campaign game ran, or `None` for a replay that
    /// does not say, which is played by any.
    pub mission_script: Option<u64>,
    /// The commands, in step order.
    pub commands: Vec<Stamped>,
    /// State hashes after given numbers of steps, in step order.
    pub checks: Vec<(u32, u64)>,
    /// How many steps the replay runs.
    pub end: u32,
}

/// Why a replay could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {reason}")]
pub struct ParseError {
    /// The line, from 1.
    pub line: usize,
    /// What was wrong.
    pub reason: String,
}

/// Where a played replay first left its recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("after {step} steps the state hash is {found:016x}, the replay recorded {expected:016x}")]
pub struct Mismatch {
    /// Steps run.
    pub step: u32,
    /// The recorded hash.
    pub expected: u64,
    /// The hash the game reached.
    pub found: u64,
}

impl Replay {
    /// A replay of a game on `map` started with `setup`, with no commands or checks yet.
    #[must_use]
    pub fn new(map: MapSource, map_hash: u64, script: Option<u64>, setup: Setup) -> Self {
        Self {
            map,
            map_hash,
            script,
            setup,
            waypoints: Vec::new(),
            nations: Vec::new(),
            alliances: Alliances::None,
            exploration: true,
            mods: None,
            tables: None,
            mission_script: None,
            commands: Vec::new(),
            checks: Vec::new(),
            end: 0,
        }
    }

    /// Gives `game` the waypoint sets, nations and alliances the replay names.
    pub fn apply_players(&self, game: &mut Game) {
        for (index, &nation) in self.nations.iter().enumerate() {
            if let Some(player) = u8::try_from(index).ok().and_then(Player::new) {
                game.set_nation(player, nation);
            }
        }
        for (index, &set) in self.waypoints.iter().enumerate() {
            if let Some(player) = u8::try_from(index).ok().and_then(Player::new)
                && let Some(set) = game.world().waypoint_set(usize::from(set)).cloned()
            {
                game.set_waypoints(player, set);
            }
        }
        if self.alliances != Alliances::None {
            game.set_alliances(self.alliances);
        }
    }

    /// Plays the replay on `game`, freshly started as the replay says, to its end, checking
    /// each recorded hash on the way. Refused commands are skipped, as [`Game::run`] skips them.
    ///
    /// # Errors
    ///
    /// [`Mismatch`] at the first check the game fails.
    pub fn play(&self, game: &mut Game) -> Result<(), Mismatch> {
        let mut playback = Playback::new(self);
        while game.clock().step() < self.end {
            playback.catch_up(game);
            game.step();
            playback.check(game)?;
        }
        Ok(())
    }

    /// Records checks on `game`, started as the replay says, every `every` steps up to the end,
    /// replacing any there were.
    pub fn record_checks(&mut self, game: &mut Game, every: u32) {
        self.checks.clear();
        let mut playback = Playback::new(self);
        let mut checks = Vec::new();
        while game.clock().step() < self.end {
            playback.catch_up(game);
            game.step();
            let step = game.clock().step();
            if every > 0 && (step.is_multiple_of(every) || step == self.end) {
                checks.push((step, game.state_hash()));
            }
        }
        self.checks = checks;
    }

    /// The replay as text, which [`Self::parse`] reads back.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::from("# robigo replay\n");
        let _ = match &self.map {
            MapSource::File(path) => writeln!(out, "map-file {path}"),
            MapSource::Data(path) => writeln!(out, "map-data {path}"),
        };
        let _ = writeln!(out, "map-hash {:016x}", self.map_hash);
        let _ = match self.script {
            Some(hash) => writeln!(out, "script {hash:016x}"),
            None => writeln!(out, "script none"),
        };
        let _ = match self.setup.players {
            Some(players) => writeln!(out, "players {players}"),
            None => writeln!(out, "players all"),
        };
        let _ = writeln!(out, "animals {}", if self.setup.animals { "yes" } else { "no" });
        let goods = GOODS
            .iter()
            .find(|(_, goods)| *goods == self.setup.goods)
            .map_or("normal", |(name, _)| name);
        let _ = writeln!(out, "goods {goods}");
        if let Some(slots) = &self.setup.slots {
            for player in slots.players() {
                let Some(slot) = slots.get(player) else {
                    continue;
                };
                let controller = CONTROLLERS
                    .iter()
                    .find(|(_, controller)| *controller == slot.controller)
                    .map_or("computer", |(name, _)| name);
                let _ = writeln!(out, "slot {} {controller} {}", player.index(), slot.portrait.0);
            }
        }
        if self.setup.starts != StartOrder::InSequence
            && let Some((name, _)) = STARTS.iter().find(|(_, starts)| *starts == self.setup.starts)
        {
            let _ = writeln!(out, "starts {name}");
        }
        if self.setup.objective != Objective::None
            && let Some((name, _)) = OBJECTIVES
                .iter()
                .find(|(_, objective)| *objective == self.setup.objective)
        {
            let _ = writeln!(out, "objective {name}");
        }
        if self.alliances != Alliances::None
            && let Some((name, _)) = ALLIANCES
                .iter()
                .find(|(_, alliances)| *alliances == self.alliances)
        {
            let _ = writeln!(out, "alliances {name}");
        }
        if !self.exploration {
            out.push_str("exploration off\n");
        }
        if let Some(mission) = self.setup.mission {
            let _ = match self.mission_script {
                Some(hash) => writeln!(out, "mission {mission} {hash:016x}"),
                None => writeln!(out, "mission {mission}"),
            };
        }
        if let Some((name, _)) = BUG_FIXES.iter().find(|(_, fixes)| *fixes == self.setup.bug_fixes) {
            let _ = writeln!(out, "bug-fixes {name}");
        }
        if !self.waypoints.is_empty() {
            out.push_str("waypoints");
            for set in &self.waypoints {
                let _ = write!(out, " {set}");
            }
            out.push('\n');
        }
        if !self.nations.is_empty() {
            out.push_str("nations");
            for nation in &self.nations {
                let _ = write!(out, " {nation}");
            }
            out.push('\n');
        }
        if let Some(mods) = &self.mods {
            out.push_str("mods");
            for id in mods {
                let _ = write!(out, " {id}");
            }
            out.push('\n');
        }
        if let Some(hash) = self.tables {
            let _ = writeln!(out, "tables {hash:016x}");
        }
        let _ = writeln!(out, "end {}", self.end);
        for (step, hash) in &self.checks {
            let _ = writeln!(out, "check {step} {hash:016x}");
        }
        out.push_str("# STEP PLAYER VERB ARGS\n");
        for stamped in &self.commands {
            let (verb, args) = verb_and_args(stamped.command);
            let _ = writeln!(out, "{} {} {verb}{args}", stamped.step, stamped.player.index());
        }
        out
    }

    /// Reads a replay written by [`Self::to_text`] or by hand. A replay needs a map and its
    /// hash; the other settings default to a game with every player, animals, normal goods,
    /// no script, no waypoint sets and the bug fixes off, and `end` to the last command's step.
    ///
    /// # Errors
    ///
    /// [`ParseError`] for the first line that cannot be read, or line 0 for a missing map.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut map = None;
        let mut map_hash = None;
        let mut replay = Self::new(
            MapSource::File(String::new()),
            0,
            None,
            Setup {
                bug_fixes: BugFixes::Off,
                ..Setup::default()
            },
        );
        let mut end = None;
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fail = |reason: &str| ParseError {
                line: index + 1,
                reason: reason.to_owned(),
            };
            let words: Vec<&str> = line.split_whitespace().collect();
            let rest = || {
                line.split_once(char::is_whitespace)
                    .map(|(_, rest)| rest.trim().to_owned())
            };
            let number = |i: usize| words.get(i).and_then(|w| w.parse::<u32>().ok());
            let hex = |i: usize| words.get(i).and_then(|w| u64::from_str_radix(w, 16).ok());
            match words[0] {
                "map-file" => map = Some(MapSource::File(rest().ok_or_else(|| fail("expected a path"))?)),
                "map-data" => map = Some(MapSource::Data(rest().ok_or_else(|| fail("expected a path"))?)),
                "map-hash" => map_hash = Some(hex(1).ok_or_else(|| fail("expected a hex hash"))?),
                "script" => {
                    replay.script = match words.get(1) {
                        Some(&"none") => None,
                        _ => Some(hex(1).ok_or_else(|| fail("expected a hex hash or none"))?),
                    };
                }
                "players" => {
                    replay.setup.players = match words.get(1) {
                        Some(&"all") => None,
                        _ => Some(
                            number(1)
                                .and_then(|n| usize::try_from(n).ok())
                                .ok_or_else(|| fail("expected a count or all"))?,
                        ),
                    };
                }
                "animals" => {
                    replay.setup.animals = match words.get(1) {
                        Some(&"yes") => true,
                        Some(&"no") => false,
                        _ => return Err(fail("expected yes or no")),
                    };
                }
                "slot" => slot(&words, &mut replay.setup.slots).map_err(fail)?,
                "goods" | "starts" | "objective" | "bug-fixes" | "alliances" | "exploration" | "mission" => {
                    choice(&words, &mut replay).map_err(fail)?;
                }
                "waypoints" => {
                    replay.waypoints = (1..words.len())
                        .map(|i| number(i).and_then(|n| u8::try_from(n).ok()))
                        .collect::<Option<_>>()
                        .ok_or_else(|| fail("expected waypoint set numbers"))?;
                }
                "nations" => {
                    replay.nations = (1..words.len())
                        .map(|i| {
                            number(i)
                                .and_then(|n| u8::try_from(n).ok())
                                .filter(|&n| n < NATIONS)
                        })
                        .collect::<Option<_>>()
                        .ok_or_else(|| fail("expected nation numbers 0 to 3"))?;
                }
                "tables" => replay.tables = Some(hex(1).ok_or_else(|| fail("expected a hex hash"))?),
                "mods" => replay.mods = Some(words[1..].iter().map(|&id| id.to_owned()).collect()),
                "end" => end = Some(number(1).ok_or_else(|| fail("expected a step count"))?),
                "check" => {
                    let step = number(1).ok_or_else(|| fail("expected a step count"))?;
                    let hash = hex(2).ok_or_else(|| fail("expected a hex hash"))?;
                    replay.checks.push((step, hash));
                }
                word if word.parse::<u32>().is_ok() => {
                    replay.commands.push(command(&words).map_err(|r| fail(&r))?);
                }
                word => return Err(fail(&format!("unknown setting {word}"))),
            }
        }
        let missing = |reason: &str| ParseError {
            line: 0,
            reason: reason.to_owned(),
        };
        replay.map = map.ok_or_else(|| missing("no map-file or map-data line"))?;
        replay.map_hash = map_hash.ok_or_else(|| missing("no map-hash line"))?;
        replay.commands.sort_by_key(|stamped| stamped.step);
        replay.checks.sort_by_key(|&(step, _)| step);
        replay.end = end.unwrap_or_else(|| replay.commands.last().map_or(0, |stamped| stamped.step));
        Ok(replay)
    }
}

/// A replay being played into a game, a step at a time.
#[derive(Debug, Clone)]
pub struct Playback<'a> {
    replay: &'a Replay,
    next: usize,
    check: usize,
}

impl<'a> Playback<'a> {
    /// Plays `replay` from its start.
    #[must_use]
    pub fn new(replay: &'a Replay) -> Self {
        Self {
            replay,
            next: 0,
            check: 0,
        }
    }

    /// Carries out the commands stamped up to `game`'s current step that have not run yet, and
    /// returns them. A refused command is skipped.
    pub fn catch_up(&mut self, game: &mut Game) -> &'a [Stamped] {
        let first = self.next;
        let now = game.clock().step();
        let commands = &self.replay.commands;
        while let Some(&stamped) = commands.get(self.next).filter(|s| s.step <= now) {
            let _ = game.execute(stamped.player, stamped.command);
            self.next += 1;
        }
        &commands[first..self.next]
    }

    /// The commands carried out so far.
    #[must_use]
    pub fn done(&self) -> &'a [Stamped] {
        &self.replay.commands[..self.next]
    }

    /// Compares `game` with every check recorded for its step or earlier that has not been
    /// compared yet.
    ///
    /// # Errors
    ///
    /// [`Mismatch`] when the game's hash differs from a check at its own step. Checks for
    /// earlier steps, missed by a caller that skipped them, are passed over.
    pub fn check(&mut self, game: &Game) -> Result<(), Mismatch> {
        let now = game.clock().step();
        while let Some(&(step, expected)) = self.replay.checks.get(self.check).filter(|(s, _)| *s <= now) {
            self.check += 1;
            let found = game.state_hash();
            if step == now && found != expected {
                return Err(Mismatch {
                    step,
                    expected,
                    found,
                });
            }
        }
        Ok(())
    }
}

fn verb_and_args(command: Command) -> (&'static str, String) {
    let at = |node: Node| format!(" {} {}", node.x, node.y);
    match command {
        Command::PlaceFlag { node, water: false } => ("flag", at(node)),
        Command::PlaceFlag { node, water: true } => ("water-flag", at(node)),
        Command::RemoveFlag { node } => ("remove-flag", at(node)),
        Command::StartRoad { node, water: false } => ("road", at(node)),
        Command::StartRoad { node, water: true } => ("water-road", at(node)),
        Command::ExtendRoad { direction } => ("step", format!(" {}", DIRECTIONS[direction.index()])),
        Command::RouteRoad { node } => ("route", at(node)),
        Command::CancelRoad => ("cancel", String::new()),
        Command::RemoveRoad { node } => ("remove-road", at(node)),
        Command::PlaceBuilding { node, kind } => ("build", format!("{} {}", at(node), kind.0)),
        Command::Demolish { node } => ("demolish", at(node)),
        Command::SetStock {
            node,
            item,
            setting,
            on,
        } => {
            let (page, number) = match item {
                StockItem::Ware(ware) => ("ware", ware.0),
                StockItem::Job(job) => ("job", job.0),
            };
            let words = format!(" {page} {number} {} {}", setting_word(setting), switch_word(on));
            ("stock", at(node) + &words)
        }
        Command::SetAllStock {
            node,
            people,
            setting,
            on,
        } => {
            let page = if people { "people" } else { "wares" };
            let words = format!(" {page} {} {}", setting_word(setting), switch_word(on));
            ("stock-all", at(node) + &words)
        }
        Command::StopProduction { node, stop } => ("stop-production", at(node) + " " + switch_word(stop)),
        Command::ShipyardMode { node, ships } => {
            ("shipyard", at(node) + if ships { " ships" } else { " boats" })
        }
        Command::SetReserve { node, rank, wanted } => {
            ("reserve", format!("{} {} {wanted}", at(node), rank.0))
        }
        Command::SetDistribution {
            group,
            consumer,
            weight,
        } => ("distribution", format!(" {group} {consumer} {weight}")),
        Command::ResetDistribution { group } => ("distribution-default", format!(" {group}")),
        Command::SetToolWeight { tool, weight } => ("tool", format!(" {} {weight}", tool.0)),
        Command::ResetToolWeights => ("tools-default", String::new()),
        Command::MoveTransport { group, rank } => ("transport", format!(" {group} {rank}")),
        Command::ResetTransport => ("transport-default", String::new()),
        Command::SendGeologist { node } => ("geologist", at(node)),
        Command::SendScout { node } => ("scout", at(node)),
        Command::SetMilitary { setting, value } => ("military", format!(" {setting} {value}")),
        Command::ResetMilitary => ("military-default", String::new()),
        Command::Attack {
            node,
            soldiers,
            strongest,
        } => {
            let order = if strongest { "strongest" } else { "weakest" };
            ("attack", at(node) + &format!(" {soldiers} {order}"))
        }
        Command::PrepareExpedition { node } => ("expedition", at(node)),
        Command::SailExpedition { ship, sector } => ("expedition-sail", format!(" {ship} {sector}")),
        Command::FoundHarbour { ship } => ("expedition-found", format!(" {ship}")),
        Command::CancelExpedition { ship } => ("expedition-cancel", format!(" {ship}")),
        Command::ForgetMessage { serial } => ("letter-forget", format!(" {serial}")),
        Command::CloseLetter => ("letter-close", String::new()),
    }
}

/// The harbour and ship command `verb` on a line split into `words`, or `None` for another verb.
fn ship_verb(verb: &str, words: &[&str]) -> Option<Result<Command, String>> {
    let number = |i: usize, what: &str| {
        words
            .get(i)
            .and_then(|w| w.parse().ok())
            .ok_or_else(|| format!("expected {what}"))
    };
    let ship = || number(3, "a ship");
    let command = match verb {
        "expedition" => (|| {
            Ok(Command::PrepareExpedition {
                node: Node {
                    x: number(3, "X")?,
                    y: number(4, "Y")?,
                },
            })
        })(),
        "expedition-sail" => (|| {
            Ok(Command::SailExpedition {
                ship: ship()?,
                sector: u8::try_from(number(4, "a sector")?).map_err(|_| "expected a sector")?,
            })
        })(),
        "expedition-found" => ship().map(|ship| Command::FoundHarbour { ship }),
        "expedition-cancel" => ship().map(|ship| Command::CancelExpedition { ship }),
        _ => return None,
    };
    Some(command)
}

/// The economy settings command `verb` on a line split into `words`, or `None` for another verb.
fn economy(verb: &str, words: &[&str]) -> Option<Result<Command, String>> {
    let byte = |i: usize, what: &str| {
        words
            .get(i)
            .and_then(|w| w.parse::<u8>().ok())
            .ok_or_else(|| format!("expected {what}"))
    };
    let command = match verb {
        "distribution" => (|| {
            Ok(Command::SetDistribution {
                group: byte(3, "a page")?,
                consumer: byte(4, "a slider")?,
                weight: byte(5, "a weight")?,
            })
        })(),
        "distribution-default" => byte(3, "a page").map(|group| Command::ResetDistribution { group }),
        "tool" => (|| {
            Ok(Command::SetToolWeight {
                tool: WareKind(byte(3, "a tool")?),
                weight: byte(4, "a weight")?,
            })
        })(),
        "tools-default" => Ok(Command::ResetToolWeights),
        "transport" => (|| {
            Ok(Command::MoveTransport {
                group: byte(3, "a group")?,
                rank: byte(4, "a place")?,
            })
        })(),
        "transport-default" => Ok(Command::ResetTransport),
        "military" => (|| {
            Ok(Command::SetMilitary {
                setting: byte(3, "a setting")?,
                value: byte(4, "a value")?,
            })
        })(),
        "military-default" => Ok(Command::ResetMilitary),
        _ => return None,
    };
    Some(command)
}

/// The `stock` or `stock-all` command on a line split into `words`, for the storehouse on
/// `node`.
fn stock(words: &[&str], all: bool, node: Node) -> Result<Command, String> {
    let word = |i: usize| words.get(i).copied().unwrap_or_default();
    let at = if all { 6 } else { 7 };
    let setting = match word(at) {
        "stop" => StockSetting::STOP,
        "send" => StockSetting::SEND,
        _ => return Err("expected stop or send".into()),
    };
    let on = match word(at + 1) {
        "on" => true,
        "off" => false,
        _ => return Err("expected on or off".into()),
    };
    if all {
        let people = match word(5) {
            "people" => true,
            "wares" => false,
            _ => return Err("expected wares or people".into()),
        };
        return Ok(Command::SetAllStock {
            node,
            people,
            setting,
            on,
        });
    }
    let number = word(6)
        .parse::<u8>()
        .map_err(|_| "expected a ware or job number")?;
    let item = match word(5) {
        "ware" => StockItem::Ware(WareKind(number)),
        "job" => StockItem::Job(Job(number)),
        _ => return Err("expected ware or job".into()),
    };
    Ok(Command::SetStock {
        node,
        item,
        setting,
        on,
    })
}

/// The replay word for a storehouse setting.
fn setting_word(setting: StockSetting) -> &'static str {
    if setting == StockSetting::SEND {
        "send"
    } else {
        "stop"
    }
}

/// The replay word for a setting turned on or off.
const fn switch_word(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// The command on a line split into `words`.
/// `reserve X Y RANK N`.
fn reserve_verb(words: &[&str]) -> Result<Command, String> {
    let number = |index: usize| {
        words
            .get(index)
            .and_then(|word| word.parse::<u8>().ok())
            .ok_or_else(|| "expected a number".to_string())
    };
    Ok(Command::SetReserve {
        node: Node {
            x: words
                .get(3)
                .and_then(|word| word.parse().ok())
                .ok_or("expected X")?,
            y: words
                .get(4)
                .and_then(|word| word.parse().ok())
                .ok_or("expected Y")?,
        },
        rank: Job(number(5)?),
        wanted: number(6)?,
    })
}

/// The value `word` names in `names`.
fn named<T: Copy>(names: &[(&str, T)], word: Option<&&str>) -> Option<T> {
    let word = *word?;
    names
        .iter()
        .find(|(name, _)| *name == word)
        .map(|&(_, value)| value)
}

/// Reads a line naming one of a setting's choices, `goods`, `starts`, `objective`, `bug-fixes`,
/// `alliances` or `exploration`, or the `mission` number, into `replay`.
fn choice(words: &[&str], replay: &mut Replay) -> Result<(), &'static str> {
    let word = words.get(1);
    match words[0] {
        "goods" => replay.setup.goods = named(&GOODS, word).ok_or("expected low, normal or high")?,
        "starts" => replay.setup.starts = named(&STARTS, word).ok_or("expected in-sequence or random")?,
        "objective" => {
            replay.setup.objective =
                named(&OBJECTIVES, word).ok_or("expected none, land, domination or headquarters")?;
        }
        "bug-fixes" => replay.setup.bug_fixes = named(&BUG_FIXES, word).ok_or("expected on or off")?,
        "mission" => {
            replay.setup.mission = Some(
                word.and_then(|w| w.parse().ok())
                    .ok_or("expected a mission number")?,
            );
            replay.mission_script = words
                .get(2)
                .map(|w| u64::from_str_radix(w, 16).map_err(|_| "expected a hex hash of the script"))
                .transpose()?;
        }
        "alliances" => {
            replay.alliances = named(&ALLIANCES, word).ok_or("expected none, controllers or nations")?;
        }
        _ => replay.exploration = named(&[("on", true), ("off", false)], word).ok_or("expected on or off")?,
    }
    Ok(())
}

/// Reads a `slot PLAYER CONTROLLER PORTRAIT` line into `slots`.
fn slot(words: &[&str], slots: &mut Option<PlayerSlots>) -> Result<(), &'static str> {
    let number = |i: usize| words.get(i).and_then(|w| w.parse::<u8>().ok());
    let player = number(1)
        .and_then(Player::new)
        .ok_or("expected a player 0 to 6")?;
    let controller = named(&CONTROLLERS, words.get(2)).ok_or("expected first, second, computer or beaten")?;
    let portrait = number(3).map(Portrait).ok_or("expected a portrait number")?;
    slots
        .get_or_insert(PlayerSlots::EMPTY)
        .set(player, Some(PlayerSlot { controller, portrait }));
    Ok(())
}

fn command(words: &[&str]) -> Result<Stamped, String> {
    let number = |i: usize| words.get(i).and_then(|w| w.parse::<u32>().ok());
    let step = number(0).ok_or("expected a step number")?;
    let player = number(1)
        .and_then(|p| u8::try_from(p).ok())
        .and_then(Player::new)
        .ok_or("expected a player, 0 to 6")?;
    let verb = *words.get(2).ok_or("expected a verb")?;
    let node = || {
        let coordinate = |i| number(i).and_then(|n| u16::try_from(n).ok());
        coordinate(3)
            .zip(coordinate(4))
            .map(|(x, y)| Node { x, y })
            .ok_or("expected X Y")
    };
    let command = match verb {
        "flag" => Command::PlaceFlag {
            node: node()?,
            water: false,
        },
        "water-flag" => Command::PlaceFlag {
            node: node()?,
            water: true,
        },
        "remove-flag" => Command::RemoveFlag { node: node()? },
        "road" => Command::StartRoad {
            node: node()?,
            water: false,
        },
        "water-road" => Command::StartRoad {
            node: node()?,
            water: true,
        },
        "step" => {
            let name = words.get(3).copied().unwrap_or_default();
            let index = DIRECTIONS
                .iter()
                .position(|&d| d == name)
                .ok_or("expected a direction such as east or north-west")?;
            Command::ExtendRoad {
                direction: Direction::ALL[index],
            }
        }
        "route" => Command::RouteRoad { node: node()? },
        "cancel" => Command::CancelRoad,
        "letter-close" => Command::CloseLetter,
        "letter-forget" => Command::ForgetMessage {
            serial: number(3).ok_or("expected a serial")?,
        },
        "remove-road" => Command::RemoveRoad { node: node()? },
        "build" => Command::PlaceBuilding {
            node: node()?,
            kind: number(5)
                .and_then(|n| u8::try_from(n).ok())
                .map(BuildingKind)
                .ok_or("expected a building type")?,
        },
        "demolish" => Command::Demolish { node: node()? },
        "stock" => stock(words, false, node()?)?,
        "stock-all" => stock(words, true, node()?)?,
        "geologist" => Command::SendGeologist { node: node()? },
        "scout" => Command::SendScout { node: node()? },
        "attack" => Command::Attack {
            node: node()?,
            soldiers: number(5)
                .and_then(|n| u8::try_from(n).ok())
                .ok_or("expected a number of soldiers")?,
            strongest: match words.get(6).copied() {
                Some("strongest") => true,
                Some("weakest") => false,
                _ => return Err("expected strongest or weakest".into()),
            },
        },
        "shipyard" => Command::ShipyardMode {
            node: node()?,
            ships: match words.get(5).copied() {
                Some("ships") => true,
                Some("boats") => false,
                _ => return Err("expected ships or boats".into()),
            },
        },
        "reserve" => reserve_verb(words)?,
        "stop-production" => Command::StopProduction {
            node: node()?,
            stop: match words.get(5).copied() {
                Some("on") => true,
                Some("off") => false,
                _ => return Err("expected on or off".into()),
            },
        },
        _ => economy(verb, words)
            .or_else(|| ship_verb(verb, words))
            .ok_or_else(|| format!("unknown verb {verb}"))??,
    };
    Ok(Stamped {
        step,
        player,
        command,
    })
}

#[cfg(test)]
mod tests;
