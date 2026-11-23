//! The map's nodes: height, terrain, light, roads and territory for every node of the grid.

use s2_formats::map::{AreaKind, LayerKind, Map};

use bitflags::bitflags;

use crate::grid::{Direction, Node};
use crate::marks::{Edges, Roads};
use std::hash::{Hash, Hasher};

use crate::packed::Bits;
use crate::world::Player;

/// What a node holds: height, terrain, light, roads and territory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct NodeData {
    /// Altitude, 0 to 127 (slot 0 bits 0–6).
    pub altitude: u8,
    /// Terrain types, 0 to 63, of the node's two triangles: node–SE–SW (slot 1), then node–E–SE
    /// (slot 2).
    pub terrain: [u8; 2],
    /// The node's on/off states, spread over slots 0 to 2 in the original.
    pub flags: NodeFlags,
    /// Shading row, 0 to 128; 64 leaves the texture unchanged.
    pub light: u8,
    /// Roads leaving the node (slot 3).
    pub roads: Roads,
    /// Who owns the node (slot 9 bits 0–2).
    pub owner: Option<Player>,
    /// Which of its triangles players 0 and 1 have seen (slot 9 bits 4–7).
    pub seen: crate::sight::Seen,
    /// How strongly its owner holds it (slot 10 bits 0–3): a claim writes the building's
    /// strength on its node and one less on each ring out.
    pub strength: u8,
    /// Territory border edges leaving the node (slot 10 bits 4–6).
    pub borders: Edges,
    /// Whether another player's claim leaves it alone (slot 10 bit 7): a headquarters' or
    /// occupied military building's node, its six neighbours and three nodes round its flag.
    pub protected: bool,
    /// What can be built here and whether figures can pass (slot 8; see [`Sites`]).
    pub sites: Sites,
    /// The land or water area the node belongs to (slot 14), as the map file gives it.
    pub area: u8,
    /// What lies under the node (slot 11).
    pub resources: Resources,
}

bitflags! {
    /// A node's on/off states.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
    pub struct NodeFlags: u8 {
        /// A figure that stepped off its path to let another pass holds the node (slot 0 bit 7,
        /// inferred). Nothing in Robigo sets it yet.
        const HELD = 1;
        /// A harbour can be built here (slot 1 bit 6), as the map's author placed it.
        const HARBOUR = 1 << 1;
        /// Slot 1 bit 7: set on 20 nodes of `OMAP02` and read by nothing.
        const UNREAD = 1 << 2;
        /// The node's roads are drawn in the mountain band (slot 2 bit 6): set where a road
        /// marker is written next to mountain.
        const MOUNTAIN_ROADS = 1 << 3;
        /// The node lies on a territory border (slot 2 bit 7): owned, with a neighbour that
        /// someone else or nobody owns. Nothing can be built on it.
        const BORDER = 1 << 4;
    }
}

/// Slot 0.
const ALTITUDE: Bits = Bits::new(0, 7);
const HELD: Bits = Bits::bit(7);

/// Slots 1 and 2: the terrain type, then two flags.
const TERRAIN_TYPE: Bits = Bits::new(0, 6);
const HARBOUR: Bits = Bits::bit(6);
const UNREAD_FLAG: Bits = Bits::bit(7);
const MOUNTAIN_ROADS: Bits = Bits::bit(6);
const BORDER_NODE: Bits = Bits::bit(7);

impl NodeData {
    /// The slot 0 byte.
    #[must_use]
    pub const fn height_byte(self) -> u8 {
        ALTITUDE.set(self.altitude) | HELD.flag(self.flags.contains(NodeFlags::HELD))
    }

    /// Takes the altitude and the held bit from a slot 0 byte.
    pub fn set_height_byte(&mut self, byte: u8) {
        self.altitude = ALTITUDE.get(byte);
        self.flags.set(NodeFlags::HELD, HELD.is_set(byte));
    }

    /// The slot 1 and slot 2 bytes.
    #[must_use]
    pub const fn terrain_bytes(self) -> [u8; 2] {
        [
            TERRAIN_TYPE.set(self.terrain[0])
                | HARBOUR.flag(self.flags.contains(NodeFlags::HARBOUR))
                | UNREAD_FLAG.flag(self.flags.contains(NodeFlags::UNREAD)),
            TERRAIN_TYPE.set(self.terrain[1])
                | MOUNTAIN_ROADS.flag(self.flags.contains(NodeFlags::MOUNTAIN_ROADS))
                | BORDER_NODE.flag(self.flags.contains(NodeFlags::BORDER)),
        ]
    }

    /// Takes the terrain types and their flags from the slot 1 and slot 2 bytes.
    pub fn set_terrain_bytes(&mut self, [first, second]: [u8; 2]) {
        self.terrain = [TERRAIN_TYPE.get(first), TERRAIN_TYPE.get(second)];
        self.flags.set(NodeFlags::HARBOUR, HARBOUR.is_set(first));
        self.flags.set(NodeFlags::UNREAD, UNREAD_FLAG.is_set(first));
        self.flags
            .set(NodeFlags::MOUNTAIN_ROADS, MOUNTAIN_ROADS.is_set(second));
        self.flags.set(NodeFlags::BORDER, BORDER_NODE.is_set(second));
    }
}

impl NodeData {
    /// The slot 10 byte: strength, border edges and the protected bit.
    #[must_use]
    pub const fn territory_byte(&self) -> u8 {
        STRENGTH.set(self.strength)
            | Bits::new(4, 3).set(self.borders.bits())
            | PROTECTED.flag(self.protected)
    }
}

impl Hash for NodeData {
    /// Hashes the slots as bytes in their original order, so state hashes match those of the
    /// packed form.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.height_byte().hash(state);
        self.terrain_bytes().hash(state);
        self.light.hash(state);
        self.roads.hash(state);
        self.owner.hash(state);
        self.seen.hash(state);
        self.territory_byte().hash(state);
        self.sites.hash(state);
        self.area.hash(state);
        self.resources.hash(state);
    }
}

/// What lies under a node (slot 11), with how much is left, 0 to 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Resources {
    /// Nothing.
    #[default]
    Nothing,
    /// Water for a well.
    Water(u8),
    /// Fish.
    Fish(u8),
    /// A mineral for a mine.
    Mineral(Mineral, u8),
    /// Any other byte, which no routine looks for.
    Other(u8),
}

/// A mineral a mine digs for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Mineral {
    /// 0.
    Coal,
    /// 1.
    IronOre,
    /// 2.
    Gold,
    /// 3.
    Granite,
}

/// Slot 11: the amount, the mineral and the kind.
const AMOUNT: Bits = Bits::new(0, 3);
pub(crate) const MINERAL: Bits = Bits::new(3, 2);
const KIND: Bits = Bits::new(5, 3);
const WATER_KIND: u8 = 1;
const MINERAL_KIND: u8 = 2;
const FISH_KIND: u8 = 4;

impl Mineral {
    /// The mineral of slot 11's bits 3 and 4 as a number, 0 to 3.
    pub(crate) const fn of(value: u8) -> Self {
        match value {
            0 => Self::Coal,
            1 => Self::IronOre,
            2 => Self::Gold,
            _ => Self::Granite,
        }
    }
}

impl Resources {
    /// What a slot 11 byte holds. Water and fish with mineral bits set read as [`Self::Other`].
    #[must_use]
    pub const fn from_byte(byte: u8) -> Self {
        let amount = AMOUNT.get(byte);
        let mineral = MINERAL.get(byte);
        match KIND.get(byte) {
            _ if byte == 0 => Self::Nothing,
            WATER_KIND if mineral == 0 => Self::Water(amount),
            FISH_KIND if mineral == 0 => Self::Fish(amount),
            MINERAL_KIND => Self::Mineral(Mineral::of(mineral), amount),
            _ => Self::Other(byte),
        }
    }

    /// The slot 11 byte.
    #[must_use]
    pub const fn to_byte(self) -> u8 {
        match self {
            Self::Nothing => 0,
            Self::Water(amount) => KIND.set(WATER_KIND) | AMOUNT.set(amount),
            Self::Fish(amount) => KIND.set(FISH_KIND) | AMOUNT.set(amount),
            Self::Mineral(mineral, amount) => {
                KIND.set(MINERAL_KIND) | MINERAL.set(mineral as u8) | AMOUNT.set(amount)
            }
            Self::Other(byte) => byte,
        }
    }

    /// The amount, slot 11's bits 0 to 2, whatever the kind.
    #[must_use]
    pub const fn amount(self) -> u8 {
        AMOUNT.get(self.to_byte())
    }

    /// Slot 11's bits 3 and 4 in place, the rest clear: 0, 8, 16 or 24.
    #[must_use]
    pub const fn mineral_bits(self) -> u8 {
        MINERAL.set(MINERAL.get(self.to_byte()))
    }

    /// Whether the fish search takes the node (`0x2f0a2`): mineral bits clear and an amount above
    /// 0. It does not look at the kind, so water, coal and other bytes on open water count as fish.
    #[must_use]
    pub const fn fishable(self) -> bool {
        let byte = self.to_byte();
        MINERAL.get(byte) == 0 && AMOUNT.get(byte) > 0
    }

    /// One taken away (`0x3dea8`); an amount of 0 stays 0. A fisher takes twice per catch and
    /// takes from 0 on the last fish.
    pub const fn take_one(&mut self) {
        *self = match *self {
            Self::Water(amount) => Self::Water(amount.saturating_sub(1)),
            Self::Fish(amount) => Self::Fish(amount.saturating_sub(1)),
            Self::Mineral(mineral, amount) => Self::Mineral(mineral, amount.saturating_sub(1)),
            Self::Other(byte) if AMOUNT.get(byte) > 0 => Self::Other(byte - 1),
            other => other,
        };
    }
}

impl Hash for Resources {
    /// Hashes the slot 11 byte, so state hashes match those of the packed form.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_byte().hash(state);
    }
}

/// What can be built on a node and what may cross it (slot 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct Sites {
    /// The largest thing that fits.
    pub size: SiteSize,
    /// What the node rules out.
    pub flags: SiteFlags,
}

/// The largest thing a node can take (slot 8 bits 0–2).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub enum SiteSize {
    /// Nothing.
    #[default]
    Nothing,
    /// A flag only.
    Flag,
    /// A small building.
    Small,
    /// A medium building.
    Medium,
    /// A large building.
    Large,
    /// A mine.
    Mine,
}

impl SiteSize {
    /// The size a slot 8 value holds. The rules never write 6 or 7 and no shipped map holds
    /// them; both read as nothing.
    #[must_use]
    pub const fn from_field(value: u8) -> Self {
        match value {
            1 => Self::Flag,
            2 => Self::Small,
            3 => Self::Medium,
            4 => Self::Large,
            5 => Self::Mine,
            _ => Self::Nothing,
        }
    }

    /// The value slot 8 stores for this size.
    #[must_use]
    pub const fn field(self) -> u8 {
        self as u8
    }

    /// The size of building this room takes, or nothing for a node that takes no building.
    #[must_use]
    pub const fn building(self) -> Option<crate::Size> {
        match self {
            Self::Small => Some(crate::Size::Small),
            Self::Medium => Some(crate::Size::Medium),
            Self::Large => Some(crate::Size::Large),
            Self::Mine => Some(crate::Size::Mine),
            Self::Nothing | Self::Flag => None,
        }
    }
}

bitflags! {
    /// What a node rules out (slot 8 bits 3–7).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
    pub struct SiteFlags: u8 {
        /// Nothing can be planted here.
        const NO_PLANTING = 1 << 3;
        /// Figures cannot walk onto the node.
        const BLOCKED = 1 << 4;
        /// No new flag here: one is next to it.
        const NO_FLAG = 1 << 5;
        /// No road through here.
        const NO_ROAD = 1 << 6;
        /// Part of a large building's footprint.
        const FOOTPRINT = 1 << 7;
    }
}

/// Slot 8 bits 0–2.
const SITE_SIZE: Bits = Bits::new(0, 3);

impl Sites {
    /// The slot 8 byte as stored: the size in bits 0–2, the flags above.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Self {
        Self {
            size: SiteSize::from_field(SITE_SIZE.get(byte)),
            flags: SiteFlags::from_bits_truncate(byte),
        }
    }

    /// The slot 8 byte: the inverse of [`Self::from_byte`] for sizes the rules write.
    #[must_use]
    pub const fn to_byte(self) -> u8 {
        self.flags.bits() | SITE_SIZE.set(self.size.field())
    }

    /// Whether figures can walk onto the node (`0x19727`).
    #[must_use]
    pub const fn passable(self) -> bool {
        !self.flags.contains(SiteFlags::BLOCKED)
    }
}

/// The light row of a flat node, which leaves the texture unchanged.
const FLAT_LIGHT: i32 = 64;

/// The brightest light row.
const MAX_LIGHT: i32 = 128;

/// How much each neighbour's rise over a node changes its light (`0x18340`).
const SLOPE_NORTH_EAST: i32 = 9;
const SLOPE_WEST: i32 = 6;
const SLOPE_FAR_WEST: i32 = 3;
const SLOPE_SOUTH_WEST: i32 = 9;

/// How far from a node whose height changes the light is recomputed, in stored coordinates.
const LIGHT_REACH: i32 = 2;

/// Slot 10: the territory strength and the protected bit; the border edges sit between them.
const STRENGTH: Bits = Bits::new(0, 4);
const PROTECTED: Bits = Bits::bit(7);

/// The owner held in a slot 9 byte: bits 0–2, where 7 means nobody.
fn owner(slot9: u8) -> Option<Player> {
    Player::new(Bits::new(0, 3).get(slot9))
}

/// Why a map's terrain could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MapError {
    /// The map lacks a layer the terrain needs.
    #[error("the map has no {0:?} layer")]
    MissingLayer(LayerKind),
    /// The map has no nodes, or a layer is shorter than the map.
    #[error("the map's {0:?} layer does not cover the map")]
    Short(LayerKind),
}

/// The nodes a capture books as changing hands, one fewer than it gives away.
const CAPTURE_BOOKED: u32 = 9;

/// The per-node layers: heights, both terrain layers, light, roads, owners, border edges and
/// building sites.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct NodeMap {
    width: u16,
    height: u16,
    nodes: Vec<NodeData>,
    seas: Vec<bool>,
    owned: [u32; Player::COUNT],
    /// In a campaign chapter, which players' land player 0 has seen (`0x1f790d + 2·player`);
    /// `None` outside one, where nothing keeps them.
    contacts: Option<[bool; Player::COUNT]>,
}

impl NodeMap {
    /// A `width` × `height` map with every node set to `node`; `None` if either side is zero.
    #[must_use]
    pub fn filled(width: u16, height: u16, node: NodeData) -> Option<Self> {
        (width > 0 && height > 0).then(|| {
            let mut map = Self {
                width,
                height,
                nodes: vec![node; usize::from(width) * usize::from(height)],
                seas: Vec::new(),
                owned: [0; Player::COUNT],
                contacts: None,
            };
            map.recount_owners();
            map
        })
    }

    /// Reads the node layers of `map`. Roads, owners, border edges and building sites are zero
    /// where the map lacks their layer.
    ///
    /// # Errors
    ///
    /// [`MapError`] when a layer is missing or does not cover the map.
    pub fn from_map(map: &Map) -> Result<Self, MapError> {
        let (width, height) = (map.width(), map.height());
        let count = usize::from(width) * usize::from(height);
        let layer = |kind| {
            let data = map.layer(kind).ok_or(MapError::MissingLayer(kind))?.data();
            match data.get(..count) {
                Some(data) if count > 0 => Ok(data),
                _ => Err(MapError::Short(kind)),
            }
        };
        let optional = |kind| layer(kind).ok();
        let heights = layer(LayerKind::Heights)?;
        let first = layer(LayerKind::Terrain1)?;
        let second = layer(LayerKind::Terrain2)?;
        let light = layer(LayerKind::Shading)?;
        let roads = optional(LayerKind::Roads);
        let owners = optional(LayerKind::Ownership);
        let borders = optional(LayerKind::Territory);
        let sites = optional(LayerKind::BuildingSites);
        let areas = optional(LayerKind::Areas);
        let resources = optional(LayerKind::Resources);
        let at = |layer: Option<&[u8]>, i: usize| layer.map_or(0, |l| l[i]);
        let nodes = (0..count)
            .map(|i| {
                let mut data = NodeData {
                    light: light[i],
                    roads: Roads::from_byte(at(roads, i)),
                    owner: owners.and_then(|o| owner(o[i])),
                    seen: crate::sight::Seen::from_bits_truncate(at(owners, i)),
                    strength: STRENGTH.get(at(borders, i)),
                    borders: Edges::from_slot10(at(borders, i)),
                    protected: PROTECTED.is_set(at(borders, i)),
                    sites: Sites::from_byte(at(sites, i)),
                    area: at(areas, i),
                    resources: Resources::from_byte(at(resources, i)),
                    ..NodeData::default()
                };
                data.set_height_byte(heights[i]);
                data.set_terrain_bytes([first[i], second[i]]);
                data
            })
            .collect();
        let seas = map
            .areas()
            .iter()
            .map(|area| area.kind == AreaKind::Water)
            .collect();
        let mut map = Self {
            width,
            height,
            nodes,
            seas,
            owned: [0; Player::COUNT],
            contacts: None,
        };
        map.recount_owners();
        Ok(map)
    }

    /// Whether area `area` (slot 14) is a body of water in the map's area table (kind 2,
    /// `0x200d40`). The no-area marks 254 and 255 are none.
    #[must_use]
    pub fn is_sea(&self, area: u8) -> bool {
        self.seas.get(usize::from(area)).copied().unwrap_or(false)
    }

    /// Marks area `area` as a body of water or not, growing the table as needed.
    pub fn set_sea(&mut self, area: u8, sea: bool) {
        let index = usize::from(area);
        if self.seas.len() <= index {
            self.seas.resize(index + 1, false);
        }
        self.seas[index] = sea;
    }

    /// Nodes per row.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Rows.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.height
    }

    /// The data at `node`, which wraps into the map.
    #[must_use]
    pub fn get(&self, node: Node) -> NodeData {
        self.nodes.get(self.index(node)).copied().unwrap_or_default()
    }

    /// Replaces the data at `node`, which wraps into the map, keeping the owner counts right.
    pub fn set(&mut self, node: Node, data: NodeData) {
        if self.book(node, data) {
            self.note_new_owner(node, data.owner);
        }
    }

    /// The same as [`Self::set`] for a write through the owner setter, which notes a contact
    /// even when the owner stays the same (`0x3dff0`).
    pub(crate) fn set_owner(&mut self, node: Node, data: NodeData) {
        self.book(node, data);
        self.note_new_owner(node, data.owner);
    }

    /// The same as [`Self::set`] for a claim spreading over the land, which writes the owner
    /// past the owner setter and so never notes a contact (`0x3f184`).
    pub(crate) fn set_spreading(&mut self, node: Node, data: NodeData) {
        self.book(node, data);
    }

    /// Writes `node` and moves the owner counts; gives back whether the owner changed.
    fn book(&mut self, node: Node, data: NodeData) -> bool {
        let at = self.index(node);
        let Some(slot) = self.nodes.get_mut(at) else {
            return false;
        };
        let before = slot.owner;
        *slot = data;
        if before == data.owner {
            return false;
        }
        if let Some(lost) = before.and_then(|player| self.owned.get_mut(player.index())) {
            *lost = lost.saturating_sub(1);
        }
        if let Some(gained) = data.owner.and_then(|player| self.owned.get_mut(player.index())) {
            *gained = gained.saturating_add(1);
        }
        true
    }

    /// Writes `node` without touching the owners' land counts, for a change `S2.EXE` books on its
    /// own terms.
    pub(crate) fn set_unbooked(&mut self, node: Node, data: NodeData) {
        let at = self.index(node);
        if let Some(slot) = self.nodes.get_mut(at) {
            let before = slot.owner;
            *slot = data;
            if before != data.owner {
                self.note_new_owner(node, data.owner);
            }
        }
    }

    /// Starts keeping the contact words, as a campaign chapter does, with none set.
    pub(crate) fn keep_contacts(&mut self) {
        self.contacts = Some([false; Player::COUNT]);
    }

    /// Whether the contact words are kept, as only a campaign chapter does.
    pub(crate) const fn keeps_contacts(&self) -> bool {
        self.contacts.is_some()
    }

    /// Whether player 0 has seen `player`'s land in a campaign chapter (`0x1f790d`); always
    /// false outside one.
    #[must_use]
    pub fn contact(&self, player: Player) -> bool {
        self.contacts.is_some_and(|contacts| contacts[player.index()])
    }

    /// Notes `owner` as seen when player 0 can see `node` as it takes it (`0x3e031`).
    fn note_new_owner(&mut self, node: Node, owner: Option<Player>) {
        if let Some(owner) = owner
            && self.contacts.is_some()
            && self.visible(node, Player::ALL[0])
            && let Some(contacts) = self.contacts.as_mut()
        {
            contacts[owner.index()] = true;
        }
    }

    /// Notes the owner of `node`, a ring position of a reveal for player 0, as seen (`0x3ff7b`).
    pub(crate) fn note_revealed(&mut self, node: Node) {
        let owner = self.get(node).owner;
        if let Some(owner) = owner
            && let Some(contacts) = self.contacts.as_mut()
        {
            contacts[owner.index()] = true;
        }
    }

    /// Books a capture's protected nodes as `S2.EXE` does (`0x304d4`, `0x304e2`): whatever they
    /// held, `old`'s land count falls by 9, to no lower than 0, and `new`'s rises by 9. There are
    /// ten such nodes, so a capture of a building whose nodes were all `old`'s leaves each
    /// count one node off the map.
    pub(crate) fn book_capture(&mut self, old: Player, new: Player) {
        if let Some(count) = self.owned.get_mut(old.index()) {
            *count = count.saturating_sub(CAPTURE_BOOKED);
        }
        if let Some(count) = self.owned.get_mut(new.index()) {
            *count = count.saturating_add(CAPTURE_BOOKED);
        }
    }

    /// How many nodes `player` owns (`0x29be0`), which the statistics and the land goal read.
    #[must_use]
    pub fn owned(&self, player: Player) -> u32 {
        self.owned[player.index()]
    }

    /// Counts the owners from scratch, for a map that was filled or loaded in one go.
    fn recount_owners(&mut self) {
        self.owned = [0; Player::COUNT];
        for data in &self.nodes {
            if let Some(count) = data.owner.and_then(|player| self.owned.get_mut(player.index())) {
                *count = count.saturating_add(1);
            }
        }
    }

    /// The light of `node` from the slope around it (`0x18340`): row 64 lit flat, brighter facing
    /// north-east and darker facing west and south-west, clamped to rows 0 to 128. With `Δ(n)`
    /// the altitude of `n` less that of `node`:
    /// `64 + 9·Δ(NE) − 6·Δ(W) − 3·Δ(W of W) − 9·Δ(W of SW)`.
    #[must_use]
    pub fn light_at(&self, node: Node) -> u8 {
        let (width, height) = (self.width, self.height);
        let near = |from: Node, direction| from.neighbour(direction, width, height);
        let altitude = |at: Node| i32::from(self.get(at).altitude);
        let delta = |at: Node| altitude(at) - altitude(node);
        let west = near(node, Direction::West);
        let slope = SLOPE_NORTH_EAST * delta(near(node, Direction::NorthEast))
            - SLOPE_WEST * delta(west)
            - SLOPE_FAR_WEST * delta(near(west, Direction::West))
            - SLOPE_SOUTH_WEST * delta(near(near(node, Direction::SouthWest), Direction::West));
        u8::try_from((FLAT_LIGHT + slope).clamp(0, MAX_LIGHT)).unwrap_or_default()
    }

    /// Sets the altitude of `node` and recomputes the light of
    /// the 5 × 5 nodes of stored coordinates `x − 2` to `x + 2` and `y − 2` to `y + 2` around it,
    /// wrapping at the map's edges (`0x1c4b8`, `0x18540`).
    pub fn set_altitude(&mut self, node: Node, altitude: u8) {
        let mut data = self.get(node);
        data.altitude = altitude % (ALTITUDE.max() + 1);
        self.set(node, data);
        let (x, y) = (i32::from(node.x), i32::from(node.y));
        for dy in -LIGHT_REACH..=LIGHT_REACH {
            for dx in -LIGHT_REACH..=LIGHT_REACH {
                let at = Node::wrapped(x + dx, y + dy, self.width, self.height);
                let mut data = self.get(at);
                data.light = self.light_at(at);
                self.set(at, data);
            }
        }
    }

    fn index(&self, node: Node) -> usize {
        let node = Node::wrapped(node.x.into(), node.y.into(), self.width, self.height);
        usize::from(node.y) * usize::from(self.width) + usize::from(node.x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_owner_counts_follow_every_change_of_owner() {
        let player = Player::new(1).expect("player 1");
        let other = Player::new(2).expect("player 2");
        let mut map = NodeMap::filled(8, 8, NodeData::default()).expect("a map");
        assert_eq!(map.owned(player), 0);
        for x in 0..3 {
            let node = Node { x, y: 0 };
            let mut data = map.get(node);
            data.owner = Some(player);
            map.set(node, data);
        }
        assert_eq!(map.owned(player), 3);
        let taken = Node { x: 0, y: 0 };
        let mut data = map.get(taken);
        data.owner = Some(other);
        map.set(taken, data);
        assert_eq!((map.owned(player), map.owned(other)), (2, 1));
        let mut data = map.get(taken);
        data.owner = None;
        map.set(taken, data);
        assert_eq!((map.owned(player), map.owned(other)), (2, 0));
        let mut data = map.get(taken);
        data.altitude = 5;
        map.set(taken, data);
        assert_eq!(
            map.owned(other),
            0,
            "a change that is not the owner counts nothing"
        );
    }

    #[test]
    fn a_map_filled_with_an_owner_counts_every_node() {
        let player = Player::new(0).expect("player 0");
        let data = NodeData {
            owner: Some(player),
            ..NodeData::default()
        };
        let map = NodeMap::filled(8, 4, data).expect("a map");
        assert_eq!(map.owned(player), 32);
    }

    /// Every `.WLD` map under the folders of the data set `S2_DATA_PLAIN` names.
    fn shipped_maps() -> Option<Vec<(std::path::PathBuf, Map)>> {
        let root = std::path::PathBuf::from(std::env::var_os("S2_DATA_PLAIN")?);
        let mut maps = Vec::new();
        for folder in [
            "DATA/MAPS",
            "DATA/MAPS2",
            "DATA/MAPS3",
            "DATA/MAPS4",
            "DATA/MISSIONS",
        ] {
            let Ok(entries) = std::fs::read_dir(root.join(folder)) else {
                continue;
            };
            let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
            paths.sort();
            for path in paths {
                let is_world = path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("wld"));
                if is_world
                    && let Some(map) = std::fs::read(&path)
                        .ok()
                        .and_then(|bytes| Map::parse_wld(&bytes).ok())
                {
                    maps.push((path, map));
                }
            }
        }
        Some(maps)
    }

    #[test]
    fn the_light_formula_gives_every_shipped_maps_stored_light() {
        let Some(maps) = shipped_maps() else {
            eprintln!("S2_DATA_PLAIN not set; skipped");
            return;
        };
        assert!(!maps.is_empty(), "no maps found");
        for (path, map) in maps {
            let nodes = NodeMap::from_map(&map).unwrap();
            let mut wrong = 0;
            for y in 0..nodes.height() {
                for x in 0..nodes.width() {
                    let node = Node { x, y };
                    if nodes.light_at(node) != nodes.get(node).light {
                        wrong += 1;
                    }
                }
            }
            assert_eq!(wrong, 0, "{}", path.display());
        }
    }

    #[test]
    fn setting_an_altitude_keeps_the_held_bit_and_relights_five_by_five() {
        let flat = NodeData {
            altitude: 10,
            light: 64,
            ..NodeData::default()
        };
        let mut nodes = NodeMap::filled(12, 12, flat).unwrap();
        let centre = Node { x: 5, y: 6 };
        let mut held = nodes.get(centre);
        held.flags |= NodeFlags::HELD;
        nodes.set(centre, held);
        nodes.set_altitude(centre, 20);
        assert!(nodes.get(centre).flags.contains(NodeFlags::HELD));
        assert_eq!(nodes.get(centre).altitude, 20);
        for y in 0..12 {
            for x in 0..12 {
                let node = Node { x, y };
                let inside = x.abs_diff(5) <= 2 && y.abs_diff(6) <= 2;
                let expected = if inside { nodes.light_at(node) } else { 64 };
                assert_eq!(nodes.get(node).light, expected, "{node:?}");
            }
        }
        assert_ne!(nodes.get(centre).light, 64, "the raised node is lit by its slope");
    }

    #[test]
    fn every_slot_0_1_2_and_11_byte_reads_back_unchanged() {
        for byte in 0..=u8::MAX {
            let mut data = NodeData::default();
            data.set_height_byte(byte);
            data.set_terrain_bytes([byte, byte.rotate_left(3)]);
            assert_eq!(data.height_byte(), byte);
            assert_eq!(data.terrain_bytes(), [byte, byte.rotate_left(3)]);
            assert_eq!(Resources::from_byte(byte).to_byte(), byte);
        }
    }

    #[test]
    fn taking_from_nothing_left_leaves_nothing_left() {
        let mut fish = Resources::Fish(1);
        fish.take_one();
        assert_eq!(fish, Resources::Fish(0));
        fish.take_one();
        assert_eq!(fish, Resources::Fish(0));
        assert!(!fish.fishable());
        let mut odd = Resources::Other(0x61);
        odd.take_one();
        odd.take_one();
        assert_eq!(odd, Resources::Other(0x60), "only the amount bits count down");
        assert!(
            Resources::Mineral(Mineral::Coal, 2).fishable(),
            "the search ignores the kind"
        );
    }

    #[test]
    fn sites_round_trip_through_the_slot_byte() {
        for byte in 0..=u8::MAX {
            let sites = Sites::from_byte(byte);
            if (byte % 8) < 6 {
                assert_eq!(sites.to_byte(), byte);
            } else {
                assert_eq!(sites.size, SiteSize::Nothing);
            }
        }
    }

    #[test]
    fn a_capture_books_nine_nodes_for_its_ten() {
        let old = Player::new(1).expect("player 1");
        let new = Player::new(2).expect("player 2");
        let mut map = NodeMap::filled(8, 8, NodeData::default()).expect("a map");
        let protected: Vec<Node> = (0..10).map(|x| Node { x: x % 8, y: x / 8 }).collect();
        for &node in &protected {
            let mut data = map.get(node);
            data.owner = Some(old);
            map.set(node, data);
        }
        map.book_capture(old, new);
        for &node in &protected {
            let mut data = map.get(node);
            data.owner = Some(new);
            map.set_unbooked(node, data);
        }
        assert_eq!((map.owned(old), map.owned(new)), (1, 9), "one node off each way");
        map.book_capture(old, new);
        assert_eq!(map.owned(old), 0, "no lower than none");
    }
}
