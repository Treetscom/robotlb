use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Mutex,
    time::{Duration, Instant},
};

const FIRST_DELAY: Duration = Duration::from_secs(60);
const MAX_DOUBLINGS: u32 = 4;

/// Pause shared by every service. Hetzner counts API requests per project, so once one
/// reconciliation is rate limited, any other call would only spend the budget being waited for.
///
/// The generated `hcloud` client drops response headers, so `RateLimit-Reset` is not
/// available and the pause grows exponentially instead.
#[derive(Debug, Default)]
pub struct RateLimitGate {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    closed_until: Option<Instant>,
    last_delay: Duration,
    consecutive: u32,
}

impl State {
    fn remaining(&self, now: Instant) -> Option<Duration> {
        self.closed_until
            .and_then(|until| until.checked_duration_since(now))
            .filter(|remaining| !remaining.is_zero())
    }
}

impl RateLimitGate {
    /// How long calls to the API must still wait, if they must.
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.remaining(now)
    }

    /// Record a rate-limited call and return how long to wait before the next one.
    /// A call rejected while the gate is already closed was sent before it closed,
    /// so it keeps the current pause instead of lengthening it. The pause starts short
    /// again only after the API went without a 429 for as long as the last pause:
    /// a reconciliation that succeeds may not have called the API at all.
    pub fn on_rate_limited(&self, now: Instant) -> Duration {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(remaining) = state.remaining(now) {
            return remaining;
        }
        if let Some(reopened) = state.closed_until {
            if now >= reopened + state.last_delay {
                state.consecutive = 0;
            }
        }
        let delay = FIRST_DELAY * 2_u32.pow(state.consecutive.min(MAX_DOUBLINGS));
        state.consecutive += 1;
        state.closed_until = Some(now + delay);
        state.last_delay = delay;
        delay
    }
}

/// Push a wait out by up to a quarter, differently for each service, so services
/// paused together do not all call the API the moment the pause ends.
#[must_use]
pub fn spread(wait: Duration, service: &str) -> Duration {
    let mut hasher = DefaultHasher::new();
    service.hash(&mut hasher);
    let window = u64::try_from((wait / 4).as_millis())
        .unwrap_or(u64::MAX)
        .max(1);
    wait + Duration::from_millis(hasher.finish() % window)
}

#[cfg(test)]
mod tests {
    use super::{spread, RateLimitGate};
    use std::time::{Duration, Instant};

    #[test]
    fn an_untouched_gate_is_open() {
        assert_eq!(RateLimitGate::default().remaining(Instant::now()), None);
    }

    #[test]
    fn a_rate_limit_closes_the_gate_for_the_returned_delay() {
        let gate = RateLimitGate::default();
        let now = Instant::now();
        let delay = gate.on_rate_limited(now);
        assert_eq!(delay, Duration::from_secs(60));
        assert_eq!(gate.remaining(now), Some(delay));
        assert_eq!(gate.remaining(now + delay), None);
    }

    #[test]
    fn repeated_rate_limits_double_the_delay_up_to_the_cap() {
        let gate = RateLimitGate::default();
        let mut now = Instant::now();
        let mut delays = Vec::new();
        for _ in 0..8 {
            let delay = gate.on_rate_limited(now);
            delays.push(delay.as_secs());
            now += delay;
        }
        assert_eq!(delays, vec![60, 120, 240, 480, 960, 960, 960, 960]);
    }

    #[test]
    fn a_quiet_period_as_long_as_the_last_pause_resets_the_delay() {
        let gate = RateLimitGate::default();
        let now = Instant::now();
        let first = gate.on_rate_limited(now);
        let second = gate.on_rate_limited(now + first);
        let reopened = now + first + second;
        assert_eq!(
            gate.on_rate_limited(reopened + second),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn a_shorter_quiet_period_keeps_doubling() {
        let gate = RateLimitGate::default();
        let now = Instant::now();
        let first = gate.on_rate_limited(now);
        let second = gate.on_rate_limited(now + first);
        let reopened = now + first + second;
        assert_eq!(
            gate.on_rate_limited(reopened + second.checked_sub(Duration::from_secs(1)).unwrap()),
            Duration::from_secs(240)
        );
    }

    #[test]
    fn calls_already_in_flight_do_not_lengthen_the_pause() {
        let gate = RateLimitGate::default();
        let now = Instant::now();
        gate.on_rate_limited(now);
        let later = now + Duration::from_secs(10);
        assert_eq!(gate.on_rate_limited(later), Duration::from_secs(50));
        assert_eq!(
            gate.on_rate_limited(later + Duration::from_secs(50)),
            Duration::from_secs(120)
        );
    }

    #[test]
    fn spread_adds_up_to_a_quarter_of_the_pause() {
        let wait = Duration::from_secs(60);
        for name in ["web", "api", "dns", "ingress-nginx-controller"] {
            let spread_wait = spread(wait, name);
            assert!(spread_wait >= wait);
            assert!(spread_wait < wait + wait / 4);
        }
    }

    #[test]
    fn spread_is_stable_per_service_and_differs_between_services() {
        let wait = Duration::from_secs(60);
        assert_eq!(spread(wait, "web"), spread(wait, "web"));
        let wakeups = ["a", "b", "c", "d", "e", "f"]
            .map(|name| spread(wait, name))
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        assert!(wakeups.len() > 1);
    }
}
