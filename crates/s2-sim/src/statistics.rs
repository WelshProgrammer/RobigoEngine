//! The samples the statistics screens draw.
//!
//! Each player keeps four series of samples. The shortest takes one every 180 steps from the ware
//! counters the world has been filling since the last sample; each longer series adds up the four
//! newest samples of the series below it, so it covers four times as long.

use crate::nodes::NodeMap;
use crate::world::{BuildingKind, Player, WARE_TYPES, World};

/// How many samples a series keeps, newest first.
pub const SAMPLES: usize = 40;

/// How many series there are, one per span the screens offer.
pub const SERIES: usize = 4;

/// How many counters a sample holds, one per ware group the merchandise graph draws.
pub const GRAPH_SLOTS: usize = 14;

/// How often each series takes a sample, in steps (`0x26f94`). Every period divides the cycle
/// counter's own turn of 11520 steps, so the cadences never drift apart.
pub const PERIODS: [u16; SERIES] = [180, 720, 2880, 11520];

/// How many samples of the series below add up to one sample of the next.
const FOLD: usize = 4;

/// Which of the fourteen graph slots each ware's counter adds to, from the first byte of the
/// 5-byte records at `0x1fa8e4`. The six foods share slot 3, the twelve tools slot 11 and the
/// sword and the shield slot 12.
const GRAPH_SLOT: [u8; WARE_TYPES] = [
    0, 1, 2, 3, 3, 3, 3, 3, 3, 4, 5, 6, 7, 8, 9, 10, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 12, 12,
    13,
];

/// What the statistics screen can draw, in the order its eight buttons sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Statistic {
    /// How many nodes the player owns.
    Land,
    /// How many finished buildings the player holds.
    Buildings,
    /// How many settlers the player holds.
    People,
    /// How many wares the player owns.
    Wares,
    /// The five soldier ranks weighted 1 to 5.
    Military,
    /// How many coins the player has counted in.
    Gold,
    /// How productive the twenty rated building types are on average.
    Productivity,
    /// How many fights the player has won.
    FightsWon,
}

impl Statistic {
    /// Every statistic, in button order.
    pub const ALL: [Self; 8] = [
        Self::Land,
        Self::Buildings,
        Self::People,
        Self::Wares,
        Self::Military,
        Self::Gold,
        Self::Productivity,
        Self::FightsWon,
    ];

    /// How many there are.
    pub const COUNT: usize = Self::ALL.len();

    /// Its place in a sample's values.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// The twenty building types the average productivity counts, in the order of the table at
/// `0x1fa8a8`.
#[rustfmt::skip]
const RATED: [BuildingKind; 20] = [
    BuildingKind::WOODCUTTER, BuildingKind::QUARRY, BuildingKind::FISHERY, BuildingKind::HUNTER,
    BuildingKind::SAWMILL, BuildingKind::MILL, BuildingKind::BAKERY, BuildingKind::WELL, BuildingKind::FARM,
    BuildingKind::PIG_FARM, BuildingKind::SLAUGHTERHOUSE, BuildingKind::METALWORKS, BuildingKind::ARMOURY,
    BuildingKind::MINT, BuildingKind::BREWERY, BuildingKind::SHIPYARD, BuildingKind::GOLD_MINE,
    BuildingKind::IRON_MINE, BuildingKind::COAL_MINE, BuildingKind::GRANITE_MINE,
];

/// What one sample holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct Sample {
    /// How many of each ware group were counted in over the sample.
    pub counters: [u16; GRAPH_SLOTS],
    /// What each statistic stood at when the sample was taken. People, wares and military
    /// strength stay 0 until the sim keeps the counts behind them.
    pub values: [u32; Statistic::COUNT],
}

impl Sample {
    /// A sample with nothing counted.
    pub const EMPTY: Self = Self {
        counters: [0; GRAPH_SLOTS],
        values: [0; Statistic::COUNT],
    };

    /// What `statistic` stood at.
    #[must_use]
    pub fn value(&self, statistic: Statistic) -> u32 {
        self.values[statistic.index()]
    }
}

/// One series of samples, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Series {
    samples: Vec<Sample>,
}

impl Series {
    /// A series with nothing in it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            samples: vec![Sample::EMPTY; SAMPLES],
        }
    }

    /// Sample `age` of the series, 0 being the newest.
    #[must_use]
    pub fn sample(&self, age: usize) -> Sample {
        self.samples.get(age).copied().unwrap_or(Sample::EMPTY)
    }

    /// Puts `sample` at the front and drops the oldest (`0x29eee`).
    fn push(&mut self, sample: Sample) {
        if self.samples.is_empty() {
            return;
        }
        self.samples.rotate_right(1);
        self.samples[0] = sample;
    }
}

impl Default for Series {
    fn default() -> Self {
        Self::new()
    }
}

/// A player's four series.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Statistics {
    series: [Series; SERIES],
}

impl Statistics {
    /// Series with nothing in them.
    #[must_use]
    pub fn new() -> Self {
        Self {
            series: std::array::from_fn(|_| Series::new()),
        }
    }

    /// Series `series`, or `None` from [`SERIES`] up.
    #[must_use]
    pub fn series(&self, series: usize) -> Option<&Series> {
        self.series.get(series)
    }
}

impl Default for Statistics {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Sample `age` of `series` for `player`, 0 being the newest. A series or an age past its end
    /// reads as an empty sample.
    #[must_use]
    pub fn sample(&self, player: Player, series: usize, age: usize) -> Sample {
        self.statistics[player.index()]
            .series(series)
            .map_or(Sample::EMPTY, |series| series.sample(age))
    }

    /// Takes a sample of `series` for every player (`0x29eee`). Series 0 takes the ware counters
    /// and clears them; a longer series adds up the four newest samples of the one below it,
    /// holding each counter at `u16::MAX`.
    pub(crate) fn take_samples(&mut self, series: usize, nodes: &NodeMap) {
        if series >= SERIES {
            return;
        }
        for player in Player::ALL {
            let mut sample = if series == 0 {
                self.collect_counters(player)
            } else {
                self.fold_samples(player, series - 1)
            };
            sample.values = self.values(player, nodes);
            if let Some(series) = self.statistics[player.index()].series.get_mut(series) {
                series.push(sample);
            }
        }
    }

    /// What each statistic stands at for `player` (`0x29e4e`), which every series reads fresh.
    fn values(&self, player: Player, nodes: &NodeMap) -> [u32; Statistic::COUNT] {
        let mut values = [0; Statistic::COUNT];
        values[Statistic::Land.index()] = nodes.owned(player);
        values[Statistic::Buildings.index()] = u32::from(self.all_finished_buildings(player));
        values[Statistic::People.index()] = u32::from(self.people_total(player));
        values[Statistic::Military.index()] = self.military_strength(player);
        values[Statistic::Wares.index()] = u32::from(self.owned_wares_total(player));
        values[Statistic::Gold.index()] = self.coins_counted(player);
        values[Statistic::Productivity.index()] = self.average_productivity(player);
        values[Statistic::FightsWon.index()] = self.fights_won(player);
        values
    }

    /// How productive the twenty rated types are for `player` on average (`0x29e4e`): a type the
    /// player holds none of is left out, and holding none of the twenty gives 0.
    fn average_productivity(&self, player: Player) -> u32 {
        let mut total = 0;
        let mut counted = 0;
        for kind in RATED {
            if self.finished_buildings(player, kind) > 0 {
                total += u32::from(self.productivity(player, kind));
                counted += 1;
            }
        }
        total.checked_div(counted).unwrap_or_default()
    }

    /// `player`'s ware counters gathered into the graph's slots, which clears them.
    fn collect_counters(&mut self, player: Player) -> Sample {
        let mut sample = Sample::EMPTY;
        for (ware, count) in self.ware_arrivals[player.index()].iter_mut().enumerate() {
            if let Some(&slot) = GRAPH_SLOT.get(ware)
                && let Some(counter) = sample.counters.get_mut(usize::from(slot))
            {
                *counter = counter.wrapping_add(*count);
            }
            *count = 0;
        }
        sample
    }

    /// The four newest samples of `series` for `player` added up, each counter held at
    /// `u16::MAX`.
    fn fold_samples(&self, player: Player, series: usize) -> Sample {
        let mut sample = Sample::EMPTY;
        for (slot, counter) in sample.counters.iter_mut().enumerate() {
            let sum: u32 = (0..FOLD)
                .map(|age| u32::from(self.sample(player, series, age).counters[slot]))
                .sum();
            *counter = u16::try_from(sum).unwrap_or(u16::MAX);
        }
        sample
    }
}

#[cfg(test)]
mod tests;
