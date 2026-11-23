//! A military building changing hands, and the soldiers whose target or home it was.

use super::Role;
use crate::figures::{Figure, FigureKind};
use crate::game::Game;
use crate::grid::{Cursor, Direction, Node};
use crate::messages::MessageId;
use crate::military::{Noted, RECLAIM_REACH, RELEASE_BORDER, straight_distance};
use crate::nodes::NodeFlags;
use crate::territory::{border_pass, claim_order, protected_nodes};
use crate::world::{Flag, MapObject, Player};

/// The steps from a captured building's node to the seven nodes round it whose flags go with
/// their buildings (`0x1fa5c3`): south-west, west, north-west of west, north-west of
/// north-west, north-west, north-west of north-east and north-east.
const CLEARED_ROUND: [Direction; 7] = [
    Direction::SouthWest,
    Direction::NorthWest,
    Direction::NorthWest,
    Direction::NorthEast,
    Direction::SouthEast,
    Direction::NorthEast,
    Direction::SouthEast,
];

/// How far past its strength a captured building's old owner's land is looked at.
const CAPTURE_LOOK: u16 = 2;

/// An attacker this close to a building it helped capture may move in (`0x2ffee`).
const MOVE_IN_WITHIN: u32 = 6;

/// The nodes of rings 1 to `last` round `centre` with their ring, each after the step that
/// reaches it, nodes past a map edge left out (`0x22498`).
fn rings_after((width, height): (u16, u16), centre: Node, last: u16) -> Vec<(u16, Node)> {
    let mut nodes = Vec::new();
    let mut start = Cursor::at(centre);
    for ring in 1..=last {
        start = start.step(Direction::NorthWest);
        let mut at = start;
        for direction in Direction::ALL {
            for _ in 0..ring {
                at = at.step(direction);
                nodes.extend(at.node(width, height).map(|node| (ring, node)));
            }
        }
    }
    nodes
}

impl Game {
    /// The stock building `building` of type `row` holds moves from `old`'s ware count to
    /// `owner`'s (`0x303b2`, `0x303da`), one input slot at a time. The wares stay in the
    /// building.
    fn move_stock(
        &mut self,
        building: &crate::world::Building,
        row: &crate::world::BuildingType,
        old: Player,
        owner: Player,
    ) {
        for (slot, input) in row.inputs.iter().enumerate() {
            let Some(ware) = *input else { continue };
            let held = u16::from(building.stock[slot]);
            if held == 0 {
                continue;
            }
            self.world.drop_ware(old, ware, held);
            self.world.count_ware(owner, ware, held);
        }
    }

    /// The ten protected nodes round the building on `node` go from `old` to `owner`. With the
    /// bug fixes off the land counts move by 9 whatever the nodes held, as
    /// [`crate::NodeMap::book_capture`] describes; with them on each node is counted as it
    /// changes hands.
    fn hand_over_protected(&mut self, node: Node, old: Player, owner: Player) {
        let booked = !self.world.bug_fixes.on();
        if booked {
            self.nodes.book_capture(old, owner);
        }
        let size = (self.nodes.width(), self.nodes.height());
        for node in protected_nodes(size, node) {
            let mut data = self.nodes.get(node);
            data.owner = Some(owner);
            if booked {
                self.nodes.set_unbooked(node, data);
            } else {
                self.nodes.set(node, data);
            }
        }
    }

    /// Building `record` is taken by `owner` (`0x30214`). Its soldiers are counted out, the
    /// soldiers tied to it go home or move in, it is emptied, and it gets a new flag of the
    /// same kind for `owner`, losing its roads. Flags on seven nodes round it go with their
    /// buildings, the old owner's land within its strength goes back to nobody, and the
    /// occupied military buildings and headquarters near it spread again, oldest occupation
    /// first. Then the border pass, the noted flags, buildings and roads left on land of
    /// another player or on a border, and the new owner's view round it. It keeps its coins,
    /// stock, promotion timer and stop setting, and the stock it holds moves from the old
    /// owner's ware count to the new owner's (`0x303b2`, `0x303da`).
    pub(super) fn capture(&mut self, record: u16, owner: Player) {
        let Some(building) = self.world.building(record).copied() else {
            return;
        };
        let Some(old) = self.world.building_owner(record) else {
            return;
        };
        let Some(row) = self.world.building_types().get(building.kind) else {
            return;
        };
        self.post_message(
            MessageId::BUILDING_LOST,
            old,
            Some(building.node),
            Some(building.kind),
            0,
        );
        let held_before = self.land_held();
        let military = self.world.military_of(record).map(|(military, _)| military);
        if let Some(held) = military.and_then(|military| self.world.military.get_mut(military)) {
            held.inside = 0;
            held.requested = 1;
            held.ranks = [0; 5];
        }
        self.release_soldiers(record, owner, true);
        self.vacate(record);
        if let Some(held) = military.and_then(|military| self.world.military.get_mut(military)) {
            held.owner = owner;
        }
        self.replace_flag(record, owner);
        if let Some(site) = self.world.building_mut(record) {
            site.on_way[0] = 0;
            site.requested = 0;
        }
        self.hand_over_protected(building.node, old, owner);
        let size = (self.nodes.width(), self.nodes.height());
        self.world.count_building(owner, building.kind);
        self.world.uncount_building(old, building.kind);
        self.move_stock(&building, &row, old, owner);
        let mut at = building.node;
        for direction in CLEARED_ROUND {
            at = at.neighbour(direction, size.0, size.1);
            if let Some(flag) = self.world.flag_at(at) {
                self.remove_flag_record(flag);
            }
        }
        let mut noted = Noted::default();
        let strength = u16::from(row.strength);
        for (ring, node) in rings_after(size, building.node, strength + CAPTURE_LOOK) {
            let mut data = self.nodes.get(node);
            if data.protected {
                continue;
            }
            if ring <= strength && data.owner == Some(old) {
                data.strength = 0;
                data.owner = None;
                self.nodes.set(node, data);
            }
            self.note(node, &mut noted);
        }
        let mut holders = vec![record];
        holders.extend(
            claim_order(size, building.node, row.strength + RECLAIM_REACH)
                .into_iter()
                .skip(1)
                .filter_map(|node| self.world.object(node).building())
                .filter(|&other| other != record && self.holds_land(other)),
        );
        holders.sort_by_key(|&other| self.world.building(other).map_or(u16::MAX, |b| b.occupied));
        for holder in holders {
            let reach = self
                .world
                .building(holder)
                .and_then(|holder| self.world.building_types().get(holder.kind))
                .map_or(0, |row| row.strength);
            self.spread_rings(holder, reach);
        }
        border_pass(
            &mut self.nodes,
            &self.world,
            building.node,
            strength + RELEASE_BORDER,
        );
        self.remove_stranded(noted);
        self.nodes.reveal(building.node, owner, u16::from(row.sight));
        self.land_lost_messages(held_before, old, owner, building.node);
        self.post_message(
            MessageId::BUILDING_TAKEN,
            owner,
            Some(building.node),
            Some(building.kind),
            0,
        );
    }

    /// How many nodes each player owns.
    fn land_held(&self) -> [u32; Player::COUNT] {
        std::array::from_fn(|place| {
            Player::new(u8::try_from(place).unwrap_or_default()).map_or(0, |player| self.nodes.owned(player))
        })
    }

    /// Messages every player other than `old` and `owner` who holds less land than
    /// `held_before` says (`0x30214` step 17).
    fn land_lost_messages(
        &mut self,
        held_before: [u32; Player::COUNT],
        old: Player,
        owner: Player,
        node: Node,
    ) {
        for (place, before) in held_before.into_iter().enumerate() {
            let Some(player) = Player::new(u8::try_from(place).unwrap_or_default()) else {
                continue;
            };
            if player != old && player != owner && self.nodes.owned(player) < before {
                self.post_message(MessageId::LAND_LOST, player, Some(node), None, 0);
            }
        }
    }

    /// Building `record`'s flag is replaced (`0x25763` without the building, `0x25677`): the
    /// old flag goes with its roads and everything tied to it, and a new flag of the same kind
    /// for `owner` stands on the same node.
    fn replace_flag(&mut self, record: u16, owner: Player) {
        let Some(old) = self
            .world
            .building(record)
            .and_then(|building| self.world.flag(building.flag))
            .copied()
        else {
            return;
        };
        let Some(flag) = self.world.building(record).map(|building| building.flag) else {
            return;
        };
        self.remove_flag_only(flag);
        let Some(new) = self.world.add_flag(Flag::new(old.kind, owner, old.node)) else {
            return;
        };
        self.world.set_object(old.node, MapObject::Flag(new));
        for _ in 0..2 {
            crate::sites::refresh_sites(&mut self.nodes, &self.world, old.node, 2);
        }
        if let Some(building) = self.world.building_mut(record) {
            building.flag = new;
        }
    }

    /// The flags, buildings and roads noted on a captured building's land that now stand on
    /// land of a player other than their own, or on a border node, go (`0x30214` step 18):
    /// flags with their buildings, buildings demolished, roads whole.
    fn remove_stranded(&mut self, noted: Noted) {
        let stranded = |game: &Self, node: Node, owner: Option<Player>| {
            let data = game.nodes.get(node);
            data.owner != owner || data.flags.contains(NodeFlags::BORDER)
        };
        for flag in noted.flags {
            if let Some(flag_record) = self.world.flag(flag).copied()
                && stranded(self, flag_record.node, Some(flag_record.owner))
            {
                self.remove_flag_record(flag);
            }
        }
        for building in noted.buildings {
            if let Some(node) = self.world.building(building).map(|building| building.node)
                && stranded(self, node, self.world.building_owner(building))
            {
                self.demolish(building);
            }
        }
        for road in noted.roads {
            let Some(path) = self.world.road(road).map(|road| road.path.clone()) else {
                continue;
            };
            let owner = self
                .world
                .road(road)
                .and_then(|road| self.world.flag(road.start))
                .map(|flag| flag.owner);
            if path.iter().any(|&node| stranded(self, node, owner)) {
                self.remove_road_record(road);
            }
        }
    }

    /// Every soldier tied to building `record`, which is being demolished or, with `capture`,
    /// taken by `owner` (`0x2ffee`), in figure order. An attacker of `owner` close to a
    /// captured military building with room moves in, the room counted again for each, so the
    /// ones moving in fill it; any other attacker of it goes home. A
    /// soldier whose home it was loses its home and goes home, which makes it lost, unless it is
    /// a defender or soldier going home still in its door. A soldier whose role a fatal hit
    /// cleared goes home too. Soldiers in a fight are left until it ends.
    pub(crate) fn release_soldiers(&mut self, record: u16, owner: Player, capture: bool) {
        let node = self.world.building(record).map(|building| building.node);
        for figure in 0..self.world.figure_records() {
            let Some(Figure {
                kind: FigureKind::Soldier(soldier),
                ..
            }) = self.world.figure(figure).copied()
            else {
                continue;
            };
            let Some((_, held)) = self.world.soldier_of(figure) else {
                continue;
            };
            let at = self.world.figure(figure).map(Figure::node);
            let in_door = self.world.figure(figure).is_some_and(Figure::in_door);
            let home = soldier.home == Some(record);
            match held.role {
                Role::Attacker if held.target == Some(record) => {
                    let near = at
                        .zip(node)
                        .is_some_and(|(at, node)| straight_distance(at, node) < MOVE_IN_WITHIN);
                    let room = self
                        .world
                        .military_of(record)
                        .is_some_and(|(_, held)| held.capacity > held.inside + held.requested);
                    if capture && soldier.owner == owner && !in_door && room && near {
                        self.move_in(figure, record);
                        continue;
                    }
                    self.update_record(figure, |held| held.target = None);
                }
                Role::Attacker if home => self.lose_home(figure),
                Role::Defender | Role::Home if home && !in_door => self.lose_home(figure),
                Role::None => {}
                _ => continue,
            }
            if self
                .world
                .soldier_of(figure)
                .is_some_and(|(_, held)| held.opponent.is_none())
            {
                self.drop_path(figure);
                self.go_home(figure);
            }
        }
    }

    /// Soldier figure `figure` no longer has a home.
    fn lose_home(&mut self, figure: u16) {
        if let Some(FigureKind::Soldier(soldier)) = self.figure_kind_mut(figure) {
            soldier.home = None;
        }
    }

    /// Attacker figure `figure` takes building `record`, which it helped capture, as its home
    /// and walks there after any fight it is in (`0x2ffee`).
    fn move_in(&mut self, figure: u16, record: u16) {
        let Some(FigureKind::Soldier(soldier)) = self.world.figure(figure).map(|figure| figure.kind) else {
            return;
        };
        if let Some(home) = soldier.home
            && let Some((military, _)) = self.world.military_of(home)
            && let Some(held) = self.world.military.get_mut(military)
        {
            held.requested = held.requested.saturating_sub(1);
        }
        self.update_record(figure, |held| held.role = Role::Home);
        if let Some(FigureKind::Soldier(soldier)) = self.figure_kind_mut(figure) {
            soldier.home = Some(record);
        }
        if let Some((military, _)) = self.world.military_of(record)
            && let Some(held) = self.world.military.get_mut(military)
        {
            held.requested = held.requested.saturating_add(1);
        }
    }
}
