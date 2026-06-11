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
    /// Multiplicative decrease factor on a hard throttle (429 without an authoritative header).
    pub decrease_factor: f64,
    /// Softer multiplicative decrease on a timeout / 5xx.
    pub soft_decrease_factor: f64,
    /// Additive TPS added per recovery step on sustained success.
    pub increase_step: f64,
    /// Absolute lower bound on the effective rate.
    pub floor_tps: f64,
    /// Minimum spacing between decreases of the *same class* (hard vs soft tracked separately).
    pub decrease_cooldown: Duration,
    /// Minimum spacing between increases (slows recovery to avoid oscillation).
    pub increase_cooldown: Duration,
    /// Fractional change in effective rate required to rebuild the limiter bucket.
    pub rebuild_threshold: f64,
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
            rebuild_threshold: 0.01,
        }
    }
}

/// Mutable controller state for one group.
#[derive(Clone, Debug)]
pub struct ControllerState {
    /// Current effective rate (requests/second) — always the authoritative value the cadence
    /// planner reads, kept in sync with the clamp even when the bucket is not yet rebuilt.
    pub effective_tps: f64,
    /// If set, requests are parked until this instant (from `Retry-After`).
    pub parked_until: Option<Instant>,
    /// The rate the governor bucket currently reflects (may lag `effective_tps` by < threshold).
    built_tps: f64,
    /// Last hard (429) decrease — independent cooldown from soft signals.
    last_hard_decrease: Option<Instant>,
    /// Last soft (timeout / 5xx) decrease — independent cooldown from hard signals.
    last_soft_decrease: Option<Instant>,
    /// Last additive increase.
    last_increase: Option<Instant>,
}

impl ControllerState {
    /// A fresh state seeded at `tps`.
    pub fn new(tps: f64) -> Self {
        Self {
            effective_tps: tps,
            parked_until: None,
            built_tps: tps,
            last_hard_decrease: None,
            last_soft_decrease: None,
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
/// (the peak-aware documented baseline) and `now`, and **always** updates
/// `state.effective_tps` to the clamped result. Returns `Some(new_tps)` if the effective rate
/// changed, else `None`.
///
/// Invariants: the result is always within `[cfg.floor_tps, ceiling]`; `Retry-After` always
/// updates `parked_until`; hard (429) and soft (timeout/5xx) decreases have independent
/// cooldowns so a hard throttle is never suppressed by a recent soft one; an authoritative
/// `X-RateLimit-Limit` is adopted *instead of* (not in addition to) the AIMD decrease; and a
/// peak-window ceiling drop is applied on any feedback, including success.
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
                // Authoritative server capacity: adopt it directly, do NOT also AIMD-decrease.
                target = (limit as f64).min(before);
            } else if cooled_down(state.last_hard_decrease, now, cfg.decrease_cooldown) {
                // Inferred undocumented throttle (no headers) → multiplicative decrease.
                target = before * cfg.decrease_factor;
                state.last_hard_decrease = Some(now);
            }
        }
        Feedback::ServerError | Feedback::Timeout => {
            if cooled_down(state.last_soft_decrease, now, cfg.decrease_cooldown) {
                target = before * cfg.soft_decrease_factor;
                state.last_soft_decrease = Some(now);
            }
        }
    }

    // The effective rate can never exceed the current (peak-aware) ceiling, nor drop below the
    // floor — applied unconditionally so a peak-window ceiling drop takes effect on any feedback.
    let new = clamp(target, floor, ceiling);
    state.effective_tps = new;
    if (new - before).abs() > 1e-9 {
        Some(new)
    } else {
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

/// Build a limiter at `tps`. Burst is fixed at 1 cell so a rebuild (e.g. when ratcheting *down*
/// on a 429) never installs a fresh full bucket that would leak a burst at the worst moment.
fn build_limiter(tps: f64) -> DefaultDirectRateLimiter {
    let tps = tps.max(0.1);
    let period = Duration::from_secs_f64(1.0 / tps);
    let quota = Quota::with_period(period)
        .expect("period is non-zero")
        .allow_burst(NonZeroU32::new(1).unwrap());
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

    /// Await a permit (proactive shaping). Enforces the peak-window ceiling *before* awaiting,
    /// so the 09:00–09:10 KST drop takes effect on the first admission in the window even with
    /// no prior feedback.
    pub async fn until_ready(&self) {
        self.enforce_ceiling(crate::now_kst());
        let limiter = self.limiter.load_full();
        limiter.until_ready().await;
    }

    /// Lower the effective rate to the current peak-aware ceiling if it currently exceeds it.
    fn enforce_ceiling(&self, kst_time: NaiveTime) {
        let ceiling = self.group.effective_tps(kst_time) as f64;
        let rebuild = {
            let mut s = self.state.lock().unwrap();
            if s.effective_tps > ceiling {
                s.effective_tps = ceiling;
                s.built_tps = ceiling;
                Some(ceiling)
            } else {
                None
            }
        };
        if let Some(tps) = rebuild {
            self.install(tps, true); // a ceiling clamp is always a decrease → drain the burst
        }
    }

    /// Install a fresh limiter at `tps`. When `is_decrease`, drain the initial burst token so
    /// ratcheting *down* never grants an immediate fresh burst (the fix for the burst-leak).
    fn install(&self, tps: f64, is_decrease: bool) {
        let limiter = build_limiter(tps);
        if is_decrease {
            let _ = limiter.check();
        }
        self.limiter.store(Arc::new(limiter));
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

    /// Feed an outcome to the controller, rebuilding the limiter if the effective rate has
    /// drifted from the bucket's built rate by more than the rebuild threshold.
    pub fn record(&self, feedback: Feedback, now: Instant, kst_time: NaiveTime) {
        let ceiling = self.group.effective_tps(kst_time) as f64;
        let rebuild = {
            let mut s = self.state.lock().unwrap();
            apply_feedback(&mut s, &feedback, ceiling, now, &self.cfg);
            let eff = s.effective_tps;
            let old_built = s.built_tps;
            if (eff - old_built).abs() > old_built.max(1.0) * self.cfg.rebuild_threshold {
                s.built_tps = eff;
                Some((eff, eff < old_built))
            } else {
                None
            }
        };
        if let Some((tps, is_decrease)) = rebuild {
            self.install(tps, is_decrease);
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
        let new = apply_feedback(
            &mut s,
            &Feedback::RateLimited {
                retry_after: None,
                headers: Default::default(),
            },
            6.0,
            Instant::now(),
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
    fn hard_decrease_respects_its_own_cooldown() {
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
        assert!(approx(s.effective_tps, after_first));
    }

    #[test]
    fn hard_429_not_suppressed_by_recent_soft_decrease() {
        // Regression for the review's HIGH finding: hard and soft decreases must have
        // independent cooldowns.
        let mut s = ControllerState::new(10.0);
        let t0 = Instant::now();
        // Soft signal first: 10 → 8.
        apply_feedback(&mut s, &Feedback::Timeout, 10.0, t0, &cfg());
        assert!(approx(s.effective_tps, 8.0));
        // Hard 429 within the soft cooldown window must still decrease: 8 → 4.8.
        let new = apply_feedback(
            &mut s,
            &Feedback::RateLimited {
                retry_after: None,
                headers: Default::default(),
            },
            10.0,
            t0 + Duration::from_millis(100),
            &cfg(),
        );
        assert!(approx(new.unwrap(), 4.8)); // 8 * 0.6 — hard takes precedence
    }

    #[test]
    fn ratelimited_with_header_adopts_not_double_penalizes() {
        // Regression for the review's MEDIUM finding: an authoritative limit on a 429 is
        // adopted directly, NOT also multiplied by decrease_factor.
        let mut s = ControllerState::new(10.0);
        let headers = RateLimitHeaders {
            limit: Some(8),
            ..Default::default()
        };
        let new = apply_feedback(
            &mut s,
            &Feedback::RateLimited {
                retry_after: None,
                headers,
            },
            10.0,
            Instant::now(),
            &cfg(),
        );
        assert!(approx(new.unwrap(), 8.0)); // adopt 8, not 4.8
    }

    #[test]
    fn success_recovers_additively_up_to_ceiling() {
        let mut s = ControllerState::new(3.0);
        let mut t = Instant::now();
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
            t += Duration::from_secs(3);
        }
        assert!(approx(s.effective_tps, 6.0)); // never exceeds the documented ceiling
    }

    #[test]
    fn header_limit_is_authoritative_on_success() {
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
        assert!(approx(new.unwrap(), 2.0));
    }

    #[test]
    fn peak_window_ceiling_drop_applies_on_success() {
        let mut s = ControllerState::new(6.0);
        let new = apply_feedback(
            &mut s,
            &Feedback::Success {
                headers: Default::default(),
            },
            3.0,
            Instant::now(),
            &cfg(),
        );
        assert!(approx(new.unwrap(), 3.0));
        assert!(approx(s.effective_tps, 3.0));
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
            t += Duration::from_secs(1);
        }
        assert!(s.effective_tps >= cfg().floor_tps);
        assert!(approx(s.effective_tps, 1.0));
    }

    #[test]
    fn timeout_decreases_softly() {
        let mut s = ControllerState::new(10.0);
        let new = apply_feedback(&mut s, &Feedback::Timeout, 10.0, Instant::now(), &cfg());
        assert!(approx(new.unwrap(), 8.0)); // 10 * 0.8
    }

    #[test]
    fn small_drifts_accumulate_in_effective_tps() {
        // effective_tps is always accurate even when the bucket is not rebuilt — the cadence
        // planner must never read a stale rate.
        let mut s = ControllerState::new(5.0);
        let new = apply_feedback(
            &mut s,
            &Feedback::Success {
                headers: Default::default(),
            },
            5.05,
            Instant::now(),
            &cfg(),
        );
        // ceiling 5.05 clamps the +0.5 increase to 5.05 → a small but real change.
        assert!(approx(new.unwrap(), 5.05));
        assert!(approx(s.effective_tps, 5.05));
    }
}
