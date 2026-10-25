//! Pending timers for a facade's `next_event` loop: at most one per [`Timer`] kind,
//! since starting one again replaces it.

use crate::facade::Timer;
use std::time::{Duration, Instant};

/// When each pending timer is due.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Timers(Vec<(Timer, Instant)>);

impl Timers {
    /// Adds `timer`, due at `at`, replacing a pending one of the same kind.
    #[must_use]
    pub fn start(self, timer: Timer, at: Instant) -> Self {
        Self(
            self.0
                .into_iter()
                .filter(|(pending, _)| *pending != timer)
                .chain([(timer, at)])
                .collect(),
        )
    }

    /// How long until the next timer is due, from `now`; `None` if none is pending.
    #[must_use]
    pub fn time_left(&self, now: Instant) -> Option<Duration> {
        self.0
            .iter()
            .map(|(_, at)| at.saturating_duration_since(now))
            .min()
    }

    /// Removes and returns the earliest timer that is due at `now`.
    #[must_use]
    pub fn take_due(self, now: Instant) -> (Option<Timer>, Self) {
        let due = self
            .0
            .iter()
            .filter(|(_, at)| *at <= now)
            .min_by_key(|(_, at)| *at)
            .map(|(timer, _)| *timer);
        let rest = self
            .0
            .into_iter()
            .filter(|(timer, _)| Some(*timer) != due)
            .collect();
        (due, Self(rest))
    }
}

/// The sooner of two optional waits; `None` means wait forever.
#[must_use]
pub fn sooner(a: Option<Duration>, b: Option<Duration>) -> Option<Duration> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn nothing_pending_waits_forever() {
        let timers = Timers::default();

        assert_eq!(timers.time_left(Instant::now()), None);
        assert_eq!(timers.take_due(Instant::now()).0, None);
    }

    #[test]
    fn starting_a_timer_again_replaces_it() {
        let now = Instant::now();
        let timers = Timers::default()
            .start(Timer::MaxRecording, now + SECOND)
            .start(Timer::MaxRecording, now + 10 * SECOND);

        assert_eq!(timers.time_left(now), Some(10 * SECOND));
        assert_eq!(timers.clone().take_due(now + 2 * SECOND).0, None);
        assert_eq!(
            timers.take_due(now + 10 * SECOND).0,
            Some(Timer::MaxRecording)
        );
    }

    #[test]
    fn due_timers_fire_earliest_first_and_once() {
        let now = Instant::now();
        let timers = Timers::default()
            .start(Timer::MaxRecording, now + 2 * SECOND)
            .start(Timer::ClearError, now + SECOND);

        assert_eq!(timers.time_left(now), Some(SECOND));
        let later = now + 3 * SECOND;
        assert_eq!(timers.time_left(later), Some(Duration::ZERO));
        let (first, timers) = timers.take_due(later);
        let (second, timers) = timers.take_due(later);
        let (third, timers) = timers.take_due(later);
        assert_eq!(
            [first, second, third],
            [Some(Timer::ClearError), Some(Timer::MaxRecording), None]
        );
        assert_eq!(timers, Timers::default());
    }

    #[test]
    fn sooner_picks_the_shorter_wait() {
        assert_eq!(sooner(Some(SECOND), Some(2 * SECOND)), Some(SECOND));
        assert_eq!(sooner(None, Some(SECOND)), Some(SECOND));
        assert_eq!(sooner(Some(SECOND), None), Some(SECOND));
        assert_eq!(sooner(None, None), None);
    }
}
