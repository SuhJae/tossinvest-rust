//! Market-data responses: orderbook, prices, trades, price limits, candles.

use crate::enums::Currency;
use crate::scalar::Dec;
use crate::time::KstDateTime;
use serde::Deserialize;

/// One price level in the order book.
#[derive(Clone, Debug, Deserialize)]
pub struct OrderbookEntry {
    /// Quote price at this level.
    pub price: Dec,
    /// Resting quantity at this price.
    pub volume: Dec,
}

/// Bid/ask book. `asks` ascend (lowest first); `bids` descend (highest first).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderbookResponse {
    /// Data time; `None` when no data is available.
    #[serde(default)]
    pub timestamp: Option<KstDateTime>,
    /// Quote currency.
    pub currency: Currency,
    /// Ask levels, lowest price first.
    pub asks: Vec<OrderbookEntry>,
    /// Bid levels, highest price first.
    pub bids: Vec<OrderbookEntry>,
}

/// Current / last traded price for one symbol.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceResponse {
    /// The symbol, echoed back.
    pub symbol: String,
    /// Last-trade time; `None` if no trade has occurred.
    #[serde(default)]
    pub timestamp: Option<KstDateTime>,
    /// Current / last traded price.
    pub last_price: Dec,
    /// Price currency.
    pub currency: Currency,
}

/// One recent execution print.
#[derive(Clone, Debug, Deserialize)]
pub struct Trade {
    /// Trade price.
    pub price: Dec,
    /// Traded quantity.
    pub volume: Dec,
    /// Trade time (always present).
    pub timestamp: KstDateTime,
    /// Currency.
    pub currency: Currency,
}

/// Daily upper/lower price limits. Both are `None` for markets without limits (US).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceLimitResponse {
    /// Data time.
    pub timestamp: KstDateTime,
    /// Daily upper limit; `None` for limit-less markets.
    #[serde(default)]
    pub upper_limit_price: Option<Dec>,
    /// Daily lower limit; `None` for limit-less markets.
    #[serde(default)]
    pub lower_limit_price: Option<Dec>,
    /// Currency.
    pub currency: Currency,
}

/// One OHLCV candle. `timestamp` is the bar **open** time.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candle {
    /// Bar start time.
    pub timestamp: KstDateTime,
    /// Open price.
    pub open_price: Dec,
    /// High price.
    pub high_price: Dec,
    /// Low price.
    pub low_price: Dec,
    /// Close price.
    pub close_price: Dec,
    /// Volume.
    pub volume: Dec,
    /// Currency.
    pub currency: Currency,
}

/// A page of candles. Feed `next_before` back as the `before` cursor (older bars).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandlePageResponse {
    /// Candle list (up to 200), descending in time.
    pub candles: Vec<Candle>,
    /// Cursor for the next (older) page; `None` on the last page.
    #[serde(default)]
    pub next_before: Option<KstDateTime>,
}
