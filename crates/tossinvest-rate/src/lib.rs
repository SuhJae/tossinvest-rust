//! `tossinvest-rate` — per-endpoint-group rate-limiting policy for the Toss Securities Open API.
//!
//! Defines the [`RateLimitGroup`] set, the documented TPS table, the 09:00–09:10 KST
//! peak-window schedule, and a process-wide [`RateLimiterRegistry`] built on [`governor`].
//! The HTTP client's rate-limit middleware and the stateful layer's reconciler both draw
//! from the *same* registry, so all callers share one TPS budget per group.

use chrono::{NaiveTime, Timelike};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;

/// A rate-limit group. Each API endpoint belongs to exactly one group; the server limits
/// requests per `client_id × group`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RateLimitGroup {
    /// `POST /oauth2/token`.
    Auth,
    /// `GET /accounts`.
    Account,
    /// `GET /holdings`.
    Asset,
    /// `GET /stocks`, `/stocks/{symbol}/warnings`.
    Stock,
    /// `GET /exchange-rate`, `/market-calendar/*`.
    MarketInfo,
    /// `GET /orderbook`, `/prices`, `/trades`, `/price-limits`.
    MarketData,
    /// `GET /candles`.
    MarketDataChart,
    /// `POST /orders`, `/orders/{id}/modify`, `/orders/{id}/cancel`.
    Order,
    /// `GET /orders`, `/orders/{id}`.
    OrderHistory,
    /// `GET /buying-power`, `/sellable-quantity`, `/commissions`.
    OrderInfo,
}

impl RateLimitGroup {
    /// All groups, for registry construction.
    pub const ALL: [RateLimitGroup; 10] = [
        Self::Auth,
        Self::Account,
        Self::Asset,
        Self::Stock,
        Self::MarketInfo,
        Self::MarketData,
        Self::MarketDataChart,
        Self::Order,
        Self::OrderHistory,
        Self::OrderInfo,
    ];

    /// The documented baseline requests-per-second for this group.
    pub fn base_tps(self) -> u32 {
        match self {
            Self::Auth => 5,
            Self::Account => 1,
            Self::Asset => 5,
            Self::Stock => 5,
            Self::MarketInfo => 3,
            Self::MarketData => 10,
            Self::MarketDataChart => 5,
            Self::Order => 6,
            Self::OrderHistory => 5,
            Self::OrderInfo => 6,
        }
    }

    /// The reduced requests-per-second during the 09:00–09:10 KST peak window.
    /// Equal to [`Self::base_tps`] for groups without a peak window.
    pub fn peak_tps(self) -> u32 {
        match self {
            Self::Order | Self::OrderInfo => 3,
            other => other.base_tps(),
        }
    }

    /// Whether this group's limit is reduced during the morning peak window.
    pub fn has_peak_window(self) -> bool {
        matches!(self, Self::Order | Self::OrderInfo)
    }

    /// The effective TPS at a given KST wall-clock time (applies the peak window).
    pub fn effective_tps(self, kst_time: NaiveTime) -> u32 {
        if self.has_peak_window() && is_morning_peak(kst_time) {
            self.peak_tps()
        } else {
            self.base_tps()
        }
    }
}

/// `true` during the 09:00:00–09:10:00 KST morning peak window.
pub fn is_morning_peak(t: NaiveTime) -> bool {
    let secs = t.num_seconds_from_midnight();
    (9 * 3600..9 * 3600 + 10 * 60).contains(&secs)
}

/// A process-wide registry of one [`governor`] limiter per [`RateLimitGroup`], built at the
/// documented baseline TPS. All callers (HTTP middleware, reconciler) share these buckets.
#[derive(Clone)]
pub struct RateLimiterRegistry {
    limiters: Arc<HashMap<RateLimitGroup, Arc<DefaultDirectRateLimiter>>>,
}

impl std::fmt::Debug for RateLimiterRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimiterRegistry")
            .field("groups", &self.limiters.len())
            .finish()
    }
}

impl RateLimiterRegistry {
    /// Build a registry with each group seeded at its documented [`base_tps`](RateLimitGroup::base_tps).
    pub fn with_base_limits() -> Self {
        let mut limiters = HashMap::new();
        for g in RateLimitGroup::ALL {
            let tps = NonZeroU32::new(g.base_tps()).expect("base TPS is non-zero");
            let limiter = RateLimiter::direct(Quota::per_second(tps));
            limiters.insert(g, Arc::new(limiter));
        }
        Self {
            limiters: Arc::new(limiters),
        }
    }

    /// Await a permit for the given group (proactive shaping before issuing the request).
    pub async fn until_ready(&self, group: RateLimitGroup) {
        if let Some(limiter) = self.limiters.get(&group) {
            limiter.until_ready().await;
        }
    }

    /// Try to acquire a permit without waiting; `true` if one was available.
    pub fn check(&self, group: RateLimitGroup) -> bool {
        self.limiters
            .get(&group)
            .map(|l| l.check().is_ok())
            .unwrap_or(true)
    }
}

impl Default for RateLimiterRegistry {
    fn default() -> Self {
        Self::with_base_limits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tps_table_matches_spec() {
        assert_eq!(RateLimitGroup::Account.base_tps(), 1);
        assert_eq!(RateLimitGroup::MarketData.base_tps(), 10);
        assert_eq!(RateLimitGroup::Order.base_tps(), 6);
        assert_eq!(RateLimitGroup::Order.peak_tps(), 3);
        assert_eq!(RateLimitGroup::OrderInfo.peak_tps(), 3);
        // Groups without a peak window keep their base rate.
        assert_eq!(RateLimitGroup::MarketData.peak_tps(), 10);
    }

    #[test]
    fn peak_window_boundaries() {
        let at = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        assert!(!is_morning_peak(at(8, 59)));
        assert!(is_morning_peak(at(9, 0)));
        assert!(is_morning_peak(at(9, 9)));
        assert!(!is_morning_peak(at(9, 10)));
        // effective_tps reflects the window only for peak groups.
        assert_eq!(RateLimitGroup::Order.effective_tps(at(9, 5)), 3);
        assert_eq!(RateLimitGroup::Order.effective_tps(at(10, 0)), 6);
        assert_eq!(RateLimitGroup::Stock.effective_tps(at(9, 5)), 5);
    }

    #[test]
    fn registry_has_all_groups() {
        let r = RateLimiterRegistry::with_base_limits();
        for g in RateLimitGroup::ALL {
            assert!(r.check(g)); // first token available
        }
    }
}
