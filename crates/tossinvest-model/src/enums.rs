//! Shared enums used across multiple domains.
//!
//! Enums that the spec marks "tolerate unknown values" are modeled with the internal
//! `open_enum!` macro so unrecognized wire values round-trip verbatim.

use crate::enum_macro::open_enum;
use serde::{Deserialize, Serialize};

open_enum! {
    /// Currency code. Open enum — tolerate unknown values.
    pub enum Currency {
        Krw => "KRW",
        Usd => "USD",
    }
}

open_enum! {
    /// Market country. Open enum — tolerate unknown values.
    pub enum MarketCountry {
        Kr => "KR",
        Us => "US",
    }
}

open_enum! {
    /// Order price type. Open enum — tolerate unknown values.
    pub enum OrderType {
        Limit => "LIMIT",
        Market => "MARKET",
    }
}

open_enum! {
    /// Time-in-force. Open enum. `OPG` (at-the-open) is response-only and currently
    /// unsupported for order creation.
    pub enum TimeInForce {
        Day => "DAY",
        Cls => "CLS",
        Opg => "OPG",
    }
}

open_enum! {
    /// FX rate change direction.
    pub enum RateChangeType {
        Up => "UP",
        Equal => "EQUAL",
        Down => "DOWN",
    }
}

/// Order side. A closed enum — the API only ever uses `BUY` / `SELL`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Side {
    /// Buy / long.
    Buy,
    /// Sell / short-close.
    Sell,
}
