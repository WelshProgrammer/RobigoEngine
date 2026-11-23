//! The flag search `0x3b884` that every route in the game comes from.
//!
//! It is a uniform-cost search run in passes of increasing cost over a pool of open nodes. Each
//! pass walks the pool in slot order, and new nodes take the lowest free slot, so which of two
//! equally cheap flags is found first depends on that slot order. A priority queue would break
//! those ties differently.

use bitflags::bitflags;

use crate::marks::RoadKind;
use crate::pool::Pool;
use crate::world::{MAX_FLAGS, Road, World};

bitflags! {
    /// What a search charges for and where it may go (the routine's option bits).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
    pub struct SearchOptions: u8 {
        /// Charge each road its busy counter, or 10 while it has never had a carrier (`0x292f4`).
        const BUSY = 1;
        /// Charge each flag reached `wares − 3` when more than 4 wares wait there (`0x27f84`).
        const LOAD = 2;
        /// Take water roads.
        const WATER = 4;
        /// Never stop at the start flag.
        const SKIP_START = 8;
    }
}

/// Wares a flag may hold before the search charges for them.
const LOAD_FREE: u8 = 4;

/// What [`SearchOptions::LOAD`] takes off the ware count of a crowded flag.
const LOAD_DISCOUNT: u8 = 3;

/// What [`SearchOptions::BUSY`] charges for a road that has never had a carrier.
const UNSERVED_COST: u16 = 10;

/// A flag the search has reached: an entry of the 8-byte node pool at `0x1fa50a`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Reached {
    /// The flag.
    pub flag: u16,
    /// The flag it was reached from; `None` for the start.
    pub from: Option<u16>,
    /// The cheapest cost found to it.
    pub cost: u16,
    fresh: bool,
}

/// A flag's mark during a search (the flag pool's byte array).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    Unseen,
    Open(u16),
    Closed,
}

/// The open list and the marks of one search.
struct Frontier {
    open: Pool<Reached>,
    marks: Vec<Mark>,
}

impl Frontier {
    fn add(&mut self, reached: Reached) {
        if let Some(slot) = self.open.insert(reached, MAX_FLAGS)
            && let Some(mark) = self.marks.get_mut(usize::from(reached.flag))
        {
            *mark = Mark::Open(slot);
        }
    }

    /// Reaches every neighbour of `here` within `limit`, in direction order. Returns whether a new
    /// flag was opened.
    fn expand(&mut self, world: &World, here: Reached, limit: u16, options: SearchOptions) -> bool {
        let mut added = false;
        for link in world.flag(here.flag).map(|flag| flag.links).unwrap_or_default() {
            let Some((link, far)) = link.and_then(|link| Some((link, link.far?))) else {
                continue;
            };
            let mark = self.marks.get(usize::from(far)).copied().unwrap_or(Mark::Closed);
            let Some(road) = world.road(link.road) else {
                continue;
            };
            if Some(far) == here.from
                || mark == Mark::Closed
                || road.kind == RoadKind::Water && !options.contains(SearchOptions::WATER)
            {
                continue;
            }
            let cost = here.cost.saturating_add(step_cost(world, road, far, options));
            if cost > limit {
                continue;
            }
            if let Mark::Open(slot) = mark {
                if let Some(known) = self.open.get_mut(slot)
                    && cost < known.cost
                {
                    known.from = Some(here.flag);
                    known.cost = cost;
                }
            } else {
                self.add(Reached {
                    flag: far,
                    from: Some(here.flag),
                    cost,
                    fresh: true,
                });
                added = true;
            }
        }
        added
    }

    fn close(&mut self, slot: u16, flag: u16) {
        if let Some(mark) = self.marks.get_mut(usize::from(flag)) {
            *mark = Mark::Closed;
        }
        self.open.remove(slot);
    }
}

/// What it costs to take `road` to flag `far`.
fn step_cost(world: &World, road: &Road, far: u16, options: SearchOptions) -> u16 {
    let mut cost = road.edges();
    if options.contains(SearchOptions::BUSY) {
        cost = cost.saturating_add(if road.served {
            u16::from(road.busy)
        } else {
            UNSERVED_COST
        });
    }
    if options.contains(SearchOptions::LOAD) {
        let waiting = world.flag(far).map_or(0, |flag| flag.waiting);
        if waiting > LOAD_FREE {
            cost = cost.saturating_add(u16::from(waiting - LOAD_DISCOUNT));
        }
    }
    cost
}

/// Searches the road network from flag `start` for the cheapest flag that `wanted` accepts, at a
/// cost of at most `limit`, and returns it.
///
/// A road costs its edges, plus whatever `options` add. Water roads are left out unless
/// [`SearchOptions::WATER`] is given, and so are roads still being drawn. Among flags of equal
/// cost the one found first wins.
pub fn search(
    world: &World,
    start: u16,
    limit: u16,
    options: SearchOptions,
    mut wanted: impl FnMut(&Reached) -> bool,
) -> Option<Reached> {
    let mut frontier = Frontier {
        open: Pool::new(),
        marks: vec![Mark::Unseen; world.flag_slots()],
    };
    frontier.add(Reached {
        flag: start,
        from: None,
        cost: 0,
        fresh: false,
    });
    for cost in 0..=limit {
        let last = frontier.open.slots();
        let mut added = false;
        let mut slot = 0;
        while slot < last.min(frontier.open.slots()) {
            let index = u16::try_from(slot).unwrap_or(u16::MAX);
            slot += 1;
            let Some(&here) = frontier.open.get(index) else {
                continue;
            };
            if here.fresh || here.cost != cost {
                continue;
            }
            let skip = options.contains(SearchOptions::SKIP_START) && here.flag == start;
            if !skip && wanted(&here) {
                return Some(here);
            }
            added |= frontier.expand(world, here, limit, options);
            frontier.close(index, here.flag);
        }
        if frontier.open.is_empty() {
            return None;
        }
        if added {
            for (_, reached) in frontier.open.iter_mut() {
                reached.fresh = false;
            }
        }
    }
    None
}

/// The flag after `from` on the cheapest way to `to` (`0x3bc24`), searching backwards from `to`.
/// The load of `to` itself is never charged. `None` when `to` is out of reach or is `from`.
#[must_use]
pub fn next_flag(world: &World, from: u16, to: u16, limit: u16, options: SearchOptions) -> Option<u16> {
    search(world, to, limit, options, |reached| reached.flag == from)?.from
}

#[cfg(test)]
mod tests;
