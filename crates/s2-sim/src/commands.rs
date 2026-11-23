//! Player commands, the only input the game takes. Each one does what the original's interface
//! does for that action: the checks the interface makes before it offers the action, then the
//! world routines it calls. In `S2.EXE` every action takes effect the moment it is made; here a
//! command runs between two simulation steps.

use crate::construction::fits;
use crate::economy::{MAX_WEIGHT, WINDOW_LEFTOVER};
use crate::game::Game;
use crate::grid::{Direction, Node};
use crate::ground::TerrainClass;
use crate::homeward::MAX_PATHS;
use crate::job::Job;
use crate::marks::RoadKind;
use crate::military::MOST_RESERVED;
use crate::nodes::{NodeFlags, NodeMap, SiteFlags};
use crate::ware::WareKind;
use crate::world::{
    BuildingKind, BuildingState, Category, FlagKind, MAX_ROADS, MapObject, Player, RoadError, StockSetting,
    Storehouse, World,
};

/// The most segments a road can have while it is drawn: the length of the mode's path buffer.
pub const MAX_SEGMENTS: usize = 100;

/// The most segments a water road can have (`0x194c9`).
pub const MAX_WATER_SEGMENTS: usize = 4;

/// Rings round a node that a military building needs free of other military buildings and of
/// headquarters (`0x197a6`).
pub const MILITARY_SPACING: u16 = 4;

/// Something a player does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Command {
    /// Places a flag on `node`, a water flag when `water` is set (`0x27ceb`). While the player
    /// draws a road, `node` must be the road's head; the flag goes there without splitting the
    /// road being drawn and finishes it.
    PlaceFlag {
        /// Where.
        node: Node,
        /// Whether it is a water flag; one of the node's edges must have water on both sides.
        water: bool,
    },
    /// Removes the player's flag on `node` with every road at it, and demolishes the building at
    /// the flag first (`0x25763`).
    RemoveFlag {
        /// Where.
        node: Node,
    },
    /// Starts drawing a road, or a water road, from the player's flag on `node` (`0x27536`).
    StartRoad {
        /// The flag's node.
        node: Node,
        /// Whether to draw a water road.
        water: bool,
    },
    /// Takes the road being drawn one node further, as a click next to its head does
    /// (`0x27a51`). A step back along the last segment takes that segment back. A step onto one
    /// of the player's flags finishes the road. When another road runs through the next node and
    /// a flag fits there, a flag is placed first, splitting that road.
    ExtendRoad {
        /// Which way from the head.
        direction: Direction,
    },
    /// Takes the road being drawn towards `node`, as a click further than one node from its
    /// head does (`0x27a51`). The off-road search plans a route of at most 19 steps over nodes
    /// with no road on them, and the route is taken one step at a time until a step is refused.
    /// A refused last step onto another road places a flag there first, splitting that road,
    /// and ends on it. A click next to the head is [`Command::ExtendRoad`].
    RouteRoad {
        /// The node clicked.
        node: Node,
    },
    /// Stops drawing and drops the road drawn so far (`0x275cd`).
    CancelRoad,
    /// Removes the road running through `node`, if it starts at one of the player's flags
    /// (`0x27df9`).
    RemoveRoad {
        /// One of the road's inner nodes.
        node: Node,
    },
    /// Places a building site of type `kind` on `node` (`0x26221`). The node must be the
    /// player's and offer room for the type, and a harbour goes only where the map marks a
    /// harbour site (`0x1988d`).
    PlaceBuilding {
        /// Where.
        node: Node,
        /// The building type, any but the headquarters.
        kind: BuildingKind,
    },
    /// Demolishes the player's building or site on `node` (`0x2649a`, `0x2c9f7`). A finished
    /// building burns down.
    Demolish {
        /// Where.
        node: Node,
    },
    /// Turns `setting` on or off for one ware or job in the player's finished storehouse on
    /// `node`, as a click on its icon in the storehouse window does (`0x5648c`).
    SetStock {
        /// Where the storehouse stands.
        node: Node,
        /// The ware or job.
        item: StockItem,
        /// Stop or send out.
        setting: StockSetting,
        /// Whether the setting is on.
        on: bool,
    },
    /// Turns `setting` on or off for every ware, or every job, in the player's finished
    /// storehouse on `node`, as the window's "all" button does (`0x567b4`).
    SetAllStock {
        /// Where the storehouse stands.
        node: Node,
        /// The jobs page, or the wares page.
        people: bool,
        /// Stop or send out.
        setting: StockSetting,
        /// Whether the setting is on.
        on: bool,
    },
    /// Stops or restarts production at the player's building on `node`, as the stop button in
    /// its window does (`0x568a4`). Only finished workplaces of a stoppable type offer it, and
    /// military buildings, where it stops coins and frees the coin orders not yet served
    /// (`0x30e7e`).
    StopProduction {
        /// Where the building stands.
        node: Node,
        /// Whether production is stopped.
        stop: bool,
    },
    /// Sets the player's finished shipyard on `node` to build ships or boats, as the button in its
    /// window does (`0x56926`). The window writes the mode and nothing else: a site already
    /// picked and a shipwright already out are kept.
    ShipyardMode {
        /// Where the shipyard stands.
        node: Node,
        /// Ships; otherwise boats.
        ships: bool,
    },
    /// Keeps `wanted` soldiers of rank `rank` back in the headquarters' reserve, as its window's
    /// plus and minus buttons do one at a time (`0x5687b`, `0x5688c`).
    SetReserve {
        /// Where the headquarters stands.
        node: Node,
        /// The rank, one of [`Job::RANKS`].
        rank: Job,
        /// How many to keep back, 0 to 15.
        wanted: u8,
    },
    /// Sets slider `consumer` on page `group` of the distribution window to `weight`, as its
    /// plus and minus buttons do (`0x5d14e`, `0x5d1d6`).
    SetDistribution {
        /// The page, 0 for food.
        group: u8,
        /// The slider on the page.
        consumer: u8,
        /// The new weight, 0 to [`crate::MAX_WEIGHT`].
        weight: u8,
    },
    /// Puts page `group` of the distribution window back to its defaults (`0x5d0a2`).
    ResetDistribution {
        /// The page.
        group: u8,
    },
    /// Sets the weight the metalworks give tool `tool`, as the tool window's plus and minus
    /// buttons do (`0x5d6cc`, `0x5d745`).
    SetToolWeight {
        /// The tool, tongs to bow.
        tool: WareKind,
        /// The new weight, 0 to [`crate::MAX_WEIGHT`].
        weight: u8,
    },
    /// Puts the tool weights back to their defaults (`0x5d625`).
    ResetToolWeights,
    /// Moves transport group `group` to place `rank` in the transport list one place at a time,
    /// as the window's up, down, top and bottom buttons do (`0x5dc9d`, `0x5dce6`).
    MoveTransport {
        /// The group.
        group: u8,
        /// Its new place, 0 first.
        rank: u8,
    },
    /// Puts the transport list back to its default order (`0x5dd29`).
    ResetTransport,
    /// Sets military setting `setting` ([`crate::RECRUITING`] to [`crate::OCCUPATION`] + 2) to
    /// `value`, as the military window's buttons do (`0x5c96c`): they step by 10 or 25, from 0
    /// to [`crate::MOST_MILITARY`].
    SetMilitary {
        /// Which setting.
        setting: u8,
        /// Its new value.
        value: u8,
    },
    /// Puts the military settings back to their defaults (`0x298b6`).
    ResetMilitary,
    /// Orders a geologist to the player's flag on `node`, as the action window's button does
    /// (`0x5bc6f`). One click sends one; with no storehouse able to send one the order lapses.
    SendGeologist {
        /// The flag's node.
        node: Node,
    },
    /// Orders a scout to the player's flag on `node`, as the action window's button does
    /// (`0x5bca9`). One click sends one; with no storehouse able to send one the order lapses.
    SendScout {
        /// The flag's node.
        node: Node,
    },
    /// Sends soldiers against another player's building on `node`, as the attack window's
    /// button does (`0x5b9e6`): up to `soldiers`, from 1 to the count
    /// [`Game::attack_count`] gives, the strongest or the weakest first.
    Attack {
        /// The building's node.
        node: Node,
        /// How many to send.
        soldiers: u8,
        /// Strongest first; otherwise weakest first.
        strongest: bool,
    },
    /// Starts preparing an expedition at the player's finished harbour on `node`, as the harbour
    /// window's button does (`0x56a4b`, `0x43475`). Refused while one is being prepared there.
    PrepareExpedition {
        /// Where the harbour stands.
        node: Node,
    },
    /// Sends the player's expedition ship `ship` towards the harbour site it found in sector
    /// `sector`, 0 north round to 5 north-west, as the ship window's arrows do (`0x43039`). The
    /// window offers a sector only while the ship rests and has found a site there.
    SailExpedition {
        /// The ship record.
        ship: u16,
        /// The sector, 0 to [`crate::SECTORS`] − 1.
        sector: u8,
    },
    /// Founds a harbour from the player's expedition ship `ship`, as the ship window's button
    /// does (`0x43039`, `0x40949`): offered only while the ship rests where the land belongs to
    /// nobody.
    FoundHarbour {
        /// The ship record.
        ship: u16,
    },
    /// Calls off the player's expedition ship `ship`, as the ship window's button does
    /// (`0x418d0`). It becomes a transport ship with the expedition still aboard.
    CancelExpedition {
        /// The ship record.
        ship: u16,
    },
    /// Takes the player's message with `serial` off the list, which the post office's own
    /// button does (`0x2fb7`, `0x21b6b`).
    ForgetMessage {
        /// The message's place in the order the player's messages were made.
        serial: u32,
    },
    /// Closes the campaign letter on screen. Only player 0 is shown letters.
    CloseLetter,
}

/// A ware or a job in a storehouse's window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum StockItem {
    /// A ware type.
    Ware(WareKind),
    /// One of the jobs a storehouse counts.
    Job(Job),
}

/// A command stamped with the player who gave it and the step it runs before: it runs when the
/// clock has counted `step` steps. A log of these, with the map, setup and seed, replays a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Stamped {
    /// Steps run before the command.
    pub step: u32,
    /// Who gave it.
    pub player: Player,
    /// What it is.
    pub command: Command,
}

/// Why a command did nothing, or stopped part way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Refusal {
    /// The node, flag or road belongs to someone else.
    NotYours,
    /// There is no flag on the node.
    NoFlag,
    /// The node does not allow a flag.
    NoRoom,
    /// None of the node's edges has water on both sides.
    NoWater,
    /// The player is drawing a road.
    Drawing,
    /// The player is not drawing a road.
    NotDrawing,
    /// While drawing, a flag goes only on the road's head.
    NotAtHead,
    /// No road runs through the node.
    NoRoad,
    /// The off-road search found no route for the road being drawn.
    NoRoute,
    /// The road being drawn cannot go that way.
    Blocked,
    /// A record pool is full.
    PoolFull,
    /// The interface offers no way to demolish the headquarters or remove its flag.
    Headquarters,
    /// No building stands on the node.
    NoBuilding,
    /// Drawing stopped but the road could not be created.
    Road(RoadError),
    /// The interface never offers that building type.
    NotOffered,
    /// The building is not a finished storehouse.
    NotStorehouse,
    /// A storehouse window has no such ware or job.
    NoSuchStock,
    /// A settings window has no such page, slider, tool or place, or the weight is past
    /// [`crate::MAX_WEIGHT`].
    OutOfRange,
    /// The building is the player's own.
    Own,
    /// More soldiers asked for than the attack window offers, or none.
    NoSoldiers,
    /// No ship has that record.
    NoShip,
    /// The player has no message with that serial.
    NoMessage,
    /// No letter is open for the player.
    NoLetter,
    /// A military building would stand within [`MILITARY_SPACING`] rings of a headquarters or of
    /// another military building.
    TooClose,
}

/// How the icons round the head of the road being drawn show a neighbour (`0x181b9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum StepPreview {
    /// The step check passes.
    Free,
    /// A flag stands there, whoever owns it. A water road shows one only while it has fewer
    /// than [`MAX_WATER_SEGMENTS`] segments.
    Flag,
    /// The step check refuses it, or a water road has no segments left for the flag.
    Blocked,
    /// A building stands there.
    Building,
}

/// A road a player is drawing: the original's road-building mode (`[0x2162b4] + 0xe3`), one per
/// player. The road drawn so far is a real road record without an end flag, rebuilt after every
/// step.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RoadDraft {
    /// Normal or water.
    pub kind: RoadKind,
    /// The flag it starts at.
    pub start: u16,
    /// The nodes stepped onto, the last being the head.
    pub path: Vec<Node>,
    /// The record of the road drawn so far.
    pub road: Option<u16>,
}

impl RoadDraft {
    /// The node the next step leaves from: the last node stepped onto, or the start flag's node.
    #[must_use]
    pub fn head(&self, world: &World) -> Option<Node> {
        self.path
            .last()
            .copied()
            .or_else(|| world.flag(self.start).map(|flag| flag.node))
    }

    /// The direction from the head back along the last segment (`0x2775d`); `None` before the
    /// first step.
    #[must_use]
    pub fn back(&self, world: &World) -> Option<Direction> {
        let (&head, before) = self.path.split_last()?;
        let previous = match before.last() {
            Some(&node) => node,
            None => world.flag(self.start)?.node,
        };
        head.direction_to(previous, world.width(), world.height())
    }
}

/// Whether the road being drawn may step from `from` in `direction` onto `to` (`0x194c9`), with
/// `segments` already drawn.
///
/// A normal road needs walkable ground round `to`, or along the edge, and [`SiteFlags::NO_ROAD`]
/// clear on `to` unless a flag stands there. Ownership is not tested: border nodes rule roads out
/// and a step onto a flag checks its owner. A water road needs water on both sides of the edge,
/// fewer than [`MAX_WATER_SEGMENTS`] segments, and on `to` a flag or else no road, the player's
/// ownership and no border.
#[allow(
    clippy::too_many_arguments,
    reason = "the step check's own inputs, as S2.EXE passes them"
)]
fn step_allowed(
    nodes: &NodeMap,
    world: &World,
    from: Node,
    direction: Direction,
    to: Node,
    kind: RoadKind,
    player: Player,
    segments: usize,
) -> bool {
    let flag = world.flag_at(to).is_some();
    let data = nodes.get(to);
    if kind == RoadKind::Water {
        return nodes.edge_water(from, direction)
            && segments < MAX_WATER_SEGMENTS
            && (flag
                || (data.roads.is_empty()
                    && data.owner == Some(player)
                    && !data.flags.contains(NodeFlags::BORDER)));
    }
    let walkable = nodes
        .triangles_around(to)
        .into_iter()
        .any(|terrain| TerrainClass::of(terrain).walkable());
    if !walkable && !nodes.edge_walkable(from, direction) {
        return false;
    }
    !data.sites.flags.contains(SiteFlags::NO_ROAD) || flag
}

impl Game {
    /// The road `player` is drawing, if any.
    #[must_use]
    pub fn road_draft(&self, player: Player) -> Option<&RoadDraft> {
        self.drafts[player.index()].as_ref()
    }

    /// How the icons round the head show the neighbour in `direction` for `player` now
    /// (`0x181b9`); `None` when the player is not drawing. The segment limit, the flag a click
    /// places on a crossed road and the step back play no part.
    #[must_use]
    pub fn preview_step(&self, player: Player, direction: Direction) -> Option<StepPreview> {
        let draft = self.drafts[player.index()].as_ref()?;
        let head = draft.head(&self.world)?;
        let to = head.neighbour(direction, self.nodes.width(), self.nodes.height());
        let segments = draft.path.len();
        if self.world.flag_at(to).is_some() {
            let room = draft.kind != RoadKind::Water || segments < MAX_WATER_SEGMENTS;
            return Some(if room {
                StepPreview::Flag
            } else {
                StepPreview::Blocked
            });
        }
        if matches!(self.world.object(to), MapObject::Building(_)) {
            return Some(StepPreview::Building);
        }
        let allowed = step_allowed(
            &self.nodes,
            &self.world,
            head,
            direction,
            to,
            draft.kind,
            player,
            segments,
        );
        Some(if allowed {
            StepPreview::Free
        } else {
            StepPreview::Blocked
        })
    }

    /// Carries out `command` for `player` now, between two steps.
    ///
    /// # Errors
    ///
    /// [`Refusal`] when the command does nothing. [`Refusal::Road`] is the one refusal that
    /// follows a change: drawing has stopped and the drawn road is gone.
    pub fn execute(&mut self, player: Player, command: Command) -> Result<(), Refusal> {
        match command {
            Command::PlaceFlag { node, water } => self.place_flag(player, node, water),
            Command::RemoveFlag { node } => self.remove_flag(player, node),
            Command::StartRoad { node, water } => self.start_road(player, node, water),
            Command::ExtendRoad { direction } => self.extend_road(player, direction),
            Command::RouteRoad { node } => self.route_road(player, node),
            Command::CancelRoad => {
                self.drafts[player.index()].as_ref().ok_or(Refusal::NotDrawing)?;
                self.stop_drawing(player);
                Ok(())
            }
            Command::RemoveRoad { node } => self.remove_road(player, node),
            Command::PlaceBuilding { node, kind } => self.place_building(player, node, kind),
            Command::Demolish { node } => {
                let MapObject::Building(record) = self.world.object(node) else {
                    return Err(Refusal::NoBuilding);
                };
                self.demolishable(player, record)?;
                self.demolish(record);
                Ok(())
            }
            Command::SetStock {
                node,
                item,
                setting,
                on,
            } => {
                let known = match item {
                    StockItem::Ware(ware) => ware.index() < crate::world::WARE_TYPES,
                    StockItem::Job(job) => job.is_person(),
                };
                if !known {
                    return Err(Refusal::NoSuchStock);
                }
                let store = self.player_storehouse(player, node)?;
                let settings = match item {
                    StockItem::Ware(ware) => &mut store.ware_settings[ware.index()],
                    StockItem::Job(job) => &mut store.job_settings[job.index()],
                };
                settings.set(setting, on);
                Ok(())
            }
            Command::SetAllStock {
                node,
                people,
                setting,
                on,
            } => {
                let store = self.player_storehouse(player, node)?;
                let settings: &mut [StockSetting] = if people {
                    &mut store.job_settings
                } else {
                    &mut store.ware_settings
                };
                for item in settings {
                    item.set(setting, on);
                }
                Ok(())
            }
            Command::StopProduction { .. }
            | Command::ShipyardMode { .. }
            | Command::SetReserve { .. }
            | Command::SetDistribution { .. }
            | Command::ResetDistribution { .. }
            | Command::SetToolWeight { .. }
            | Command::ResetToolWeights
            | Command::MoveTransport { .. }
            | Command::ResetTransport
            | Command::SetMilitary { .. }
            | Command::ResetMilitary => self.change_economy(player, command),
            Command::SendGeologist { node } => self.send_to_flag(player, node, Job::GEOLOGIST),
            Command::SendScout { node } => self.send_to_flag(player, node, Job::SCOUT),
            Command::Attack {
                node,
                soldiers,
                strongest,
            } => {
                let target = self.world.object(node).building().ok_or(Refusal::NoBuilding)?;
                if self.world.building_owner(target) == Some(player) {
                    return Err(Refusal::Own);
                }
                if soldiers == 0 || soldiers > self.attack_count(player, target) {
                    return Err(Refusal::NoSoldiers);
                }
                self.launch_attack(player, target, soldiers, strongest);
                Ok(())
            }
            Command::PrepareExpedition { .. }
            | Command::SailExpedition { .. }
            | Command::FoundHarbour { .. }
            | Command::CancelExpedition { .. } => self.ship_command(player, command),
            Command::ForgetMessage { .. } | Command::CloseLetter => self.letter_command(player, command),
        }
    }

    /// Forgets a message of `player`'s or closes the campaign letter, which only player 0 is
    /// shown.
    fn letter_command(&mut self, player: Player, command: Command) -> Result<(), Refusal> {
        match command {
            Command::ForgetMessage { serial } => self
                .world
                .forget_message(player, serial)
                .then_some(())
                .ok_or(Refusal::NoMessage),
            _ => self
                .mission
                .as_mut()
                .filter(|_| player == Player::ALL[0])
                .is_some_and(crate::mission::Mission::close_letter)
                .then_some(())
                .ok_or(Refusal::NoLetter),
        }
    }

    /// Carries out one of the economy settings commands for `player`.
    fn change_economy(&mut self, player: Player, command: Command) -> Result<(), Refusal> {
        match command {
            Command::StopProduction { node, stop } => {
                let MapObject::Building(record) = self.world.object(node) else {
                    return Err(Refusal::NoBuilding);
                };
                let building = self.world.building(record).ok_or(Refusal::NoBuilding)?;
                if building.owner != player {
                    return Err(Refusal::NotYours);
                }
                if !self.world.offers_stop(record) {
                    return Err(Refusal::NotOffered);
                }
                if let Some(building) = self.world.building_mut(record) {
                    building.state.set(BuildingState::STOPPED, stop);
                }
                if stop && self.world.military_of(record).is_some() {
                    self.stop_coins(record);
                }
                Ok(())
            }
            Command::ShipyardMode { node, ships } => {
                let MapObject::Building(record) = self.world.object(node) else {
                    return Err(Refusal::NoBuilding);
                };
                let building = self.world.building(record).ok_or(Refusal::NoBuilding)?;
                if building.owner != player {
                    return Err(Refusal::NotYours);
                }
                if building.kind != BuildingKind::SHIPYARD
                    || !building.state.contains(BuildingState::FINISHED)
                {
                    return Err(Refusal::NotOffered);
                }
                if let Some(building) = self.world.building_mut(record) {
                    building.ships = ships;
                }
                Ok(())
            }
            Command::SetReserve { node, rank, wanted } => self.reserve_command(player, node, rank, wanted),
            Command::SetDistribution {
                group,
                consumer,
                weight,
            } => {
                let world = &mut self.world;
                (weight <= MAX_WEIGHT)
                    .then(|| world.set_slider(player, usize::from(group), usize::from(consumer), weight))
                    .flatten()
                    .ok_or(Refusal::OutOfRange)
            }
            Command::ResetDistribution { group } => self
                .world
                .reset_sliders(player, usize::from(group))
                .ok_or(Refusal::OutOfRange),
            Command::SetToolWeight { tool, weight } => {
                let world = &mut self.world;
                (weight <= MAX_WEIGHT)
                    .then(|| world.set_tool_weight(player, tool, weight))
                    .flatten()
                    .ok_or(Refusal::OutOfRange)
            }
            Command::ResetToolWeights => {
                self.world.reset_tool_weights(player);
                Ok(())
            }
            Command::MoveTransport { group, rank } => self
                .world
                .move_transport(player, group, rank)
                .ok_or(Refusal::OutOfRange),
            Command::ResetTransport => {
                self.world.reset_transport(player, WINDOW_LEFTOVER);
                Ok(())
            }
            Command::SetMilitary { setting, value } => {
                let settings = &mut self.world.military_settings[player.index()];
                match settings.get_mut(usize::from(setting)) {
                    Some(slot) if value <= crate::military::MOST_MILITARY => {
                        *slot = value;
                        Ok(())
                    }
                    _ => Err(Refusal::OutOfRange),
                }
            }
            Command::ResetMilitary => {
                self.world.military_settings[player.index()] = self.world.military_tables.settings();
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// The headquarters at `node` keeps `wanted` soldiers of rank `rank` back, for `player`.
    fn reserve_command(&mut self, player: Player, node: Node, rank: Job, wanted: u8) -> Result<(), Refusal> {
        let MapObject::Building(record) = self.world.object(node) else {
            return Err(Refusal::NoBuilding);
        };
        let building = self.world.building(record).ok_or(Refusal::NoBuilding)?;
        if building.owner != player {
            return Err(Refusal::NotYours);
        }
        if building.kind != BuildingKind::HEADQUARTERS {
            return Err(Refusal::NotOffered);
        }
        if wanted > MOST_RESERVED || !Job::RANKS.contains(&rank) {
            return Err(Refusal::OutOfRange);
        }
        self.set_reserve(record, rank, wanted);
        Ok(())
    }

    /// The stock of `player`'s finished storehouse on `node`, whose window the player can open.
    fn player_storehouse(&mut self, player: Player, node: Node) -> Result<&mut Storehouse, Refusal> {
        let store = self.world.player_storehouse(player, node)?;
        self.world.storehouse_mut(store).ok_or(Refusal::NotStorehouse)
    }

    /// Runs the game until the clock has counted `until` steps, carrying out each command in
    /// `log` when the clock reaches its stamp. Commands stamped before the current step are
    /// skipped. Commands with the same stamp run in log order.
    pub fn run(&mut self, log: &[Stamped], until: u32) {
        let begin = self.clock().step();
        let mut pending = log.iter().filter(|c| c.step >= begin).peekable();
        while self.clock().step() < until {
            let now = self.clock().step();
            while let Some(command) = pending.next_if(|c| c.step <= now) {
                let _ = self.execute(command.player, command.command);
            }
            self.step();
        }
    }

    fn place_flag(&mut self, player: Player, node: Node, water: bool) -> Result<(), Refusal> {
        let kind = if water { FlagKind::Water } else { FlagKind::Normal };
        if water
            && !Direction::ALL
                .into_iter()
                .any(|way| self.nodes.edge_water(node, way))
        {
            return Err(Refusal::NoWater);
        }
        if let Some(draft) = &self.drafts[player.index()] {
            if draft.head(&self.world) != Some(node) {
                return Err(Refusal::NotAtHead);
            }
            let owner = self.world.flag(draft.start).map(|flag| flag.owner);
            if owner.is_none() || self.nodes.get(node).owner != owner {
                return Err(Refusal::NotYours);
            }
            let flag = self
                .world
                .place_flag(&mut self.nodes, node, kind, false)
                .ok_or(Refusal::NoRoom)?;
            return self.finish_road(player, flag);
        }
        let data = self.nodes.get(node);
        if data.owner != Some(player) {
            return Err(Refusal::NotYours);
        }
        if data.sites.flags.contains(SiteFlags::NO_FLAG) {
            return Err(Refusal::NoRoom);
        }
        self.world
            .place_flag(&mut self.nodes, node, kind, true)
            .map(|_| ())
            .ok_or(Refusal::PoolFull)
    }

    fn place_building(&mut self, player: Player, node: Node, kind: BuildingKind) -> Result<(), Refusal> {
        let row = self
            .world
            .building_types()
            .get(kind)
            .filter(|_| kind != BuildingKind::HEADQUARTERS && self.may_build(player, kind))
            .ok_or(Refusal::NotOffered)?;
        if self.drafts[player.index()].is_some() {
            return Err(Refusal::Drawing);
        }
        let data = self.nodes.get(node);
        if data.owner != Some(player) {
            return Err(Refusal::NotYours);
        }
        if !fits(row.size, data.sites.size) {
            return Err(Refusal::NoRoom);
        }
        if kind == BuildingKind::HARBOUR && !data.flags.contains(NodeFlags::HARBOUR) {
            return Err(Refusal::NotOffered);
        }
        if row.category == Category::Military && !self.military_room(player, node) {
            return Err(Refusal::TooClose);
        }
        self.world
            .place_site(&mut self.nodes, node, kind, player)
            .map(|_| ())
            .ok_or(Refusal::PoolFull)
    }

    /// Whether a military building fits on `node` (`0x197a6`), as [`World::military_room`].
    #[must_use]
    pub fn military_room(&self, player: Player, node: Node) -> bool {
        self.world.military_room(&self.nodes, player, node)
    }

    /// Orders a geologist or scout, `job`, to `player`'s flag on `node`.
    fn send_to_flag(&mut self, player: Player, node: Node, job: Job) -> Result<(), Refusal> {
        let flag = self.world.flag_at(node).ok_or(Refusal::NoFlag)?;
        if self.world.flag(flag).map(|flag| flag.owner) != Some(player) {
            return Err(Refusal::NotYours);
        }
        self.send_explorer(flag, job).ok_or(Refusal::PoolFull)?;
        Ok(())
    }

    /// Carries out one of the harbour and ship window commands for `player`.
    fn ship_command(&mut self, player: Player, command: Command) -> Result<(), Refusal> {
        match command {
            Command::PrepareExpedition { node } => {
                let MapObject::Building(record) = self.world.object(node) else {
                    return Err(Refusal::NoBuilding);
                };
                let building = self.world.building(record).ok_or(Refusal::NoBuilding)?;
                if building.owner != player {
                    return Err(Refusal::NotYours);
                }
                let preparing = self
                    .world
                    .harbour_of(record)
                    .and_then(|harbour| self.world.harbour(harbour))
                    .map(|harbour| harbour.expedition);
                if preparing != Some(false) {
                    return Err(Refusal::NotOffered);
                }
                self.prepare_expedition(record);
                Ok(())
            }
            Command::SailExpedition { ship, sector } => {
                let record = self.expedition_ship(player, ship)?;
                let offered =
                    !record.sailing && record.sectors.get(usize::from(sector)).copied().unwrap_or(false);
                if !offered {
                    return Err(Refusal::NotOffered);
                }
                self.sail_expedition(ship, usize::from(sector));
                Ok(())
            }
            Command::FoundHarbour { ship } => {
                let record = self.expedition_ship(player, ship)?;
                if record.sailing || !record.unclaimed {
                    return Err(Refusal::NotOffered);
                }
                self.found_from_ship(ship);
                Ok(())
            }
            Command::CancelExpedition { ship } => {
                self.expedition_ship(player, ship)?;
                self.cancel_expedition(ship);
                Ok(())
            }
            _ => Err(Refusal::NotOffered),
        }
    }

    /// Ship `ship` of `player`, which must be on an expedition for its window to offer the
    /// expedition's buttons (`0x670a0`).
    fn expedition_ship(&self, player: Player, ship: u16) -> Result<crate::ships::Ship, Refusal> {
        let record = *self.world.ship(ship).ok_or(Refusal::NoShip)?;
        if record.owner != player {
            return Err(Refusal::NotYours);
        }
        if !matches!(record.mode, crate::ships::ShipMode::Expedition { .. }) {
            return Err(Refusal::NotOffered);
        }
        Ok(record)
    }

    /// Whether `player` may demolish building `record`: theirs, and not the headquarters
    /// (`0x5a46b`).
    fn demolishable(&self, player: Player, record: u16) -> Result<(), Refusal> {
        let building = self.world.building(record).ok_or(Refusal::NoBuilding)?;
        if building.owner != player {
            return Err(Refusal::NotYours);
        }
        if building.kind == BuildingKind::HEADQUARTERS {
            return Err(Refusal::Headquarters);
        }
        Ok(())
    }

    fn remove_flag(&mut self, player: Player, node: Node) -> Result<(), Refusal> {
        let record = self.world.flag_at(node).ok_or(Refusal::NoFlag)?;
        if self.world.flag(record).map(|flag| flag.owner) != Some(player) {
            return Err(Refusal::NotYours);
        }
        let building = node.neighbour(Direction::NorthWest, self.nodes.width(), self.nodes.height());
        if let MapObject::Building(building) = self.world.object(building) {
            self.demolishable(player, building)?;
        }
        self.remove_flag_record(record);
        Ok(())
    }

    /// Removes flag `record` (`0x25763`): road drawing from it stops, the building at it is
    /// demolished, its roads go, the wares waiting at it are freed, the settlers sent to it are
    /// called off, everyone on its node is lost, and geologists and scouts working from it are
    /// lost.
    pub(crate) fn remove_flag_record(&mut self, record: u16) {
        let Some(node) = self.world.flag(record).map(|flag| flag.node) else {
            return;
        };
        let building = node.neighbour(Direction::NorthWest, self.nodes.width(), self.nodes.height());
        if let MapObject::Building(building) = self.world.object(building) {
            self.stop_drafts_at(record);
            self.demolish(building);
        }
        self.remove_flag_only(record);
    }

    /// Removes flag `record` as [`Self::remove_flag_record`] does, leaving the building at it
    /// standing (`0x25763` with the building kept).
    pub(crate) fn remove_flag_only(&mut self, record: u16) {
        self.stop_drafts_at(record);
        let roads = self.world.flag(record).map(|flag| flag.links).unwrap_or_default();
        for link in roads.into_iter().flatten() {
            self.remove_road_record(link.road);
        }
        self.release_wares(record);
        self.cancel_sent(record);
        self.sweep_flag_node(record);
        self.world.remove_flag(&mut self.nodes, record);
        self.lose_explorers(record);
    }

    /// Every player drawing a road from flag `record` stops drawing.
    fn stop_drafts_at(&mut self, record: u16) {
        for index in 0..Player::COUNT {
            if self.drafts[index]
                .as_ref()
                .is_some_and(|draft| draft.start == record)
                && let Some(drawer) = u8::try_from(index).ok().and_then(Player::new)
            {
                self.stop_drawing(drawer);
            }
        }
    }

    fn start_road(&mut self, player: Player, node: Node, water: bool) -> Result<(), Refusal> {
        if self.drafts[player.index()].is_some() {
            return Err(Refusal::Drawing);
        }
        if self.world.roads().count() >= MAX_ROADS {
            return Err(Refusal::PoolFull);
        }
        let start = self.world.flag_at(node).ok_or(Refusal::NoFlag)?;
        if self.world.flag(start).map(|flag| flag.owner) != Some(player) {
            return Err(Refusal::NotYours);
        }
        self.drafts[player.index()] = Some(RoadDraft {
            kind: if water { RoadKind::Water } else { RoadKind::Normal },
            start,
            path: Vec::new(),
            road: None,
        });
        Ok(())
    }

    /// A click next to the head (`0x27c97`), then the step (`0x27631`).
    fn extend_road(&mut self, player: Player, direction: Direction) -> Result<(), Refusal> {
        let draft = self.drafts[player.index()].as_ref().ok_or(Refusal::NotDrawing)?;
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let head = draft.head(&self.world).ok_or(Refusal::NotDrawing)?;
        let to = head.neighbour(direction, width, height);
        if let Some(crossing) = self.world.road_through(&self.nodes, to)
            && draft.road != Some(crossing)
            && !self.nodes.get(to).sites.flags.contains(SiteFlags::NO_FLAG)
        {
            self.world.place_flag(&mut self.nodes, to, FlagKind::Normal, true);
        }
        self.step_road(player, direction)
    }

    /// A click further from the head (`0x27a51`). The route's draws change the game's generator,
    /// so once a route is found the command succeeds whether or not a step is kept.
    fn route_road(&mut self, player: Player, node: Node) -> Result<(), Refusal> {
        let draft = self.drafts[player.index()].as_ref().ok_or(Refusal::NotDrawing)?;
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let head = draft.head(&self.world).ok_or(Refusal::NotDrawing)?;
        if let Some(direction) = head.direction_to(node, width, height) {
            return self.extend_road(player, direction);
        }
        if node == head {
            return Err(Refusal::NoRoute);
        }
        if self.world.paths.len() >= MAX_PATHS {
            return Err(Refusal::PoolFull);
        }
        let kind = draft.kind;
        let route = self.road_route(head, node, kind).ok_or(Refusal::NoRoute)?;
        for direction in route {
            let Some(draft) = self.drafts[player.index()].as_ref() else {
                break;
            };
            let Some(head) = draft.head(&self.world) else {
                break;
            };
            if head == node {
                break;
            }
            let next = head.neighbour(direction, width, height);
            let allowed = step_allowed(
                &self.nodes,
                &self.world,
                head,
                direction,
                next,
                draft.kind,
                player,
                draft.path.len(),
            );
            if !allowed {
                if next == node
                    && self.world.road_through(&self.nodes, next).is_some()
                    && !self.nodes.get(next).sites.flags.contains(SiteFlags::NO_FLAG)
                {
                    let _ = self.extend_road(player, direction);
                }
                break;
            }
            let _ = self.step_road(player, direction);
            let moved = self.drafts[player.index()]
                .as_ref()
                .and_then(|draft| draft.head(&self.world));
            if moved != Some(next) {
                break;
            }
        }
        Ok(())
    }

    /// One step of the road being drawn (`0x27631`).
    fn step_road(&mut self, player: Player, direction: Direction) -> Result<(), Refusal> {
        let (width, height) = (self.nodes.width(), self.nodes.height());
        let Some(draft) = self.drafts[player.index()].as_mut() else {
            return Err(Refusal::NotDrawing);
        };
        let head = draft.head(&self.world).ok_or(Refusal::NotDrawing)?;
        let to = head.neighbour(direction, width, height);
        if draft.back(&self.world) == Some(direction) {
            draft.path.pop();
            self.redraw(player);
            return Ok(());
        }
        let (kind, segments) = (draft.kind, draft.path.len());
        if !step_allowed(
            &self.nodes,
            &self.world,
            head,
            direction,
            to,
            kind,
            player,
            segments,
        ) || segments >= MAX_SEGMENTS
        {
            return Err(Refusal::Blocked);
        }
        draft.path.push(to);
        if let Some(flag) = self.world.flag_at(to) {
            if self.world.flag(flag).map(|flag| flag.owner) == Some(player) {
                return self.finish_road(player, flag);
            }
            draft.path.pop();
            return Err(Refusal::NotYours);
        }
        self.redraw(player);
        Ok(())
    }

    /// Replaces the road drawn so far with one along the current path (`0x27839`).
    fn redraw(&mut self, player: Player) {
        let Some(draft) = self.drafts[player.index()].as_mut() else {
            return;
        };
        if let Some(road) = draft.road.take() {
            self.world.remove_road(&mut self.nodes, road);
        }
        if !draft.path.is_empty() {
            draft.road = self
                .world
                .create_road(&mut self.nodes, draft.kind, draft.start, None, draft.path.clone())
                .ok();
        }
    }

    /// Leaves the road-building mode and removes the road drawn so far (`0x275cd`).
    fn stop_drawing(&mut self, player: Player) {
        if let Some(draft) = self.drafts[player.index()].take()
            && let Some(road) = draft.road
        {
            self.world.remove_road(&mut self.nodes, road);
        }
    }

    /// Stops drawing and creates the road from its start flag to `end`, whose node is the last
    /// one on the path (`0x275ed`).
    fn finish_road(&mut self, player: Player, end: u16) -> Result<(), Refusal> {
        let Some(mut draft) = self.drafts[player.index()].clone() else {
            return Err(Refusal::NotDrawing);
        };
        self.stop_drawing(player);
        if draft.path.pop().is_none() {
            return Ok(());
        }
        self.world
            .create_road(&mut self.nodes, draft.kind, draft.start, Some(end), draft.path)
            .map(|_| ())
            .map_err(Refusal::Road)
    }

    fn remove_road(&mut self, player: Player, node: Node) -> Result<(), Refusal> {
        if self.drafts[player.index()].is_some() {
            return Err(Refusal::Drawing);
        }
        let record = self
            .world
            .road_through(&self.nodes, node)
            .ok_or(Refusal::NoRoad)?;
        let start = self.world.road(record).map(|road| road.start);
        if start
            .and_then(|flag| self.world.flag(flag))
            .map(|flag| flag.owner)
            != Some(player)
        {
            return Err(Refusal::NotYours);
        }
        self.remove_road_record(record);
        Ok(())
    }
}

#[cfg(test)]
mod tests;

impl World {
    /// Whether a military building fits on `node` (`0x197a6`): no headquarters of any owner,
    /// no military building of `player` and no finished military building of another player
    /// within [`MILITARY_SPACING`] rings.
    #[must_use]
    pub fn military_room(&self, nodes: &NodeMap, player: Player, node: Node) -> bool {
        let (width, height) = (nodes.width(), nodes.height());
        !(1..=MILITARY_SPACING)
            .flat_map(|radius| node.ring(radius, width, height))
            .any(|at| {
                let MapObject::Building(record) = self.object(at) else {
                    return false;
                };
                let Some(building) = self.building(record) else {
                    return false;
                };
                let military = self
                    .building_types()
                    .get(building.kind)
                    .is_some_and(|row| row.category == Category::Military);
                if !military {
                    return building.kind == BuildingKind::HEADQUARTERS;
                }
                building.owner == player || building.state.contains(BuildingState::FINISHED)
            })
    }
}
