//! The seven player records as Unlimited Play's settings screen keeps them, in the game or not,
//! and the rules its buttons follow.

use crate::world::Player;

use super::{Controller, PlayerSlot, PlayerSlots, Portrait, Portraits};

/// One record: whether it is in the game, who plays it and its face. A record taken out of the
/// game keeps its face and controller for when it comes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RosterEntry {
    /// Whether the player is in the game.
    pub in_game: bool,
    /// Who plays it.
    pub controller: Controller,
    /// Its face.
    pub portrait: Portrait,
}

/// Every player's record on the settings screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roster {
    entries: [RosterEntry; Player::COUNT],
    faces: usize,
}

impl Roster {
    /// What the screen shows for a map offering `count` players: the header load (`0x1574c`),
    /// which gives player `n` face `n` and puts the first `count` in the game, the first played
    /// by the person at the machine and the rest by the computer, then the walk that moves each
    /// face past the ones already taken (`0x612a2`).
    #[must_use]
    pub fn offered(count: usize, portraits: &Portraits) -> Self {
        let offered = PlayerSlots::offered(count, portraits);
        let entries = Player::ALL.map(|player| {
            offered.get(player).map_or(
                RosterEntry {
                    in_game: false,
                    controller: Controller::Computer,
                    portrait: Portrait(u8::try_from(player.index()).unwrap_or_default()),
                },
                |slot| RosterEntry {
                    in_game: true,
                    controller: slot.controller,
                    portrait: slot.portrait,
                },
            )
        });
        Self {
            entries,
            faces: portraits.len().max(1),
        }
    }

    /// `player`'s record.
    #[must_use]
    pub const fn entry(&self, player: Player) -> RosterEntry {
        self.entries[player.index()]
    }

    /// Puts `player` in the game or takes it out (`0x2d96` + player). A player coming in moves
    /// on to the next free face, its own counted as taken (`0x607b8`). Then a player the computer does not play goes to the
    /// computer when anyone in the game has its controller, which a player just brought in
    /// always finds in itself.
    pub fn toggle(&mut self, player: Player) {
        let index = player.index();
        self.entries[index].in_game = !self.entries[index].in_game;
        if self.entries[index].in_game {
            self.next_face(player);
        }
        let controller = self.entries[index].controller;
        if controller != Controller::Computer && self.held(controller) {
            self.entries[index].controller = Controller::Computer;
        }
    }

    /// Gives `player` the next face (`0x2da0` + player): one up, wrapping after the last, then
    /// the first from there that nobody in the game has.
    pub fn next_face(&mut self, player: Player) {
        let index = player.index();
        let up = (self.entries[index].portrait.index() + 1) % self.faces;
        let from = Portrait(u8::try_from(up).unwrap_or_default());
        self.entries[index].portrait = self.first_free(from);
    }

    /// Moves `player` to the next controller (`0x2daa` + player). The first person becomes the
    /// second when nobody in the game is the second and the computer otherwise; the second
    /// becomes the computer; the computer becomes the first person when nobody is, else the
    /// second when nobody is, else stays; a beaten player becomes the computer.
    pub fn next_controller(&mut self, player: Player) {
        let index = player.index();
        let first_free = !self.held(Controller::First);
        let second_free = !self.held(Controller::Second);
        let next = match self.entries[index].controller {
            Controller::Computer if first_free => Controller::First,
            Controller::First | Controller::Computer if second_free => Controller::Second,
            _ => Controller::Computer,
        };
        self.entries[index].controller = next;
    }

    /// Whether the game can start (`0x60626` with 0): somebody is in the game and a person
    /// plays one of them.
    #[must_use]
    pub fn can_start(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.in_game && entry.controller.is_person())
    }

    /// The slots the game starts with (`0x5ea27`). With no first person, every second person in
    /// the game becomes the first (`0x60626` with 1). Then the second person's record swaps with
    /// player 1's and the first person's with player 0's (`0x6068c`), so the person at the
    /// machine always starts at position 0.
    #[must_use]
    pub fn slots(&self) -> PlayerSlots {
        let mut entries = self.entries;
        if !entries
            .iter()
            .any(|entry| entry.in_game && entry.controller == Controller::First)
        {
            for entry in entries.iter_mut().filter(|entry| entry.in_game) {
                if entry.controller == Controller::Second {
                    entry.controller = Controller::First;
                }
            }
        }
        for (controller, to) in [(Controller::Second, 1), (Controller::First, 0)] {
            if let Some(from) = entries
                .iter()
                .position(|entry| entry.in_game && entry.controller == controller)
            {
                entries.swap(from, to);
            }
        }
        let mut slots = PlayerSlots::EMPTY;
        for (player, entry) in Player::ALL.into_iter().zip(entries) {
            if entry.in_game {
                slots.set(
                    player,
                    Some(PlayerSlot {
                        controller: entry.controller,
                        portrait: entry.portrait,
                    }),
                );
            }
        }
        slots
    }

    /// Whether anyone in the game has `controller` (`0x6077e`).
    fn held(&self, controller: Controller) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.in_game && entry.controller == controller)
    }

    /// The first face from `from` up, wrapping, that nobody in the game has.
    fn first_free(&self, from: Portrait) -> Portrait {
        (0..self.faces)
            .filter_map(|step| u8::try_from((from.index() + step) % self.faces).ok())
            .map(Portrait)
            .find(|&face| {
                self.entries
                    .iter()
                    .all(|entry| !entry.in_game || entry.portrait != face)
            })
            .unwrap_or(from)
    }
}
