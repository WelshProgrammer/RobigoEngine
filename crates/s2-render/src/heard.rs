//! What a view lets the player hear: the counters the ambient-sound picker reads after a frame,
//! and the things drawn that roll for a sound of their own.
//!
//! `S2.EXE` fills these while it draws the map view, over the object pass's walk. Here they are
//! counted over the same walk without drawing, so a caller can ask for a view of the original's
//! size wherever it likes, however much of the map its own window shows.

use s2_sim::{
    AnimalState, BuildingKind, BuildingState, FigureKind, MapObject, Node, NodeData, NodeFlags, Species,
    Transport, World,
};

use crate::terrain::{Point, Terrain, Walk};

/// The original's map view on its 640 × 480 screen, in world pixels: how much of the map the
/// player hears at once.
pub const HEARING: (u32, u32) = (616, 456);

/// The terrain types whose node-to-east triangle counts for the terrain sounds: those the type
/// table gives class 7 or 8 (`0x1eee2`).
const SOUNDING_TERRAIN: [u8; 3] = [5, 6, 18];

/// Productivity per pig a pig farm shows (`0x1a81c`).
const PRODUCTIVITY_PER_PIG: u8 = 15;

/// The most pigs a pig farm shows.
const MOST_PIGS: u16 = 5;

/// The phases a working building's animation cycles through.
const PHASES: u32 = 8;

/// The phase of the mill's sails at which it may creak (`0x1a74c`).
const CREAKING_PHASE: u32 = 6;

/// The phase at which a fire crackles (`0x93ead`).
const CRACKLING_PHASE: u32 = 0;

/// The frame of a rowing boat at which its oars splash (`0x1bc24`).
const SPLASHING_FRAME: u32 = 1;

/// What one frame of a view would let the player hear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Heard {
    /// Ducks drawn alive (`0x201674`).
    pub ducks: u16,
    /// Sheep drawn alive (`0x201676`).
    pub sheep: u16,
    /// Trees drawn (`0x201670`).
    pub trees: u16,
    /// Pigs drawn at finished pig farms (`0x201678`).
    pub pigs: u16,
    /// Nodes in sight whose node-to-east triangle is water-like terrain (`0x20166c`).
    pub terrain: u16,
    /// Working mills drawn with their sails at the creaking phase, each of which rolls once.
    pub mills: u16,
    /// Ships drawn, each of which rolls once.
    pub ships: u16,
    /// Fires drawn at the crackling phase, each of which asks for its sound.
    pub fires: u16,
    /// Boats drawn rowing at the splashing frame, each of which asks for its sound.
    pub boats: u16,
}

/// What a `view`-sized map view scrolled to `scroll` world pixels lets the player hear, with the
/// animation counter at `anim`. Only nodes the player can see count.
#[must_use]
pub fn heard(terrain: &Terrain, world: &World, scroll: Point, view: (u32, u32), anim: u32) -> Heard {
    let map = terrain.map();
    let period = terrain.world_size();
    let wrap = |at: i32, size: u32| at.rem_euclid(i32::try_from(size.max(1)).unwrap_or(i32::MAX));
    let walk = Walk::objects(wrap(scroll.x, period.0), wrap(scroll.y, period.1), view.0, view.1);
    let mut heard = Heard::default();
    for row in walk.row..walk.row + walk.rows {
        for column in walk.column..walk.column + walk.columns {
            let node = s2_sim::Node::wrapped(column, row, map.width(), map.height());
            if !terrain.node_visible(node) {
                continue;
            }
            if sounding(&map.get(node)) {
                heard.terrain += 1;
            }
            match world.object(node) {
                MapObject::Tree(_) => heard.trees += 1,
                MapObject::Fire(fire) if crackling(node, fire.count, anim) => heard.fires += 1,
                MapObject::Building(record) => {
                    if let Some(building) = world.building(record) {
                        let finished = building.state.contains(BuildingState::FINISHED);
                        if finished && building.kind == BuildingKind::PIG_FARM {
                            heard.pigs += pigs(building.productivity);
                        }
                        let working = building.state.contains(BuildingState::WORKING);
                        if finished
                            && working
                            && building.kind == BuildingKind::MILL
                            && creaking(record, anim)
                        {
                            heard.mills += 1;
                        }
                    }
                }
                _ => {}
            }
            for (_, figure) in world.figures_at(node) {
                match figure.kind {
                    FigureKind::Animal {
                        species: Species::Duck,
                        state,
                        ..
                    } if state != AnimalState::Dead => heard.ducks += 1,
                    FigureKind::Animal {
                        species: Species::Sheep,
                        state,
                        ..
                    } if state != AnimalState::Dead => heard.sheep += 1,
                    FigureKind::Ship { .. } => heard.ships += 1,
                    FigureKind::Carrier(carrier)
                        if carrier.transport == Transport::Boat
                            && figure.walking()
                            && anim % PHASES == SPLASHING_FRAME =>
                    {
                        heard.boats += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    heard
}

/// Whether a node counts for the terrain sounds. The original indexes the type table with the
/// whole slot 2 byte, flags included; a flagged byte lands past the table on bytes that are
/// never class 7 or 8 in the shipped executable, so a node with either flag never counts.
fn sounding(node: &NodeData) -> bool {
    SOUNDING_TERRAIN.contains(&node.terrain[1])
        && !node
            .flags
            .intersects(NodeFlags::MOUNTAIN_ROADS | NodeFlags::BORDER)
}

/// How many pigs a finished pig farm shows at `productivity`.
fn pigs(productivity: u8) -> u16 {
    (u16::from(productivity / PRODUCTIVITY_PER_PIG) + 1).min(MOST_PIGS)
}

/// Whether mill `record`'s sails stand at the creaking phase with the animation counter at
/// `anim`.
fn creaking(record: u16, anim: u32) -> bool {
    (u32::from(record) + anim) % PHASES == CREAKING_PHASE
}

/// Whether a fire at `node` whose count is `count` stands at its crackling phase with the
/// animation counter at `anim`: its frame is `(3x + 2y + anim + count) & 7`.
fn crackling(node: Node, count: u8, anim: u32) -> bool {
    let place = 3 * u32::from(node.x) + 2 * u32::from(node.y);
    (place + anim + u32::from(count)) % PHASES == CRACKLING_PHASE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pig_farm_shows_one_pig_per_fifteen_points_of_productivity_up_to_five() {
        assert_eq!(pigs(0), 1);
        assert_eq!(pigs(14), 1);
        assert_eq!(pigs(15), 2);
        assert_eq!(pigs(59), 4);
        assert_eq!(pigs(60), 5);
        assert_eq!(pigs(100), 5);
    }

    #[test]
    fn water_like_terrain_counts_unless_the_byte_carries_a_flag() {
        let node = |terrain: u8, flags| NodeData {
            terrain: [0, terrain],
            flags,
            ..NodeData::default()
        };
        assert!(sounding(&node(5, NodeFlags::empty())));
        assert!(sounding(&node(6, NodeFlags::empty())));
        assert!(sounding(&node(18, NodeFlags::empty())));
        assert!(!sounding(&node(4, NodeFlags::empty())));
        assert!(!sounding(&node(5, NodeFlags::BORDER)));
        assert!(!sounding(&node(6, NodeFlags::MOUNTAIN_ROADS)));
        assert!(
            sounding(&node(5, NodeFlags::HARBOUR)),
            "a slot 1 flag leaves slot 2 alone"
        );
    }

    #[test]
    fn a_fire_crackles_where_its_frame_comes_round_to_zero() {
        assert!(crackling(Node { x: 0, y: 0 }, 0, 0));
        assert!(crackling(Node { x: 1, y: 1 }, 3, 0), "3 + 2 + 3 is 8");
        assert!(crackling(Node { x: 0, y: 0 }, 7, 1));
        assert!(!crackling(Node { x: 0, y: 0 }, 0, 1));
    }

    #[test]
    fn a_mill_creaks_one_phase_in_eight() {
        assert!(creaking(6, 0));
        assert!(creaking(0, 6));
        assert!(creaking(3, 11));
        assert!(!creaking(0, 0));
    }
}
