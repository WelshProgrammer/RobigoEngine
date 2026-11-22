//! The game objective Unlimited Play's settings choose, and the pass that decides the game
//! (`0x29f50`). The World Campaign's own check (`0x2a1a0`) is one more objective.

use crate::game::Game;
use crate::world::{BuildingKind, Player};

/// What wins a game: one of Unlimited Play's settings (the option word at `0x1f78e6`), or the
/// World Campaign's rule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Objective {
    /// Nothing: the game plays on.
    #[default]
    None,
    /// Holding three quarters of the land the players own between them, with allies' land.
    Land,
    /// Being left with no finished building standing outside one's own side.
    Domination,
    /// The World Campaign's: being the only player in the game with a finished headquarters,
    /// allies or not. Only a person's player can win it.
    Headquarters,
}

/// Where a game stands on its objective: the setting, and the player it was decided for.
/// `S2.EXE` keeps only a word saying it was decided (`0x1f78e8`) and hands the winner to the
/// window it opens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Goal {
    /// What wins the game.
    pub objective: Objective,
    /// The player the game was decided for, once it has been.
    pub winner: Option<Player>,
}

impl Goal {
    /// The share of the land, in percent, that wins [`Objective::Land`] (`0x2a018`).
    pub const LAND_SHARE: u32 = 75;
    /// What a share is counted out of.
    const PERCENT: u32 = 100;
}

impl Game {
    /// The player the game was decided for, once the objective pass has found one.
    #[must_use]
    pub fn winner(&self) -> Option<Player> {
        self.goal.winner
    }

    /// The game's objective.
    #[must_use]
    pub fn objective(&self) -> Objective {
        self.goal.objective
    }

    /// The objective pass (`0x29f50`). Once a player is found it is the winner and the pass
    /// never looks again. Players count while their slot is in the game, beaten or not, and the
    /// lowest player that meets the objective wins.
    ///
    /// For [`Objective::Land`] a player's share is its own node count and every ally's, out of
    /// the counts of every player in the game added up; `share · 100 / total` of
    /// [`Goal::LAND_SHARE`] or more wins. A game where nobody owns land has no total, which
    /// `S2.EXE` divides by anyway; here nobody wins. For [`Objective::Domination`] a player wins
    /// when the finished buildings of every other player it does not count as an ally add up to
    /// 0, summed in 16 bits as `S2.EXE` does.
    ///
    /// For [`Objective::Headquarters`] a player wins when the finished headquarters of every
    /// other player in the game add up to 0, allies included (`0x2a1a0`). `S2.EXE` lets any
    /// player win it and runs it again after a win; here only a player a person controls wins,
    /// and the pass stops once the game is decided, as for the other objectives.
    pub(crate) fn decide(&mut self) {
        if self.goal.winner.is_some() {
            return;
        }
        let playing: Vec<Player> = self.world.slots().players().collect();
        let others = |player: Player| playing.iter().copied().filter(move |&other| other != player);
        self.goal.winner = match self.goal.objective {
            Objective::None => None,
            Objective::Land => {
                let land = |player: Player| self.nodes.owned(player);
                let total = playing
                    .iter()
                    .fold(0u32, |sum, &player| sum.wrapping_add(land(player)));
                if total == 0 {
                    return;
                }
                playing.iter().copied().find(|&player| {
                    let share = others(player)
                        .filter(|&ally| self.world.allied(player, ally))
                        .fold(land(player), |sum, ally| sum.wrapping_add(land(ally)));
                    share.wrapping_mul(Goal::PERCENT) / total >= Goal::LAND_SHARE
                })
            }
            Objective::Domination => playing.iter().copied().find(|&player| {
                others(player)
                    .filter(|&enemy| !self.world.allied(player, enemy))
                    .fold(0u16, |sum, enemy| {
                        sum.wrapping_add(self.world.all_finished_buildings(enemy))
                    })
                    == 0
            }),
            Objective::Headquarters => playing
                .iter()
                .copied()
                .filter(|&player| {
                    self.world
                        .slots()
                        .get(player)
                        .is_some_and(|slot| slot.controller.is_person())
                })
                .find(|&player| {
                    others(player).fold(0u16, |sum, other| {
                        sum.wrapping_add(self.world.finished_buildings(other, BuildingKind::HEADQUARTERS))
                    }) == 0
                }),
        };
    }
}

#[cfg(test)]
mod tests;
