//! How many people each player holds, per job.
//!
//! `S2.EXE` keeps a count per job and a total for every player, adds to them wherever a person
//! comes into being and takes off wherever one stops being a person. The counts are what the
//! statistics screens draw and what military strength is worked out from, so they are kept here
//! rather than counted off the figures each time a sample is taken.
//!
//! Byte 0 of the same job table names the tool a helper turns into one with, and byte 4 names the
//! ware a settler carries; both live in [`crate::wares`].
//!
//! Places the original counts that Robigo does not yet: the builder of a demolished site
//! (`0x2cdb8`), the figure removed at `0x2ec76`, and the figure removed at `0x35c07`.

use crate::job::Job;
use crate::world::{Player, World};

/// The jobs the table covers.
pub(crate) const JOBS: usize = 72;

/// The entry that keeps the job as it is.
const SAME_JOB: Job = Job(u8::MAX);

/// What a settler of each job counts as, by nation (byte 3 of each row of the job table at
/// `0x1fad97`, so `0x1fad9a + 0x240·nation + 8·job`); [`SAME_JOB`] keeps the job. The entries turn
/// what a worker carries back into its job, [`Job::CARRYING_WOOD`] into [`Job::WOODCUTTER`] for
/// one. A lost settler takes the job this names, and so does a person coming off a player's count
/// (`0x29a11`). The nations differ in five entries: a Viking farmer carrying grain becomes a
/// [`Job::PIG_BREEDER`], and an African farmer out sowing becomes a [`Job::FARMER`].
#[rustfmt::skip]
const COUNTS_AS: [[Job; JOBS]; 4] = [
    [
        SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB,
        SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB,
        SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB,
        SAME_JOB, Job::BREWER, Job::ARMOURER, Job::ARMOURER, Job::ARMOURER, Job::ARMOURER, Job::ARMOURER,
        Job::ARMOURER, Job::WOODCUTTER, Job::CARPENTER, Job::STONEMASON, Job::MINTER, Job::MINER, Job::MINER,
        Job::MINER, Job::MINER, Job::IRON_FOUNDER, Job::FISHER, Job::FARMER, Job::PIG_BREEDER,
        Job::PIG_BREEDER, Job::BUTCHER, Job::MILLER, Job::BAKER, SAME_JOB, SAME_JOB, SAME_JOB,
        Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER,
        Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER,
        Job::HUNTER, Job::SHIPWRIGHT, Job::SHIPWRIGHT, Job::HELPER, Job::HELPER,
    ],
    [
        SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB,
        SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB,
        SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB,
        SAME_JOB, Job::BREWER, Job::ARMOURER, Job::ARMOURER, Job::ARMOURER, Job::ARMOURER, Job::ARMOURER,
        Job::ARMOURER, Job::WOODCUTTER, Job::CARPENTER, Job::STONEMASON, Job::MINTER, Job::MINER, Job::MINER,
        Job::MINER, Job::MINER, Job::IRON_FOUNDER, Job::FISHER, Job::PIG_BREEDER, Job::PIG_BREEDER,
        Job::PIG_BREEDER, Job::BUTCHER, Job::MILLER, Job::BAKER, SAME_JOB, SAME_JOB, SAME_JOB,
        Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER,
        Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER,
        Job::HUNTER, Job::SHIPWRIGHT, Job::SHIPWRIGHT, Job::HELPER, Job::HELPER,
    ],
    [
        SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB,
        SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB,
        SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB,
        SAME_JOB, Job::BREWER, Job::ARMOURER, Job::ARMOURER, Job::ARMOURER, Job::ARMOURER, Job::ARMOURER,
        SAME_JOB, Job::WOODCUTTER, Job::CARPENTER, Job::STONEMASON, Job::MINTER, Job::MINER, Job::MINER,
        Job::MINER, Job::MINER, Job::IRON_FOUNDER, Job::FISHER, Job::FARMER, Job::PIG_BREEDER,
        Job::PIG_BREEDER, Job::BUTCHER, Job::MILLER, Job::BAKER, SAME_JOB, SAME_JOB, SAME_JOB,
        Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER,
        Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER,
        Job::HUNTER, Job::SHIPWRIGHT, Job::SHIPWRIGHT, Job::HELPER, Job::HELPER,
    ],
    [
        SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB,
        SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB,
        SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB, SAME_JOB,
        Job::FARMER, Job::BREWER, Job::ARMOURER, Job::ARMOURER, Job::ARMOURER, Job::ARMOURER, Job::ARMOURER,
        Job::ARMOURER, Job::WOODCUTTER, Job::CARPENTER, Job::STONEMASON, Job::MINTER, Job::MINER, Job::MINER,
        Job::MINER, Job::MINER, Job::IRON_FOUNDER, Job::FISHER, Job::FARMER, Job::PIG_BREEDER,
        Job::PIG_BREEDER, Job::BUTCHER, Job::MILLER, Job::BAKER, Job::METALWORKER, SAME_JOB,
        Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER,
        Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER, Job::METALWORKER,
        Job::METALWORKER, Job::HUNTER, Job::SHIPWRIGHT, Job::SHIPWRIGHT, Job::HELPER, Job::HELPER,
    ],
];

/// The row and column of the job table `S2.EXE` reads for `job` and `nation`, at
/// `0x240·nation + 8·job`. A job past the 72 a row holds reads on into a later nation's row, as
/// the job of a freed record does; past the last row there is none.
pub(crate) fn table_entry(nation: u8, job: Job) -> Option<(usize, usize)> {
    let flat = usize::from(nation) * JOBS + job.index();
    (flat < COUNTS_AS.len() * JOBS).then_some((flat / JOBS, flat % JOBS))
}

/// The job byte 3 of `job`'s row names for a player of `nation`, or `None` where the row keeps
/// the job.
#[must_use]
pub(crate) fn named_job(nation: u8, job: Job) -> Option<Job> {
    let (row, column) = table_entry(nation, job)?;
    Some(COUNTS_AS[row][column]).filter(|&named| named != SAME_JOB)
}

/// What a settler of `job` counts as for a player of `nation`, which is `job` itself for most of
/// them.
#[must_use]
pub(crate) fn counts_as(nation: u8, job: Job) -> Job {
    named_job(nation, job).unwrap_or(job)
}

impl World {
    /// How many people of `job` `player` holds (`0x29a87`).
    #[must_use]
    pub fn people(&self, player: Player, job: Job) -> u16 {
        self.people
            .get(player.index())
            .and_then(|counts| counts.get(job.index()))
            .copied()
            .unwrap_or_default()
    }

    /// How many people `player` holds in all (`0x29c54`), which the original keeps as a word of
    /// its own rather than adding the jobs up.
    #[must_use]
    pub fn people_total(&self, player: Player) -> u16 {
        self.people_total.get(player.index()).copied().unwrap_or_default()
    }

    /// `player`'s military strength: each rank's count times its rank, 1 for a private up to 5
    /// for a general (`0x29e4e`).
    #[must_use]
    pub fn military_strength(&self, player: Player) -> u32 {
        Job::RANKS
            .iter()
            .enumerate()
            .map(|(rank, &job)| {
                u32::from(self.people(player, job)) * u32::try_from(rank + 1).unwrap_or_default()
            })
            .sum()
    }

    /// Counts `count` people of `job` in for `player` (`0x299f5`). The job is taken as it comes,
    /// with no remap, and a job past the people is left out; the original would write past the
    /// array.
    pub(crate) fn count_people(&mut self, player: Player, job: Job, count: u16) {
        let Some(counts) = self.people.get_mut(player.index()) else {
            return;
        };
        let Some(held) = counts.get_mut(job.index()) else {
            return;
        };
        *held = held.wrapping_add(count);
        let total = &mut self.people_total[player.index()];
        *total = total.wrapping_add(count);
    }

    /// Counts `count` people of `job` off `player` (`0x29a11`). The job the player's nation names
    /// is taken off instead, and `count` is held to what that job's count holds, so neither it nor
    /// the total can go below zero. A job past the people is left out; the original would write
    /// past the array.
    pub(crate) fn drop_people(&mut self, player: Player, job: Job, count: u16) {
        let nation = self.nations.get(player.index()).copied().unwrap_or_default();
        let job = counts_as(nation, job);
        let Some(counts) = self.people.get_mut(player.index()) else {
            return;
        };
        let Some(held) = counts.get_mut(job.index()) else {
            return;
        };
        let count = count.min(*held);
        *held -= count;
        let total = &mut self.people_total[player.index()];
        *total = total.saturating_sub(count);
    }
}

#[cfg(test)]
mod tests;
