//! Dynamic, congestion-aware rate limiting per group.
//!
//! The documented TPS is a **ceiling**, not gospel: under heavy load the server may throttle
//! harder than the docs say. A per-group AIMD controller ratchets the effective rate *down*
//! on observed throttling (429 / timeouts / 5xx) and recovers it *up to — never beyond —*
//! the documented (peak-aware) baseline. Server `X-RateLimit-*` headers, when present, are
//! authoritative and set the rate directly; `Retry-After` parks the group until a deadline.
//!
//! The control law ([`apply_feedback`]) is a pure function of `(state, feedback, ceiling, now)`
//! so it is exhaustively unit-testable without a clock or network.

use crate::RateLimitGroup;
use arc_swap::ArcSwap;
use chrono::NaiveTime;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Parsed `X-RateLimit-*` response headers.
#[derive(Clone, Copy, Debug, Default)]
pub struct RateLimitHeaders {
    /// `X-RateLimit-Limit` — the server's current burst capacity (authoritative).
    pub limit: Option<u32>,
    /// `X-RateLimit-Remaining` — tokens left in the bucket.
    pub remaining: Option<u32>,
    /// `X-RateLimit-Reset` — seconds until one token replenishes.
    pub reset: Option<u64>,
}

/// Feedback about one request's outcome, fed to the controller.
#[derive(Clone, Debug)]
pub enum Feedback {
    /// A successful (2xx) response.
    Success {
        /// Rate-limit headers, if present.
        headers: RateLimitHeaders,
    },
    /// A 429. `retry_after` parks the group; the rate is also decreased.
    RateLimited {
        /// `Retry-After`, if present.
        retry_after: Option<Duration>,
        /// Rate-limit headers, if present.
        headers: RateLimitHeaders,
    },
    /// A 5xx server error (soft congestion signal).
    ServerError,
    /// A timeout (soft congestion signal).
    Timeout,
}

/// AIMD tuning constants.
#[derive(Clone, Copy, Debug)]
pub struct AimdConfig {
    /// Multiplicative decrease factor on a hard throttle (429).
    pub decrease_factor: f64,
    /// Softer multiplicative decrease on a timeout / 5xx.
    pub soft_decrease_factor: f64,
    /// Additive TPS added per recovery step on sustained success.
    pub increase_step: f64,
    /// Absolute lower bound on the effective rate.
    pub floor_tps: f64,
    /// Minimum spacing between decreases (avoids over-throttling on a burst of 429s).
    pub decrease_cooldown: Duration,
    /// Minimum spacing between increases (slows recovery to avoid oscillation).
    pub increase_cooldown: Duration,
}

impl Default for AimdConfig {
    fn default() -> Self {
        Self {
            decrease_factor: 0.6,
            soft_decrease_factor: 0.8,
            increase_step: 0.5,
            floor_tps: 1.0,
            decrease_cooldown: Duration::from_millis(750),
            increase_cooldown: Duration::from_secs(2),
        }
    }
}

/// Mutable controller state for one group.
#[derive(Clone, Debug)]
pub struct ControllerState {
    /// Current effective rate (requests/second).
    pub effective_tps: f64,
    /// If set, requests are parked until this instant (from `Retry-After`).
    pub parked_until: Option<Instant>,
    last_decrease: Option<Instant>,
    last_increase: Option<Instant>,
}

impl ControllerState {
    /// A fresh state seeded at `tps`.
    pub fn new(tps: f64) -> Self {
        Self {
            effective_tps: tps,
            parked_until: None,
            last_decrease: None,
            last_increase: None,
        }
    }
}

fn cooled_down(last: Option<Instant>, now: Instant, cooldown: Duration) -> bool {
    last.map(|t| now.duration_since(t) >= cooldown)
        .unwrap_or(true)
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi.max(lo))
}

/// The pure AIMD control law. Mutates `state` for `feedback` given the current `ceiling`
/// (the peak-aware documented baseline) and `now`. Returns `Some(new_tps)` if the effective
/// rate changed enough to warrant rebuilding the limiter, else `None`.
///
/// Invariants: the result is always within `[cfg.floor_tps, ceiling]`; `Retry-After` always
/// updates `parked_until`; decreases/increases respect their cooldowns; and a peak-window
/// ceiling drop is applied even on success (the rate can never exceed the current ceiling).
pub fn apply_feedback(
    state: &mut ControllerState,
    feedback: &Feedback,
    ceiling: f64,
    now: Instant,
    cfg: &AimdConfig,
) -> Option<f64> {
    let floor = cfg.floor_tps;
    let before = state.effective_tps;
    let mut target = before;

    match feedback {
        Feedback::Success { headers } => {
            if let Some(limit) = headers.limit {
                // The server told us its current capacity — authoritative.
                target = limit as f64;
            } else if cooled_down(state.last_increase, now, cfg.increase_cooldown) {
                target = before + cfg.increase_step;
                state.last_increase = Some(now);
            }
        }
        Feedback::RateLimited {
            retry_after,
            headers,
        } => {
            if let Some(ra) = retry_after {
                state.parked_until = Some(now + *ra);
            }
            if let Some(limit) = headers.limit {
                target = (limit as f64).min(before);
            }
            if cooled_down(state.last_decrease, now, cfg.decrease_cooldown) {
                target = (target * cfg.decrease_factor).min(target);
                state.last_decrease = Some(now);
            }
        }
        Feedback::ServerError | Feedback::Timeout => {
            if cooled_down(state.last_decrease, now, cfg.decrease_cooldown) {
                target = before * cfg.soft_decrease_factor;
                state.last_decrease = Some(now);
            }
        }
    }

    // The effective rate can never exceed the current (peak-aware) ceiling, nor drop below
    // the floor — applied unconditionally so a peak-window ceiling drop takes effect on any
    // feedback, including success.
    let new = clamp(target, floor, ceiling);
    if (new - before).abs() > before.max(1.0) * 0.01 {
        state.effective_tps = new;
        Some(new)
    } else {
        // Keep `effective_tps` exactly at the clamped value even on a no-op-sized change so
        // the stored value never drifts above the ceiling.
        state.effective_tps = new;
        None
    }
}

/// A per-group limiter whose quota is swapped at runtime by the AIMD controller.
pub struct DynamicLimiter {
    group: RateLimitGroup,
    cfg: AimdConfig,
    limiter: ArcSwap<DefaultDirectRateLimiter>,
    state: Mutex<ControllerState>,
}

impl std::fmt::Debug for DynamicLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.state.lock().unwrap();
        f.debug_struct("DynamicLimiter")
            .field("group", &self.group)
            .field("effective_tps", &s.effective_tps)
            .field("parked", &s.parked_until.is_some())
            .finish()
    }
}

fn build_limiter(tps: f64) -> DefaultDirectRateLimiter {
    let tps = tps.max(0.1);
    let period = Duration::from_secs_f64(1.0 / tps);
    let burst = NonZeroU32::new((tps.round() as u32).max(1)).unwrap();
    let quota = Quota::with_period(period)
        .expect("period is non-zero")
        .allow_burst(burst);
    RateLimiter::direct(quota)
}

impl DynamicLimiter {
    /// A limiter seeded at the group's documented base TPS.
    pub fn new(group: RateLimitGroup, cfg: AimdConfig) -> Self {
        let tps = group.base_tps() as f64;
        Self {
            group,
            cfg,
            limiter: ArcSwap::from_pointee(build_limiter(tps)),
            state: Mutex::new(ControllerState::new(tps)),
        }
    }

    /// Await a permit (proactive shaping).
    pub async fn until_ready(&self) {
        let limiter = self.limiter.load_full();
        limiter.until_ready().await;
    }

    /// Try to acquire a permit without waiting.
    pub fn check(&self) -> bool {
        self.limiter.load().check().is_ok()
    }

    /// The current effective rate.
    pub fn effective_tps(&self) -> f64 {
        self.state.lock().unwrap().effective_tps
    }

    /// The park deadline, if the group is currently parked.
    pub fn parked_until(&self) -> Option<Instant> {
        let until = self.state.lock().unwrap().parked_until;
        until.filter(|t| *t > Instant::now())
    }

    /// Feed an outcome to the controller, rebuilding the limiter if the rate changed.
    pub fn record(&self, feedback: Feedback, now: Instant, kst_time: NaiveTime) {
        let ceiling = self.group.effective_tps(kst_time) as f64;
        let changed = {
            let mut s = self.state.lock().unwrap();
            apply_feedback(&mut s, &feedback, ceiling, now, &self.cfg)
        };
        if let Some(tps) = changed {
            self.limiter.store(Arc::new(build_limiter(tps)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> AimdConfig {
        AimdConfig::default()
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn rate_limited_decreases_multiplicatively() {
        let mut s = ControllerState::new(6.0);
        let now = Instant::now();
        let new = apply_feedback(
            &mut s,
            &Feedback::RateLimited {
                retry_after: None,
                headers: Default::default(),
            },
            6.0,
            now,
            &cfg(),
        );
        assert!(approx(new.unwrap(), 3.6)); // 6 * 0.6
        assert!(approx(s.effective_tps, 3.6));
    }

    #[test]
    fn retry_after_parks_the_group() {
        let mut s = ControllerState::new(6.0);
        let now = Instant::now();
        apply_feedback(
            &mut s,
            &Feedback::RateLimited {
                retry_after: Some(Duration::from_secs(2)),
                headers: Default::default(),
            },
            6.0,
            now,
            &cfg(),
        );
        assert!(s.parked_until.is_some());
        assert!(s.parked_until.unwrap() > now);
    }

    #[test]
    fn decrease_respects_cooldown() {
        let mut s = ControllerState::new(6.0);
        let t0 = Instant::now();
        apply_feedback(
            &mut s,
            &Feedback::RateLimited {
                retry_after: None,
                headers: Default::default(),
            },
            6.0,
            t0,
            &cfg(),
        );
        let after_first = s.effective_tps;
        // A second 429 immediately (within cooldown) must not decrease again.
        let again = apply_feedback(
            &mut s,
            &Feedback::RateLimited {
                retry_after: None,
                headers: Default::default(),
            },
            6.0,
            t0 + Duration::from_millis(100),
            &cfg(),
        );
        assert_eq!(again, None);
        assert_eq!(s.effective_tps, after_first);
    }

    #[test]
    fn success_recovers_additively_up_to_ceiling() {
        let mut s = ControllerState::new(3.0);
        let mut t = Instant::now();
        // Each success after the cooldown adds increase_step, capped at the ceiling (6).
        for _ in 0..10 {
            apply_feedback(
                &mut s,
                &Feedback::Success {
                    headers: Default::default(),
                },
                6.0,
                t,
                &cfg(),
            );
            t += Duration::from_secs(3); // exceed increase_cooldown
        }
        assert_eq!(s.effective_tps, 6.0); // never exceeds the documented ceiling
    }

    #[test]
    fn header_limit_is_authoritative() {
        let mut s = ControllerState::new(6.0);
        let headers = RateLimitHeaders {
            limit: Some(2),
            ..Default::default()
        };
        let new = apply_feedback(
            &mut s,
            &Feedback::Success { headers },
            6.0,
            Instant::now(),
            &cfg(),
        );
        assert_eq!(new, Some(2.0)); // server said 2 → adopt it, even on success
    }

    #[test]
    fn peak_window_ceiling_drop_applies_on_success() {
        let mut s = ControllerState::new(6.0);
        // Ceiling drops to 3 (peak window); a success must clamp the rate down to it.
        let new = apply_feedback(
            &mut s,
            &Feedback::Success {
                headers: Default::default(),
            },
            3.0,
            Instant::now(),
            &cfg(),
        );
        assert_eq!(new, Some(3.0));
        assert_eq!(s.effective_tps, 3.0);
    }

    #[test]
    fn never_drops_below_floor() {
        let mut s = ControllerState::new(1.2);
        let mut t = Instant::now();
        for _ in 0..10 {
            apply_feedback(
                &mut s,
                &Feedback::RateLimited {
                    retry_after: None,
                    headers: Default::default(),
                },
                6.0,
                t,
                &cfg(),
            );
            t += Duration::from_secs(1); // exceed decrease cooldown each time
        }
        assert!(s.effective_tps >= cfg().floor_tps);
        assert_eq!(s.effective_tps, 1.0);
    }

    #[test]
    fn timeout_decreases_softly() {
        let mut s = ControllerState::new(10.0);
        let new = apply_feedback(&mut s, &Feedback::Timeout, 10.0, Instant::now(), &cfg());
        assert!(approx(new.unwrap(), 8.0)); // 10 * 0.8
    }
}
