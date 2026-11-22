//! The military settings: how many there are, what they are and the defaults a game starts with.

use std::borrow::Cow;

/// How many military settings each player has.
pub const MILITARY_SETTINGS: usize = 7;

/// The highest value a military setting takes.
pub const MOST_MILITARY: u8 = 100;

/// The settings: recruiting share, rank sent, defenders' chance, attack share, then occupation
/// near the centre, in the middle and at the border, all in percent.
pub const RECRUITING: usize = 0;
/// See [`RECRUITING`].
pub const RANK: usize = 1;
/// See [`RECRUITING`].
pub const DEFENCE: usize = 2;
/// See [`RECRUITING`].
pub const ATTACK: usize = 3;
/// See [`RECRUITING`]; the occupation settings follow it by border class.
pub const OCCUPATION: usize = 4;

/// The settings every player starts a game with and the window's default button restores
/// (`0x1fa568`, copied by `0x298b6`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilitaryTables {
    settings: Cow<'static, [u8; MILITARY_SETTINGS]>,
}

/// Why custom military tables were refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MilitaryTablesError {
    /// A setting past [`MOST_MILITARY`].
    #[error("a military setting of {0}, more than {MOST_MILITARY}")]
    Setting(u8),
}

impl MilitaryTables {
    /// The defaults of `S2.EXE` (Gold has the same bytes).
    pub const ORIGINAL: Self = Self {
        settings: Cow::Borrowed(&[100, 50, 100, 50, 10, 30, 100]),
    };

    /// Custom defaults, in the order of [`RECRUITING`] to [`OCCUPATION`] + 2.
    ///
    /// # Errors
    ///
    /// [`MilitaryTablesError::Setting`] for a value past [`MOST_MILITARY`].
    pub fn new(settings: [u8; MILITARY_SETTINGS]) -> Result<Self, MilitaryTablesError> {
        if let Some(&value) = settings.iter().find(|&&value| value > MOST_MILITARY) {
            return Err(MilitaryTablesError::Setting(value));
        }
        Ok(Self {
            settings: Cow::Owned(settings),
        })
    }

    /// The default settings.
    #[must_use]
    pub fn settings(&self) -> [u8; MILITARY_SETTINGS] {
        *self.settings
    }
}

impl Default for MilitaryTables {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

impl std::hash::Hash for MilitaryTables {
    /// The tables are input data, checked by the data hash a replay carries; the state hash
    /// leaves them out.
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {}
}
