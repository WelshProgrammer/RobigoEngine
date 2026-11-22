//! What ships carry: unloading at a harbour, choosing the harbour to sail to next and loading
//! for it.

use std::borrow::Cow;

use crate::game::Game;
use crate::job::Job;
use crate::world::{StockSetting, Storehouse, Taken, WARE_TYPES};

/// Most wares and people one voyage carries (`0x42dad`).
const HOLD: u16 = 40;

/// What a harbour with an expedition ready scores (`0x42a96`).
const EXPEDITION_SCORE: u32 = 1000;

/// How much each ware and each job counts when ships weigh the work between two harbours
/// (`0x1fbd60` and `0x1fbd7f`, one byte each).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoWeights {
    wares: Cow<'static, [u8; WARE_TYPES]>,
    people: Cow<'static, [u8; Job::PEOPLE]>,
}

impl CargoWeights {
    /// The weights of `S2.EXE`.
    pub const ORIGINAL: Self = Self {
        wares: Cow::Borrowed(&[
            1, 5, 5, 3, 2, 2, 3, 3, 3, 2, 30, 5, 5, 20, 10, 40, 70, 70, 70, 70, 70, 70, 70, 70, 70, 70, 70,
            70, 30, 30, 1,
        ]),
        people: Cow::Borrowed(&[
            1, 3, 2, 3, 4, 3, 2, 3, 4, 4, 4, 10, 10, 3, 2, 8, 20, 2, 15, 10, 10, 3, 50, 50, 50, 50, 50, 10,
            2, 1,
        ]),
    };

    /// Custom weights, by ware type and by job.
    #[must_use]
    pub fn new(wares: [u8; WARE_TYPES], people: [u8; Job::PEOPLE]) -> Self {
        Self {
            wares: Cow::Owned(wares),
            people: Cow::Owned(people),
        }
    }

    /// Each ware type's weight, by type.
    #[must_use]
    pub fn wares(&self) -> &[u8; WARE_TYPES] {
        &self.wares
    }

    /// Each job's weight, by job.
    #[must_use]
    pub fn people(&self) -> &[u8; Job::PEOPLE] {
        &self.people
    }
}

impl Default for CargoWeights {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

impl std::hash::Hash for CargoWeights {
    /// The weights are input data, checked by the data hash a replay carries; the state hash
    /// leaves them out.
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {}
}

/// The low 16 bits of `value`.
fn low_word(value: u32) -> u16 {
    let [low, high, ..] = value.to_le_bytes();
    u16::from_le_bytes([low, high])
}

/// How many of job `job` storehouse `store` can spare for a demand of `wanted` (`0x31677`):
/// all of them when it holds that many. Otherwise, when it can supply the job and holds none of
/// the job it settles on, a helper takes that job's tool and becomes one. The count of the job
/// it settled on is the answer, which for a soldier can be another rank's.
fn spare(store: &mut Storehouse, job: Job, wanted: u16) -> (u16, Option<(Job, Taken)>) {
    if wanted <= store.jobs[job.index()] {
        return (wanted, None);
    }
    let settled = store.supplies(job).unwrap_or(job);
    let mut converted = None;
    if store.supplies(job).is_some() && store.jobs[settled.index()] == 0 {
        converted = Some((settled, store.take_person(settled)));
        store.jobs[settled.index()] += 1;
    }
    (store.jobs[settled.index()], converted)
}

/// `count`, less what a harbour wanting `kept` keeps for itself (`0x42dad`): cut so that `kept`
/// stays of `stock`, or to 0 when the stock is no more than that.
fn keeping(count: u16, stock: u16, kept: u16) -> u16 {
    if i32::from(stock) - i32::from(count) >= i32::from(kept) {
        count
    } else {
        stock.saturating_sub(kept)
    }
}

impl Game {
    /// The storehouse record of harbour record `harbour`, to change (`0x42710`).
    fn harbour_store(&mut self, harbour: u16) -> Option<&mut Storehouse> {
        let building = self.world.harbours.get(harbour)?.building;
        let (store, _) = self.world.storehouse_of(building)?;
        self.world.storehouse_mut(store)
    }

    /// Ship `ship` puts everything aboard into harbour `harbour`'s stock (`0x406e7`), people as
    /// their own jobs.
    pub(crate) fn unload(&mut self, ship: u16, harbour: u16) {
        let Some(cargo) = self.world.ships.get_mut(ship) else {
            return;
        };
        let (wares, people) = (cargo.wares, cargo.people);
        cargo.wares = [0; WARE_TYPES];
        cargo.people = [0; Job::PEOPLE];
        let Some(store) = self.harbour_store(harbour) else {
            return;
        };
        for (stock, count) in store.wares.iter_mut().zip(wares) {
            *stock = stock.wrapping_add(count);
        }
        for (stock, count) in store.jobs.iter_mut().zip(people) {
            *stock = stock.wrapping_add(count);
        }
    }

    /// Whether harbour `record` has its expedition ready (`0x42a54`): being prepared, with the
    /// harbour type's boards and stones and a builder collected.
    pub(crate) fn expedition_ready(&self, record: u16) -> bool {
        let Some(harbour) = self.world.harbours.get(record) else {
            return false;
        };
        let Some(row) = self
            .world
            .building(harbour.building)
            .and_then(|building| self.world.building_types().get(building.kind))
        else {
            return false;
        };
        harbour.expedition
            && harbour.boards >= row.cost(0)
            && harbour.stones >= row.cost(1)
            && harbour.builders != 0
    }

    /// The harbour a ship of harbour `home`'s owner on sea `sea` should sail to (`0x42a96`).
    /// Each pair of that owner's harbours on the sea is scored by what each could send the
    /// other, weighted by type, plus how long the other has waited when that is not 0, and 1,000
    /// for an expedition ready. A harbour with an expedition ready scores 1,000 alone. The ship
    /// sails to the first harbour of the best pair, or the other one when the first is `home`.
    /// Scoring can turn helpers into the jobs wanted.
    pub(crate) fn choose_harbour(&mut self, home: u16, sea: u8) -> Option<u16> {
        let owner = self.world.harbours.get(home)?.owner;
        let on_sea: Vec<u16> = self
            .world
            .harbours()
            .filter(|(_, harbour)| harbour.owner == owner && harbour.touches(sea))
            .map(|(record, _)| record)
            .collect();
        let weights = self.world.cargo_weights.clone();
        let (mut best, mut result) = (0_u32, None);
        for &first in &on_sea {
            if self.expedition_ready(first) && best < EXPEDITION_SCORE {
                (best, result) = (EXPEDITION_SCORE, Some(first));
            }
            for &second in &on_sea {
                if second == first {
                    continue;
                }
                let mut score = self.pair_score(first, second, &weights);
                if score != 0 {
                    score += self
                        .world
                        .harbours
                        .get(second)
                        .map_or(0, |h| u32::from(h.waiting));
                }
                if self.expedition_ready(second) {
                    score += EXPEDITION_SCORE;
                }
                if score > best {
                    best = score;
                    result = Some(if first == home { second } else { first });
                }
            }
        }
        result
    }

    /// What harbours `first` and `second` could send each other, weighted (`0x42a96`): the
    /// wares each wants that the other holds, then the people, `first`'s stock first.
    fn pair_score(&mut self, first: u16, second: u16, weights: &CargoWeights) -> u32 {
        let (Some(a), Some(b)) = (
            self.world.harbours.get(first).copied(),
            self.world.harbours.get(second).copied(),
        ) else {
            return 0;
        };
        let stock = |game: &Self, building: u16| {
            game.world
                .storehouse_of(building)
                .map(|(_, store)| store.wares)
                .unwrap_or_default()
        };
        let (from_a, from_b) = (stock(self, a.building), stock(self, b.building));
        let mut score = 0_u32;
        for ware in 0..WARE_TYPES {
            let weight = u32::from(weights.wares[ware]);
            score += weight * u32::from(b.wares_wanted[ware].min(from_a[ware]));
            score += weight * u32::from(a.wares_wanted[ware].min(from_b[ware]));
        }
        let mut converted = Vec::new();
        for (index, &weight) in weights.people.iter().enumerate() {
            let job = Job(u8::try_from(index).unwrap_or(u8::MAX));
            let weight = u32::from(weight);
            let (a_spares, a_taken) = self
                .harbour_store(first)
                .map_or((0, None), |store| spare(store, job, b.people_wanted[index]));
            score += weight * u32::from(a_spares);
            converted.extend(a_taken.map(|taken| (a.building, taken)));
            let (b_spares, b_taken) = self
                .harbour_store(second)
                .map_or((0, None), |store| spare(store, job, a.people_wanted[index]));
            score += weight * u32::from(b_spares);
            converted.extend(b_taken.map(|taken| (b.building, taken)));
        }
        for (building, (job, taken)) in converted {
            self.world.count_conversion(building, job, taken);
        }
        score
    }

    /// Ship `ship` loads at harbour `from` for harbour `to` (`0x42dad`), and neither has waited
    /// since. Each ware goes in its share of `to`'s demand, less what `from` wants to keep, then
    /// each job as many as `from` can spare for `to`; people are never split. A voyage takes 40
    /// at most.
    pub(crate) fn load(&mut self, ship: u16, from: u16, to: u16) {
        for record in [from, to] {
            if let Some(harbour) = self.world.harbours.get_mut(record) {
                harbour.waiting = 0;
            }
        }
        let (Some(home), Some(away)) = (
            self.world.harbours.get(from).copied(),
            self.world.harbours.get(to).copied(),
        ) else {
            return;
        };
        let shares: Vec<u16> = (0..WARE_TYPES)
            .map(|ware| {
                let stock = self.harbour_store(from).map_or(0, |store| store.wares[ware]);
                self.ware_share(to, ware, stock)
            })
            .collect();
        let mut total = 0_u16;
        let mut wares = [0_u16; WARE_TYPES];
        let mut people = [0_u16; Job::PEOPLE];
        let mut converted = Vec::new();
        let Some(store) = self.harbour_store(from) else {
            return;
        };
        for (ware, &share) in shares.iter().enumerate() {
            let mut count = keeping(share, store.wares[ware], home.wares_wanted[ware]);
            if total + count > HOLD {
                count = HOLD - total;
            }
            wares[ware] = count;
            store.wares[ware] -= count;
            total += count;
        }
        for (index, aboard) in people.iter_mut().enumerate() {
            let job = Job(u8::try_from(index).unwrap_or(u8::MAX));
            let (count, taken) = spare(store, job, away.people_wanted[index]);
            converted.extend(taken.map(|taken| (home.building, taken)));
            let count = keeping(count, store.jobs[index], home.people_wanted[index]);
            if count != 0 && total + count <= HOLD {
                *aboard = count;
                store.jobs[index] -= count;
                total += count;
            }
        }
        for (building, (job, taken)) in converted {
            self.world.count_conversion(building, job, taken);
        }
        if let Some(cargo) = self.world.ships.get_mut(ship) {
            for (held, count) in cargo.wares.iter_mut().zip(wares) {
                *held = held.wrapping_add(count);
            }
            for (held, count) in cargo.people.iter_mut().zip(people) {
                *held = held.wrapping_add(count);
            }
        }
    }

    /// How many of `stock` wares of type `ware` go to harbour `to` (`0x42cf4`): none when its
    /// storehouse stops the ware, all when it sends the ware out, otherwise its share of the
    /// demand of all its owner's harbours for the ware. The product is cut to 16 bits before
    /// the division, and the demand summed in 16 bits.
    fn ware_share(&self, to: u16, ware: usize, stock: u16) -> u16 {
        let Some(harbour) = self.world.harbours.get(to) else {
            return 0;
        };
        let setting = self
            .world
            .storehouse_of(harbour.building)
            .and_then(|(_, store)| store.ware_settings.get(ware).copied())
            .unwrap_or_default();
        if setting.contains(StockSetting::STOP) {
            return 0;
        }
        if setting.contains(StockSetting::SEND) {
            return stock;
        }
        let demand = self
            .world
            .harbours()
            .filter(|(_, other)| other.owner == harbour.owner)
            .fold(0_u16, |sum, (_, other)| {
                sum.wrapping_add(other.wares_wanted[ware])
            });
        if demand == 0 {
            return 0;
        }
        low_word(u32::from(harbour.wares_wanted[ware]) * u32::from(stock)) / demand
    }
}
