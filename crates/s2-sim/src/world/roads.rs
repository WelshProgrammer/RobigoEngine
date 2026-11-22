//! Flags and roads: their records, how a road is written into slot 3, and the world routines that
//! place and remove them, in `S2.EXE`'s order. These routines check nothing about who is asking;
//! the commands that call them do.

use crate::figures::{CarrierState, FigureKind};
use crate::grid::{Direction, Node};
use crate::ground::Property;
use crate::job::Job;
use crate::marks::{Edges, RoadKind};
use crate::nodes::{NodeFlags, NodeMap, SiteFlags};
use crate::sites::refresh_sites;
use crate::transport::ROAD_MODE;

use super::{MapObject, Player, World};

/// The most flag records in use at once (`0x283ec`).
pub const MAX_FLAGS: usize = 8_000;

/// The most road records in use at once (`0x295c8`).
pub const MAX_ROADS: usize = 8_000;

/// The most wares a flag holds; a flag with this many is full.
pub const FLAG_CAPACITY: u8 = 8;

/// A flag's type (record byte `+1`), which picks its bitmaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FlagKind {
    /// 1: an ordinary flag.
    Normal,
    /// 2: a flag on a donkey road.
    Donkey,
    /// 3: a flag on water, where boats land.
    Water,
}

impl FlagKind {
    /// The type's number in the record: 1, 2 or 3.
    #[must_use]
    pub const fn number(self) -> u8 {
        match self {
            Self::Normal => 1,
            Self::Donkey => 2,
            Self::Water => 3,
        }
    }
}

/// One direction of a flag's road table: words `+0x06` and `+0x12` of its record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FlagLink {
    /// The road record leaving this way.
    pub road: u16,
    /// The flag at its other end; `None` while the road is still being drawn (65534).
    pub far: Option<u16>,
}

/// A flag record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Flag {
    /// Its type.
    pub kind: FlagKind,
    /// Owner (byte `+0x21`), copied from the node when the flag is placed.
    pub owner: Player,
    /// Where it stands.
    pub node: Node,
    /// The road leaving in each direction, indexed by [`Direction::index`].
    pub links: [Option<FlagLink>; 6],
    /// Wares waiting here, 0 to [`FLAG_CAPACITY`] (byte `+0x20`).
    pub waiting: u8,
    /// The ware put down here last, at the head of the list of waiting wares (`+0x1e`).
    pub wares: Option<u16>,
}

impl Flag {
    /// A flag of `kind` for `owner` on `node` with no roads.
    #[must_use]
    pub const fn new(kind: FlagKind, owner: Player, node: Node) -> Self {
        Self {
            kind,
            owner,
            node,
            links: [None; 6],
            waiting: 0,
            wares: None,
        }
    }
}

/// A road record (18 bytes in `S2.EXE`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Road {
    /// Its type.
    pub kind: RoadKind,
    /// The flag it starts at.
    pub start: u16,
    /// The flag it ends at; `None` while it is still being drawn.
    pub end: Option<u16>,
    /// The nodes between the two flags, from the start.
    pub path: Vec<Node>,
    /// Traffic counter (word `+0x10`); at 100 an ordinary road gets a donkey.
    pub traffic: u16,
    /// Carriers on the road, 0 to 2 (byte `+0x02`).
    pub carriers: u8,
    /// Whether a carrier has ever taken the road (byte `+0x01` bit 2). Until then routing charges
    /// a flat 10 for it instead of [`Self::busy`].
    pub served: bool,
    /// How busy the road is, 0 to 255 (byte `+0x03`): up 1 each time a carrier takes a ware on,
    /// down 10 each time an idle carrier finds none.
    pub busy: u8,
    /// The open transport order for its carrier or donkey (word `+0x04`).
    pub order: Option<u16>,
}

impl Road {
    /// A road with no carrier and every counter at 0.
    #[must_use]
    pub const fn new(kind: RoadKind, start: u16, end: Option<u16>, path: Vec<Node>) -> Self {
        Self {
            kind,
            start,
            end,
            path,
            traffic: 0,
            carriers: 0,
            served: false,
            busy: 0,
            order: None,
        }
    }

    /// Edges from flag to flag: one more than the inner nodes.
    #[must_use]
    pub fn edges(&self) -> u16 {
        u16::try_from(self.path.len() + 1).unwrap_or(u16::MAX)
    }
}

/// Why [`World::create_road`] failed, with the reason code `S2.EXE` leaves at `0x201684`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum RoadError {
    /// 1: the road starts and ends at the same flag.
    SameFlag,
    /// 2: [`MAX_ROADS`] roads are in use.
    PoolFull,
    /// 3: the start flag already has a road leaving towards the road's first node.
    StartTaken,
    /// 4: the end flag already has a road leaving towards the road's last node.
    EndTaken,
}

/// Writes one road segment into slot 3: from `from` in `direction`, of type `kind` or cleared
/// (`0x1c258`). Both ends get the marker and the mountain-band bit, the edge goes on whichever end
/// owns it, and the far end and its ring are recomputed.
fn write_segment(
    nodes: &mut NodeMap,
    world: &World,
    from: Node,
    direction: Direction,
    kind: Option<RoadKind>,
) {
    let to = from.neighbour(direction, nodes.width(), nodes.height());
    write_marker(nodes, from, kind);
    let (owner, edge) = match Edges::leaving(direction) {
        Some(edge) => (from, edge),
        None => (to, Edges::leaving(direction.opposite()).unwrap_or_default()),
    };
    let mut data = nodes.get(owner);
    data.roads = data.roads.with_road(edge, kind);
    nodes.set(owner, data);
    write_marker(nodes, to, kind);
    refresh_sites(nodes, world, to, 1);
}

/// Sets `node`'s marker to `kind` and its mountain-band bit to whether mountain touches it
/// (`0x1c1c0`).
fn write_marker(nodes: &mut NodeMap, node: Node, kind: Option<RoadKind>) {
    let mountain = nodes.property(node).contains(Property::ANY_MOUNTAIN);
    let mut data = nodes.get(node);
    data.roads = data.roads.with_marker(kind);
    data.flags.set(NodeFlags::MOUNTAIN_ROADS, mountain);
    nodes.set(node, data);
}

/// Lays the type 1 road from the building on `building` to its flag south-east of it
/// (`0x1c428`), which recomputes the flag's ring. Every building in the Gold Edition savegames has
/// this road. Other roads at either node stay.
pub fn connect_to_flag(nodes: &mut NodeMap, world: &World, building: Node) {
    write_segment(
        nodes,
        world,
        building,
        Direction::SouthEast,
        Some(RoadKind::Normal),
    );
}

/// Clears the type 1 road from the building on `building` to its flag (`0x1c468`), which
/// recomputes the flag's ring.
pub fn disconnect_from_flag(nodes: &mut NodeMap, world: &World, building: Node) {
    write_segment(nodes, world, building, Direction::SouthEast, None);
}

impl World {
    /// Flag record `record`, if it is in use.
    #[must_use]
    pub fn flag(&self, record: u16) -> Option<&Flag> {
        self.flags.get(record)
    }

    /// The flag records in use, in record order, with their numbers.
    pub fn flags(&self) -> impl Iterator<Item = (u16, &Flag)> {
        self.flags.iter()
    }

    /// One past the highest flag record in use.
    #[must_use]
    pub fn flag_slots(&self) -> usize {
        self.flags.slots()
    }

    /// The flag on `node`, if there is one.
    #[must_use]
    pub fn flag_at(&self, node: Node) -> Option<u16> {
        match self.object(node) {
            MapObject::Flag(record) => Some(record),
            _ => None,
        }
    }

    /// Road record `record`, if it is in use.
    #[must_use]
    pub fn road(&self, record: u16) -> Option<&Road> {
        self.roads.get(record)
    }

    /// Road record `record` to change, if it is in use. For tools and tests that build a state
    /// directly; a game changes roads only through steps and commands.
    pub fn road_mut(&mut self, record: u16) -> Option<&mut Road> {
        self.roads.get_mut(record)
    }

    /// Flag record `record` to change, if it is in use. For tools and tests that build a state
    /// directly; a game changes flags only through steps and commands.
    pub fn flag_mut(&mut self, record: u16) -> Option<&mut Flag> {
        self.flags.get_mut(record)
    }

    /// The road records in use, in record order, with their numbers.
    pub fn roads(&self) -> impl Iterator<Item = (u16, &Road)> {
        self.roads.iter()
    }

    /// Adds a flag record in the lowest free slot and returns its number; `None` when
    /// [`MAX_FLAGS`] are in use. The object word is separate.
    pub fn add_flag(&mut self, flag: Flag) -> Option<u16> {
        self.flags.insert(flag, MAX_FLAGS)
    }

    /// Every node of `road` in order: the start flag, the path, then the end flag if it has one.
    #[must_use]
    pub fn road_nodes(&self, road: &Road) -> Vec<Node> {
        let flag_node = |record: u16| self.flag(record).map(|flag| flag.node);
        flag_node(road.start)
            .into_iter()
            .chain(road.path.iter().copied())
            .chain(road.end.and_then(flag_node))
            .collect()
    }

    /// The road with `node` among its inner nodes (`0x25f99`): only where slot 3 holds something
    /// and no flag stands, and the first such road in record order.
    #[must_use]
    pub fn road_through(&self, nodes: &NodeMap, node: Node) -> Option<u16> {
        if nodes.get(node).roads.is_empty() || self.flag_at(node).is_some() {
            return None;
        }
        self.roads
            .iter()
            .find(|(_, road)| road.path.contains(&node))
            .map(|(record, _)| record)
    }

    /// Writes `road` into slot 3 as `kind`, or clears it (`0x1c2f4`): segment by segment from the
    /// start flag, ending at the last path node while the road has no end flag.
    pub(crate) fn paint(&self, nodes: &mut NodeMap, road: &Road, kind: Option<RoadKind>) {
        let (width, height) = (nodes.width(), nodes.height());
        let route = self.road_nodes(road);
        for pair in route.windows(2) {
            if let Some(direction) = pair[0].direction_to(pair[1], width, height) {
                write_segment(nodes, self, pair[0], direction, kind);
            }
        }
    }

    /// Creates a road of `kind` from flag `start` along `path` to flag `end` (`0x2591b`), or a
    /// road still being drawn when `end` is `None`. The road is written into slot 3 before its
    /// flags are checked. A refused road is cleared again and leaves the recomputes of both
    /// passes behind. A finished road then links both flags, orders a helper to its start flag
    /// (a water road orders [`Job::WATER_ROAD`]) and clears decorations such as flowers and
    /// mushrooms off its path (`0x3d71c`) without recomputing those nodes.
    ///
    /// # Errors
    ///
    /// [`RoadError`] when the road cannot be made.
    pub fn create_road(
        &mut self,
        nodes: &mut NodeMap,
        kind: RoadKind,
        start: u16,
        end: Option<u16>,
        path: Vec<Node>,
    ) -> Result<u16, RoadError> {
        if end == Some(start) {
            return Err(RoadError::SameFlag);
        }
        let road = Road::new(kind, start, end, path);
        let record = self
            .roads
            .insert(road.clone(), MAX_ROADS)
            .ok_or(RoadError::PoolFull)?;
        self.paint(nodes, &road, Some(kind));

        let (width, height) = (nodes.width(), nodes.height());
        let route = self.road_nodes(&road);
        let leaving = |from: usize, to: usize| {
            let (&a, &b) = (route.get(from)?, route.get(to)?);
            a.direction_to(b, width, height)
        };
        let start_way = leaving(0, 1);
        if start_way.is_none_or(|way| self.link(start, way).is_some()) {
            self.free_road(nodes, record);
            return Err(RoadError::StartTaken);
        }
        let start_way = start_way.unwrap_or(Direction::East);
        let Some(end) = end else {
            self.set_link(
                start,
                start_way,
                Some(FlagLink {
                    road: record,
                    far: None,
                }),
            );
            return Ok(record);
        };
        let last = route.len().saturating_sub(1);
        let end_way = last.checked_sub(1).and_then(|before| leaving(last, before));
        if end_way.is_none_or(|way| self.link(end, way).is_some()) {
            self.free_road(nodes, record);
            return Err(RoadError::EndTaken);
        }
        let end_way = end_way.unwrap_or(Direction::East);
        self.set_link(
            start,
            start_way,
            Some(FlagLink {
                road: record,
                far: Some(end),
            }),
        );
        self.set_link(
            end,
            end_way,
            Some(FlagLink {
                road: record,
                far: Some(start),
            }),
        );
        let job = if kind == RoadKind::Water {
            Job::WATER_ROAD
        } else {
            Job::HELPER
        };
        let order = self.order_person(start, job, ROAD_MODE, 0);
        if let Some(road) = self.roads.get_mut(record) {
            road.order = order;
        }
        for &node in &road.path {
            if let MapObject::Decoration(decoration) = self.object(node)
                && decoration.cleared_by_roads()
            {
                self.set_object(node, MapObject::Nothing);
            }
        }
        Ok(record)
    }

    fn link(&self, flag: u16, way: Direction) -> Option<FlagLink> {
        self.flag(flag)?.links[way.index()]
    }

    fn set_link(&mut self, flag: u16, way: Direction, link: Option<FlagLink>) {
        if let Some(flag) = self.flags.get_mut(flag) {
            flag.links[way.index()] = link;
        }
    }

    /// Takes one off the traffic counter of every ordinary road that has any (`0x295dc`), in
    /// record order.
    pub(crate) fn decay_traffic(&mut self) {
        for (_, road) in self.roads.iter_mut() {
            if road.kind == RoadKind::Normal {
                road.traffic = road.traffic.saturating_sub(1);
            }
        }
    }

    /// Clears `record` out of slot 3 and frees it (`0x29528`).
    fn free_road(&mut self, nodes: &mut NodeMap, record: u16) {
        if let Some(road) = self.roads.get(record).cloned() {
            self.paint(nodes, &road, None);
            self.roads.remove(record);
        }
    }

    /// Removes road `record` (`0x25b9c`, step 3): strands the wares at either flag waiting to
    /// cross it, unlinks it from its flags, clears it out of slot 3 and frees the record. Its
    /// flags stay. The game's own removal deals with the settlers on it and its order first.
    pub fn remove_road(&mut self, nodes: &mut NodeMap, record: u16) {
        let Some(road) = self.roads.get(record) else {
            return;
        };
        let (start, end) = (road.start, road.end);
        if let Some(end) = end {
            self.strand_wares_for(start, end);
            self.strand_wares_for(end, start);
        }
        self.unlink_road(start, end, record);
        self.free_road(nodes, record);
    }

    /// Takes road `record` off its flags (`0x25e3e`): the first matching direction at the start
    /// flag, then at the end flag.
    fn unlink_road(&mut self, start: u16, end: Option<u16>, record: u16) {
        let matches = |link: &Option<FlagLink>, far: Option<u16>| {
            link.is_some_and(|link| link.far == far && (far.is_none() || link.road == record))
        };
        if let Some(flag) = self.flags.get_mut(start)
            && let Some(slot) = flag.links.iter_mut().find(|link| matches(link, end))
        {
            *slot = None;
        }
        if let Some(end) = end
            && let Some(flag) = self.flags.get_mut(end)
            && let Some(slot) = flag.links.iter_mut().find(|link| matches(link, Some(start)))
        {
            *slot = None;
        }
    }

    /// Places a flag of `kind` on `node` for the node's owner (`0x25677`) and returns its record.
    /// The node must allow a flag ([`SiteFlags::NO_FLAG`] clear). A flag on a donkey road becomes
    /// a donkey flag. A road through the node is split in two unless `split` is false. Then the
    /// object word is written and the node and two rings are recomputed twice.
    ///
    /// `None` when the node rules a flag out, the flag pool is full, or nobody owns the node.
    /// `S2.EXE` does not look at the owner and would copy slot 9's "nobody" into the record; no
    /// command reaches that case.
    pub fn place_flag(
        &mut self,
        nodes: &mut NodeMap,
        node: Node,
        kind: FlagKind,
        split: bool,
    ) -> Option<u16> {
        let data = nodes.get(node);
        if data.sites.flags.contains(SiteFlags::NO_FLAG) {
            return None;
        }
        let road = self.road_through(nodes, node);
        let kind = match road.and_then(|record| self.road(record)) {
            Some(road) if road.kind == RoadKind::Donkey => FlagKind::Donkey,
            _ => kind,
        };
        let flag = self.add_flag(Flag::new(kind, data.owner?, node))?;
        if let Some(road) = road
            && split
        {
            self.split_road(nodes, road, flag);
        }
        self.set_object(node, MapObject::Flag(flag));
        for _ in 0..2 {
            refresh_sites(nodes, self, node, 2);
        }
        Some(flag)
    }

    /// Splits road `record` at the new flag `flag` on one of its inner nodes (`0x286cc`). The old
    /// road is unlinked and freed. Then two roads of its type are created, start to new flag and
    /// new flag to end, each keeping the low byte of the old traffic counter and each ordering a
    /// helper.
    ///
    /// Every carrier and walker of the old road moves to the half it stands on, keeping its
    /// place; an idle carrier starts afresh at state idle, dropping any idle animation. A half
    /// that got a carrier cancels its order. The first half without one takes the old order if a
    /// helper is already on its way, and its own fresh order otherwise.
    ///
    /// Wares waiting at either end for the other now wait for the new flag.
    ///
    /// Not yet modelled: the helper of an old order that neither half took, which `S2.EXE` makes
    /// a lost settler. That needs two carriers and an open order on one road at once.
    fn split_road(&mut self, nodes: &mut NodeMap, record: u16, flag: u16) {
        let (Some(road), Some(at)) = (self.roads.get(record).cloned(), self.flag(flag).map(|f| f.node))
        else {
            return;
        };
        let Some(cut) = road.path.iter().position(|&node| node == at) else {
            return;
        };
        if let Some(end) = road.end {
            self.redirect_wares(road.start, end, flag);
            self.redirect_wares(end, road.start, flag);
        }
        self.unlink_road(road.start, road.end, record);
        self.free_road(nodes, record);
        let traffic = road.traffic % 256;
        let parts = [
            (road.start, Some(flag), road.path[..cut].to_vec()),
            (flag, road.end, road.path[cut + 1..].to_vec()),
        ];
        let mut halves = [None; 2];
        for (half, (start, end, path)) in halves.iter_mut().zip(parts) {
            *half = self.create_road(nodes, road.kind, start, end, path).ok();
            if let Some(made) = half.and_then(|half| self.roads.get_mut(half)) {
                made.traffic = traffic;
            }
        }
        let k = u8::try_from(cut + 1).unwrap_or(u8::MAX);
        let manned = self.move_figures_to_halves(nodes, &road, record, halves, k);
        let mut old = road.order;
        for (half, manned) in halves.into_iter().zip(manned) {
            let Some(half) = half else {
                continue;
            };
            let fresh = self.roads.get(half).and_then(|road| road.order);
            let start = self.roads.get(half).map(|road| road.start);
            if manned {
                if let Some(fresh) = fresh {
                    self.orders.remove(fresh);
                }
                if let Some(road) = self.roads.get_mut(half) {
                    road.order = None;
                }
                continue;
            }
            let Some(held) = old.take() else {
                continue;
            };
            let on_way = self.orders.get(held).is_some_and(|order| order.delivering);
            if !on_way {
                self.orders.remove(held);
                continue;
            }
            if let (Some(order), Some(start)) = (self.orders.get_mut(held), start) {
                order.flag = start;
            }
            if let Some(fresh) = fresh {
                self.orders.remove(fresh);
            }
            if let Some(road) = self.roads.get_mut(half) {
                road.order = Some(held);
            }
        }
        if let Some(held) = old {
            self.orders.remove(held);
        }
    }

    /// Step 5 of [`Self::split_road`]: moves each carrier and walker of the old road `old`
    /// (record `record`) to the half it stands on, node by node from the building behind the
    /// start flag to the one behind the end flag. A figure before position `k` keeps its
    /// position on the first half; any other moves to the second at its position less `k`. A
    /// carrier delivering into a building gets the first half at the start flag or its building,
    /// otherwise the second. Returns which halves got a carrier.
    fn move_figures_to_halves(
        &mut self,
        nodes: &NodeMap,
        old: &Road,
        record: u16,
        halves: [Option<u16>; 2],
        k: u8,
    ) -> [bool; 2] {
        let (width, height) = (nodes.width(), nodes.height());
        let flag_node = |flag: Option<u16>| flag.and_then(|flag| self.flag(flag)).map(|flag| flag.node);
        let mut route = Vec::new();
        let mut at_start = Vec::new();
        if let Some(start) = flag_node(Some(old.start)) {
            let door = start.neighbour(Direction::NorthWest, width, height);
            route.push(door);
            at_start.extend([start, door]);
        }
        route.extend(self.road_nodes(old));
        if let Some(end) = flag_node(old.end) {
            route.push(end.neighbour(Direction::NorthWest, width, height));
        }
        let mut manned = [false; 2];
        for node in route {
            let figures: Vec<u16> = self.figures_at(node).map(|(figure, _)| figure).collect();
            for figure in figures {
                let (index, position) = match self.figure(figure).map(|figure| figure.kind) {
                    Some(FigureKind::Carrier(carrier)) if carrier.on.road == record => {
                        half_of(carrier.on.position, k)
                    }
                    Some(FigureKind::Walker(walker)) if walker.on.road == record => {
                        half_of(walker.on.position, k)
                    }
                    Some(FigureKind::Delivering(delivery)) if delivery.road == Some(record) => {
                        (usize::from(!at_start.contains(&node)), 0)
                    }
                    _ => continue,
                };
                let Some(half) = halves[index] else {
                    continue;
                };
                let mut freed = None;
                let mut carrier = false;
                match self.figure_mut(figure).map(|figure| &mut figure.kind) {
                    Some(FigureKind::Carrier(on_road)) => {
                        on_road.on.road = half;
                        on_road.on.position = position;
                        if matches!(
                            on_road.state,
                            CarrierState::Waiting | CarrierState::Animating | CarrierState::Fetching
                        ) {
                            on_road.state = CarrierState::Idle;
                            freed = on_road.animation.take();
                        }
                        carrier = true;
                    }
                    Some(FigureKind::Walker(walker)) => {
                        walker.on.road = half;
                        walker.on.position = position;
                    }
                    Some(FigureKind::Delivering(delivery)) => {
                        delivery.road = Some(half);
                        carrier = true;
                    }
                    _ => {}
                }
                if let Some(animation) = freed {
                    self.animations.remove(animation);
                }
                if carrier {
                    manned[index] = true;
                    if let Some(road) = self.roads.get_mut(half) {
                        road.carriers = road.carriers.saturating_add(1);
                        road.served = true;
                    }
                }
            }
        }
        manned
    }

    /// Removes flag `record` and every road at it (`0x25763`), in direction order. Then its
    /// object word is cleared, the record freed and the node and two rings recomputed twice.
    ///
    /// Not yet modelled: the building on its north-west neighbour, the wares waiting at it and the
    /// figures and jobs tied to it.
    pub fn remove_flag(&mut self, nodes: &mut NodeMap, record: u16) {
        let Some(flag) = self.flags.get(record).copied() else {
            return;
        };
        for link in flag.links.into_iter().flatten() {
            self.remove_road(nodes, link.road);
        }
        self.set_object(flag.node, MapObject::Nothing);
        self.flags.remove(record);
        for _ in 0..2 {
            refresh_sites(nodes, self, flag.node, 2);
        }
    }
}

/// The half a figure at `position` on a road split at position `k` goes to, and its position
/// on that half.
const fn half_of(position: u8, k: u8) -> (usize, u8) {
    if position < k {
        (0, position)
    } else {
        (1, position - k)
    }
}

#[cfg(test)]
mod tests;
