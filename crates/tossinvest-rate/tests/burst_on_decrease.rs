//! Regression suite for the burst-leak finding: ratcheting the rate *down* (on a 429, a soft
//! timeout, or a peak-window ceiling clamp) must NOT install a fresh full bucket that grants an
//! immediate burst at the worst moment. Black-box, public-API only.

use chrono::NaiveTime;
use std::time::Instant;
use tossinvest_rate::{AimdConfig, DynamicLimiter, Feedback, RateLimitGroup, RateLimitHeaders};

fn drain(dl: &DynamicLimiter) -> u32 {
    let mut n = 0;
    while dl.check() {
        n += 1;
        if n > 1000 {
            break;
        }
    }
    n
}

#[test]
fn fresh_build_has_minimal_burst() {
    // Limiters are built with burst = 1, so a fresh limiter grants at most one immediate token
    // (not the full round(tps) burst that would leak on every rebuild).
    let dl = DynamicLimiter::new(RateLimitGroup::Order, AimdConfig::default());
    assert_eq!(drain(&dl), 1, "fresh limiter grants a burst of exactly 1");
}

#[test]
fn hard_429_decrease_leaks_no_burst() {
    let dl = DynamicLimiter::new(RateLimitGroup::Order, AimdConfig::default());
    let _ = drain(&dl);
    assert!(!dl.check(), "bucket must be exhausted before feedback");

    // 429 at noon (Order ceiling 6, no peak window) → hard decrease 6*0.6 = 3.6.
    let noon = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
    let before = dl.effective_tps();
    dl.record(
        Feedback::RateLimited {
            retry_after: None,
            headers: RateLimitHeaders::default(),
        },
        Instant::now(),
        noon,
    );
    let after = dl.effective_tps();
    assert!(
        after < before,
        "429 must ratchet DOWN ({before} -> {after})"
    );

    // At the moment of backoff, the decrease must grant ZERO immediate requests.
    assert_eq!(drain(&dl), 0, "decrease must not leak a fresh burst");
}

#[test]
fn soft_timeout_decrease_leaks_no_burst() {
    let dl = DynamicLimiter::new(RateLimitGroup::Order, AimdConfig::default());
    let _ = drain(&dl);
    assert!(!dl.check(), "exhausted");

    let noon = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
    let before = dl.effective_tps();
    dl.record(Feedback::Timeout, Instant::now(), noon);
    let after = dl.effective_tps();
    assert!(
        after < before,
        "timeout must decrease ({before} -> {after})"
    );

    assert_eq!(drain(&dl), 0, "soft-signal decrease must not leak a burst");
}

#[test]
fn increase_keeps_its_token() {
    // Recovery (an increase) is allowed to keep its fresh token — only decreases drain.
    let dl = DynamicLimiter::new(RateLimitGroup::Stock, AimdConfig::default());
    // Throttle down first so a later success produces an increase rebuild.
    let noon = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
    dl.record(
        Feedback::RateLimited {
            retry_after: None,
            headers: RateLimitHeaders::default(),
        },
        Instant::now(),
        noon,
    );
    let lowered = dl.effective_tps();
    // A success with an authoritative higher limit rebuilds upward.
    dl.record(
        Feedback::Success {
            headers: RateLimitHeaders {
                limit: Some(5),
                ..Default::default()
            },
        },
        Instant::now(),
        noon,
    );
    assert!(dl.effective_tps() > lowered, "should recover upward");
    assert_eq!(drain(&dl), 1, "an increase keeps its fresh token");
}
