//! Task 13 attacks (`0x711b9`): every so many calls it rates the enemy military buildings near the
//! border and attacks the most valuable it can beat, and every so many more it tries each player's
//! headquarters in turn.

use super::ComputerPlayer;
use super::tasks::Think;
use crate::grid::Node;
use crate::world::{BuildingKind, BuildingState, MapObject, Player};

/// The base of a target's value (`0x7137a`).
const VALUE_BASE: u32 = 10_000;

/// The rings round a target summed for its value (`0x7136d`).
const VALUE_RINGS: u16 = 6;

/// The headquarters tries count players round to this, one more than there are, so one turn in
/// eight finds nobody (`0x7151b`).
const ROUND: u16 = 8;

/// Task 13's fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Attacks {
    /// Calls since the last rating of targets (`+0x06`).
    pub calls: u16,
    /// The military record rated last, `None` when there was none (`+0x08`).
    pub cursor: Option<u16>,
    /// Calls since the last try at a headquarters (`+0x0a`).
    pub headquarters_calls: u16,
    /// The player whose headquarters was tried last (`+0x0c`).
    pub target: u16,
}

impl Think<'_> {
    /// The next military record after `cursor` whose owner is not the thinking player, round to
    /// the start, `cursor` itself last (`0x688a2`). `None` starts at record 0.
    fn next_enemy_military(&self, cursor: Option<u16>) -> Option<u16> {
        let start = cursor.map_or(0, |at| at.wrapping_add(1));
        let records: Vec<u16> = self
            .game
            .world
            .military()
            .filter(|(_, military)| military.owner != self.player)
            .map(|(record, _)| record)
            .collect();
        records
            .iter()
            .copied()
            .find(|&record| record >= start)
            .or_else(|| records.first().copied())
    }

    /// The buildings on rings 1 to 6 round `node` on nodes the player does not own, each worth
    /// the profile's value for its type, half that unfinished (`0x68c76`).
    fn target_value(&self, node: Node) -> u16 {
        let (width, height) = (self.game.nodes.width(), self.game.nodes.height());
        let values = self.profile.attacks.values;
        let mut sum: u32 = 0;
        for at in node.rings(VALUE_RINGS, width, height).skip(1) {
            let MapObject::Building(record) = self.game.world.object(at) else {
                continue;
            };
            if self.game.nodes.get(at).owner == Some(self.player) {
                continue;
            }
            let Some(building) = self.game.world.building(record) else {
                continue;
            };
            let worth = match building.kind {
                BuildingKind::CATAPULT => values.catapult,
                BuildingKind::GOLD_MINE => values.gold_mine,
                BuildingKind::MINT => values.mint,
                BuildingKind::IRON_MINE => values.iron_mine,
                BuildingKind::COAL_MINE => values.coal_mine,
                BuildingKind::ARMOURY => values.armoury,
                BuildingKind::METALWORKS => values.metalworks,
                BuildingKind::IRON_SMELTER => values.smelter,
                BuildingKind::STOREHOUSE | BuildingKind::HARBOUR => values.storehouse,
                BuildingKind::HEADQUARTERS => values.headquarters,
                _ => values.other,
            };
            let worth = if building.state.contains(BuildingState::FINISHED) {
                worth
            } else {
                worth >> 1
            };
            sum = sum.wrapping_add(u32::from(worth));
        }
        u16::try_from(sum & u32::from(u16::MAX)).unwrap_or_default()
    }
}

impl ComputerPlayer {
    /// Task 13's call (`0x711b9`): the targets, then the headquarters.
    pub(super) fn attack(&mut self, think: &mut Think<'_>) {
        self.attack_targets(think);
        self.attack_headquarters(think);
    }

    /// The targets (`0x711da`), every profile's number of calls: up to the profile's number of
    /// enemy military records from the cursor on, each on the border, occupied, not an ally's,
    /// past a draw, with enough of the player's soldiers in reach and no more defenders. The most
    /// valuable is attacked with a share of the soldiers over its defenders picked by a draw.
    fn attack_targets(&mut self, think: &mut Think<'_>) {
        let numbers = think.profile.attacks;
        let attacks = &mut self.tasks.attacks;
        attacks.calls += 1;
        if attacks.calls < numbers.every {
            return;
        }
        attacks.calls = 0;
        let mut best: Option<(u16, i16)> = None;
        let mut best_value: u32 = 0;
        let mut first = None;
        for _ in 0..numbers.records {
            let Some(record) = think.next_enemy_military(self.tasks.attacks.cursor) else {
                self.tasks.attacks.cursor = None;
                return;
            };
            self.tasks.attacks.cursor = Some(record);
            match first {
                None => first = Some(record),
                Some(first) if first == record => break,
                Some(_) => {}
            }
            let Some(military) = think
                .game
                .world
                .military()
                .find(|&(at, _)| at == record)
                .map(|(_, it)| *it)
            else {
                continue;
            };
            if military.border < numbers.border {
                continue;
            }
            let Some(building) = think.game.world.building(military.building).copied() else {
                continue;
            };
            if !building.state.contains(BuildingState::MANNED) {
                continue;
            }
            let Some(owner) = think.game.world.building_owner(military.building) else {
                continue;
            };
            if owner == think.player || think.game.world.allied(think.player, owner) {
                continue;
            }
            if think.game.rng.up_to(100) > numbers.skip_chance {
                continue;
            }
            let count = think.game.attack_count(think.player, military.building);
            if u16::from(count) < numbers.least {
                continue;
            }
            let defenders = i16::from(military.inside).wrapping_add(defence(building.kind, &numbers));
            if defenders > i16::from(count) {
                continue;
            }
            let mut value = u32::from(think.target_value(building.node)) + VALUE_BASE;
            if building.owner == think.player {
                value += u32::from(numbers.own_bonus);
            }
            if value > best_value {
                best_value = value;
                best = Some((military.building, defenders));
            }
        }
        let Some((target, defenders)) = best else {
            return;
        };
        let count = u16::from(think.game.attack_count(think.player, target)).min(numbers.most);
        let defenders = defenders.max(1);
        let over = count.wrapping_sub(defenders.cast_unsigned());
        let sent = if over == 0 {
            count
        } else {
            let share = u32::from(over) * u32::from(think.game.rng.up_to(100)) / 100;
            u16::try_from(share & u32::from(u16::MAX))
                .unwrap_or_default()
                .wrapping_add(defenders.cast_unsigned())
        };
        let strongest = think.game.rng.up_to(100) < numbers.strongest_chance;
        let wanted = sent.to_le_bytes()[0];
        think.game.launch_attack(think.player, target, wanted, strongest);
    }

    /// The headquarters (`0x714ea`), every profile's number of calls: the next player in turn,
    /// in the game, not an ally, with a headquarters and enough of the player's soldiers in reach,
    /// is attacked with as many as the profile allows.
    fn attack_headquarters(&mut self, think: &mut Think<'_>) {
        let numbers = think.profile.attacks;
        let attacks = &mut self.tasks.attacks;
        attacks.headquarters_calls += 1;
        if attacks.headquarters_calls < numbers.headquarters_every {
            return;
        }
        attacks.headquarters_calls = 0;
        attacks.target = (attacks.target + 1) % ROUND;
        let Some(target) = u8::try_from(attacks.target).ok().and_then(Player::new) else {
            return;
        };
        if target == think.player
            || !think.game.world.slots().in_game(target)
            || think.game.world.allied(think.player, target)
        {
            return;
        }
        let (x, y) = super::economy::headquarters(&think.game.world, &think.game.nodes, target);
        if x < 0 {
            return;
        }
        let node = Node {
            x: x.cast_unsigned(),
            y: y.cast_unsigned(),
        };
        let MapObject::Building(record) = think.game.world.object(node) else {
            return;
        };
        let count = think.game.attack_count(think.player, record);
        if u16::from(count) < numbers.headquarters_least {
            return;
        }
        let wanted = u16::from(count).min(numbers.most).to_le_bytes()[0];
        let strongest = think.game.rng.up_to(100) < numbers.strongest_chance;
        think.game.launch_attack(think.player, record, wanted, strongest);
    }
}

/// What a military building of `kind` adds to its defenders, a signed word (`0x7130f`).
fn defence(kind: BuildingKind, numbers: &super::profiles::Attacking) -> i16 {
    match kind {
        BuildingKind::BARRACKS => numbers.defence[0],
        BuildingKind::GUARDHOUSE => numbers.defence[1],
        BuildingKind::WATCHTOWER => numbers.defence[2],
        BuildingKind::FORTRESS => numbers.defence[3],
        _ => 0,
    }
}

#[cfg(test)]
mod tests;
