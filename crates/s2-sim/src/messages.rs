//! The messages a player is sent.
//!
//! Each player keeps one list. A new message goes on the end, the oldest comes off once the list
//! is full, and the one kind that would otherwise stack up, a building under attack, is dropped
//! when the player already has one for that node.

use crate::grid::Node;
use crate::world::{BuildingKind, Player, World};

/// How many messages a player's list holds (`0x21891`).
pub const MOST_MESSAGES: usize = 20;

/// How many ids the type table covers; every id from here up reads as zero.
pub const MESSAGE_IDS: usize = 25;

/// One kind of message, by the id `S2.EXE` posts it under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct MessageId(pub u8);

impl MessageId {
    /// A geologist has found something.
    pub const GEOLOGIST: Self = Self(1);
    /// A building type may now be built, posted by a mission event.
    pub const BUILDING_ALLOWED: Self = Self(7);
    /// The game has not been written for half an hour.
    pub const UNSAVED_HALF_HOUR: Self = Self(4);
    /// The game has not been written for an hour.
    pub const UNSAVED_HOUR: Self = Self(5);
    /// A mine has found nothing and has given up.
    pub const MINE_DRY: Self = Self(11);
    /// A well has found nothing.
    pub const WELL_DRY: Self = Self(12);
    /// A building has been lost to its owner.
    pub const BUILDING_LOST: Self = Self(15);
    /// A building has been taken.
    pub const BUILDING_TAKEN: Self = Self(16);
    /// Land has been lost to a third player.
    pub const LAND_LOST: Self = Self(22);
    /// A catapult's stone has landed on a building.
    pub const CATAPULT_HIT: Self = Self(23);
    /// A building is under attack, the one kind that is dropped rather than stacked.
    pub const UNDER_ATTACK: Self = Self(24);

    /// The id's number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// What the type table holds for one id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageType {
    /// Which of the four lists the message also goes on, 0 to 3.
    pub category: u8,
    /// The effect to play. `0x23b0e` is silent for 0 and for anything above 70, which leaves the
    /// one 255 entry silent as well.
    pub effect: u16,
}

/// The type table (`0x1f9698`, three bytes an id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageTypes([MessageType; MESSAGE_IDS]);

impl MessageTypes {
    /// The table as `S2.EXE` holds it.
    pub const ORIGINAL: Self = Self([
        MessageType {
            category: 0,
            effect: 60,
        },
        MessageType {
            category: 1,
            effect: 64,
        },
        MessageType {
            category: 0,
            effect: 64,
        },
        MessageType {
            category: 2,
            effect: 64,
        },
        MessageType {
            category: 3,
            effect: 64,
        },
        MessageType {
            category: 3,
            effect: 64,
        },
        MessageType {
            category: 3,
            effect: 64,
        },
        MessageType {
            category: 3,
            effect: 255,
        },
        MessageType {
            category: 2,
            effect: 64,
        },
        MessageType {
            category: 2,
            effect: 64,
        },
        MessageType {
            category: 3,
            effect: 64,
        },
        MessageType {
            category: 3,
            effect: 64,
        },
        MessageType {
            category: 3,
            effect: 64,
        },
        MessageType {
            category: 3,
            effect: 64,
        },
        MessageType {
            category: 3,
            effect: 64,
        },
        MessageType {
            category: 0,
            effect: 64,
        },
        MessageType {
            category: 0,
            effect: 60,
        },
        MessageType {
            category: 3,
            effect: 64,
        },
        MessageType {
            category: 3,
            effect: 64,
        },
        MessageType {
            category: 3,
            effect: 64,
        },
        MessageType {
            category: 3,
            effect: 64,
        },
        MessageType {
            category: 0,
            effect: 64,
        },
        MessageType {
            category: 0,
            effect: 64,
        },
        MessageType {
            category: 0,
            effect: 64,
        },
        MessageType {
            category: 0,
            effect: 64,
        },
    ]);

    /// A custom table: `types[id]` is what id `id` posts as.
    #[must_use]
    pub const fn new(types: [MessageType; MESSAGE_IDS]) -> Self {
        Self(types)
    }

    /// What the table holds for `id`, which is zero for an id the table does not cover.
    #[must_use]
    pub fn get(&self, id: MessageId) -> MessageType {
        self.0.get(usize::from(id.get())).copied().unwrap_or(MessageType {
            category: 0,
            effect: 0,
        })
    }
}

/// One message in a player's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Message {
    /// Which kind it is.
    pub id: MessageId,
    /// Whose message it is.
    pub owner: Player,
    /// The step it was made on (`+0x03`).
    pub step: u32,
    /// Its place in the order the player's messages were made (`+0x07`).
    pub serial: u32,
    /// Where it happened, for a kind that has a place (`+0x0b`, `+0x0d`).
    pub node: Option<Node>,
    /// What it is about, for a kind that names a building (`+0x12`).
    pub building: Option<BuildingKind>,
    /// The value the kind carries (`+0x14`), which is the sign's kind for a geologist's find.
    pub value: u32,
}

impl Message {
    /// Whether the post office gives it the button that opens the save screen, which `0x21891`
    /// marks with 1 in the record's `+0x0f` for the two reminders alone.
    #[must_use]
    pub const fn asks_to_save(&self) -> bool {
        matches!(self.id, MessageId::UNSAVED_HALF_HOUR | MessageId::UNSAVED_HOUR)
    }
}

/// A player's messages, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct Messages {
    list: Vec<Message>,
    serial: u32,
    posted: bool,
    dropped: bool,
}

impl Messages {
    /// The messages, oldest first.
    #[must_use]
    pub fn list(&self) -> &[Message] {
        &self.list
    }

    /// Whether a message has arrived since the list was last looked at (`+0x7e`). What clears it
    /// in `S2.EXE` has not been read out, so nothing clears it here.
    #[must_use]
    pub const fn posted(&self) -> bool {
        self.posted
    }

    /// How many messages have been posted to the player, ever.
    #[must_use]
    pub const fn posts(&self) -> u32 {
        self.serial
    }

    /// Whether a full list has dropped its oldest message (`+0x80`).
    #[must_use]
    pub const fn dropped(&self) -> bool {
        self.dropped
    }

    /// Takes the message with `serial` off the list and says whether one went.
    fn forget(&mut self, serial: u32) -> bool {
        let Some(place) = self.list.iter().position(|message| message.serial == serial) else {
            return false;
        };
        self.list.remove(place);
        true
    }

    /// Adds `message`, dropping the oldest when the list is full.
    fn push(&mut self, message: Message) {
        if self.list.len() >= MOST_MESSAGES {
            self.list.remove(0);
            self.dropped = true;
        }
        self.list.push(message);
        self.posted = true;
        self.serial = self.serial.wrapping_add(1);
    }
}

impl World {
    /// `player`'s messages, oldest first.
    #[must_use]
    pub fn messages(&self, player: Player) -> &Messages {
        &self.messages[player.index()]
    }

    /// Takes `player`'s message with `serial` off the list and says whether one went.
    pub(crate) fn forget_message(&mut self, player: Player, serial: u32) -> bool {
        self.messages[player.index()].forget(serial)
    }

    /// Posts a message of `id` to `owner` (`0x21891`). An under-attack message for a node the
    /// player already has one for is dropped.
    pub(crate) fn post_message(
        &mut self,
        id: MessageId,
        owner: Player,
        node: Option<Node>,
        building: Option<BuildingKind>,
        value: u32,
        step: u32,
    ) {
        let messages = &mut self.messages[owner.index()];
        if id == MessageId::UNDER_ATTACK
            && messages
                .list
                .iter()
                .any(|held| held.id == id && held.node == node)
        {
            return;
        }
        let serial = messages.serial;
        messages.push(Message {
            id,
            owner,
            step,
            serial,
            node,
            building,
            value,
        });
    }
}

impl crate::game::Game {
    /// Posts a message of `id` to `owner`, stamped with the step the game is on.
    pub(crate) fn post_message(
        &mut self,
        id: MessageId,
        owner: Player,
        node: Option<Node>,
        building: Option<BuildingKind>,
        value: u32,
    ) {
        let step = self.clock().step();
        self.world.post_message(id, owner, node, building, value, step);
    }
}

#[cfg(test)]
mod tests;
