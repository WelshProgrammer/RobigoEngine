//! Figures: the things that walk the map, kept in per-node lists: animals,
//! and the settlers that carry goods along the roads.

use crate::grid::{Direction, Node};
use crate::job::Job;
use crate::rng::GameRng;
use crate::ware::WareKind;
use crate::world::Player;

/// An animal species, as `S2.EXE` numbers them in its species table (`0x1fbb24`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Species {
    /// 0.
    PolarBear,
    /// 1.
    LightRabbit,
    /// 2.
    GreyRabbit,
    /// 3.
    Fox,
    /// 4.
    Stag,
    /// 5.
    Deer,
    /// 6.
    Duck,
    /// 7.
    Donkey,
    /// 8.
    Sheep,
}

impl Species {
    /// The species of a map file's animal list value, looked up in the table after the
    /// `WORLD_V1.0` signature (`0x1f756e`). The editor's 1 to 6 are rabbit, fox, stag, deer, duck
    /// and sheep. Maps have one rabbit value: each rabbit costs one draw from `rng` and is grey on
    /// 0, light otherwise. No other value draws. `None` past the table's ten entries, which
    /// `S2.EXE` does not check. Inferred: such records are skipped. No shipped map has one.
    pub fn from_map(value: u8, rng: &mut GameRng) -> Option<Self> {
        Some(match value {
            0 => Self::PolarBear,
            1 if rng.up_to(1) == 0 => Self::GreyRabbit,
            1 => Self::LightRabbit,
            2 => Self::Fox,
            3 => Self::Stag,
            4 | 7 => Self::Deer,
            5 | 8 => Self::Duck,
            6 => Self::Sheep,
            9 => Self::Donkey,
            _ => return None,
        })
    }

    /// The species with `index` in the species table, as a mission script names it; `None`
    /// past the table.
    #[must_use]
    pub const fn from_index(index: u8) -> Option<Self> {
        Some(match index {
            0 => Self::PolarBear,
            1 => Self::LightRabbit,
            2 => Self::GreyRabbit,
            3 => Self::Fox,
            4 => Self::Stag,
            5 => Self::Deer,
            6 => Self::Duck,
            7 => Self::Donkey,
            8 => Self::Sheep,
            _ => return None,
        })
    }
}

/// How a species behaves: its row of the species table (`0x1fbb24`, 56 bytes per species).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SpeciesTraits {
    /// Counts towards the spawner's limit of animals per block (`+0`).
    pub counted: bool,
    /// Swims: walks only along edges with water on both sides (`+2`).
    pub swims: bool,
    /// Walks only to nodes with class 4 ground around them (`+4`).
    pub polar: bool,
    /// Extra weight of a neighbour with a grown tree (`+6`).
    pub tree_weight: u8,
    /// Walking speeds are divided by this (`+7`).
    pub slowness: u8,
}

impl Species {
    /// The species' row of the species table.
    #[must_use]
    pub const fn traits(self) -> SpeciesTraits {
        let (counted, swims, polar, tree_weight, slowness) = match self {
            Self::PolarBear => (false, false, true, 0, 2),
            Self::LightRabbit | Self::GreyRabbit | Self::Sheep => (true, false, false, 0, 1),
            Self::Fox => (true, false, false, 1, 1),
            Self::Stag | Self::Deer => (true, false, false, 3, 1),
            Self::Duck => (false, true, false, 0, 4),
            Self::Donkey => (false, false, false, 0, 2),
        };
        SpeciesTraits {
            counted,
            swims,
            polar,
            tree_weight,
            slowness,
        }
    }
}

/// The distance a figure covers walking from one node to the next.
pub const EDGE_LENGTH: u16 = 256;

/// What an animal is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub enum AnimalState {
    /// Standing or walking.
    #[default]
    Active,
    /// Resting: shows the first walking frame of its direction.
    Resting,
    /// Shot by its hunter (`+0x0d = 1`): it stands still until the shot ends.
    Shot,
    /// Dead.
    Dead,
}

/// What a figure is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FigureKind {
    /// An animal (owner 7, type 1).
    Animal {
        /// Its species.
        species: Species,
        /// What it is doing.
        state: AnimalState,
        /// The hunter after it (`+0x10`), from the moment a hunter picks it.
        hunter: Option<u16>,
    },
    /// A settler walking out of a storehouse to its flag to serve an order (kind 56, state 3).
    Leaving {
        /// Owner.
        owner: Player,
        /// Its job (`+0x01`).
        job: Job,
        /// The order it serves (`+0x0d`).
        order: Option<u16>,
    },
    /// A settler walking the road network to the flag its order names (kind 16).
    Walker(Walker),
    /// A carrier on its road (kind 8).
    Carrier(Carrier),
    /// A lost settler walking a cross-country path back to a storehouse's flag (kind 32,
    /// sub-state 3).
    Homeward {
        /// Owner.
        owner: Player,
        /// Its job (`+0x01`).
        job: Job,
        /// Its path record (`+0x10`).
        path: u16,
        /// What is left of its lost countdown (`+0x0e`). Demolition reads the same byte as a
        /// building record, so a match makes it lost again, unless it is a helper.
        countdown: u8,
    },
    /// A settler walking from a storehouse's flag in through its door, to be added to its stock
    /// (kind 56, `+0x0c` 3 with bit 7 set).
    Entering {
        /// Owner.
        owner: Player,
        /// Its job (`+0x01`).
        job: Job,
    },
    /// A worker walking from its workplace's flag in through the door, to start work there
    /// (kind 56, `+0x0c` 0 with bit 7 set; `0x37cb4`).
    Settling {
        /// Owner.
        owner: Player,
        /// Its job (`+0x01`).
        job: Job,
        /// The order it serves (`+0x0d`).
        order: u16,
    },
    /// A settler bringing a ware out of a storehouse to its flag, and walking back in (kind
    /// 56, `+0x0c` 2 with bit 6 set, then with bit 7 as well).
    Bringing {
        /// Owner.
        owner: Player,
        /// The storehouse's building record (`+0x0d`).
        building: u16,
        /// The ware it carries (`+0x0f`), until it puts it down at the flag.
        ware: Option<u16>,
        /// The bitmap of the ware it shows (`+0x01`).
        shows: Option<u8>,
        /// Walking back in (`+0x0c` bit 7).
        inside: bool,
    },
    /// A carrier taking a ware in through the door of the building at its destination flag, and
    /// walking back out to its road (kind 56, `+0x0c` 1 with bits 6 and 7 set, then with bit 6).
    Delivering(Delivery),
    /// A builder at work on a site: an owner 7, type 2 record on no node list, which walks
    /// between the site's waypoints by pixel offsets (`0x3340c`).
    Builder(Builder),
    /// A planer levelling a site's ground: kind 32 walking between the nodes it levels,
    /// kind 40 while it digs (`0x37e0a`).
    Planer(Planer),
    /// A workplace's worker out on a trip to a node and back (`0x334c4`).
    Gatherer(Gatherer),
    /// A geologist or scout the player sent to a flag, working cross-country from it: kind
    /// 32 while it walks, 40 while a geologist taps.
    Explorer(Explorer),
    /// A workplace's worker carrying a finished ware out to the flag and walking back in (kind
    /// 56, `+0x0c` 6, then 6 with bit 7 set; `0x36bc8`).
    Porter(Porter),
    /// A soldier outside a building: attacking, defending or going home.
    Soldier(Soldier),
    /// A ship (owner 7, type 0).
    Ship {
        /// Owner (`+0x0c`).
        owner: Player,
        /// Its ship record (`+0x0d`).
        record: u16,
    },
    /// A settler that lost its road or its way (kind 24).
    Lost {
        /// Owner.
        owner: Player,
        /// Its job (`+0x01`).
        job: Job,
        /// Steps left before it gives up and disappears (`+0x0e`).
        countdown: u8,
    },
}

/// Where a figure on a road is: which road, how far along it, and which end it heads for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct OnRoad {
    /// The road (`+0x0c`).
    pub road: u16,
    /// Position from 0 at the start flag to the road's edge count at the end flag (`+0x10`).
    pub position: u8,
    /// Heading for the start flag (`+0x11` bit 3); otherwise for the end flag.
    pub to_start: bool,
}

/// A settler of kind 16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Walker {
    /// Owner.
    pub owner: Player,
    /// Its job (`+0x01`).
    pub job: Job,
    /// The order it serves (`+0x0e`).
    pub order: u16,
    /// The mode of that order (`+0x11` bits 0–2); 3 for a road's carrier.
    pub mode: u8,
    /// The road it walks.
    pub on: OnRoad,
}

/// How a carrier gets about (`+0x09`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Transport {
    /// 0: on foot.
    Foot,
    /// 1: in a boat, on a water road.
    Boat,
    /// 2: a pack donkey.
    Donkey,
}

/// What a carrier is doing (`+0x11` bits 0–2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CarrierState {
    /// 0: nothing to carry, not yet facing the viewer at the middle of its road.
    Idle,
    /// 1: nothing to carry, waiting at the middle and rolling for idle animations.
    Waiting,
    /// 2: on its way to fetch a ware.
    Fetching,
    /// 3: carrying a ware.
    Carrying,
    /// 4: playing an idle animation.
    Animating,
}

/// A carrier of kind 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Carrier {
    /// Owner.
    pub owner: Player,
    /// Its road and where on it.
    pub on: OnRoad,
    /// What it is doing.
    pub state: CarrierState,
    /// How it gets about.
    pub transport: Transport,
    /// The ware it fetches or carries (`+0x0e`).
    pub ware: Option<u16>,
    /// The bitmap of the ware it shows (`+0x01`).
    pub shows: Option<u8>,
    /// Its running idle animation (`+0x0a`).
    pub animation: Option<u16>,
}

/// A carrier walking a ware into a building.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Delivery {
    /// Owner.
    pub owner: Player,
    /// How it gets about (`+0x09`); a pack donkey walks in too.
    pub transport: Transport,
    /// The road it comes back to (`+0x0d`), or `None` once that road is removed.
    pub road: Option<u16>,
    /// The ware it carries in (`+0x0f`), until the building takes it.
    pub ware: Option<u16>,
    /// The bitmap of the ware it shows (`+0x01`).
    pub shows: Option<u8>,
    /// Walking in (`+0x0c` bit 7); clear on the way back out.
    pub inside: bool,
}

/// A builder at work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Builder {
    /// The site (`+0x0c`).
    pub building: u16,
    /// The way it faces, set at the start of each leg (`+0x0e`).
    pub facing: Direction,
    /// The waypoint [`Self::facing`] was set for (`+0x0f`).
    pub leg: Option<u8>,
    /// Horizontal offset in pixels from the building's screen position (`+0x10`).
    pub x: i8,
    /// Vertical offset in pixels (`+0x11`).
    pub y: i8,
}

/// A geologist or scout working from a flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Explorer {
    /// Owner.
    pub owner: Player,
    /// Geologist or scout.
    pub job: Job,
    /// The flag it was sent to (`+0x0e`).
    pub flag: u16,
    /// Where that flag stood. A removed flag's record keeps its position, and a geologist busy
    /// at a node when its flag goes carries on round it.
    pub centre: Node,
    /// Nodes it has gone to since leaving the flag (`+0x09`).
    pub visits: u8,
    /// What it is doing.
    pub work: Exploring,
}

/// What a geologist or scout is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Exploring {
    /// Walking a cross-country path (`+0x10`), to a node or back to the flag.
    Walking {
        /// Its path record.
        path: u16,
    },
    /// A geologist tapping the ground (`ROM_ANIM` 3507).
    Tapping {
        /// Its animation, or `None` when no animation script is loaded.
        animation: Option<u16>,
    },
    /// A geologist pleased with a find (`ROM_ANIM` 3617).
    Celebrating {
        /// Its animation, or `None` when no animation script is loaded.
        animation: Option<u16>,
    },
}

/// A planer at work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Planer {
    /// Owner.
    pub owner: Player,
    /// The site's flag (`+0x0e`).
    pub flag: u16,
    /// The altitude it levels to (`+0x0a`).
    pub target: u8,
    /// Nodes levelled so far (`+0x09`).
    pub levelled: u8,
    /// What it is doing (`+0x10`).
    pub work: PlanerWork,
}

/// A workplace's worker out on a trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Gatherer {
    /// Owner.
    pub owner: Player,
    /// Its look (`+0x01`): the worker's job, or what it carries home, such as
    /// [`Job::CARRYING_WOOD`] for a woodcutter with wood.
    pub job: Job,
    /// Its workplace (`+0x0d`, later `+0x0e`).
    pub building: u16,
    /// Where it is on the trip.
    pub trip: Trip,
    /// A hunter's prey (`+0x09`), the animal record it waits for, shoots and butchers.
    pub prey: Option<u16>,
    /// A hunter's searches left (`+0x0b`): 6 at the flag, one less for each search further
    /// afield, `u8::MAX` while butchering.
    pub searches: u8,
}

/// Where a workplace's worker is on its trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Trip {
    /// Walking out of the door to the flag (kind 56, `+0x0c = 4`), its path ready.
    Leaving {
        /// Its path record (`+0x0f`).
        path: u16,
    },
    /// Walking a cross-country path (kind 32, sub-state 2), out to its target or back to the
    /// flag.
    Walking {
        /// Its path record (`+0x10`).
        path: u16,
    },
    /// At work on its target (kind 40).
    Working {
        /// Its `ROM_ANIM` animation, or `None` when no animation script is loaded.
        animation: Option<u16>,
    },
    /// Walking in through the door (kind 56, `+0x0c` 4 with bit 7 set).
    Entering,
    /// A hunter waiting for its prey to stop (kind 48): the figure loop passes it by.
    Waiting,
}

/// A workplace's worker carrying a finished ware out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Porter {
    /// Owner.
    pub owner: Player,
    /// Its look (`+0x01`): a worker carrying the ware, or the worker's job once it is put down.
    pub job: Job,
    /// Its workplace (`+0x0d`).
    pub building: u16,
    /// The ware type it carries (`+0x0f`).
    pub ware: Option<WareKind>,
    /// Walking back in (`+0x0c` bit 7).
    pub inside: bool,
}

/// A soldier outside a building.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Soldier {
    /// Owner.
    pub owner: Player,
    /// Its rank job (`+0x01`).
    pub job: Job,
    /// Its soldier record (`+0x09`), `None` once freed.
    pub record: Option<u16>,
    /// The building it belongs to (`+0x0e`), `None` once that building is gone or taken.
    pub home: Option<u16>,
    /// What it is doing.
    pub state: SoldierState,
    /// Held before the edge in the figure's direction, which it took off its path as the wait
    /// began (`+0x0c` bits 3 to 5 at `0x10`, `+0x08`). Fights and new paths leave it as it is,
    /// so the soldier still tests that edge first; only the node clearing or a route the held
    /// search finds ends it.
    pub held: bool,
}

/// What a soldier outside a building is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SoldierState {
    /// Walking out of building `door` to its flag (kind 56, `+0x0c` 4 or 5).
    Leaving {
        /// The building.
        door: u16,
        /// An attacker's path to its target's flag (`+0x0f`).
        path: Option<u16>,
        /// Waiting in the door for another figure to clear it (`+0x0c` bit 5).
        waiting: bool,
    },
    /// Walking in from the flag through building `door` (kind 56, `+0x0c` bit 7).
    Entering {
        /// The building.
        door: u16,
        /// What it does inside.
        entry: Entry,
    },
    /// Walking cross-country (kind 32).
    Walking {
        /// Its path record (`+0x10`).
        path: Option<u16>,
    },
    /// Standing, waiting for its opponent to come (kind 48).
    Waiting,
    /// Playing a fight or death animation (kind 40).
    Animating {
        /// Its `ROM_ANIM` animation, or `None` when no animation script is loaded.
        animation: Option<u16>,
    },
}

/// What a soldier walking in through a door does inside (`+0x0c` low nibble).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Entry {
    /// Disappears at the door (4).
    Vanish,
    /// A defender back home (5).
    Defender,
    /// An attacker taking the building, or a soldier coming home (7).
    Home,
}

/// What a planer is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PlanerWork {
    /// Walking a cross-country path (kind 32).
    Walking {
        /// Its path record.
        path: u16,
    },
    /// Digging (kind 40).
    Digging {
        /// Its `ROM_ANIM` animation, or `None` when no animation script is loaded.
        animation: Option<u16>,
    },
}

/// One figure record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Figure {
    /// What it is.
    pub kind: FigureKind,
    /// The way it faces, and walks while walking.
    pub direction: Direction,
    /// While walking, the distance still to go to its node, 255 down to 1; 0 when standing.
    pub remaining: u8,
    /// Distance covered per simulation step while walking.
    pub speed: u8,
    /// The walk frame counter, one up per simulation step of walking. A resting animal counts
    /// its remaining steps of rest here.
    pub counter: u16,
    /// Held where it is (`+0x06` bit 0): the figure loop passes it by until a step that is a
    /// multiple of 7. A settler made by a storehouse starts held, because new records are filled
    /// with byte 207, which has bits 0 and 1 set.
    pub held: bool,
    /// Whether it took the held bit on the node behind it when it stopped (`+0x06` bit 1), which
    /// releasing it gives back. The 207 fill sets this bit too, so a settler a storehouse
    /// makes starts with it.
    pub holds_behind: bool,
    /// Steps a cross-country walker has waited for the held node ahead of it to clear (`+0x0d`).
    pub waits: u8,
    pub(crate) node: Node,
    pub(crate) next: Option<u16>,
}

impl Figure {
    /// A new animal as the map loader makes one: standing, facing east, at walk frame 0.
    #[must_use]
    pub const fn animal(species: Species) -> Self {
        Self {
            kind: FigureKind::Animal {
                species,
                state: AnimalState::Active,
                hunter: None,
            },
            direction: Direction::East,
            remaining: 0,
            speed: 0,
            counter: 0,
            held: false,
            holds_behind: false,
            waits: 0,
            node: Node { x: 0, y: 0 },
            next: None,
        }
    }

    /// Whether it is walking through a building's door (kind 56), which opens and shuts the
    /// door as it goes.
    #[must_use]
    pub const fn in_door(&self) -> bool {
        matches!(
            self.kind,
            FigureKind::Leaving { .. }
                | FigureKind::Entering { .. }
                | FigureKind::Settling { .. }
                | FigureKind::Delivering(_)
                | FigureKind::Bringing { .. }
                | FigureKind::Porter(_)
                | FigureKind::Gatherer(Gatherer {
                    trip: Trip::Leaving { .. } | Trip::Entering,
                    ..
                })
                | FigureKind::Soldier(Soldier {
                    state: SoldierState::Leaving { .. } | SoldierState::Entering { .. },
                    ..
                })
        )
    }

    /// Whether it is walking out of a door to the flag and not waiting in it, so that reaching
    /// the flag lets the next waiting figure out (`0x3596c`).
    #[must_use]
    pub const fn leaving_door(&self) -> bool {
        matches!(
            self.kind,
            FigureKind::Leaving { .. }
                | FigureKind::Bringing { inside: false, .. }
                | FigureKind::Delivering(Delivery { inside: false, .. })
                | FigureKind::Porter(Porter { inside: false, .. })
                | FigureKind::Gatherer(Gatherer {
                    trip: Trip::Leaving { .. },
                    ..
                })
                | FigureKind::Soldier(Soldier {
                    state: SoldierState::Leaving { waiting: false, .. },
                    ..
                })
        )
    }

    /// Whether a held node ahead of it stops it part way along its edge, so that the figures
    /// behind it queue (kinds 8, 16 and 56, `0x32d61`): carriers, settlers walking the
    /// roads, and figures in a door.
    #[must_use]
    pub const fn queues(&self) -> bool {
        matches!(self.kind, FigureKind::Carrier(_) | FigureKind::Walker(_)) || self.in_door()
    }

    /// Whether removing a road or a flag passes it by (`0x3229c`): an owner 7 record, a lost
    /// settler (kind 24), or a settler walking, working or waiting cross-country (kinds 32, 40
    /// and 48). Settlers on the roads and in doorways are made lost.
    #[must_use]
    pub const fn spared_by_removal(&self) -> bool {
        match self.kind {
            FigureKind::Animal { .. }
            | FigureKind::Ship { .. }
            | FigureKind::Builder(_)
            | FigureKind::Lost { .. }
            | FigureKind::Homeward { .. }
            | FigureKind::Explorer(_)
            | FigureKind::Planer(_) => true,
            FigureKind::Gatherer(gatherer) => !matches!(gatherer.trip, Trip::Leaving { .. } | Trip::Entering),
            FigureKind::Soldier(_) => !self.in_door(),
            _ => false,
        }
    }

    /// Whether it belongs to a player: anything but an animal or a ship (owner 7).
    #[must_use]
    pub const fn is_settler(&self) -> bool {
        !matches!(self.kind, FigureKind::Animal { .. } | FigureKind::Ship { .. })
    }

    /// The node the figure stands on, or walks to.
    #[must_use]
    pub const fn node(&self) -> Node {
        self.node
    }

    /// Whether it is walking (byte `+0` bit 6).
    #[must_use]
    pub const fn walking(&self) -> bool {
        self.remaining > 0
    }

    /// Starts walking at `speed` (`0x322f8`): this step already covers `speed` of the edge's
    /// [`EDGE_LENGTH`], so `EDGE_LENGTH − speed` is left. The caller has moved the figure to
    /// its destination's list. Speeds never exceed 24, so the original's five-bit speed field
    /// never cuts one short.
    pub(crate) fn start_walk(&mut self, direction: Direction, speed: u8) {
        self.direction = direction;
        self.speed = speed;
        self.remaining = u8::try_from(EDGE_LENGTH - u16::from(speed)).unwrap_or(0);
    }

    /// One step further along the walk (`0x32d48`): the distance left shrinks by the speed and
    /// stops at 0, which ends the walk.
    pub(crate) fn advance_walk(&mut self) {
        self.remaining = self.remaining.saturating_sub(self.speed);
    }

    /// A figure of `kind` as the game creates one: facing east, standing, every counter at 0.
    #[must_use]
    pub const fn new(kind: FigureKind) -> Self {
        Self {
            kind,
            direction: Direction::East,
            remaining: 0,
            speed: 0,
            counter: 0,
            held: false,
            holds_behind: false,
            waits: 0,
            node: Node { x: 0, y: 0 },
            next: None,
        }
    }

    /// A settler of `kind` as a storehouse makes one: like [`Self::new`], but with the two flags
    /// the 207 fill leaves set, so the figure loop holds it until it releases it and the
    /// release clears the held bit on the node west of it.
    #[must_use]
    pub const fn fresh(kind: FigureKind) -> Self {
        let mut figure = Self::new(kind);
        figure.held = true;
        figure.holds_behind = true;
        figure
    }

    /// The next figure in the node's list.
    #[must_use]
    pub const fn next(&self) -> Option<u16> {
        self.next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_values_translate_through_the_signature_table() {
        let mut rng = GameRng::at_start();
        let all: Vec<_> = (0..=10).map(|value| Species::from_map(value, &mut rng)).collect();
        assert_eq!(
            all,
            [
                Some(Species::PolarBear),
                Some(Species::GreyRabbit),
                Some(Species::Fox),
                Some(Species::Stag),
                Some(Species::Deer),
                Some(Species::Duck),
                Some(Species::Sheep),
                Some(Species::Deer),
                Some(Species::Duck),
                Some(Species::Donkey),
                None
            ]
        );
        assert_eq!(
            Species::from_map(1, &mut rng),
            Some(Species::LightRabbit),
            "the second draw is 1"
        );
        let mut two_draws = GameRng::at_start();
        two_draws.step();
        two_draws.step();
        assert_eq!(rng, two_draws, "only the two rabbits drew");
    }
}
