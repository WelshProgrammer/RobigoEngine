//! Commands being played into the viewer's game from a replay (`s2_sim::replay`).

use s2_sim::{Game, Stamped};

/// A replay's commands being played back into a game, a step at a time.
#[derive(Debug, Default)]
pub(crate) struct Queue {
    log: Vec<Stamped>,
    next: usize,
}

impl Queue {
    /// Plays `log`, already in step order.
    pub(crate) fn new(log: Vec<Stamped>) -> Self {
        Self { log, next: 0 }
    }

    /// Plays `log` into `game`, restored from a save: the commands stamped up to its current
    /// step count as carried out.
    pub(crate) fn resumed(log: Vec<Stamped>, game: &Game) -> Self {
        let now = game.clock().step();
        let next = log.partition_point(|stamped| stamped.step <= now);
        Self { log, next }
    }

    /// Carries out the commands stamped up to `game`'s current step that have not run yet, and
    /// returns them. A refused command is logged and skipped, as `Game::run` skips it.
    pub(crate) fn catch_up(&mut self, game: &mut Game) -> &[Stamped] {
        let first = self.next;
        let now = game.clock().step();
        while let Some(&stamped) = self.log.get(self.next).filter(|s| s.step <= now) {
            if let Err(refusal) = game.execute(stamped.player, stamped.command) {
                log::warn!(
                    "replay, step {}: {:?} refused: {refusal:?}",
                    stamped.step,
                    stamped.command
                );
            }
            self.next += 1;
        }
        &self.log[first..self.next]
    }

    /// Whether every command has been carried out.
    pub(crate) fn finished(&self) -> bool {
        self.next >= self.log.len()
    }

    /// The commands carried out so far.
    pub(crate) fn done(&self) -> &[Stamped] {
        &self.log[..self.next]
    }
}
