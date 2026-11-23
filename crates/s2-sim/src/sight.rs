//! What players 0 and 1 have seen of the map: the seen bits of slot 9 and the reveal that sets
//! them. Players 2 and up have no fog: they see everything.

use bitflags::bitflags;

pub use crate::grid::reveal_walk;

use crate::grid::{Direction, Node};
use crate::nodes::NodeMap;
use crate::world::Player;

bitflags! {
    /// A node's seen bits (slot 9 bits 4–7): which of its two triangles players 0 and 1 have
    /// seen. The first triangle is node, south-east, south-west; the second node, east,
    /// south-east.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
    pub struct Seen: u8 {
        /// Player 1 has seen the second triangle.
        const PLAYER_1_SECOND = 1 << 4;
        /// Player 1 has seen the first triangle.
        const PLAYER_1_FIRST = 1 << 5;
        /// Player 0 has seen the second triangle.
        const PLAYER_0_SECOND = 1 << 6;
        /// Player 0 has seen the first triangle.
        const PLAYER_0_FIRST = 1 << 7;
    }
}

impl Seen {
    /// `player`'s bits for the first and second triangle, or `None` for a player with no fog.
    #[must_use]
    pub const fn of(player: Player) -> Option<[Self; 2]> {
        match player.index() {
            0 => Some([Self::PLAYER_0_FIRST, Self::PLAYER_0_SECOND]),
            1 => Some([Self::PLAYER_1_FIRST, Self::PLAYER_1_SECOND]),
            _ => None,
        }
    }
}

impl NodeMap {
    /// Which of `node`'s two triangles `player` has seen. A player with no fog has seen both.
    #[must_use]
    pub fn seen(&self, node: Node, player: Player) -> [bool; 2] {
        let seen = self.get(node).seen;
        Seen::of(player).map_or([true; 2], |bits| bits.map(|bit| seen.contains(bit)))
    }

    /// Whether `player` has seen both of `node`'s triangles (`0x3f8a8` gives 63).
    #[must_use]
    pub fn fully_seen(&self, node: Node, player: Player) -> bool {
        self.seen(node, player) == [true; 2]
    }

    /// Whether anything on `node` shows to `player` (`0x3f8a8` bit 5): it has seen one of the
    /// node's triangles, the north-east neighbour's first or the west neighbour's second.
    #[must_use]
    pub fn visible(&self, node: Node, player: Player) -> bool {
        let (width, height) = (self.width(), self.height());
        self.seen(node, player).contains(&true)
            || self.seen(node.neighbour(Direction::NorthEast, width, height), player)[0]
            || self.seen(node.neighbour(Direction::West, width, height), player)[1]
    }

    /// Marks every triangle within `radius` rings of `centre` as seen by `player` (`0x3fe8c`);
    /// nothing for a player with no fog. Nothing ever unmarks one.
    pub fn reveal(&mut self, centre: Node, player: Player, radius: u16) {
        let Some(bits) = Seen::of(player) else {
            return;
        };
        let size = (self.width(), self.height());
        reveal_walk(size, centre, radius, |node, triangles| {
            let mut data = self.get(node);
            for (bit, on) in bits.into_iter().zip(triangles) {
                if on {
                    data.seen |= bit;
                }
            }
            self.set(node, data);
        });
        if player == Player::ALL[0] && self.keeps_contacts() {
            crate::grid::reveal_ring(size, centre, radius, |node| self.note_revealed(node));
        }
    }
}

#[cfg(test)]
mod tests;
