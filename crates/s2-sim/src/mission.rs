//! A campaign chapter's mission script at run time: the start-up commands, the event records and
//! the list of events being watched, the check that fires them every 20 steps, the letters they
//! open and the building types each player may build.

use std::borrow::Cow;

use s2_formats::rtx::{AmountEvent, Op, PositionEvent, Section, SimpleEvent};

use crate::figures::{Figure, Species};
use crate::game::Game;
use crate::grid::Node;
use crate::job::Job;
use crate::messages::MessageId;
use crate::pool::Pool;
use crate::ware::WareKind;
use crate::world::{BUILDING_TYPES, BuildingKind, FINDS, Player, WARE_TYPES};

/// How many events the active list holds (`0x215254`).
pub const ACTIVE_SLOTS: usize = 40;

/// How many event records the pool holds at most (`0x252a7`).
const MOST_EVENTS: usize = 100;

/// How many chapter flags `MISSION.DAT` holds; a final event opens only chapters below this.
pub const CHAPTER_FLAGS: usize = 30;

/// How many computer barriers a script can set (`0x1f798d`).
pub const BARRIERS: usize = 10;

/// The byte an event id, `next` or building slot holds for "none".
const NONE_BYTE: u8 = u8::MAX;

/// The word a text slot holds for "none".
const NONE_WORD: u16 = u16::MAX;

/// The kind byte of `MET_FOUND_GOLD`, the first of the five found words' kinds.
const FIRST_FIND: u8 = 9;

/// The second chapter's mission number.
const SECOND_CHAPTER: u16 = 1;

/// What the campaign start adds to the mission number for the map's file number (`0x5eb6d`).
const MAP_OFFSET: u16 = 200;

/// The data path of mission `number`'s script, as `data\missions\mis_%4.4d.rtx` names it.
#[must_use]
pub fn script_path(number: u16) -> String {
    format!("DATA/MISSIONS/MIS_{number:04}.RTX")
}

/// The data path of mission `number`'s map, as `data\maps\miss%3.3d.wld` names it with the
/// number plus 200.
#[must_use]
pub fn map_path(number: u16) -> String {
    format!("DATA/MAPS/MISS{:03}.WLD", number.saturating_add(MAP_OFFSET))
}

/// A building type the computer players never place in one mission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComputerBan {
    /// The mission number.
    pub mission: u16,
    /// The building type.
    pub kind: BuildingKind,
}

/// The rules missions play by besides their scripts, which a mod can replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionTables {
    computer_bans: Cow<'static, [ComputerBan]>,
}

impl MissionTables {
    /// The rules of `S2.EXE`: in the second chapter the computer players never place a
    /// catapult (`0x729c7`).
    pub const ORIGINAL: Self = Self {
        computer_bans: Cow::Borrowed(&[ComputerBan {
            mission: SECOND_CHAPTER,
            kind: BuildingKind::CATAPULT,
        }]),
    };

    /// Tables holding `computer_bans`.
    #[must_use]
    pub const fn new(computer_bans: Vec<ComputerBan>) -> Self {
        Self {
            computer_bans: Cow::Owned(computer_bans),
        }
    }

    /// The building types the computer players never place, mission by mission.
    #[must_use]
    pub fn computer_bans(&self) -> &[ComputerBan] {
        &self.computer_bans
    }

    /// The building types mission `number`'s computer players never place, by type.
    fn banned(&self, number: u16) -> [bool; BUILDING_TYPES] {
        let mut banned = [false; BUILDING_TYPES];
        for ban in self.computer_bans.iter().filter(|ban| ban.mission == number) {
            if let Some(slot) = banned.get_mut(usize::from(ban.kind.0)) {
                *slot = true;
            }
        }
        banned
    }
}

impl Default for MissionTables {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

/// A letter a fired event opened in the diary window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Letter {
    /// Its slot in the mission's text table.
    pub text: u16,
    /// The event's second text word, which S2 keeps on the window and never reads.
    pub page: Option<u16>,
    /// Whether the chapter's final event opened it, which adds the button that ends the game.
    pub closing: bool,
}

/// One of the circles the computer players place no military building in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Barrier {
    /// How far from the centre the circle reaches.
    pub radius: u16,
    /// The centre's column.
    pub x: u16,
    /// The centre's row.
    pub y: u16,
}

/// What an event waits for, with the values its defining keyword stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
enum Trigger {
    /// Kind 0, `MET_END_EVENT`: enough of the events naming it as `next` have fired.
    End { counter: u8, times: u8 },
    /// Kind 1: player 0 owns the node.
    Occupied { x: u16, y: u16 },
    /// Kind 2: the node is visible to player 0.
    Explored { x: u16, y: u16 },
    /// Kind 3: player 0 owns at least `amount` of the ware.
    Wares { ware: u8, amount: u16 },
    /// Kind 4: player 0 has at least `amount` finished buildings of the type.
    Buildings { kind: u8, amount: u16 },
    /// Kind 5: player 0's land is at least `size` nodes.
    Land { size: u16 },
    /// Kind 6: player 0 holds at least `amount` people of the job.
    People { job: u8, amount: u16 },
    /// Kinds 9 to 13: player 0's geologists have reported the find with this word.
    Found { word: usize },
    /// Kind 14: at once.
    Direct,
    /// Kind 16: writes a decoration to the node at once, without firing.
    Element { x: u16, y: u16, element: u16 },
    /// Kind 17: player 0 has seen the player's land.
    Contact { player: u16 },
}

impl Trigger {
    /// The kind byte `S2.EXE` keeps at `+0x01`.
    fn kind(self) -> u8 {
        match self {
            Self::End { .. } => 0,
            Self::Occupied { .. } => 1,
            Self::Explored { .. } => 2,
            Self::Wares { .. } => 3,
            Self::Buildings { .. } => 4,
            Self::Land { .. } => 5,
            Self::People { .. } => 6,
            Self::Found { word } => FIRST_FIND.saturating_add(u8::try_from(word).unwrap_or(u8::MAX)),
            Self::Direct => 14,
            Self::Element { .. } => 16,
            Self::Contact { .. } => 17,
        }
    }
}

/// An event record in use, as `S2.EXE` holds the fields a trace compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventRecord {
    /// The record's number in the pool.
    pub record: u16,
    /// The event's id.
    pub id: u8,
    /// The kind byte `S2.EXE` keeps at `+0x01`, which names what fires the event.
    pub kind: u8,
    /// The letter's text slot, if it has a letter.
    pub text: Option<u16>,
    /// An end event's counter.
    pub counter: Option<u8>,
}

/// An event record (`[0x2016e0]`, 31 bytes in `S2.EXE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
struct Event {
    id: u8,
    trigger: Trigger,
    text: Option<u16>,
    page: Option<u16>,
    next: Option<u8>,
    activates: [Option<u8>; 4],
    enables: [Option<u8>; 6],
    /// The three chapter flags a final event opens, `None` for an event that is not final.
    chapters: Option<[u16; 3]>,
}

impl Event {
    /// A new record (`0x31afa`): no letter, no buildings, not final.
    const fn new(id: u8, trigger: Trigger, next: Option<u8>) -> Self {
        Self {
            id,
            trigger,
            text: None,
            page: None,
            next,
            activates: [None; 4],
            enables: [None; 6],
            chapters: None,
        }
    }

    const fn with_letter(mut self, text: Option<u16>, page: Option<u16>) -> Self {
        self.text = text;
        self.page = page;
        self
    }
}

/// A running mission: its script's events, the active list, the open letter and the building
/// types each player may build.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Mission {
    number: u16,
    texts: Option<u16>,
    events: Pool<Event>,
    /// [`ACTIVE_SLOTS`] slots, each a record number or `None`.
    active: Vec<Option<u8>>,
    letter: Option<Letter>,
    won: bool,
    opened: Vec<u16>,
    allowed: [[bool; BUILDING_TYPES]; Player::COUNT],
    barriers: Vec<Barrier>,
    computer_bans: [bool; BUILDING_TYPES],
}

impl Mission {
    /// Mission `number` before its script has run: no events, nothing allowed, and the computer
    /// players kept from the building types `tables` bans for it.
    #[must_use]
    pub fn new(number: u16, tables: &MissionTables) -> Self {
        Self {
            number,
            texts: None,
            events: Pool::new(),
            active: vec![None; ACTIVE_SLOTS],
            letter: None,
            won: false,
            opened: Vec::new(),
            allowed: [[false; BUILDING_TYPES]; Player::COUNT],
            barriers: Vec::new(),
            computer_bans: tables.banned(number),
        }
    }

    /// The mission's number, which picks its script and some of its rules.
    #[must_use]
    pub const fn number(&self) -> u16 {
        self.number
    }

    /// The mission text table `LOAD_MISSION_TEXTS` chose (`0x1f7901`), if it ran.
    #[must_use]
    pub const fn texts(&self) -> Option<u16> {
        self.texts
    }

    /// The letter on screen, if one is open.
    #[must_use]
    pub const fn letter(&self) -> Option<Letter> {
        self.letter
    }

    /// Whether the chapter's final event has fired (`0x1f7903`).
    #[must_use]
    pub const fn won(&self) -> bool {
        self.won
    }

    /// The chapter flags the final event opened, below [`CHAPTER_FLAGS`], in the order it named
    /// them. Writing them to `MISSION.DAT` is the caller's job.
    #[must_use]
    pub fn opened(&self) -> &[u16] {
        &self.opened
    }

    /// Whether `player` may build `kind` (`0x20174c + 0x2745·player + 2·type`).
    #[must_use]
    pub fn allowed(&self, player: Player, kind: BuildingKind) -> bool {
        self.allowed[player.index()]
            .get(usize::from(kind.0))
            .copied()
            .unwrap_or_default()
    }

    /// The computer barriers the script set, in order.
    #[must_use]
    pub fn barriers(&self) -> &[Barrier] {
        &self.barriers
    }

    /// The event ids on the active list, slot by slot.
    pub fn active(&self) -> impl Iterator<Item = Option<u8>> + '_ {
        self.active
            .iter()
            .map(|slot| slot.and_then(|record| self.events.get(record.into()).map(|event| event.id)))
    }

    /// The active list as `S2.EXE` holds it (`0x215254`): a record number or `None` in each slot.
    #[must_use]
    pub fn active_records(&self) -> &[Option<u8>] {
        &self.active
    }

    /// Every event record in use, by record number.
    pub fn records(&self) -> impl Iterator<Item = EventRecord> + '_ {
        self.events.iter().map(|(record, event)| EventRecord {
            record,
            id: event.id,
            kind: event.trigger.kind(),
            text: event.text,
            counter: match event.trigger {
                Trigger::End { counter, .. } => Some(counter),
                _ => None,
            },
        })
    }

    /// Whether an event with `id` is defined and has not fired yet.
    #[must_use]
    pub fn waiting(&self, id: u8) -> bool {
        self.find(id).is_some()
    }

    /// The record of the lowest event in use with `id` (`0x31bc7`).
    fn find(&self, id: u8) -> Option<u8> {
        self.events
            .iter()
            .find(|(_, event)| event.id == id)
            .and_then(|(record, _)| u8::try_from(record).ok())
    }

    /// Defines an event in the lowest free record (`0x31afa`). An id of 255 marks the record
    /// free, which leaves the pool as it was. With the pool full, `S2.EXE` writes the event past
    /// its end; here nothing is defined.
    fn define(&mut self, event: Event) {
        if event.id != NONE_BYTE {
            let _ = self.events.insert(event, MOST_EVENTS);
        }
    }

    /// Puts the event with `id` on the active list, in the lowest empty slot (`0x31b7d`). A
    /// record already on the list goes on again; an unknown id or a full list adds nothing.
    fn activate(&mut self, id: u8) {
        let Some(record) = self.find(id) else {
            return;
        };
        if let Some(slot) = self.active.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(record);
        }
    }

    /// Whether a letter is open, which holds back every other event with a letter (`0x454d0`).
    const fn letter_open(&self) -> bool {
        self.letter.is_some()
    }

    /// Closes the open letter.
    pub(crate) fn close_letter(&mut self) -> bool {
        self.letter.take().is_some()
    }

    /// Allows or forbids `kind` for every player.
    fn allow_all(&mut self, kind: u8, on: bool) {
        for row in &mut self.allowed {
            if let Some(allowed) = row.get_mut(usize::from(kind)) {
                *allowed = on;
            }
        }
    }
}

/// The low 16 bits of a script argument, as `S2.EXE` keeps most of them.
fn word(value: i32) -> u16 {
    let [low, high, ..] = value.to_le_bytes();
    u16::from_le_bytes([low, high])
}

/// The low 8 bits of a script argument.
fn byte(value: i32) -> u8 {
    value.to_le_bytes()[0]
}

/// A byte argument where 255 means none.
fn some_byte(value: i32) -> Option<u8> {
    Some(byte(value)).filter(|&value| value != NONE_BYTE)
}

/// A text slot argument where -1 means none.
fn text(value: i32) -> Option<u16> {
    Some(word(value)).filter(|&value| value != NONE_WORD)
}

/// A node from a script's coordinates, which `S2.EXE` reads as signed words and wraps onto the
/// map.
fn spot(x: u16, y: u16, (width, height): (u16, u16)) -> Node {
    let signed = |value: u16| i32::from(i16::from_le_bytes(value.to_le_bytes()));
    Node::wrapped(signed(x), signed(y), width, height)
}

/// The record a position keyword defines.
fn position(event: &PositionEvent, explored: bool) -> Event {
    let (x, y) = (word(event.pos.x), word(event.pos.y));
    let trigger = if explored {
        Trigger::Explored { x, y }
    } else {
        Trigger::Occupied { x, y }
    };
    Event::new(byte(event.event), trigger, some_byte(event.next))
        .with_letter(text(event.text), text(event.text2))
}

/// The record a ware, building or people keyword defines.
fn amount(event: &AmountEvent, trigger: fn(u8, u16) -> Trigger) -> Event {
    Event::new(
        byte(event.event),
        trigger(byte(event.kind), word(event.amount)),
        some_byte(event.next),
    )
    .with_letter(text(event.text), text(event.text2))
}

/// The record a find or direct keyword defines.
fn simple(event: &SimpleEvent, trigger: Trigger) -> Event {
    Event::new(byte(event.event), trigger, some_byte(event.next))
        .with_letter(text(event.text), text(event.text2))
}

/// The record a defining keyword makes, or `None` for any other keyword.
fn defined(op: &Op) -> Option<Event> {
    Some(match *op {
        Op::MetPositionOccupied(event) => position(&event, false),
        Op::MetPositionExplored(event) => position(&event, true),
        Op::MetEndEvent {
            event,
            next,
            times,
            activate,
            text: letter,
            text2,
        } => {
            let mut record = Event::new(
                byte(event),
                Trigger::End {
                    counter: 0,
                    times: byte(times),
                },
                some_byte(next),
            )
            .with_letter(text(letter), text(text2));
            record.activates = activate.map(some_byte);
            record
        }
        Op::MetHouseAmount(event) => amount(&event, |kind, amount| Trigger::Buildings { kind, amount }),
        Op::MetPeopleAmount(event) => amount(&event, |job, amount| Trigger::People { job, amount }),
        Op::MetWareAmount(event) => amount(&event, |ware, amount| Trigger::Wares { ware, amount }),
        Op::MetLandSize {
            event,
            next,
            size,
            text: letter,
            text2,
        } => Event::new(byte(event), Trigger::Land { size: word(size) }, some_byte(next))
            .with_letter(text(letter), text(text2)),
        Op::MetContactToPlayer {
            event,
            next,
            player,
            text: letter,
            text2,
        } => Event::new(
            byte(event),
            Trigger::Contact { player: word(player) },
            some_byte(next),
        )
        .with_letter(text(letter), text(text2)),
        Op::MetFoundGold(event) => simple(&event, Trigger::Found { word: 0 }),
        Op::MetFoundIron(event) => simple(&event, Trigger::Found { word: 1 }),
        Op::MetFoundCoal(event) => simple(&event, Trigger::Found { word: 2 }),
        Op::MetFoundGranite(event) => simple(&event, Trigger::Found { word: 3 }),
        Op::MetFoundWater(event) => simple(&event, Trigger::Found { word: 4 }),
        Op::MetDirectEvent(event) => simple(&event, Trigger::Direct),
        Op::MetSetMapElement { event, pos, element } => Event::new(
            byte(event),
            Trigger::Element {
                x: word(pos.x),
                y: word(pos.y),
                element: word(element),
            },
            None,
        ),
        _ => return None,
    })
}

impl Game {
    /// The running mission, if the game is a campaign chapter.
    #[must_use]
    pub fn mission(&self) -> Option<&Mission> {
        self.mission.as_ref()
    }

    /// Whether `player` may build `kind`: always outside a campaign chapter, and inside one only
    /// once the script has allowed it.
    #[must_use]
    pub fn may_build(&self, player: Player, kind: BuildingKind) -> bool {
        self.mission
            .as_ref()
            .is_none_or(|mission| mission.allowed(player, kind))
    }

    /// Whether a computer player may not place a military building on `node` (`0x68abc`): its
    /// distance to a barrier's centre, `⌊√(dx² + dy²)⌋` without wrapping round the map
    /// (`0x227b0`), is below the barrier's radius.
    pub(crate) fn in_barrier(&self, node: Node) -> bool {
        self.mission.as_ref().is_some_and(|mission| {
            mission.barriers.iter().any(|barrier| {
                let dx = u32::from(barrier.x.abs_diff(node.x));
                let dy = u32::from(barrier.y.abs_diff(node.y));
                dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)).isqrt() < u32::from(barrier.radius)
            })
        })
    }

    /// Whether the mission keeps the computer players from placing `kind`.
    pub(crate) fn computers_never_build(&self, kind: BuildingKind) -> bool {
        self.mission.as_ref().is_some_and(|mission| {
            mission
                .computer_bans
                .get(usize::from(kind.0))
                .copied()
                .unwrap_or(false)
        })
    }

    /// Reads what of `[GLOBAL_MAP_SETTINGS]` outlasts the world being built: the text table, the
    /// computer barriers and the alliances. The mission screen runs the section before the map
    /// loads (`0x5ebd3`), so anything else it might set is cleared by the load or, for the
    /// computer players, read by [`crate::PlayerSlots::campaign`].
    pub(crate) fn read_settings(&mut self, section: &Section) {
        let mut acting = Player::ALL[0];
        for command in &section.commands {
            if matches!(
                command.op,
                Op::LoadMissionTexts { .. }
                    | Op::GlobalSetComputerBarrier { .. }
                    | Op::GlobalSetComputerAlliance { .. }
            ) {
                self.run_op(&command.op, &mut acting);
            }
        }
    }

    /// Runs a script section the way the interpreter `0x23ebe` does, line by line from player
    /// 0. Keywords that only matter before the world is built (the map, the object sets, the
    /// computer players and the free-play settings) do nothing here: the game's start reads them
    /// from the script before building it.
    pub(crate) fn run_section(&mut self, section: &Section) {
        let mut acting = Player::ALL[0];
        for command in &section.commands {
            self.run_op(&command.op, &mut acting);
        }
    }

    fn run_op(&mut self, op: &Op, acting: &mut Player) {
        let size = (self.nodes.width(), self.nodes.height());
        let Some(mission) = self.mission.as_mut() else {
            return;
        };
        if let Some(event) = defined(op) {
            mission.define(event);
            return;
        }
        match *op {
            Op::LoadMissionTexts { number } => mission.texts = text(number),
            Op::SetActPlayer { player } => {
                if let Some(player) = Player::new(byte(player)) {
                    *acting = player;
                }
            }
            Op::SetHouse { building, pos } => {
                let node = spot(word(pos.x), word(pos.y), size);
                self.script_building(node, BuildingKind(byte(building)), *acting);
            }
            Op::AddWare { ware, amount } => {
                self.stock_headquarters(*acting, Stock::Ware(byte(ware)), word(amount));
            }
            Op::AddPeople { job, amount } => {
                self.stock_headquarters(*acting, Stock::Job(byte(job)), word(amount));
            }
            Op::EnableHouse { building } => mission.allow_all(byte(building), true),
            Op::DisableHouse { building } => mission.allow_all(byte(building), false),
            Op::EnableAllHouses => mission.allowed = [[true; BUILDING_TYPES]; Player::COUNT],
            Op::AddAnimal { species, amount, pos } => {
                let node = spot(word(pos.x), word(pos.y), size);
                if let Some(species) = Species::from_index(byte(species)) {
                    for _ in 0..word(amount) {
                        self.world.add_figure(node, Figure::animal(species));
                    }
                }
            }
            Op::GlobalSetComputerBarrier { radius, pos } => {
                if word(radius) != 0 && mission.barriers.len() < BARRIERS {
                    mission.barriers.push(Barrier {
                        radius: word(radius),
                        x: word(pos.x),
                        y: word(pos.y),
                    });
                }
            }
            Op::GlobalSetComputerAlliance { player, ally } => {
                if let (Some(player), Some(ally)) = (Player::new(byte(player)), Player::new(byte(ally))) {
                    self.world.allies[player.index()][ally.index()] = true;
                }
            }
            Op::MetClearActEvents => mission.active.fill(None),
            Op::MetAddAsActEvent { event } => mission.activate(byte(event)),
            Op::MetHouseEnabling { event, buildings } => {
                if let Some(record) = mission.find(byte(event))
                    && let Some(event) = mission.events.get_mut(record.into())
                {
                    event.enables = buildings.map(some_byte);
                }
            }
            Op::MetSetFinalEvent { event, chapter, rest } => {
                if let Some(record) = mission.find(byte(event))
                    && let Some(event) = mission.events.get_mut(record.into())
                {
                    event.chapters = Some([word(chapter), word(rest[0]), word(rest[1])]);
                }
            }
            _ => {}
        }
    }

    /// `SET_HOUSE`: a headquarters on ground that takes one, with an empty stock, or any other
    /// type as a construction site where slot 8 offers its size (`0x26221`). Neither checks who
    /// owns the node, and a type the table does not have, or a placement that fails, places
    /// nothing.
    fn script_building(&mut self, node: Node, kind: BuildingKind, owner: Player) {
        if kind == BuildingKind::HEADQUARTERS {
            if self.world.headquarters_fit(&self.nodes, node) {
                self.world
                    .place_headquarters_with(&mut self.nodes, node, owner, None);
            }
            return;
        }
        let Some(row) = self.world.building_types().get(kind) else {
            return;
        };
        if crate::construction::fits(row.size, self.nodes.get(node).sites.size) {
            let _ = self.world.place_site(&mut self.nodes, node, kind, owner);
        }
    }

    /// `ADD_WARE` and `ADD_PEOPLE`: `amount` of `stock` into every headquarters `player` owns,
    /// in record order, counted for the player. Wares count as arrived before the game began,
    /// so none shows as production. A ware or job past the stock is left out; `S2.EXE` writes
    /// past the records.
    fn stock_headquarters(&mut self, player: Player, stock: Stock, amount: u16) {
        let headquarters: Vec<u16> = self
            .world
            .buildings()
            .filter(|(_, building)| building.kind == BuildingKind::HEADQUARTERS && building.owner == player)
            .map(|(record, _)| record)
            .collect();
        for building in headquarters {
            let Some((record, _)) = self.world.storehouse_of(building) else {
                continue;
            };
            match stock {
                Stock::Ware(ware) if usize::from(ware) < WARE_TYPES => {
                    let ware = WareKind(ware);
                    if let Some(store) = self.world.storehouse_mut(record) {
                        store.wares[ware.index()] = store.wares[ware.index()].wrapping_add(amount);
                    }
                    self.world.count_ware(player, ware, amount);
                    self.world.clear_arrivals(player, ware);
                }
                Stock::Job(job) if Job(job).is_person() => {
                    let job = Job(job);
                    if let Some(store) = self.world.storehouse_mut(record) {
                        store.jobs[job.index()] = store.jobs[job.index()].wrapping_add(amount);
                    }
                    self.world.count_people(player, job, amount);
                }
                Stock::Ware(_) | Stock::Job(_) => {}
            }
        }
    }

    /// The event check (`0x31ca4`): every slot of the active list in order, each record's test,
    /// and each event that passes fired. An event with a letter waits while a letter that holds
    /// others back is open.
    pub(crate) fn check_events(&mut self) {
        for slot in 0..ACTIVE_SLOTS {
            let Some(mission) = self.mission.as_ref() else {
                return;
            };
            let Some(record) = mission.active.get(slot).copied().flatten() else {
                continue;
            };
            let Some(event) = mission.events.get(record.into()).copied() else {
                continue;
            };
            if let Trigger::Element { x, y, element } = event.trigger {
                let node = spot(x, y, (self.nodes.width(), self.nodes.height()));
                self.world
                    .set_object(node, crate::world::MapObject::element(element));
                self.free_event(slot, record);
                continue;
            }
            if !self.passes(event.trigger) {
                continue;
            }
            if event.text.is_some() && mission.letter_open() {
                continue;
            }
            self.fire(event);
            self.free_event(slot, record);
        }
    }

    fn free_event(&mut self, slot: usize, record: u8) {
        if let Some(mission) = self.mission.as_mut() {
            let _ = mission.events.remove(record.into());
            if let Some(held) = mission.active.get_mut(slot) {
                *held = None;
            }
        }
    }

    /// Whether `trigger`'s test passes. Every test is about player 0.
    fn passes(&self, trigger: Trigger) -> bool {
        let first = Player::ALL[0];
        let size = (self.nodes.width(), self.nodes.height());
        match trigger {
            Trigger::End { counter, times } => counter >= times,
            Trigger::Occupied { x, y } => self.nodes.get(spot(x, y, size)).owner == Some(first),
            Trigger::Explored { x, y } => self.nodes.visible(spot(x, y, size), first),
            Trigger::Wares { ware, amount } => self.world.owned_wares(first, WareKind(ware)) >= amount,
            Trigger::Buildings { kind, amount } => {
                self.world.finished_buildings(first, BuildingKind(kind)) >= amount
            }
            Trigger::Land { size } => self.nodes.owned(first) >= u32::from(size),
            Trigger::People { job, amount } => self.world.people(first, Job(job)) >= amount,
            Trigger::Found { word } => word < FINDS && self.world.found_signs[first.index()][word],
            Trigger::Direct => true,
            Trigger::Element { .. } => false,
            Trigger::Contact { player } => u8::try_from(player)
                .ok()
                .and_then(Player::new)
                .is_some_and(|player| self.nodes.contact(player)),
        }
    }

    /// Fires `event`: its letter, its buildings for player 0 with a message for each, its
    /// `next` event's counter, and for an end event the events it activates and the chapter
    /// it wins.
    fn fire(&mut self, event: Event) {
        let first = Player::ALL[0];
        if let Some(text) = event.text
            && let Some(mission) = self.mission.as_mut()
            && mission.letter.is_none()
        {
            mission.letter = Some(Letter {
                text,
                page: event.page,
                closing: event.chapters.is_some(),
            });
        }
        for kind in event.enables.into_iter().flatten() {
            if let Some(mission) = self.mission.as_mut()
                && let Some(allowed) = mission.allowed[first.index()].get_mut(usize::from(kind))
            {
                *allowed = true;
                self.post_message(
                    MessageId::BUILDING_ALLOWED,
                    first,
                    None,
                    Some(BuildingKind(kind)),
                    0,
                );
            }
        }
        let Some(mission) = self.mission.as_mut() else {
            return;
        };
        if let Some(next) = event.next
            && let Some(record) = mission.find(next)
            && let Some(Event {
                trigger: Trigger::End { counter, .. },
                ..
            }) = mission.events.get_mut(record.into())
        {
            *counter = counter.wrapping_add(1);
        }
        if matches!(event.trigger, Trigger::End { .. }) {
            for id in event.activates.into_iter().flatten() {
                mission.activate(id);
            }
            if let Some(chapters) = event.chapters {
                mission.won = true;
                for chapter in chapters {
                    if usize::from(chapter) < CHAPTER_FLAGS && !mission.opened.contains(&chapter) {
                        mission.opened.push(chapter);
                    }
                }
            }
        }
    }
}

/// What `ADD_WARE` or `ADD_PEOPLE` puts in a headquarters.
#[derive(Debug, Clone, Copy)]
enum Stock {
    Ware(u8),
    Job(u8),
}

#[cfg(test)]
mod tests;
