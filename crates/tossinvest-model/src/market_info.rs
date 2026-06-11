//! FX rate and market-calendar (trading-hours) responses.
//!
//! The KR and US calendars differ structurally: KR nests three sessions under an
//! optional `integrated` wrapper; US lists four flat sessions. All session times are
//! expressed in KST, so US sessions cross midnight into the next KST date.

use crate::enums::{Currency, RateChangeType};
use crate::scalar::Dec;
use crate::time::{KstDate, KstDateTime};
use serde::Deserialize;

/// KRW↔USD reference FX rate with a validity window (~1-minute refresh).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeRateResponse {
    /// Base currency.
    pub base_currency: Currency,
    /// Quote/display currency.
    pub quote_currency: Currency,
    /// Buy rate (1 base = ? quote).
    pub rate: Dec,
    /// Interbank mid rate (매매기준율).
    pub mid_rate: Dec,
    /// Basis points of `rate` vs `mid_rate`; may be negative.
    pub basis_point: Dec,
    /// Up/flat/down movement indicator.
    pub rate_change_type: RateChangeType,
    /// Validity window start.
    pub valid_from: KstDateTime,
    /// Validity window end.
    pub valid_until: KstDateTime,
}

// ── KR calendar ────────────────────────────────────────────────────────────────

/// Three-business-day KR calendar envelope.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KrMarketCalendarResponse {
    /// The queried business day.
    pub today: KrMarketDay,
    /// The previous business day.
    pub previous_business_day: KrMarketDay,
    /// The next business day.
    pub next_business_day: KrMarketDay,
}

/// One KR business day. `integrated` is `None` on a full holiday.
#[derive(Clone, Debug, Deserialize)]
pub struct KrMarketDay {
    /// Business day (KST).
    pub date: KstDate,
    /// Integrated KRX+NXT hours; `None` when both are fully closed.
    #[serde(default)]
    pub integrated: Option<IntegratedHour>,
}

/// Integrated KRX+NXT tradable hours; each session is independently nullable.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegratedHour {
    /// Pre-market (NXT continuous trading).
    #[serde(default)]
    pub pre_market: Option<PreMarketSession>,
    /// Regular session (union of KRX and NXT).
    #[serde(default)]
    pub regular_market: Option<RegularMarketSession>,
    /// After-market (NXT).
    #[serde(default)]
    pub after_market: Option<AfterMarketSession>,
}

/// KR pre-market session (opens with a single-price auction sub-window).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreMarketSession {
    /// Session start.
    pub start_time: KstDateTime,
    /// Start of the opening single-price auction window; `None` if absent.
    #[serde(default)]
    pub single_price_auction_start_time: Option<KstDateTime>,
    /// Session end.
    pub end_time: KstDateTime,
}

/// KR regular session (includes the closing single-price auction).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegularMarketSession {
    /// Session start.
    pub start_time: KstDateTime,
    /// Start of the closing single-price auction window (KRX basis); `None` if KRX closed.
    #[serde(default)]
    pub single_price_auction_start_time: Option<KstDateTime>,
    /// Session end.
    pub end_time: KstDateTime,
}

/// KR after-market session (NXT).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AfterMarketSession {
    /// Session start.
    pub start_time: KstDateTime,
    /// End of the single-price auction sub-window; `None` if absent.
    #[serde(default)]
    pub single_price_auction_end_time: Option<KstDateTime>,
    /// Session end.
    pub end_time: KstDateTime,
}

// ── US calendar ────────────────────────────────────────────────────────────────

/// Three-business-day US calendar envelope.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsMarketCalendarResponse {
    /// The queried business day.
    pub today: UsMarketDay,
    /// The previous business day.
    pub previous_business_day: UsMarketDay,
    /// The next business day.
    pub next_business_day: UsMarketDay,
}

/// One US business day. Times are in KST (sessions cross midnight). All four sessions
/// are `None` on a full holiday.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsMarketDay {
    /// Business day (US-local).
    pub date: KstDate,
    /// Day-market session (Toss-specific).
    #[serde(default)]
    pub day_market: Option<UsSession>,
    /// Pre-market session.
    #[serde(default)]
    pub pre_market: Option<UsSession>,
    /// Regular session.
    #[serde(default)]
    pub regular_market: Option<UsSession>,
    /// After-market session.
    #[serde(default)]
    pub after_market: Option<UsSession>,
}

/// A US trading session — a plain start/end window (all four US sessions share this shape).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsSession {
    /// Session start (KST).
    pub start_time: KstDateTime,
    /// Session end (KST; may be the next calendar day).
    pub end_time: KstDateTime,
}
