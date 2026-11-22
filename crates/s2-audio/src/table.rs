//! The effect table.
//!
//! One row an effect id, giving the volume, the jitter amplitude and the playback rate. The
//! samples in `SOUND.LST` are raw 8-bit PCM and carry no rate of their own, so the rate here is
//! the only one there is.

/// How many rows the table holds, including the unreachable row 0.
pub const EFFECT_IDS: usize = 71;

/// The highest id the original will play (`0x23b0e` is silent above it).
pub const LAST_EFFECT: u8 = 70;

/// What the container item for an effect id is, past the 1000 the loader reserves.
pub const ITEM_BASE: u16 = 50;

/// The most volume a row can ask for.
pub const MOST_VOLUME: u8 = 100;

/// The widest jitter a row can ask for.
pub const MOST_JITTER: u8 = 80;

/// How a sound plays from its sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Playback {
    /// As the original plays it: an unsigned 8-bit mono sample at the row's rate, stepped to
    /// 11,025 Hz by the nearest byte.
    Original,
    /// From the sample's own rate, moved by the row's jitter, through a cubic interpolation.
    Smooth,
}

/// One row of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effect {
    /// Volume, 0 to 100.
    pub volume: u8,
    /// The jitter amplitude, 0 to 80.
    pub jitter: u8,
    /// The playback rate in hertz, which an original sample plays at. A smooth sample plays at
    /// its own rate, moved by as much as the jitter moves this one.
    pub rate: u16,
    /// How the sound plays. Without one, an original sample plays the original way and any
    /// other sample plays smooth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playback: Option<Playback>,
}

/// Why a table of effects was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EffectsError {
    /// An id the original never plays.
    #[error("effect {0} is outside 1 to {LAST_EFFECT}")]
    Id(u8),
    /// A volume past [`MOST_VOLUME`].
    #[error("effect {id} asks for volume {volume}, past {MOST_VOLUME}")]
    Volume {
        /// The effect.
        id: u8,
        /// What it asked for.
        volume: u8,
    },
    /// A jitter past [`MOST_JITTER`].
    #[error("effect {id} asks for jitter {jitter}, past {MOST_JITTER}")]
    Jitter {
        /// The effect.
        id: u8,
        /// What it asked for.
        jitter: u8,
    },
}

/// The table, which a mod can replace with one of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Effects([Effect; EFFECT_IDS]);

impl Default for Effects {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

impl Effects {
    /// The table as `S2.EXE` holds it at `0x1f972c`. Row 0 is zeroes and unreachable.
    pub const ORIGINAL: Self = Self([
        Effect {
            volume: 0,
            jitter: 0,
            rate: 0,
            playback: None,
        },
        Effect {
            volume: 60,
            jitter: 0,
            rate: 9000,
            playback: None,
        },
        Effect {
            volume: 70,
            jitter: 0,
            rate: 16000,
            playback: None,
        },
        Effect {
            volume: 75,
            jitter: 0,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 30,
            jitter: 0,
            rate: 11025,
            playback: None,
        },
        Effect {
            volume: 28,
            jitter: 0,
            rate: 11025,
            playback: None,
        },
        Effect {
            volume: 90,
            jitter: 0,
            rate: 8000,
            playback: None,
        },
        Effect {
            volume: 80,
            jitter: 0,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 50,
            jitter: 0,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 30,
            jitter: 0,
            rate: 8000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 0,
            rate: 8000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 50,
            jitter: 0,
            rate: 8000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 13000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 8,
            jitter: 0,
            rate: 6000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 0,
            rate: 7000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 70,
            jitter: 0,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 0,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 16000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 20,
            jitter: 0,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 18,
            jitter: 0,
            rate: 6000,
            playback: None,
        },
        Effect {
            volume: 50,
            jitter: 5,
            rate: 6000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 70,
            jitter: 0,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 80,
            jitter: 5,
            rate: 8000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 8000,
            playback: None,
        },
        Effect {
            volume: 30,
            jitter: 45,
            rate: 8000,
            playback: None,
        },
        Effect {
            volume: 80,
            jitter: 0,
            rate: 8000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 13,
            jitter: 10,
            rate: 16000,
            playback: None,
        },
        Effect {
            volume: 8,
            jitter: 26,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 11,
            jitter: 30,
            rate: 21000,
            playback: None,
        },
        Effect {
            volume: 9,
            jitter: 15,
            rate: 16000,
            playback: None,
        },
        Effect {
            volume: 11,
            jitter: 30,
            rate: 8000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 70,
            jitter: 50,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 10,
            jitter: 30,
            rate: 6500,
            playback: None,
        },
        Effect {
            volume: 20,
            jitter: 30,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 40,
            jitter: 80,
            rate: 8000,
            playback: None,
        },
        Effect {
            volume: 70,
            jitter: 0,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 18,
            jitter: 15,
            rate: 6500,
            playback: None,
        },
        Effect {
            volume: 19,
            jitter: 13,
            rate: 7500,
            playback: None,
        },
        Effect {
            volume: 19,
            jitter: 12,
            rate: 7500,
            playback: None,
        },
        Effect {
            volume: 60,
            jitter: 55,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 35,
            jitter: 30,
            rate: 7000,
            playback: None,
        },
        Effect {
            volume: 20,
            jitter: 65,
            rate: 11200,
            playback: None,
        },
        Effect {
            volume: 38,
            jitter: 75,
            rate: 6500,
            playback: None,
        },
        Effect {
            volume: 45,
            jitter: 55,
            rate: 8000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 70,
            jitter: 45,
            rate: 9000,
            playback: None,
        },
        Effect {
            volume: 70,
            jitter: 0,
            rate: 10000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 8000,
            playback: None,
        },
        Effect {
            volume: 70,
            jitter: 0,
            rate: 8800,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 0,
            rate: 22000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 0,
            rate: 22000,
            playback: None,
        },
        Effect {
            volume: 70,
            jitter: 0,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 90,
            jitter: 0,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 0,
            rate: 4000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
        Effect {
            volume: 100,
            jitter: 5,
            rate: 11000,
            playback: None,
        },
    ]);

    /// A table of `rows`.
    #[must_use]
    pub const fn new(rows: [Effect; EFFECT_IDS]) -> Self {
        Self(rows)
    }

    /// A table of the rows `rows` names by id; every id it leaves out is silent.
    ///
    /// # Errors
    ///
    /// [`EffectsError`] for an id past [`LAST_EFFECT`] or 0, or a volume or jitter past what the
    /// original's arithmetic takes.
    pub fn from_rows(rows: impl IntoIterator<Item = (u8, Effect)>) -> Result<Self, EffectsError> {
        let mut table = [Effect::default(); EFFECT_IDS];
        for (id, row) in rows {
            if id == 0 || id > LAST_EFFECT {
                return Err(EffectsError::Id(id));
            }
            if row.volume > MOST_VOLUME {
                return Err(EffectsError::Volume {
                    id,
                    volume: row.volume,
                });
            }
            if row.jitter > MOST_JITTER {
                return Err(EffectsError::Jitter {
                    id,
                    jitter: row.jitter,
                });
            }
            table[usize::from(id)] = row;
        }
        Ok(Self(table))
    }

    /// Every playable id and its row, in id order.
    pub fn rows(&self) -> impl Iterator<Item = (u8, Effect)> + '_ {
        (1..=LAST_EFFECT).map(|id| (id, self.get(id)))
    }

    /// The row for `id`, which is zeroes for an id the table does not cover.
    #[must_use]
    pub fn get(&self, id: u8) -> Effect {
        self.0.get(usize::from(id)).copied().unwrap_or_default()
    }

    /// The container item `id`'s sample comes from.
    #[must_use]
    pub const fn item(id: u8) -> u16 {
        ITEM_BASE + id as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_rebuilt_from_its_rows_is_the_same_table() {
        let rebuilt = Effects::from_rows(Effects::ORIGINAL.rows()).unwrap();
        assert_eq!(rebuilt, Effects::ORIGINAL);
    }

    #[test]
    fn rows_out_of_range_are_refused_by_name() {
        let loud = Effect {
            volume: MOST_VOLUME + 1,
            ..Effect::default()
        };
        assert_eq!(
            Effects::from_rows([(0, Effect::default())]),
            Err(EffectsError::Id(0))
        );
        assert_eq!(
            Effects::from_rows([(LAST_EFFECT + 1, Effect::default())]),
            Err(EffectsError::Id(LAST_EFFECT + 1))
        );
        assert_eq!(
            Effects::from_rows([(5, loud)]),
            Err(EffectsError::Volume {
                id: 5,
                volume: MOST_VOLUME + 1
            })
        );
    }

    #[test]
    fn the_table_has_a_row_an_id_and_an_empty_first_row() {
        let table = Effects::ORIGINAL;
        assert_eq!(table.get(0), Effect::default());
        assert_eq!(
            table.get(1),
            Effect {
                volume: 60,
                jitter: 0,
                rate: 9000,
                playback: None,
            }
        );
        assert_eq!(
            table.get(LAST_EFFECT),
            Effect {
                volume: 100,
                jitter: 5,
                rate: 11000,
                playback: None,
            }
        );
        assert_eq!(table.get(LAST_EFFECT + 1), Effect::default());
    }

    #[test]
    fn the_rates_are_the_sixteen_the_original_uses() {
        let mut rates: Vec<u16> = (1..=LAST_EFFECT)
            .map(|id| Effects::ORIGINAL.get(id).rate)
            .collect();
        rates.sort_unstable();
        rates.dedup();
        assert_eq!(
            rates,
            [
                4000, 6000, 6500, 7000, 7500, 8000, 8800, 9000, 10000, 11000, 11025, 11200, 13000, 16000,
                21000, 22000
            ]
        );
    }

    #[test]
    fn the_loudest_jitter_is_the_one_on_the_table() {
        let worst = (1..=LAST_EFFECT)
            .max_by_key(|&id| Effects::ORIGINAL.get(id).jitter)
            .expect("the table has rows");
        assert_eq!(worst, 46);
        assert_eq!(Effects::ORIGINAL.get(worst).jitter, 80);
        assert_eq!(Effects::ORIGINAL.get(worst).volume, 40);
        assert_eq!(Effects::ORIGINAL.get(worst).rate, 8000);
    }

    #[test]
    fn an_id_names_the_container_item_past_the_reserved_block() {
        assert_eq!(Effects::item(1), 51);
        assert_eq!(Effects::item(LAST_EFFECT), 120);
    }
}
