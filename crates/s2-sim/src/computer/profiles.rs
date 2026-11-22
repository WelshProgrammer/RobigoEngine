//! The four profiles computer players play by (`0x1fd678`, 358 bytes each). A face picks one. The
//! fields come in as the tasks that read them do.

use std::borrow::Cow;

use crate::military::MILITARY_SETTINGS;

use super::board::KINDS;

/// One profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ComputerProfile {
    /// How the AI places a building: nodes a site search tests a turn (`+0x00`), rings kept free
    /// of military buildings round a barracks or guardhouse (`+0x02`), rings a road search floods
    /// (`+0x04`), nodes between the flags of a new road (`+0x06`) and the least ore a mine needs
    /// (`+0x08`).
    pub placing: Placing,
    /// Task 14's numbers.
    pub economy: Economy,
    /// Land the request board wants for each site under construction before it grants more
    /// (`+0x26`).
    pub land_per_site: u16,
    /// The cooldown a request of a kind starts when one is added (`+0x88`).
    pub add_wait: u16,
    /// The cooldown each kind of request starts when one is granted (`+0x8a`).
    pub take_waits: [u16; KINDS],
    /// Task 12's least trees for a woodcutter (`+0x9c`), granite piles for a quarry (`+0x9e`),
    /// fish for a fishery (`+0xa0`) and trees for a hunter (`+0xa2`).
    pub land_use: [u8; 4],
    /// Task 17's three military sets, the seven military settings each (`+0xbc`, a word a
    /// setting of which the low byte is read).
    pub military_sets: [[u8; MILITARY_SETTINGS]; 3],
    /// Which set each of the 16 values of a draw's low four bits picks (`+0xe6`).
    pub set_picks: [u8; 16],
    /// The military settings task 17 holds for a while now and then (`+0xf6`).
    pub special_set: [u8; MILITARY_SETTINGS],
    /// How task 10 rates the flags a road search from a flag meets (`+0x0a` to `+0x14`).
    pub flag_keeper: FlagKeeping,
    /// Task 9's and task 7's numbers.
    pub scans: Scans,
    /// Task 13's numbers.
    pub attacks: Attacking,
    /// Task 15's numbers.
    pub towers: Towers,
    /// How far from a flag task 11 wants no other storehouse pending (`+0xae`); five rings
    /// fewer, none standing.
    pub storehouse_spacing: u16,
    /// The chance in hundredths, against a draw of 0 to 100, that a storehouse on open land
    /// comes with a sawmill, two woodcutters and a forester (`+0xb0`).
    pub wood_chance: u16,
    /// Task 16's chances in hundredths, against a draw of 0 to 100, of a guardhouse by a weapons,
    /// a tools and a gold project and of a watchtower by a gold project (`+0x136` to `+0x13c`).
    pub guards: [u16; 4],
    /// The chance in hundredths, against a draw of 0 to 100, that `0x68e68` passes over a cell
    /// (`+0x150`).
    pub skip_chance: u8,
    /// The fewest and most thinks between task 17's special military sets (`+0x15e`, `+0x162`).
    pub special_wait: (u32, u32),
}

/// How task 10 rates the flags a road search meets: the search's limit, the least cost worth a
/// look and the divisor of the cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RoadRating {
    /// The road search's cost limit.
    pub limit: u16,
    /// The least cost of a flag worth a look.
    pub floor: u16,
    /// What the cost is divided by before the distance is taken off.
    pub divisor: u16,
}

/// Task 10's ratings from most flags (`+0x0a` to `+0x0e`) and from a storehouse's (`+0x10` to
/// `+0x14`), the same in every profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FlagKeeping {
    /// From a flag with no storehouse.
    pub other: RoadRating,
    /// From a storehouse's flag.
    pub storehouse: RoadRating,
}

/// Task 10's ratings of every original profile.
const FLAG_KEEPING: FlagKeeping = FlagKeeping {
    other: RoadRating {
        limit: 40,
        floor: 20,
        divisor: 3,
    },
    storehouse: RoadRating {
        limit: 20,
        floor: 8,
        divisor: 2,
    },
};

/// What each type of building near a target adds to its value (`+0x114` to `+0x128`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TargetValues {
    /// A catapult (`+0x114`).
    pub catapult: u16,
    /// A gold mine (`+0x116`).
    pub gold_mine: u16,
    /// A mint (`+0x118`).
    pub mint: u16,
    /// An iron mine (`+0x11a`).
    pub iron_mine: u16,
    /// A coal mine (`+0x11c`).
    pub coal_mine: u16,
    /// An armoury (`+0x11e`).
    pub armoury: u16,
    /// A metalworks (`+0x120`).
    pub metalworks: u16,
    /// An iron smelter (`+0x122`).
    pub smelter: u16,
    /// A storehouse or harbour (`+0x124`).
    pub storehouse: u16,
    /// A headquarters (`+0x126`).
    pub headquarters: u16,
    /// Any other type (`+0x128`).
    pub other: u16,
}

/// Task 13's numbers (`+0x104` to `+0x134`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Attacking {
    /// Calls between ratings of targets (`+0x104`).
    pub every: u16,
    /// Military records a rating looks at (`+0x106`).
    pub records: u16,
    /// The least border class of a target (`+0x108`).
    pub border: u8,
    /// The fewest soldiers in reach worth a target (`+0x10a`).
    pub least: u16,
    /// The most soldiers sent (`+0x10c`).
    pub most: u16,
    /// The chance in hundredths, against a draw of 0 to 100 taken as at most, that a target is
    /// rated at all (`+0x10e`).
    pub skip_chance: u16,
    /// The chance in hundredths, against a draw of 0 to 100, that the strongest soldiers go first
    /// (`+0x110`).
    pub strongest_chance: u16,
    /// What a target the player once owned adds to its value (`+0x112`).
    pub own_bonus: u16,
    /// What the buildings near a target add to its value.
    pub values: TargetValues,
    /// What a barracks, guardhouse, watchtower and fortress add to their defenders, signed words
    /// (`+0x12a` to `+0x130`).
    pub defence: [i16; 4],
    /// Calls between tries at a headquarters (`+0x132`).
    pub headquarters_every: u16,
    /// The fewest soldiers in reach worth a headquarters (`+0x134`).
    pub headquarters_least: u16,
}

/// Task 15's numbers (`+0x13e` to `+0x15c`, but for `+0x150`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Towers {
    /// Calls between scans (`+0x13e`).
    pub every: u16,
    /// Cells a scan looks at (`+0x140`).
    pub cells: u16,
    /// What a mint adds to a cell's value and to its military weight (`+0x142`, `+0x152`).
    pub mint: (u16, u16),
    /// What a gold mine adds to the value (`+0x144`).
    pub gold_mine: u16,
    /// What a storehouse or harbour adds to the value and to the weight (`+0x146`, `+0x154`).
    pub storehouse: (u16, u16),
    /// What an armoury adds to the value (`+0x148`).
    pub armoury: u16,
    /// What a metalworks adds to the value (`+0x14a`).
    pub metalworks: u16,
    /// The chance in hundredths, against a draw of 0 to 100 taken as at least, of a tower by a
    /// cell of enough value (`+0x14c`).
    pub tower_chance: u16,
    /// The chance in hundredths, against a draw of 0 to 100, that the tower is a fortress
    /// (`+0x14e`).
    pub fortress_chance: u16,
    /// What a watchtower adds to the weight (`+0x156`).
    pub watchtower: u16,
    /// What a fortress adds to the weight (`+0x158`).
    pub fortress: u16,
    /// The chance in hundredths, against a draw of 0 to 100, of a catapult by a cell of enough
    /// weight (`+0x15a`).
    pub catapult_chance: u16,
    /// The rings round a cell's centre summed (`+0x15c`).
    pub rings: u16,
}

/// Task 9's and task 7's numbers (`+0xa4` to `+0xba`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Scans {
    /// Calls between searches for land round the headquarters (`+0xa4`).
    pub headquarters_every: u16,
    /// How far a search for land floods when its job gives no radius (`+0xa6`).
    pub reach: u16,
    /// Calls between searches for land round the next storehouse, with fewer than five
    /// storehouses, harbours and headquarters and with more (`+0xa8`, `+0xaa`).
    pub storehouse_every: (u16, u16),
    /// How far a search round a storehouse floods (`+0xac`).
    pub storehouse_reach: u16,
    /// Calls between searches for ore (`+0xb2`).
    pub ore_every: u16,
    /// The chances in hundredths, against a draw of 0 to 100 taken in turn, of a search for gold,
    /// iron and coal (`+0xb4` to `+0xb8`).
    pub ore_chances: [u16; 3],
    /// How many cells either way of its own a search for ore looks at (`+0xba`).
    pub ore_cells: u16,
}

/// The cooldowns of every profile for a granted request.
const TAKE_WAITS: [u16; KINDS] = [30, 30, 90, 150, 120, 12, 180, 60, 180];

/// How the AI places a building (`+0x00` to `+0x08`), the same in every profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Placing {
    /// Nodes a site search tests a turn.
    pub tests: u16,
    /// Rings round a barracks or guardhouse with no military building.
    pub military_free: u16,
    /// Rings a road search floods.
    pub route_rings: u16,
    /// Nodes between the flags of a new road.
    pub flag_spacing: u16,
    /// The least ore a mine needs over its node and two rings.
    pub mine_ore: u16,
}

/// Task 14's numbers (`+0x16` to `+0x87`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Economy {
    /// Calls between runs (`+0x16`).
    pub calls: u16,
    /// The stock rules' thresholds: gold, coal, iron ore, the armoury's chance in hundredths
    /// against the metalworks, wood, flour and grain (`+0x18` to `+0x24`).
    pub stock: [u16; 7],
    /// The distribution sliders the first run writes, six groups of five (`+0x28 + 10g + 2j`).
    pub sliders: [[u8; 5]; 6],
    /// The tool weights the first run writes, tools 16 to 27 (`+0x64 + 2i`).
    pub tools: [u8; 12],
    /// Land for each sawmill, quarry, armoury, mint, metalworks and donkey breeder before task 14
    /// asks for another (`+0x7c` to `+0x86`).
    pub land: [u16; 6],
}

/// Task 14's numbers of profiles 0 to 2.
const ECONOMY: Economy = Economy {
    calls: 5,
    stock: [10, 10, 15, 75, 30, 15, 20],
    sliders: [
        [90, 90, 90, 100, 0],
        [100, 100, 90, 90, 0],
        [100, 100, 0, 0, 0],
        [100, 90, 100, 0, 0],
        [100, 100, 20, 0, 0],
        [100, 100, 100, 100, 0],
    ],
    tools: [10, 70, 30, 70, 70, 60, 60, 30, 50, 10, 20, 10],
    land: [800, 1200, 800, 700, 1500, 2500],
};

/// The placing fields of every original profile.
const PLACING: Placing = Placing {
    tests: 10,
    military_free: 5,
    route_rings: 24,
    flag_spacing: 3,
    mine_ore: 12,
};

/// The sets of profiles 0 and 2, which profile 1 shares but for set 1.
const SETS: [[u8; MILITARY_SETTINGS]; 3] = [
    [100, 50, 100, 75, 30, 50, 100],
    [100, 50, 100, 75, 60, 80, 100],
    [100, 50, 100, 100, 10, 40, 100],
];

/// The picks of profiles 0 and 2.
const PICKS: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 2, 2];

/// The special set of profiles 0 and 2.
const SPECIAL: [u8; MILITARY_SETTINGS] = [100, 0, 0, 0, 0, 10, 20];

/// The original's four.
const ORIGINAL_PROFILES: [ComputerProfile; 4] = [
    ComputerProfile {
        placing: PLACING,
        economy: ECONOMY,
        military_sets: SETS,
        set_picks: PICKS,
        special_set: SPECIAL,
        land_per_site: 80,
        add_wait: 30,
        take_waits: TAKE_WAITS,
        land_use: [30, 12, 12, 40],
        flag_keeper: FLAG_KEEPING,
        scans: Scans {
            headquarters_every: 77,
            reach: 48,
            storehouse_every: (83, 47),
            storehouse_reach: 36,
            ore_every: 120,
            ore_chances: [40, 30, 10],
            ore_cells: 6,
        },
        storehouse_spacing: 25,
        wood_chance: 15,
        attacks: Attacking {
            every: 6,
            records: 20,
            border: 2,
            least: 2,
            most: 12,
            skip_chance: 100,
            strongest_chance: 75,
            own_bonus: 100,
            values: TargetValues {
                catapult: 50,
                gold_mine: 30,
                mint: 60,
                iron_mine: 20,
                coal_mine: 15,
                armoury: 40,
                metalworks: 30,
                smelter: 30,
                storehouse: 80,
                headquarters: 100,
                other: 10,
            },
            defence: [-1, 0, 0, -1],
            headquarters_every: 80,
            headquarters_least: 2,
        },
        towers: Towers {
            every: 2,
            cells: 8,
            mint: (60, 50),
            gold_mine: 60,
            storehouse: (120, 50),
            armoury: 20,
            metalworks: 20,
            tower_chance: 50,
            fortress_chance: 60,
            watchtower: 50,
            fortress: 50,
            catapult_chance: 25,
            rings: 8,
        },
        guards: [33, 20, 75, 75],
        skip_chance: 55,
        special_wait: (600, 1200),
    },
    ComputerProfile {
        placing: PLACING,
        economy: ECONOMY,
        military_sets: [SETS[0], [100, 50, 100, 100, 60, 80, 100], SETS[2]],
        set_picks: [0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2],
        special_set: [100, 0, 0, 0, 0, 0, 0],
        land_per_site: 80,
        add_wait: 30,
        take_waits: TAKE_WAITS,
        land_use: [30, 12, 12, 40],
        flag_keeper: FLAG_KEEPING,
        scans: Scans {
            headquarters_every: 77,
            reach: 48,
            storehouse_every: (83, 47),
            storehouse_reach: 36,
            ore_every: 120,
            ore_chances: [60, 30, 10],
            ore_cells: 6,
        },
        storehouse_spacing: 25,
        wood_chance: 15,
        attacks: Attacking {
            every: 5,
            records: 20,
            border: 2,
            least: 3,
            most: 12,
            skip_chance: 100,
            strongest_chance: 100,
            own_bonus: 100,
            values: TargetValues {
                catapult: 80,
                gold_mine: 30,
                mint: 80,
                iron_mine: 20,
                coal_mine: 15,
                armoury: 40,
                metalworks: 30,
                smelter: 30,
                storehouse: 120,
                headquarters: 120,
                other: 10,
            },
            defence: [0, 0, -1, -2],
            headquarters_every: 80,
            headquarters_least: 2,
        },
        towers: Towers {
            every: 3,
            cells: 8,
            mint: (40, 40),
            gold_mine: 20,
            storehouse: (90, 40),
            armoury: 10,
            metalworks: 10,
            tower_chance: 40,
            fortress_chance: 50,
            watchtower: 50,
            fortress: 60,
            catapult_chance: 25,
            rings: 8,
        },
        guards: [16, 10, 50, 50],
        skip_chance: 45,
        special_wait: (800, 1000),
    },
    ComputerProfile {
        placing: PLACING,
        economy: ECONOMY,
        military_sets: SETS,
        set_picks: PICKS,
        special_set: SPECIAL,
        land_per_site: 80,
        add_wait: 30,
        take_waits: TAKE_WAITS,
        land_use: [30, 12, 12, 40],
        flag_keeper: FLAG_KEEPING,
        scans: Scans {
            headquarters_every: 87,
            reach: 48,
            storehouse_every: (99, 67),
            storehouse_reach: 32,
            ore_every: 120,
            ore_chances: [50, 40, 10],
            ore_cells: 6,
        },
        storehouse_spacing: 22,
        wood_chance: 10,
        attacks: Attacking {
            every: 8,
            records: 12,
            border: 2,
            least: 2,
            most: 12,
            skip_chance: 80,
            strongest_chance: 100,
            own_bonus: 90,
            values: TargetValues {
                catapult: 50,
                gold_mine: 30,
                mint: 60,
                iron_mine: 20,
                coal_mine: 15,
                armoury: 40,
                metalworks: 30,
                smelter: 30,
                storehouse: 80,
                headquarters: 100,
                other: 10,
            },
            defence: [0, 0, 0, 0],
            headquarters_every: 80,
            headquarters_least: 2,
        },
        towers: Towers {
            every: 2,
            cells: 12,
            mint: (60, 75),
            gold_mine: 60,
            storehouse: (120, 75),
            armoury: 20,
            metalworks: 20,
            tower_chance: 50,
            fortress_chance: 70,
            watchtower: 50,
            fortress: 75,
            catapult_chance: 50,
            rings: 12,
        },
        guards: [40, 20, 75, 100],
        skip_chance: 75,
        special_wait: (900, 1500),
    },
    ComputerProfile {
        placing: PLACING,
        economy: Economy {
            stock: [20, 20, 25, 65, 40, 25, 30],
            land: [1200, 1500, 1200, 1000, 1800, 2500],
            ..ECONOMY
        },
        military_sets: [[100, 50, 100, 50, 30, 50, 100], SETS[1], SETS[2]],
        set_picks: [0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2],
        special_set: [100, 0, 0, 0, 0, 20, 50],
        land_per_site: 100,
        add_wait: 40,
        take_waits: TAKE_WAITS,
        land_use: [40, 15, 15, 60],
        flag_keeper: FLAG_KEEPING,
        scans: Scans {
            headquarters_every: 87,
            reach: 44,
            storehouse_every: (99, 67),
            storehouse_reach: 36,
            ore_every: 180,
            ore_chances: [30, 20, 5],
            ore_cells: 4,
        },
        storehouse_spacing: 28,
        wood_chance: 15,
        attacks: Attacking {
            every: 8,
            records: 12,
            border: 2,
            least: 1,
            most: 12,
            skip_chance: 50,
            strongest_chance: 50,
            own_bonus: 50,
            values: TargetValues {
                catapult: 70,
                gold_mine: 20,
                mint: 50,
                iron_mine: 10,
                coal_mine: 5,
                armoury: 20,
                metalworks: 10,
                smelter: 10,
                storehouse: 70,
                headquarters: 100,
                other: 5,
            },
            defence: [-1, -1, -2, -3],
            headquarters_every: 96,
            headquarters_least: 3,
        },
        towers: Towers {
            every: 3,
            cells: 8,
            mint: (50, 50),
            gold_mine: 50,
            storehouse: (100, 50),
            armoury: 20,
            metalworks: 20,
            tower_chance: 50,
            fortress_chance: 60,
            watchtower: 50,
            fortress: 50,
            catapult_chance: 20,
            rings: 8,
        },
        guards: [25, 20, 50, 50],
        skip_chance: 55,
        special_wait: (600, 1800),
    },
];

/// The profiles, by number.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ComputerProfiles(Cow<'static, [ComputerProfile]>);

impl ComputerProfiles {
    /// The original's four.
    pub const ORIGINAL: Self = Self(Cow::Borrowed(&ORIGINAL_PROFILES));

    /// Profile `index`, or the first for an index past the end.
    #[must_use]
    pub fn get(&self, index: u8) -> ComputerProfile {
        self.0
            .get(usize::from(index))
            .or_else(|| self.0.first())
            .copied()
            .unwrap_or(ORIGINAL_PROFILES[0])
    }
}
