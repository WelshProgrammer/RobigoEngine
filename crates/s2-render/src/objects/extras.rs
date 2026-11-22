//! What a finished building draws over itself, for each nation and type.
//!
//! [`Extras::ORIGINAL`] holds, from `S2.EXE`'s per-nation building table at `0x1f7d78`
//! (1152 bytes per nation, 36 per type), the work glow's offset (`+0x00` to `+0x04`), the small
//! flags' (`+0x0a`, `+0x0c`), the smoke's (`+0x0e` to `+0x12`) and the stopped sign's (`+0x22`,
//! `+0x23`), and for the military types the banner's from the table at `0x1f8fc6`.
//! [`Extras::new`] builds other tables, for tests or mods. [`Yard::ORIGINAL`] holds the offsets of
//! what a pig farm, a donkey breeder and a harbour draw beside them.

use std::borrow::Cow;

use s2_assets::Nation;
use s2_sim::BuildingKind;

use super::doors::MAX_DOOR_TYPES;

/// The number of building types in the original tables.
const TYPES: usize = 32;

/// An offset in pixels from a building's screen position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Offset {
    /// Rightwards.
    pub x: i16,
    /// Downwards.
    pub y: i16,
}

/// What one building type draws over itself, and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Extra {
    /// The glow while it works, if the type has one.
    pub work: Option<Offset>,
    /// The top of the column of small flags on a military building or a headquarters.
    pub flags: Offset,
    /// The smoke, if the type has some.
    pub smoke: Option<Offset>,
    /// The sign that production is stopped.
    pub stopped: Offset,
    /// The banner on a military building's pole, which shows how near the enemy is.
    pub banner: Option<Offset>,
}

/// One nation's extras, indexed by building type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extras {
    rows: Cow<'static, [Extra]>,
}

/// Why a custom table was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ExtrasError {
    /// More rows than a type byte can name.
    #[error("{0} rows, more than {MAX_DOOR_TYPES}")]
    TooMany(usize),
}

impl Extras {
    /// The four nations' tables of `S2.EXE`, by nation number.
    pub const ORIGINAL: [Self; 4] = [
        Self {
            rows: Cow::Borrowed(&ROMANS),
        },
        Self {
            rows: Cow::Borrowed(&VIKINGS),
        },
        Self {
            rows: Cow::Borrowed(&JAPANESE),
        },
        Self {
            rows: Cow::Borrowed(&AFRICANS),
        },
    ];

    /// A custom table: `rows[t]` is what building type `t` draws.
    ///
    /// # Errors
    ///
    /// [`ExtrasError::TooMany`] past [`MAX_DOOR_TYPES`] rows.
    pub fn new(rows: Vec<Extra>) -> Result<Self, ExtrasError> {
        if rows.len() > MAX_DOOR_TYPES {
            return Err(ExtrasError::TooMany(rows.len()));
        }
        Ok(Self {
            rows: Cow::Owned(rows),
        })
    }

    /// The original table of `nation`.
    #[must_use]
    pub fn original(nation: Nation) -> Self {
        Self::ORIGINAL[nation.index()].clone()
    }

    /// What building type `kind` draws, or nothing past the table.
    #[must_use]
    pub fn get(&self, kind: BuildingKind) -> Extra {
        self.rows.get(kind.index()).copied().unwrap_or_default()
    }
}

impl Default for Extras {
    fn default() -> Self {
        Self::original(Nation::Romans)
    }
}

const fn at(x: i16, y: i16) -> Offset {
    Offset { x, y }
}

const fn extra(
    work: Option<Offset>,
    flags: Offset,
    smoke: Option<Offset>,
    stopped: Offset,
    banner: Option<Offset>,
) -> Extra {
    Extra {
        work,
        flags,
        smoke,
        stopped,
        banner,
    }
}

const ROMANS: [Extra; TYPES] = [
    extra(None, at(0, 0), None, at(-28, -10), None),
    extra(None, at(0, 0), None, at(7, -20), None),
    extra(None, at(0, 0), None, at(-4, 4), None),
    extra(None, at(0, 0), None, at(-4, 1), None),
    extra(None, at(0, 0), None, at(20, -19), None),
    extra(None, at(0, 0), None, at(11, -29), None),
    extra(None, at(0, 0), None, at(41, -43), None),
    extra(None, at(0, 0), None, at(35, -27), None),
    extra(None, at(0, 0), None, at(-35, -19), None),
    extra(None, at(0, 0), None, at(3, -13), None),
    extra(None, at(0, 0), None, at(2, -21), None),
    extra(Some(at(-15, -25)), at(0, 0), None, at(15, -13), None),
    extra(Some(at(-16, -33)), at(0, 0), None, at(23, -10), None),
    extra(None, at(0, 0), None, at(6, -14), None),
    extra(Some(at(-36, -32)), at(0, 0), None, at(18, -11), None),
    extra(Some(at(20, -49)), at(0, 0), None, at(23, -5), None),
    extra(None, at(0, 0), None, at(-1, -23), None),
    extra(Some(at(-26, -44)), at(0, 0), None, at(22, -8), None),
    extra(None, at(0, 0), None, at(21, 4), None),
    extra(None, at(0, 0), None, at(21, 4), None),
    extra(None, at(0, 0), None, at(26, 4), None),
    extra(None, at(0, 0), None, at(15, -2), None),
    extra(None, at(0, 0), None, at(0, 0), None),
    extra(None, at(0, 0), Some(at(8, -115)), at(0, 0), None),
    extra(None, at(-18, -112), None, at(0, 0), None),
    extra(None, at(-24, -33), None, at(15, -3), Some(at(28, -17))),
    extra(None, at(9, -59), None, at(14, -2), Some(at(29, -17))),
    extra(None, at(-2, -76), None, at(4, -22), Some(at(-26, -10))),
    extra(None, at(23, -71), None, at(20, -39), Some(at(-2, -71))),
    extra(None, at(0, 0), None, at(0, 0), None),
    extra(None, at(0, 0), None, at(10, -9), None),
    extra(None, at(0, 0), None, at(0, 0), None),
];

const VIKINGS: [Extra; TYPES] = [
    extra(Some(at(2, -34)), at(0, 0), None, at(16, -11), None),
    extra(Some(at(-5, -26)), at(0, 0), None, at(4, -16), None),
    extra(Some(at(0, -32)), at(0, 0), None, at(15, -13), None),
    extra(Some(at(4, -34)), at(0, 0), None, at(-8, -4), None),
    extra(Some(at(-5, -36)), at(0, 0), None, at(-8, -3), None),
    extra(Some(at(-10, -42)), at(0, 0), None, at(10, -26), None),
    extra(Some(at(-17, -45)), at(0, 0), None, at(-26, -21), None),
    extra(Some(at(-30, -34)), at(0, 0), None, at(-25, -12), None),
    extra(Some(at(-27, -38)), at(0, 0), None, at(-15, -32), None),
    extra(Some(at(8, -40)), at(0, 0), None, at(2, -13), None),
    extra(None, at(0, 0), None, at(3, -17), None),
    extra(Some(at(-21, -25)), at(0, 0), None, at(18, -14), None),
    extra(Some(at(-2, -36)), at(0, 0), None, at(-1, -20), None),
    extra(Some(at(-9, -33)), at(0, 0), None, at(27, -17), None),
    extra(Some(at(-24, -35)), at(0, 0), None, at(28, -11), None),
    extra(Some(at(16, -35)), at(0, 0), None, at(5, -15), None),
    extra(None, at(0, 0), None, at(14, -17), None),
    extra(Some(at(4, -38)), at(0, 0), None, at(-8, 0), None),
    extra(None, at(0, 0), None, at(17, -4), None),
    extra(None, at(0, 0), None, at(18, -4), None),
    extra(None, at(0, 0), None, at(16, -4), None),
    extra(None, at(0, 0), None, at(14, -4), None),
    extra(Some(at(-12, -49)), at(0, 0), None, at(0, 0), None),
    extra(None, at(0, 0), Some(at(5, -81)), at(0, 0), None),
    extra(None, at(20, -52), None, at(0, 0), None),
    extra(None, at(-5, -48), None, at(-3, -1), Some(at(21, -18))),
    extra(None, at(-5, -48), None, at(-3, -1), Some(at(21, -18))),
    extra(None, at(-9, -72), None, at(-1, -27), Some(at(15, -51))),
    extra(None, at(-12, -56), None, at(-15, -2), Some(at(-37, -31))),
    extra(None, at(0, 0), None, at(0, 0), None),
    extra(None, at(0, 0), None, at(-3, -20), None),
    extra(None, at(0, 0), None, at(0, 0), None),
];

const JAPANESE: [Extra; TYPES] = [
    extra(None, at(0, 0), None, at(-8, -6), None),
    extra(None, at(0, 0), None, at(14, -11), None),
    extra(None, at(0, 0), None, at(-13, -7), None),
    extra(None, at(0, 0), None, at(-10, -11), None),
    extra(None, at(0, 0), None, at(-13, -7), None),
    extra(None, at(0, 0), None, at(35, -20), None),
    extra(None, at(0, 0), None, at(-27, 22), None),
    extra(None, at(0, 0), None, at(30, -25), None),
    extra(None, at(0, 0), None, at(-15, -30), None),
    extra(None, at(0, 0), None, at(14, -10), None),
    extra(None, at(0, 0), None, at(0, -22), None),
    extra(Some(at(-30, -38)), at(0, 0), None, at(-30, -13), None),
    extra(None, at(0, 0), None, at(14, -14), None),
    extra(None, at(0, 0), None, at(-6, -2), None),
    extra(Some(at(-23, -42)), at(0, 0), None, at(21, -3), None),
    extra(Some(at(18, -56)), at(0, 0), None, at(13, -34), None),
    extra(None, at(0, 0), None, at(19, -7), None),
    extra(None, at(0, 0), None, at(16, -11), None),
    extra(None, at(0, 0), None, at(13, -14), None),
    extra(None, at(0, 0), None, at(14, -13), None),
    extra(None, at(0, 0), None, at(12, -14), None),
    extra(None, at(0, 0), None, at(20, -2), None),
    extra(None, at(0, 0), None, at(0, 0), None),
    extra(None, at(0, 0), None, at(0, 0), None),
    extra(None, at(-19, -90), None, at(0, 0), None),
    extra(None, at(-9, -47), None, at(12, -10), Some(at(17, -43))),
    extra(None, at(15, -56), None, at(13, -10), Some(at(-3, -47))),
    extra(None, at(17, -60), None, at(25, -22), Some(at(36, -50))),
    extra(None, at(0, -41), None, at(10, -14), Some(at(22, -51))),
    extra(None, at(0, 0), None, at(0, 0), None),
    extra(None, at(0, 0), None, at(19, -15), None),
    extra(None, at(0, 0), None, at(0, 0), None),
];

const AFRICANS: [Extra; TYPES] = [
    extra(None, at(0, 0), None, at(3, -16), None),
    extra(None, at(0, 0), None, at(27, -18), None),
    extra(Some(at(3, -33)), at(0, 0), None, at(6, -10), None),
    extra(None, at(0, 0), None, at(8, -12), None),
    extra(None, at(0, 0), None, at(-3, -15), None),
    extra(None, at(0, 0), None, at(-4, -20), None),
    extra(None, at(0, 0), None, at(-22, -16), None),
    extra(None, at(0, 0), None, at(-18, -34), None),
    extra(None, at(0, 0), None, at(-4, -21), None),
    extra(None, at(0, 0), None, at(-6, -19), None),
    extra(None, at(0, 0), None, at(2, -13), None),
    extra(Some(at(27, -40)), at(0, 0), None, at(12, -13), None),
    extra(Some(at(-20, -37)), at(0, 0), None, at(10, -9), None),
    extra(Some(at(-27, -47)), at(0, 0), None, at(-14, -9), None),
    extra(Some(at(-32, -23)), at(0, 0), None, at(13, -7), None),
    extra(Some(at(17, -52)), at(0, 0), None, at(6, -14), None),
    extra(None, at(0, 0), None, at(33, -15), None),
    extra(None, at(0, 0), None, at(20, -11), None),
    extra(None, at(0, 0), Some(at(32, -6)), at(-5, -15), None),
    extra(None, at(0, 0), Some(at(30, -7)), at(-5, -15), None),
    extra(None, at(0, 0), Some(at(34, -6)), at(1, -12), None),
    extra(None, at(0, 0), Some(at(31, -20)), at(-1, -15), None),
    extra(None, at(0, 0), None, at(0, 0), None),
    extra(None, at(0, 0), Some(at(36, -51)), at(0, 0), None),
    extra(None, at(-12, -100), None, at(0, 0), None),
    extra(None, at(24, -33), None, at(18, -4), Some(at(-8, -37))),
    extra(None, at(19, -37), None, at(18, -3), Some(at(7, -46))),
    extra(None, at(31, -89), None, at(22, -19), Some(at(-20, -25))),
    extra(None, at(35, -66), None, at(28, -23), Some(at(-46, -61))),
    extra(None, at(0, 0), None, at(0, 0), None),
    extra(None, at(0, 0), None, at(14, -4), None),
    extra(None, at(0, 0), None, at(0, 0), None),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_headquarters_flags_sit_where_each_nation_draws_them() {
        let hq = BuildingKind::HEADQUARTERS;
        assert_eq!(Extras::original(Nation::Romans).get(hq).flags, at(-18, -112));
        assert_eq!(Extras::original(Nation::Africans).get(hq).flags, at(-12, -100));
    }

    #[test]
    fn only_the_military_types_carry_a_banner() {
        for nation in Nation::ALL {
            let extras = Extras::original(nation);
            for kind in 0..TYPES {
                let kind = BuildingKind(u8::try_from(kind).unwrap());
                let military = (BuildingKind::BARRACKS.0..=BuildingKind::FORTRESS.0).contains(&kind.0);
                assert_eq!(extras.get(kind).banner.is_some(), military, "{nation:?} {kind:?}");
            }
        }
    }

    #[test]
    fn a_type_past_the_table_draws_nothing() {
        assert_eq!(
            Extras::original(Nation::Vikings).get(BuildingKind(200)),
            Extra::default()
        );
    }
}

/// Where one nation's pig farm, donkey breeder and harbour put what they draw beside them, from
/// `S2.EXE`'s per-nation tables at `0x1f8f78` (pigs), `0x1f8fb8` (donkeys) and `0x1f8fa0`
/// (the harbour's expedition boards, stones and builder).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Yard {
    /// Each pig, the sow first.
    pub pigs: [Offset; 5],
    /// Each donkey.
    pub donkeys: [Offset; 3],
    /// The harbour's boards, its stones and its builder.
    pub harbour: [Offset; 3],
}

impl Yard {
    /// The four nations' offsets of `S2.EXE`, by nation number.
    pub const ORIGINAL: [Self; 4] = [
        Self {
            pigs: [at(-17, 0), at(-25, 11), at(-8, 8), at(-18, 21), at(2, 20)],
            donkeys: [at(3, -20), at(16, -21), at(29, -21)],
            harbour: [at(-55, -7), at(-34, 12), at(-8, 18)],
        },
        Self {
            pigs: [at(8, -5), at(25, -13), at(-11, 7), at(-31, 12), at(-13, 16)],
            donkeys: [at(7, -17), at(18, -17), at(28, -17)],
            harbour: [at(-49, 5), at(-34, 19), at(-2, 25)],
        },
        Self {
            pigs: [at(-16, -1), at(-36, -2), at(-34, 7), at(-16, 11), at(-22, 21)],
            donkeys: [at(5, -17), at(17, -18), at(30, -18)],
            harbour: [at(-55, 6), at(-34, 19), at(-8, 23)],
        },
        Self {
            pigs: [at(2, -9), at(-17, 3), at(16, 2), at(-26, 14), at(-6, 13)],
            donkeys: [at(13, -8), at(26, -9), at(39, -9)],
            harbour: [at(-48, 16), at(-20, 18), at(0, 24)],
        },
    ];
}
