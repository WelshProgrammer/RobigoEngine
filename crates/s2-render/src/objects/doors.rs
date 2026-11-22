//! Where the door of each building type is for each nation.
//!
//! [`Doors::ORIGINAL`] holds words `+0x06` and `+0x08` of each type's entry in `S2.EXE`'s
//! per-nation building table at `0x1f7d78` (1152 bytes per nation, 36 per type; Gold has
//! the same bytes). [`Doors::new`] builds other tables, for tests or mods.

use std::borrow::Cow;

use s2_assets::Nation;
use s2_sim::BuildingKind;

/// The number of building types in the original tables.
const TYPES: usize = 32;

/// The most types a table can hold: a building record keeps its type in one byte.
pub const MAX_DOOR_TYPES: usize = 256;

/// A door's offset in pixels from its building's screen position. Settlers walk in and out
/// through it, and a building site piles its boards and stones there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Door {
    /// Rightwards.
    pub x: i16,
    /// Downwards.
    pub y: i16,
}

/// The doors of one nation's buildings, indexed by building type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doors {
    doors: Cow<'static, [Door]>,
}

/// Why custom doors were refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DoorsError {
    /// More doors than a type byte can name.
    #[error("{0} doors, more than {MAX_DOOR_TYPES}")]
    TooMany(usize),
}

impl Doors {
    /// The four nations' tables of `S2.EXE`, by nation number.
    pub const ORIGINAL: [Self; 4] = [
        Self {
            doors: Cow::Borrowed(&ROMANS),
        },
        Self {
            doors: Cow::Borrowed(&VIKINGS),
        },
        Self {
            doors: Cow::Borrowed(&JAPANESE),
        },
        Self {
            doors: Cow::Borrowed(&AFRICANS),
        },
    ];

    /// A custom table: `doors[t]` is the door of building type `t`.
    ///
    /// # Errors
    ///
    /// [`DoorsError::TooMany`] past [`MAX_DOOR_TYPES`] doors.
    pub fn new(doors: Vec<Door>) -> Result<Self, DoorsError> {
        if doors.len() > MAX_DOOR_TYPES {
            return Err(DoorsError::TooMany(doors.len()));
        }
        Ok(Self {
            doors: Cow::Owned(doors),
        })
    }

    /// The original table of `nation`.
    #[must_use]
    pub fn original(nation: Nation) -> Self {
        Self::ORIGINAL[nation.index()].clone()
    }

    /// The door of building type `kind`, or no offset past the table.
    #[must_use]
    pub fn get(&self, kind: BuildingKind) -> Door {
        self.doors.get(kind.index()).copied().unwrap_or_default()
    }
}

impl Default for Doors {
    fn default() -> Self {
        Self::original(Nation::Romans)
    }
}

const fn door(x: i16, y: i16) -> Door {
    Door { x, y }
}

const ROMANS: [Door; TYPES] = [
    door(8, 11),
    door(6, 8),
    door(10, 12),
    door(10, 13),
    door(9, 11),
    door(4, 7),
    door(-18, -15),
    door(-10, -9),
    door(-9, -9),
    door(7, 11),
    door(5, 7),
    door(7, 9),
    door(11, 14),
    door(10, 13),
    door(16, 18),
    door(11, 14),
    door(2, 2),
    door(11, 15),
    door(7, 7),
    door(7, 7),
    door(7, 7),
    door(7, 7),
    door(14, 15),
    door(3, 5),
    door(3, 5),
    door(2, 3),
    door(2, 4),
    door(6, 5),
    door(-6, -5),
    door(0, 0),
    door(0, 3),
    door(9, 11),
];

const VIKINGS: [Door; TYPES] = [
    door(8, 9),
    door(8, 9),
    door(9, 9),
    door(9, 8),
    door(9, 9),
    door(2, 6),
    door(-8, -10),
    door(-9, -7),
    door(-7, -6),
    door(11, 13),
    door(8, 9),
    door(9, 9),
    door(4, 9),
    door(4, 5),
    door(0, 3),
    door(12, 13),
    door(11, 11),
    door(11, 11),
    door(8, 9),
    door(8, 9),
    door(7, 9),
    door(8, 9),
    door(8, 8),
    door(8, 11),
    door(3, 5),
    door(10, 10),
    door(10, 10),
    door(7, 6),
    door(9, 10),
    door(0, 0),
    door(1, 4),
    door(9, 11),
];

const JAPANESE: [Door; TYPES] = [
    door(7, 8),
    door(6, 9),
    door(3, 2),
    door(3, 1),
    door(1, 1),
    door(3, 7),
    door(-8, -7),
    door(-7, -8),
    door(-10, -6),
    door(8, 9),
    door(10, 11),
    door(9, 9),
    door(5, 6),
    door(9, 12),
    door(10, 11),
    door(8, 10),
    door(11, 14),
    door(8, 11),
    door(7, 9),
    door(7, 9),
    door(7, 9),
    door(7, 9),
    door(10, 10),
    door(11, 12),
    door(2, 7),
    door(3, 1),
    door(1, 2),
    door(8, 11),
    door(1, 6),
    door(0, 0),
    door(-5, -1),
    door(9, 11),
];

const AFRICANS: [Door; TYPES] = [
    door(4, 7),
    door(1, 5),
    door(7, 11),
    door(2, 7),
    door(1, 7),
    door(-2, 3),
    door(-5, -1),
    door(-17, -12),
    door(-2, 1),
    door(-5, 2),
    door(4, 8),
    door(4, 8),
    door(11, 15),
    door(5, 9),
    door(13, 16),
    door(7, 10),
    door(2, 6),
    door(7, 11),
    door(-2, 4),
    door(-1, 5),
    door(4, 7),
    door(1, 5),
    door(12, 16),
    door(10, 13),
    door(-1, 1),
    door(5, 9),
    door(5, 9),
    door(2, 7),
    door(-2, 0),
    door(-3, 0),
    door(0, 2),
    door(9, 11),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_headquarters_doors_match_each_nation() {
        let doors: Vec<Door> = Nation::ALL
            .map(|nation| Doors::original(nation).get(BuildingKind::HEADQUARTERS))
            .to_vec();
        assert_eq!(doors, [door(3, 5), door(3, 5), door(2, 7), door(-1, 1)]);
    }

    #[test]
    fn types_past_the_table_have_no_offset() {
        assert_eq!(Doors::default().get(BuildingKind(200)), Door::default());
        assert!(matches!(
            Doors::new(vec![Door::default(); MAX_DOOR_TYPES + 1]),
            Err(DoorsError::TooMany(_))
        ));
    }
}
