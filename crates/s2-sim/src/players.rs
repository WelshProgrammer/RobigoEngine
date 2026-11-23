//! Who plays each player and with which face: the player slots a game is set up with.

use std::borrow::Cow;

use s2_formats::map::Map;

use crate::rng::GameRng;
use crate::world::Player;

mod roster;

pub use roster::{Roster, RosterEntry};

/// How many nations there are, 0 Romans to 3 Africans.
pub const NATIONS: u8 = 4;

/// How the players are given the map's start positions (the option word at `0x1f7907`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum StartOrder {
    /// The players in the game take the positions the map has in turn, lowest first.
    #[default]
    InSequence,
    /// The positions are shuffled with the game's generator first.
    Random,
}

impl StartOrder {
    /// How many swaps the shuffle makes (`0x107cb`).
    const SWAPS: u16 = 100;

    /// The start positions the players in the game take in turn, from the header's `starts`,
    /// as a new Unlimited Play game sets them out (`0x10749` to `0x108ec`): the positions the
    /// map has, in slot order, then for [`Self::Random`] 100 swaps of two of them, each picked
    /// as a draw modulo how many there are.
    pub fn arrange<T: Copy>(self, starts: &[Option<T>], rng: &mut GameRng) -> Vec<T> {
        let mut starts: Vec<T> = starts.iter().flatten().copied().collect();
        let count = u16::try_from(starts.len()).unwrap_or(u16::MAX);
        if self == Self::Random && count > 0 {
            for _ in 0..Self::SWAPS {
                let first = rng.step() % count;
                let second = rng.step() % count;
                starts.swap(usize::from(first), usize::from(second));
            }
        }
        starts
    }
}

/// Who plays a player (`+0x03` of the slot record at `0x1f794d`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Controller {
    /// The person at the machine, shown as `#1`.
    First,
    /// A second person at the same machine, shown as `#2`.
    Second,
    /// The computer.
    Computer,
    /// Nobody any more: the player's last finished building is gone (`0x29aba`).
    Beaten,
}

impl Controller {
    /// All four, in the order of the bytes the original stores.
    pub const ALL: [Self; 4] = [Self::First, Self::Second, Self::Computer, Self::Beaten];

    /// Whether a person plays it.
    #[must_use]
    pub const fn is_person(self) -> bool {
        matches!(self, Self::First | Self::Second)
    }
}

/// One of the faces a player can take, by its place in [`Portraits`] (`+0x04` of the slot
/// record).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct Portrait(pub u8);

impl Portrait {
    /// Its place in [`Portraits`].
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// What a face brings with it (`0x1f7850`, 12 bytes a face): the nation and the computer
/// player's profile the game start copies into the slot (`0x16f7c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortraitRow {
    /// The nation, 0 Romans to 3 Africans.
    pub nation: u8,
    /// Which of the computer player's profiles it plays by (`0x1fd678`, 358 bytes each).
    pub profile: u8,
}

/// The faces of the original's table, three a nation.
const ORIGINAL_PORTRAITS: [PortraitRow; 12] = [
    PortraitRow {
        nation: 0,
        profile: 0,
    },
    PortraitRow {
        nation: 0,
        profile: 2,
    },
    PortraitRow {
        nation: 0,
        profile: 1,
    },
    PortraitRow {
        nation: 1,
        profile: 0,
    },
    PortraitRow {
        nation: 1,
        profile: 3,
    },
    PortraitRow {
        nation: 1,
        profile: 1,
    },
    PortraitRow {
        nation: 2,
        profile: 1,
    },
    PortraitRow {
        nation: 2,
        profile: 0,
    },
    PortraitRow {
        nation: 2,
        profile: 2,
    },
    PortraitRow {
        nation: 3,
        profile: 3,
    },
    PortraitRow {
        nation: 3,
        profile: 2,
    },
    PortraitRow {
        nation: 3,
        profile: 0,
    },
];

/// Why a portrait table was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PortraitsError {
    /// Fewer faces than players, so the setup screen could not give each player its own.
    #[error("{0} portraits, fewer than the {count} players", count = Player::COUNT)]
    TooFew(usize),
    /// More faces than a slot record's byte can name.
    #[error("{0} portraits, more than 256")]
    TooMany(usize),
    /// A face names a nation past the four.
    #[error("portrait {portrait} has nation {nation}")]
    Nation {
        /// The face.
        portrait: usize,
        /// The nation it names.
        nation: u8,
    },
}

/// The faces a player can take, by [`Portrait`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Portraits(Cow<'static, [PortraitRow]>);

impl Portraits {
    /// The original's twelve.
    pub const ORIGINAL: Self = Self(Cow::Borrowed(&ORIGINAL_PORTRAITS));

    /// A custom table, at least one face a player.
    ///
    /// # Errors
    ///
    /// [`PortraitsError`] for too few or too many faces, or a nation past the four.
    pub fn new(rows: Vec<PortraitRow>) -> Result<Self, PortraitsError> {
        if rows.len() < Player::COUNT {
            return Err(PortraitsError::TooFew(rows.len()));
        }
        if rows.len() > usize::from(u8::MAX) + 1 {
            return Err(PortraitsError::TooMany(rows.len()));
        }
        if let Some((portrait, row)) = rows.iter().enumerate().find(|(_, row)| row.nation >= NATIONS) {
            return Err(PortraitsError::Nation {
                portrait,
                nation: row.nation,
            });
        }
        Ok(Self(Cow::Owned(rows)))
    }

    /// Face `portrait`, or `None` past the table.
    #[must_use]
    pub fn get(&self, portrait: Portrait) -> Option<&PortraitRow> {
        self.0.get(portrait.index())
    }

    /// How many faces there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are none, which [`Self::new`] refuses.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Default for Portraits {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

/// A player in the game: who plays it and its face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PlayerSlot {
    /// Who plays it.
    pub controller: Controller,
    /// Its face.
    pub portrait: Portrait,
}

/// Every player's slot: `None` for a player not in the game, whose word at `0x1f794d + 8·player`
/// is clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct PlayerSlots([Option<PlayerSlot>; Player::COUNT]);

impl PlayerSlots {
    /// Nobody in the game.
    pub const EMPTY: Self = Self([None; Player::COUNT]);

    /// The slots `slots` describes, by player.
    #[must_use]
    pub const fn new(slots: [Option<PlayerSlot>; Player::COUNT]) -> Self {
        Self(slots)
    }

    /// How many players the settings screen offers for `map`: the header's player count, held to
    /// the start positions it holds so that every player put in the game gets one. `S2.EXE`
    /// offers the whole count, and a player past the positions gets its headquarters wherever
    /// the stack points; every map in `DATA/MAPS3` holds as many positions as it counts players.
    #[must_use]
    pub fn room(map: &Map) -> usize {
        map.header().map_or(0, |header| {
            let positions = header.headquarters().iter().flatten().count();
            usize::from(header.player_count()).min(positions)
        })
    }

    /// What Unlimited Play's setup screen offers for a map with `count` start positions, with
    /// faces from `portraits`. Loading the map's header puts each of the first `count` players in the
    /// game with face `player`, the first played by [`Controller::First`] and the rest by the
    /// computer (`0x1574c`). The screen then walks players 1 to 6 in turn and moves each one's
    /// face on to the next that no player in the game has, its own included (`0x612a2`,
    /// `0x607b8`), which gives the first Unlimited Play map's four players faces 0, 4, 5 and 6.
    #[must_use]
    pub fn offered(count: usize, portraits: &Portraits) -> Self {
        let faces = portraits.len();
        let mut slots = Self::EMPTY;
        for (index, slot) in slots.0.iter_mut().enumerate().take(count) {
            *slot = Some(PlayerSlot {
                controller: if index == 0 {
                    Controller::First
                } else {
                    Controller::Computer
                },
                portrait: Portrait(u8::try_from(index).unwrap_or_default()),
            });
        }
        for index in 1..Player::COUNT {
            let Some(own) = slots.0[index].map(|slot| slot.portrait) else {
                continue;
            };
            let free = (0..faces)
                .filter_map(|step| u8::try_from((own.index() + step) % faces).ok())
                .map(Portrait)
                .find(|&face| slots.0.iter().flatten().all(|slot| slot.portrait != face));
            if let (Some(face), Some(slot)) = (free, slots.0[index].as_mut()) {
                slot.portrait = face;
            }
        }
        slots
    }

    /// The slots a new game on `map` starts with when nothing on the setup screen is changed:
    /// [`Self::offered`] for the header's player count, less the players from `players` up and
    /// any player whose start position the header leaves empty. A map without a header has
    /// nobody in the game.
    #[must_use]
    pub fn new_game(map: &Map, players: Option<usize>, portraits: &Portraits) -> Self {
        let Some(header) = map.header() else {
            return Self::EMPTY;
        };
        let mut slots = Self::offered(usize::from(header.player_count()), portraits);
        let count = players.unwrap_or(Player::COUNT);
        for (index, (slot, start)) in slots.0.iter_mut().zip(header.headquarters()).enumerate() {
            if index >= count || start.is_none() {
                *slot = None;
            }
        }
        slots
    }

    /// The slots a campaign chapter on `map` with `script` starts with (`0x5eb16`): each player
    /// below the header's player count in the game, played by the computer with its own number
    /// as its face (`0x1574c`); each `GLOBAL_ADD_COMPUTER_PLAYER` in the script's
    /// `[GLOBAL_MAP_SETTINGS]` putting its player in the game the same way with the face it
    /// names; then player 0 in the game as the first person with face 0.
    #[must_use]
    pub fn campaign(map: &Map, script: &s2_formats::rtx::Script) -> Self {
        let mut slots = Self::EMPTY;
        let count = map
            .header()
            .map_or(0, |header| usize::from(header.player_count()));
        for (index, slot) in slots.0.iter_mut().enumerate().take(count) {
            *slot = Some(PlayerSlot {
                controller: Controller::Computer,
                portrait: Portrait(u8::try_from(index).unwrap_or_default()),
            });
        }
        let added = script
            .global_settings()
            .into_iter()
            .flat_map(|section| &section.commands);
        for command in added {
            if let s2_formats::rtx::Op::GlobalAddComputerPlayer { player, character } = command.op
                && let Some(player) = Player::new(player.to_le_bytes()[0])
            {
                slots.set(
                    player,
                    Some(PlayerSlot {
                        controller: Controller::Computer,
                        portrait: Portrait(character.to_le_bytes()[0]),
                    }),
                );
            }
        }
        slots.0[0] = Some(PlayerSlot {
            controller: Controller::First,
            portrait: Portrait(0),
        });
        slots
    }

    /// The slots a World Campaign continent on `map` starts with (`0x5efcd`): each player below
    /// the header's player count in the game, played by the computer with the face the header
    /// names for it (`0x157af` reads the header's faces in this mode alone), then player 0 played
    /// by the first person, keeping its face from the header.
    #[must_use]
    pub fn world_campaign(map: &Map) -> Self {
        let mut slots = Self::EMPTY;
        let Some(header) = map.header() else {
            return slots;
        };
        let count = usize::from(header.player_count());
        for ((index, slot), &face) in slots.0.iter_mut().enumerate().zip(header.leaders()).take(count) {
            *slot = Some(PlayerSlot {
                controller: if index == 0 {
                    Controller::First
                } else {
                    Controller::Computer
                },
                portrait: Portrait(face),
            });
        }
        slots
    }

    /// The same slots less any player whose start position `map`'s header leaves empty, and
    /// everyone for a map without a header.
    #[must_use]
    pub fn on_map(mut self, map: &Map) -> Self {
        let starts = map.header().map(s2_formats::map::Header::headquarters);
        for (index, slot) in self.0.iter_mut().enumerate() {
            if !starts.is_some_and(|starts| starts.get(index).is_some_and(Option::is_some)) {
                *slot = None;
            }
        }
        self
    }

    /// `player`'s slot, `None` when it is not in the game.
    #[must_use]
    pub fn get(&self, player: Player) -> Option<PlayerSlot> {
        self.0[player.index()]
    }

    /// Puts `player` in the game with `slot`, or takes it out with `None`.
    pub fn set(&mut self, player: Player, slot: Option<PlayerSlot>) {
        self.0[player.index()] = slot;
    }

    /// Whether `player` is in the game.
    #[must_use]
    pub fn in_game(&self, player: Player) -> bool {
        self.0[player.index()].is_some()
    }

    /// The players in the game, lowest first.
    pub fn players(&self) -> impl Iterator<Item = Player> + '_ {
        Player::ALL.into_iter().filter(|&player| self.in_game(player))
    }

    /// Each player's nation as the game start reads it from the face (`0x16f7c`), 0 for a
    /// player not in the game or a face past `portraits`.
    #[must_use]
    pub fn nations(&self, portraits: &Portraits) -> [u8; Player::COUNT] {
        self.0.map(|slot| {
            slot.and_then(|slot| portraits.get(slot.portrait))
                .map_or(0, |row| row.nation)
        })
    }

    /// Marks `player` beaten, as `0x29aba` does when its last finished building goes.
    pub(crate) fn beat(&mut self, player: Player) {
        if let Some(slot) = self.0[player.index()].as_mut() {
            slot.controller = Controller::Beaten;
        }
    }
}

#[cfg(test)]
mod tests;
