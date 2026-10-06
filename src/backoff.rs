use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

const FIRST_DELAY: Duration = Duration::from_secs(5);
const MAX_DELAY: Duration = Duration::from_secs(300);
/// A service deleted before it got the robotlb finalizer is not reconciled again to be
/// forgotten on success, so an entry that has not failed for this long is dropped. It is
/// well above `MAX_DELAY` plus one rate limit pause; a service held at the rate limit gate
/// for longer starts short again.
const FORGET_AFTER: Duration = Duration::from_secs(3600);

/// Retry delay of each service whose reconciliation keeps failing.
///
/// It doubles from `FIRST_DELAY` up to `MAX_DELAY` like the upstream cloud-provider
/// service controller:
/// <https://github.com/kubernetes/cloud-provider/blob/master/controllers/service/controller.go>
#[derive(Debug, Default)]
pub struct ErrorBackoff {
    failures: Mutex<HashMap<String, Failures>>,
}

#[derive(Debug)]
struct Failures {
    count: u32,
    last: Instant,
}

impl ErrorBackoff {
    /// Record a failed reconciliation of `service` and return how long to wait before the next one.
    pub fn on_failure(&self, service: &str, now: Instant) -> Duration {
        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        failures.retain(|_, failure| now.saturating_duration_since(failure.last) <= FORGET_AFTER);
        let failure = failures.entry(service.to_string()).or_insert(Failures {
            count: 0,
            last: now,
        });
        let delay = FIRST_DELAY
            .saturating_mul(2_u32.saturating_pow(failure.count))
            .min(MAX_DELAY);
        failure.count = failure.count.saturating_add(1);
        failure.last = now;
        drop(failures);
        delay
    }

    /// Start the delay of `service` short again.
    pub fn forget(&self, service: &str) {
        self.failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(service);
    }

    #[cfg(test)]
    fn tracked(&self) -> usize {
        self.failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorBackoff, FORGET_AFTER};
    use std::time::{Duration, Instant};

    #[test]
    fn repeated_failures_double_the_delay_up_to_the_cap() {
        let backoff = ErrorBackoff::default();
        let mut now = Instant::now();
        let mut delays = Vec::new();
        for _ in 0..9 {
            let delay = backoff.on_failure("shop/web", now);
            delays.push(delay.as_secs());
            now += delay;
        }
        assert_eq!(delays, vec![5, 10, 20, 40, 80, 160, 300, 300, 300]);
    }

    #[test]
    fn services_back_off_independently() {
        let backoff = ErrorBackoff::default();
        let now = Instant::now();
        backoff.on_failure("shop/web", now);
        backoff.on_failure("shop/web", now);
        assert_eq!(backoff.on_failure("shop/api", now), Duration::from_secs(5));
        assert_eq!(backoff.on_failure("shop/web", now), Duration::from_secs(20));
    }

    #[test]
    fn a_forgotten_service_starts_short_again() {
        let backoff = ErrorBackoff::default();
        let now = Instant::now();
        backoff.on_failure("shop/web", now);
        backoff.on_failure("shop/web", now);
        backoff.forget("shop/web");
        assert_eq!(backoff.on_failure("shop/web", now), Duration::from_secs(5));
    }

    #[test]
    fn a_long_failing_service_is_not_forgotten() {
        let backoff = ErrorBackoff::default();
        let mut now = Instant::now();
        for _ in 0..20 {
            now += backoff.on_failure("shop/web", now);
        }
        assert_eq!(
            backoff.on_failure("shop/web", now),
            Duration::from_secs(300)
        );
    }

    #[test]
    fn services_that_stopped_failing_are_dropped() {
        let backoff = ErrorBackoff::default();
        let now = Instant::now();
        backoff.on_failure("shop/deleted", now);
        backoff.on_failure("shop/web", now + FORGET_AFTER + Duration::from_secs(1));
        assert_eq!(backoff.tracked(), 1);
    }
}
