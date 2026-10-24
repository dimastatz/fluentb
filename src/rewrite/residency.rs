//! When to unload the rewrite model to free memory.

use crate::config::Rewrite;
use std::time::{Duration, Instant};

/// How long an idle model stays in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnloadPolicy {
    /// `None` keeps the model loaded until fluentb exits.
    pub idle: Option<Duration>,
}

impl UnloadPolicy {
    #[must_use]
    pub fn from_config(rewrite: &Rewrite) -> Self {
        Self {
            idle: (!rewrite.keep_loaded)
                .then(|| Duration::from_secs(u64::from(rewrite.unload_after_minutes) * 60)),
        }
    }

    /// How long until a model last used at `last_used` should be unloaded: zero when
    /// it is due, `None` when it stays loaded.
    #[must_use]
    pub fn remaining(self, last_used: Instant, now: Instant) -> Option<Duration> {
        self.idle
            .map(|idle| idle.saturating_sub(now.saturating_duration_since(last_used)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::platform::Os;

    fn policy(keep_loaded: bool, minutes: u32) -> UnloadPolicy {
        UnloadPolicy::from_config(&Rewrite {
            keep_loaded,
            unload_after_minutes: minutes,
            ..Config::defaults(Os::MacOs).rewrite
        })
    }

    #[test]
    fn counts_down_from_the_last_rewrite() {
        let policy = policy(false, 10);
        let used = Instant::now();
        let minute = Duration::from_secs(60);

        assert_eq!(policy.remaining(used, used), Some(10 * minute));
        assert_eq!(policy.remaining(used, used + 4 * minute), Some(6 * minute));
        assert_eq!(
            policy.remaining(used, used + 10 * minute),
            Some(Duration::ZERO)
        );
        assert_eq!(
            policy.remaining(used, used + 60 * minute),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn a_clock_that_goes_backwards_restarts_the_count() {
        let policy = policy(false, 1);
        let now = Instant::now();

        assert_eq!(
            policy.remaining(now + Duration::from_secs(5), now),
            Some(Duration::from_secs(60))
        );
    }

    #[test]
    fn keep_loaded_never_unloads() {
        let used = Instant::now();

        assert_eq!(
            policy(true, 10).remaining(used, used + Duration::from_secs(86_400)),
            None
        );
    }
}
