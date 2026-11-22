//! A player slot, as the map and the records number them.

/// A player, 0 to 6. Slot 9 stores 7 for "nobody".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct Player(u8);

impl Player {
    /// How many players a map can hold.
    pub const COUNT: usize = 7;

    /// Every player, 0 first.
    pub const ALL: [Self; Self::COUNT] = [Self(0), Self(1), Self(2), Self(3), Self(4), Self(5), Self(6)];

    /// Player `index`, or `None` from 7 up.
    #[must_use]
    pub const fn new(index: u8) -> Option<Self> {
        if (index as usize) < Self::COUNT {
            Some(Self(index))
        } else {
            None
        }
    }

    /// The player's number.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
