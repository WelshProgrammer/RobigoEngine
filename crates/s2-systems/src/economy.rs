//! The tables behind each player's economy settings: the weight each building type gives a ware,
//! the metalworks' tool weights, the distribution groups and the transport ranks.
//!
//! [`EconomyTables::ORIGINAL`] holds `S2.EXE`'s tables (Gold has the same bytes).
//! [`EconomyTables::new`] builds others, for tests or mods.

use std::borrow::Cow;

use crate::building_types::BUILDING_TYPES;
use crate::building_types::BuildingKind;
use crate::ware::{TRANSPORT_GROUPS, WareKind};

/// The highest weight a settings window offers.
pub const MAX_WEIGHT: u8 = 100;

/// The most consumers a distribution group's page has room for.
pub const MAX_CONSUMERS: usize = 5;

/// Who a distribution slider weighs a ware for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Consumer {
    /// Every building of the type.
    Building(BuildingKind),
    /// Building sites (`254`).
    Construction,
}

/// One page of the distribution window: a ware and the consumers it is shared out between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionGroup {
    /// The wares each slider sets, the first being the one it shows. Food sets fish, bread and
    /// meat together (`0x1fa734`).
    pub wares: Cow<'static, [WareKind]>,
    /// The consumers, one slider each.
    pub consumers: Cow<'static, [Consumer]>,
}

/// The defaults a new game gives every player's economy settings, and the ones each window's
/// default button goes back to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EconomyTables {
    type_weights: Cow<'static, [u8]>,
    construction: u8,
    tools: [u8; WareKind::TOOLS],
    groups: Cow<'static, [DistributionGroup]>,
    ranks: [u8; TRANSPORT_GROUPS],
}

/// Why custom economy tables were refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EconomyTablesError {
    /// A weight past [`MAX_WEIGHT`].
    #[error("a weight of {0}, more than {MAX_WEIGHT}")]
    Weight(u8),
    /// The type weights do not cover every building type.
    #[error("{0} type weights for {BUILDING_TYPES} building types")]
    TypeWeights(usize),
    /// A group with no wares, or more consumers than a page shows.
    #[error("distribution group {0} has no ware or more than {MAX_CONSUMERS} consumers")]
    Group(usize),
    /// The transport ranks do not give each group its own place.
    #[error("the transport ranks are not each place once")]
    Ranks,
}

use Consumer::{Building as B, Construction};

const TYPE_WEIGHTS: [u8; BUILDING_TYPES] = [
    0, 0, 0, 0, 0, 70, 0, 30, 20, 50, 50, 60, 70, 40, 80, 100, 20, 30, 100, 70, 50, 30, 10, 40, 10, 40, 50,
    60, 70, 40, 10, 80,
];

const GROUPS: [DistributionGroup; 6] = [
    DistributionGroup {
        wares: Cow::Borrowed(&[WareKind::MEAT, WareKind::FISH, WareKind::BREAD]),
        consumers: Cow::Borrowed(&[
            B(BuildingKind::GRANITE_MINE),
            B(BuildingKind::COAL_MINE),
            B(BuildingKind::IRON_MINE),
            B(BuildingKind::GOLD_MINE),
        ]),
    },
    DistributionGroup {
        wares: Cow::Borrowed(&[WareKind::GRAIN]),
        consumers: Cow::Borrowed(&[
            B(BuildingKind::MILL),
            B(BuildingKind::PIG_FARM),
            B(BuildingKind::DONKEY_BREEDER),
            B(BuildingKind::BREWERY),
        ]),
    },
    DistributionGroup {
        wares: Cow::Borrowed(&[WareKind::IRON]),
        consumers: Cow::Borrowed(&[B(BuildingKind::ARMOURY), B(BuildingKind::METALWORKS)]),
    },
    DistributionGroup {
        wares: Cow::Borrowed(&[WareKind::COAL]),
        consumers: Cow::Borrowed(&[
            B(BuildingKind::ARMOURY),
            B(BuildingKind::IRON_SMELTER),
            B(BuildingKind::MINT),
        ]),
    },
    DistributionGroup {
        wares: Cow::Borrowed(&[WareKind::BOARDS]),
        consumers: Cow::Borrowed(&[
            Construction,
            B(BuildingKind::METALWORKS),
            B(BuildingKind::SHIPYARD),
        ]),
    },
    DistributionGroup {
        wares: Cow::Borrowed(&[WareKind::WATER]),
        consumers: Cow::Borrowed(&[
            B(BuildingKind::BAKERY),
            B(BuildingKind::BREWERY),
            B(BuildingKind::DONKEY_BREEDER),
            B(BuildingKind::PIG_FARM),
        ]),
    },
];

impl EconomyTables {
    /// The tables of `S2.EXE`: type weights `0x1fa751`, tool weights `0x1fa737`,
    /// distribution groups `0x1fa771` and transport ranks `0x1fa743`.
    pub const ORIGINAL: Self = Self {
        type_weights: Cow::Borrowed(&TYPE_WEIGHTS),
        construction: MAX_WEIGHT,
        tools: [10, 40, 20, 50, 70, 10, 30, 10, 20, 10, 20, 10],
        groups: Cow::Borrowed(&GROUPS),
        ranks: [9, 7, 8, 11, 10, 2, 6, 5, 4, 3, 0, 12, 1, 13],
    };

    /// Custom tables: `type_weights[t]` is the weight building type `t` gives any ware,
    /// `construction` a site's weight for boards and stones, `tools` the metalworks' weights from
    /// tongs up, and `ranks[g]` transport group `g`'s place in the list, 0 first.
    ///
    /// # Errors
    ///
    /// [`EconomyTablesError`] for a weight past [`MAX_WEIGHT`], type weights that do not cover
    /// every building type, a group with no ware or too many consumers, or ranks that are not
    /// each place once.
    pub fn new(
        type_weights: Vec<u8>,
        construction: u8,
        tools: [u8; WareKind::TOOLS],
        groups: Vec<DistributionGroup>,
        ranks: [u8; TRANSPORT_GROUPS],
    ) -> Result<Self, EconomyTablesError> {
        if type_weights.len() != BUILDING_TYPES {
            return Err(EconomyTablesError::TypeWeights(type_weights.len()));
        }
        if let Some(&weight) = type_weights
            .iter()
            .chain(&tools)
            .chain([&construction])
            .find(|&&weight| weight > MAX_WEIGHT)
        {
            return Err(EconomyTablesError::Weight(weight));
        }
        if let Some(index) = groups
            .iter()
            .position(|group| group.wares.is_empty() || group.consumers.len() > MAX_CONSUMERS)
        {
            return Err(EconomyTablesError::Group(index));
        }
        let mut seen = [false; TRANSPORT_GROUPS];
        for &rank in &ranks {
            let place = seen.get_mut(usize::from(rank)).ok_or(EconomyTablesError::Ranks)?;
            if *place {
                return Err(EconomyTablesError::Ranks);
            }
            *place = true;
        }
        Ok(Self {
            type_weights: Cow::Owned(type_weights),
            construction,
            tools,
            groups: Cow::Owned(groups),
            ranks,
        })
    }

    /// The weight building type `kind` gives any ware to start with.
    #[must_use]
    pub fn type_weight(&self, kind: BuildingKind) -> u8 {
        self.type_weights.get(kind.index()).copied().unwrap_or(0)
    }

    /// A site's weight for boards and stones to start with.
    #[must_use]
    pub const fn construction(&self) -> u8 {
        self.construction
    }

    /// The metalworks' weights for the tools to start with, from tongs up.
    #[must_use]
    pub const fn tools(&self) -> &[u8; WareKind::TOOLS] {
        &self.tools
    }

    /// The distribution window's pages.
    #[must_use]
    pub fn groups(&self) -> &[DistributionGroup] {
        &self.groups
    }

    /// Each transport group's place in the list to start with, 0 first.
    #[must_use]
    pub const fn ranks(&self) -> &[u8; TRANSPORT_GROUPS] {
        &self.ranks
    }
}

impl Default for EconomyTables {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

impl std::hash::Hash for EconomyTables {
    /// The tables are input data, checked by the data hash a replay carries; the state hash
    /// leaves them out.
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_original_tables_pass_their_own_checks() {
        let tables = EconomyTables::ORIGINAL;
        let rebuilt = EconomyTables::new(
            tables.type_weights.to_vec(),
            tables.construction,
            tables.tools,
            tables.groups.to_vec(),
            tables.ranks,
        );
        assert_eq!(rebuilt, Ok(tables));
    }

    #[test]
    fn ranks_must_give_each_group_its_own_place() {
        let tables = EconomyTables::ORIGINAL;
        let mut ranks = tables.ranks;
        ranks[0] = ranks[1];
        let refused = EconomyTables::new(
            tables.type_weights.to_vec(),
            tables.construction,
            tables.tools,
            tables.groups.to_vec(),
            ranks,
        );
        assert_eq!(refused, Err(EconomyTablesError::Ranks));
    }
}
