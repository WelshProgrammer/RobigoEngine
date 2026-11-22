//! When the game runs simulation steps: the original's main loop, driven by its 60 Hz timer.
//! Each pass counts the whole eighths of a second since the last pass that ran anything, at
//! most 8. It advances the animation counter by that count and runs that many simulation steps
//! per unit of speed. Setting the last time to now drops the remainder, so at normal speed a
//! step comes every 8 ticks: 133⅓ ms.

use std::time::Duration;

/// The timer's rate.
const TICKS_PER_SECOND: u64 = 60;

/// Milliseconds in a second, to turn elapsed time into ticks.
const MILLIS_PER_SECOND: u64 = 1000;

/// The pass counts time in eighths of a second.
const PARTS_PER_SECOND: u64 = 8;

/// The most a slow pass catches up.
const MOST_PARTS: u64 = 8;

/// Above this speed a pass runs one unit of steps per unit of speed, however long it took.
const CATCH_UP_SPEED: u16 = 5;

/// The speeds the fast-forward key toggles between.
const NORMAL: u16 = 1;
const FAST: u16 = 2;

/// The main loop's timing state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pace {
    last: Option<u64>,
    speed: u16,
    paused: bool,
    anim: u32,
}

impl Pace {
    /// A loop at normal speed that has not run a pass yet.
    pub(crate) const fn new() -> Self {
        Self {
            last: None,
            speed: NORMAL,
            paused: false,
            anim: 0,
        }
    }

    /// The animation counter.
    pub(crate) const fn anim(&self) -> u32 {
        self.anim
    }

    /// The current speed: 1 normal, 2 fast.
    pub(crate) const fn speed(&self) -> u16 {
        self.speed
    }

    /// Whether the game is paused.
    pub(crate) const fn paused(&self) -> bool {
        self.paused
    }

    /// Switches between normal and fast speed.
    pub(crate) fn toggle_speed(&mut self) {
        self.speed = if self.speed == NORMAL { FAST } else { NORMAL };
    }

    /// Pauses or resumes. While paused a pass only records the time, so the game picks up where
    /// it stopped instead of catching up.
    pub(crate) fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    /// Runs one pass of the main loop at `elapsed` since the program started. Returns how many
    /// simulation steps to run now.
    pub(crate) fn pass(&mut self, elapsed: Duration) -> u32 {
        let millis = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        let now = millis.saturating_mul(TICKS_PER_SECOND) / MILLIS_PER_SECOND;
        if self.paused {
            self.last = Some(now);
            return 0;
        }
        let parts = match self.last {
            None => 1,
            Some(last) => (now.saturating_sub(last) * PARTS_PER_SECOND / TICKS_PER_SECOND).min(MOST_PARTS),
        };
        if parts == 0 {
            return 0;
        }
        self.last = Some(now);
        let parts = u32::try_from(parts).unwrap_or_default();
        let units = if self.speed > CATCH_UP_SPEED { 1 } else { parts };
        self.anim = self.anim.wrapping_add(units);
        units * u32::from(self.speed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn a_step_every_eight_ticks_with_the_remainder_dropped() {
        let mut pace = Pace::new();
        assert_eq!(pace.pass(at(0)), 1, "the first pass always runs");
        assert_eq!(pace.pass(at(116)), 0, "7 ticks");
        assert_eq!(pace.pass(at(134)), 1, "8 ticks");
        assert_eq!(pace.pass(at(250)), 0, "7 more ticks since the last step");
        assert_eq!(pace.pass(at(400)), 2, "16 ticks");
        assert_eq!(pace.pass(at(10_000)), 8, "a long stall catches up 8 at most");
        assert_eq!(pace.anim(), 12);
    }

    #[test]
    fn fast_doubles_the_steps_but_not_the_animation() {
        let mut pace = Pace::new();
        pace.toggle_speed();
        assert_eq!(pace.pass(at(0)), 2);
        assert_eq!(pace.pass(at(150)), 2);
        assert_eq!(pace.anim(), 2);
    }

    #[test]
    fn pausing_stops_the_clock_without_catching_up() {
        let mut pace = Pace::new();
        pace.pass(at(0));
        pace.toggle_pause();
        assert_eq!(pace.pass(at(5_000)), 0);
        pace.toggle_pause();
        assert_eq!(pace.pass(at(5_100)), 0);
        assert_eq!(pace.pass(at(5_140)), 1);
    }
}
