//! Task 14's request board: what the economy has asked to be built and where.

use crate::grid::Node;
use crate::world::Player;

/// The kinds of request, 0 to 8.
pub const KINDS: usize = 9;

/// The kinds of request, which are also the kinds of project.
pub(super) const WOOD: u8 = 0;
pub(super) const STONE: u8 = 1;
pub(super) const WEAPONS: u8 = 2;
pub(super) const TOOLS: u8 = 3;
pub(super) const GOLD: u8 = 4;
pub(super) const FOOD: u8 = 5;
pub(super) const BEER: u8 = 6;
pub(super) const COAL: u8 = 7;
pub(super) const DONKEYS: u8 = 8;

/// How many requests the board holds in all (`+0x3e`).
const ENTRIES: usize = 100;

/// The most requests of one kind (`0x72417`).
const MOST_OF_A_KIND: u16 = 10;

/// The furthest, squared, a granted request may lie from where it is wanted: kind 6's and the
/// rest's (`0x72557`, `0x7255e`).
const REACH_SQUARED: (i32, i32) = (900, 625);

/// The kind with the wider reach.
const WIDE: u8 = BEER;

/// The kinds a grant does not hold back for want of land (`0x724d6`).
const ALWAYS_GRANTED: [u8; 2] = [WOOD, GOLD];

/// One request: its kind and where it is wanted, `None` for anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Request {
    /// What is asked for, 0 to 8.
    pub kind: u8,
    /// Where, as `S2.EXE` keeps it: signed words, x of −1 for anywhere.
    pub at: (i16, i16),
}

/// What a grant reads about its player.
#[derive(Debug, Clone, Copy)]
pub struct Ledger {
    /// The nodes the player owns (`0x29be0`).
    pub land: u32,
    /// The player's sites under construction (`0x29c43`).
    pub sites: u16,
    /// Land a site under construction needs, the profile's `+0x26`.
    pub land_per_site: u16,
    /// The cooldown a granted kind starts, the profile's `+0x8a`.
    pub take_waits: [u16; KINDS],
}

/// The board.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RequestBoard {
    counts: [u16; KINDS],
    add_waits: [u16; KINDS],
    take_waits: [u16; KINDS],
    entries: Vec<Option<Request>>,
}

impl Default for RequestBoard {
    fn default() -> Self {
        Self {
            counts: [0; KINDS],
            add_waits: [0; KINDS],
            take_waits: [0; KINDS],
            entries: vec![None; ENTRIES],
        }
    }
}

impl RequestBoard {
    /// How many requests of `kind` it holds (`0x72480`).
    #[must_use]
    pub fn count(&self, kind: u8) -> u16 {
        self.counts.get(usize::from(kind)).copied().unwrap_or_default()
    }

    /// The cooldown before another request of `kind` may be added by the task that adds it.
    #[must_use]
    pub fn add_wait(&self, kind: u8) -> u16 {
        self.add_waits.get(usize::from(kind)).copied().unwrap_or_default()
    }

    /// Takes one off every cooldown still running (`0x71737` to `0x7176e`).
    pub fn tick(&mut self) {
        for wait in self.add_waits.iter_mut().chain(&mut self.take_waits) {
            *wait = wait.saturating_sub(1);
        }
    }

    /// The requests, by entry.
    #[must_use]
    pub fn entries(&self) -> &[Option<Request>] {
        &self.entries
    }

    /// Adds a request of `kind` at `at` (`0x723fe`) unless 10 of the kind or 100 in all are
    /// held, starting the kind's add cooldown at `add_wait`, and with `clear` its take cooldown
    /// at 0. Returns the entry it took.
    pub fn add(&mut self, kind: u8, at: (i16, i16), add_wait: u16, clear: bool) -> Option<usize> {
        let k = usize::from(kind);
        if self.counts.get(k).is_none_or(|&count| count == MOST_OF_A_KIND) {
            return None;
        }
        let entry = self.entries.iter().position(Option::is_none)?;
        self.counts[k] += 1;
        self.add_waits[k] = add_wait;
        self.entries[entry] = Some(Request { kind, at });
        if clear {
            self.take_waits[k] = 0;
        }
        Some(entry)
    }

    /// Takes away the first request of `kind`, wherever it is wanted (`0x725fb`).
    pub fn remove_first(&mut self, kind: u8) {
        if self.count(kind) == 0 {
            return;
        }
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.is_some_and(|request| request.kind == kind))
        {
            *entry = None;
            self.counts[usize::from(kind)] -= 1;
        }
    }

    /// Grants a request of `kind` for a build at `at` (`0x72489`), or refuses. A grant needs a
    /// request of the kind and no cooldown running, and for kinds other than 0 and 4 fewer
    /// sites under construction than the player's land over its land per site. It takes the
    /// request nearest `at` by squared distance, an anywhere request at once, when that is within
    /// 30 nodes for kind 6 and 25 for the rest. A grant starts the kind's cooldown and adds the
    /// follow-on requests: 7 and 5 at `at` for kinds 2, 3 and 4, and 5 for kind 7.
    pub fn grant(&mut self, kind: u8, at: Node, ledger: &Ledger, add_wait: u16) -> bool {
        let k = usize::from(kind);
        if self.count(kind) == 0 || self.take_waits.get(k).is_none_or(|&wait| wait != 0) {
            return false;
        }
        let room = ledger.land / u32::from(ledger.land_per_site.max(1));
        if !ALWAYS_GRANTED.contains(&kind) && u32::from(ledger.sites) >= room {
            return false;
        }
        let (x, y) = (i32::from(at.x.cast_signed()), i32::from(at.y.cast_signed()));
        let mut best: Option<(i32, usize)> = None;
        for (entry, request) in self.entries.iter().enumerate() {
            let Some(request) = request.filter(|request| request.kind == kind) else {
                continue;
            };
            if request.at.0 == -1 {
                best = Some((0, entry));
                break;
            }
            let (dx, dy) = (x - i32::from(request.at.0), y - i32::from(request.at.1));
            let distance = dx * dx + dy * dy;
            if best.is_none_or(|(nearest, _)| distance < nearest) {
                best = Some((distance, entry));
            }
        }
        let reach = if kind == WIDE {
            REACH_SQUARED.0
        } else {
            REACH_SQUARED.1
        };
        let Some((_, entry)) = best.filter(|&(distance, _)| distance <= reach) else {
            return false;
        };
        self.entries[entry] = None;
        self.take_waits[k] = ledger.take_waits[k];
        let here = (at.x.cast_signed(), at.y.cast_signed());
        match kind {
            2..=4 => {
                self.add(7, here, add_wait, false);
                self.add(5, here, add_wait, false);
            }
            7 => {
                self.add(5, here, add_wait, false);
            }
            _ => {}
        }
        self.counts[k] -= 1;
        true
    }
}

/// What a grant needs to know about `player`, from its counters and its profile.
#[must_use]
pub fn ledger(
    world: &crate::world::World,
    land: u32,
    player: Player,
    profile: &super::ComputerProfile,
) -> Ledger {
    Ledger {
        land,
        sites: world.all_building_sites(player),
        land_per_site: profile.land_per_site,
        take_waits: profile.take_waits,
    }
}

#[cfg(test)]
mod tests;
