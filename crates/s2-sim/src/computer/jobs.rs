//! The jobs a computer player's tasks hand each other.

use bitflags::bitflags;

use super::TaskKind;
use crate::grid::Node;
use crate::world::BuildingKind;

bitflags! {
    /// Where a job has got to (`+0x00` of the job base).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
    pub struct JobFlags: u16 {
        /// Its handler has been offered it.
        const STARTED = 1;
        /// Its handler took it.
        const ACCEPTED = 1 << 1;
        /// It is on the list of running jobs.
        const RUNNING = 1 << 2;
        /// It has reported its result.
        const FINISHED = 1 << 3;
    }
}

/// What a job asks for (`+0x02` of the job base).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum JobKind {
    /// Reserve a site's node.
    Reserve,
    /// Find a site for a building.
    Site,
    /// Join a flag to the roads.
    Connect,
    /// Lay a road between two flags.
    Shortcut,
    /// Place a building and its road.
    Place,
    /// Push the border towards a node.
    Expand,
    /// Look for somewhere to expand.
    Scout,
    /// Build a type near a node.
    BuildOrder,
    /// Build an economy chain.
    Project,
    /// Nothing makes one (`0x7326a`).
    Unused9,
    /// Nothing makes one (`0x7334a`).
    Unused10,
}

impl JobKind {
    /// How many kinds there are.
    pub const COUNT: usize = 11;

    /// Every kind, by number.
    pub const ALL: [Self; Self::COUNT] = [
        Self::Reserve,
        Self::Site,
        Self::Connect,
        Self::Shortcut,
        Self::Place,
        Self::Expand,
        Self::Scout,
        Self::BuildOrder,
        Self::Project,
        Self::Unused9,
        Self::Unused10,
    ];

    /// Its number, the index into the handler table.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// A site search's own fields (kind 1, `0x6a3ae`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SiteSearch {
    /// What to build (`+0x1a`).
    pub kind: BuildingKind,
    /// The centre of the spiral (`+0x1c`, `+0x1e`).
    pub centre: Node,
    /// Whether a node has passed (`+0x20`).
    pub passed: bool,
    /// The node the spiral stands on (`+0x24`).
    pub at: Node,
    /// The spiral's ring, side 0 to 5 and step along the side (`+0x30`, `+0x32`, `+0x34`).
    pub ring: u16,
    /// See [`Self::ring`].
    pub side: u16,
    /// See [`Self::ring`].
    pub step: u16,
    /// The radius the maker passes, which nothing reads (`+0x36`).
    pub radius: u16,
    /// Whether the next turn passes over the node the spiral stands on (`+0x38`).
    pub skip: bool,
}

/// A placement's own fields (kind 4, `0x6b9ea`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Placement {
    /// What to build (`+0x1a`).
    pub kind: BuildingKind,
    /// Where (`+0x1c`, `+0x1e`).
    pub node: Node,
    /// The maker's number (`+0x20`).
    pub radius: u16,
    /// Bit value 1 once the site search has succeeded, 2 once the reservation has (`+0x22`).
    pub found: u16,
    /// Results heard (`+0x24`).
    pub reports: u16,
    /// The site search's extra, its site packed as `y << 16 | x` (`+0x26`).
    pub extra: u32,
}

/// Where an expansion has got to (kind 5's `+0x1e`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Expansion {
    /// It plans and orders a building.
    Plan,
    /// It waits for its placement.
    Ordered,
    /// It waits for its building to be finished and manned.
    Building,
    /// It failed and ends on its next turn.
    Failed,
}

/// What a scout looks for (kind 6's `+0x1e`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ScoutMode {
    /// The cell nearby richest in gold.
    Gold,
    /// The cell nearby richest in iron.
    Iron,
    /// The cell nearby richest in coal.
    Coal,
    /// Somewhere off the player's land with room for a building.
    Land,
}

/// A job's own fields, after the base (`+0x1a`). Only the kinds the modelled tasks make carry
/// theirs yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Payload {
    /// None kept.
    None,
    /// A reservation (kind 0, `0x6a246`) at `node`.
    Reserve {
        /// Where (`+0x1e`, `+0x20`).
        node: Node,
    },
    /// A site search (kind 1).
    Site(SiteSearch),
    /// A flag to join to the roads (kind 2, `0x6ac8a`).
    Connect {
        /// The flag's node (`+0x1a`, `+0x1c`).
        flag: Node,
        /// 0 for a new building's flag, 1 from task 10 (`+0x1e`).
        mode: u16,
        /// The building it serves (`+0x20`).
        kind: BuildingKind,
    },
    /// A road between two of the player's flags (kind 3, `0x6b847`).
    Shortcut {
        /// The flag it starts from (`+0x1a`).
        from: u16,
        /// The flag it goes to (`+0x1c`).
        to: u16,
        /// How far the search for a way floods (`+0x1e`).
        radius: u16,
    },
    /// A placement (kind 4).
    Place(Placement),
    /// An expansion (kind 5, `0x6bf5a`): a military building pushing the border towards
    /// `target`.
    Expand {
        /// Where the border should go (`+0x1a`, `+0x1c`).
        target: Node,
        /// Where it has got to (`+0x1e`).
        state: Expansion,
        /// The node the building goes on, then the placed site (`+0x20`, `+0x22`).
        chosen: Node,
        /// The placed building's record (`+0x24`).
        building: Option<u16>,
    },
    /// A scout (kind 6, `0x6c94e`): where to look for land, or for ore, to expand to.
    Scout {
        /// Where to look from (`+0x1a`, `+0x1c`).
        node: Node,
        /// What to look for (`+0x1e`).
        mode: ScoutMode,
        /// How far a search for land floods, 0 for the profile's reach (`+0x22`).
        radius: u16,
    },
    /// A build order (kind 7, `0x72bee`): a building of `kind` near `node`, `radius` the number
    /// the maker passes (`+0x20`), which nothing reads.
    BuildOrder {
        /// What to build.
        kind: BuildingKind,
        /// Where (`+0x1c`, `+0x1e`).
        node: Node,
        /// The maker's number.
        radius: u16,
        /// How the last placement went: 2 done, 4 failed for good, 8 failed but worth another try
        /// (`+0x22`).
        result: u16,
        /// Placements tried (`+0x24`).
        attempts: u16,
    },
    /// A building project (kind 8, `0x6cfda`): an economy chain of kind `project` from `node`.
    Project {
        /// Which chain, 0 to 8 (`+0x1a`).
        project: u8,
        /// Where it was asked for (`+0x1c`, `+0x1e`).
        node: Node,
        /// The maker's options: the low byte a variant, bit 8 an extra woodcutter (`+0x22`).
        options: u16,
        /// Whether its first turn has run step 0 (`+0x24`).
        started: bool,
        /// The step it has got to (`+0x26`).
        step: u16,
        /// Where its next buildings go, the last one placed (`+0x2a`, `+0x2c`).
        anchor: Node,
        /// An earlier building's node that a later step goes back to (`+0x2e`, `+0x30`).
        saved: Node,
    },
}

/// One job: the 22-byte base every kind shares, and its own fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Job {
    /// Where it has got to.
    pub flags: JobFlags,
    /// What it asks for.
    pub kind: JobKind,
    /// The step it was made on (`+0x08`).
    pub made: u32,
    /// The job it works for; `None` for a root job.
    pub parent: Option<u16>,
    /// The next child of the same parent.
    pub next: Option<u16>,
    /// Its first child.
    pub child: Option<u16>,
    /// The task that hears its result.
    pub owner: Option<TaskKind>,
    /// The task that took it.
    pub handler: Option<TaskKind>,
    /// Its own fields.
    pub payload: Payload,
}

impl Job {
    /// A job of `kind` made on step `made`, not yet added to a player's pool.
    #[must_use]
    pub const fn new(kind: JobKind, made: u32) -> Self {
        Self {
            flags: JobFlags::empty(),
            kind,
            made,
            parent: None,
            next: None,
            child: None,
            owner: None,
            handler: None,
            payload: Payload::None,
        }
    }

    /// A build order for a building of `kind` near `node` (`0x72bee`).
    #[must_use]
    pub const fn build_order(kind: BuildingKind, node: Node, radius: u16, made: u32) -> Self {
        let mut job = Self::new(JobKind::BuildOrder, made);
        job.payload = Payload::BuildOrder {
            kind,
            node,
            radius,
            result: 0,
            attempts: 0,
        };
        job
    }

    /// A job of `kind` with `payload`, made on step `made`.
    #[must_use]
    pub const fn with(kind: JobKind, payload: Payload, made: u32) -> Self {
        let mut job = Self::new(kind, made);
        job.payload = payload;
        job
    }

    /// A road from flag `from` to flag `to` within `radius` (`0x6b847`).
    #[must_use]
    pub const fn shortcut(from: u16, to: u16, radius: u16, made: u32) -> Self {
        let mut job = Self::new(JobKind::Shortcut, made);
        job.payload = Payload::Shortcut { from, to, radius };
        job
    }

    /// An expansion towards `target` (`0x6bf5a`).
    #[must_use]
    pub const fn expand(target: Node, made: u32) -> Self {
        let mut job = Self::new(JobKind::Expand, made);
        job.payload = Payload::Expand {
            target,
            state: Expansion::Plan,
            chosen: Node { x: 0, y: 0 },
            building: None,
        };
        job
    }

    /// A scout from `node` looking for `wanted` (`0x6c94e`).
    #[must_use]
    pub const fn scout(node: Node, wanted: ScoutMode, radius: u16, made: u32) -> Self {
        let mut job = Self::new(JobKind::Scout, made);
        job.payload = Payload::Scout {
            node,
            mode: wanted,
            radius,
        };
        job
    }

    /// A project of kind `project` from `node` (`0x6cfda`).
    #[must_use]
    pub const fn project(project: u8, node: Node, options: u16, made: u32) -> Self {
        let mut job = Self::new(JobKind::Project, made);
        job.payload = Payload::Project {
            project,
            node,
            options,
            started: false,
            step: 0,
            anchor: node,
            saved: Node { x: 0, y: 0 },
        };
        job
    }
}
