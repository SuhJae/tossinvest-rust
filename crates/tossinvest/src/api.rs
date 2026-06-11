//! The typed endpoint surface: one method per operation, grouped by handle.

use crate::client::{AccountClient, TossClient};
use crate::error::Result;
use crate::transport::RawRequest;
use tossinvest_model::{
    Account, BuyingPowerResponse, CandlePageResponse, Commission, Currency, ExchangeRateResponse,
    HoldingsOverview, KrMarketCalendarResponse, Order, OrderCreateRequest, OrderId,
    OrderListFilter, OrderModifyRequest, OrderOperationResponse, OrderResponse, OrderbookResponse,
    PaginatedOrderResponse, PriceLimitResponse, PriceResponse, SellableQuantityResponse, StockInfo,
    StockWarning, Trade, UsMarketCalendarResponse,
};
use tossinvest_rate::RateLimitGroup;

/// Candle interval.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CandleInterval {
    /// One-minute bars.
    OneMinute,
    /// Daily bars.
    OneDay,
}

impl CandleInterval {
    fn as_wire(self) -> &'static str {
        match self {
            Self::OneMinute => "1m",
            Self::OneDay => "1d",
        }
    }
}

// ── account-agnostic endpoints (market data, stock info, market info, accounts) ──────
impl TossClient {
    /// Current/last prices for one or more symbols (`GET /api/v1/prices`).
    pub async fn prices(&self, symbols: &[&str]) -> Result<Vec<PriceResponse>> {
        let req = RawRequest::get("/api/v1/prices", RateLimitGroup::MarketData)
            .query("symbols", symbols.join(","));
        self.call(req).await
    }

    /// The order book for a symbol (`GET /api/v1/orderbook`).
    pub async fn orderbook(&self, symbol: &str) -> Result<OrderbookResponse> {
        let req = RawRequest::get("/api/v1/orderbook", RateLimitGroup::MarketData)
            .query("symbol", symbol);
        self.call(req).await
    }

    /// Recent trades for a symbol (`GET /api/v1/trades`).
    pub async fn trades(&self, symbol: &str, count: Option<u32>) -> Result<Vec<Trade>> {
        let req = RawRequest::get("/api/v1/trades", RateLimitGroup::MarketData)
            .query("symbol", symbol)
            .query_opt("count", count.map(|c| c.to_string()));
        self.call(req).await
    }

    /// Daily price limits for a symbol (`GET /api/v1/price-limits`).
    pub async fn price_limits(&self, symbol: &str) -> Result<PriceLimitResponse> {
        let req = RawRequest::get("/api/v1/price-limits", RateLimitGroup::MarketData)
            .query("symbol", symbol);
        self.call(req).await
    }

    /// Candle chart data (`GET /api/v1/candles`). `before` is the pagination cursor.
    pub async fn candles(
        &self,
        symbol: &str,
        interval: CandleInterval,
        count: Option<u32>,
        before: Option<&str>,
    ) -> Result<CandlePageResponse> {
        let req = RawRequest::get("/api/v1/candles", RateLimitGroup::MarketDataChart)
            .query("symbol", symbol)
            .query("interval", interval.as_wire())
            .query_opt("count", count.map(|c| c.to_string()))
            .query_opt("before", before.map(str::to_owned));
        self.call(req).await
    }

    /// Reference data for one or more symbols (`GET /api/v1/stocks`).
    pub async fn stocks(&self, symbols: &[&str]) -> Result<Vec<StockInfo>> {
        let req = RawRequest::get("/api/v1/stocks", RateLimitGroup::Stock)
            .query("symbols", symbols.join(","));
        self.call(req).await
    }

    /// Buy-warnings for a symbol (`GET /api/v1/stocks/{symbol}/warnings`).
    pub async fn stock_warnings(&self, symbol: &str) -> Result<Vec<StockWarning>> {
        let req = RawRequest::get(
            format!("/api/v1/stocks/{symbol}/warnings"),
            RateLimitGroup::Stock,
        );
        self.call(req).await
    }

    /// KRW↔USD reference exchange rate (`GET /api/v1/exchange-rate`).
    pub async fn exchange_rate(
        &self,
        base: Currency,
        quote: Currency,
        date_time: Option<&str>,
    ) -> Result<ExchangeRateResponse> {
        let req = RawRequest::get("/api/v1/exchange-rate", RateLimitGroup::MarketInfo)
            .query("baseCurrency", base.as_wire())
            .query("quoteCurrency", quote.as_wire())
            .query_opt("dateTime", date_time.map(str::to_owned));
        self.call(req).await
    }

    /// KR market calendar (`GET /api/v1/market-calendar/KR`).
    pub async fn kr_market_calendar(&self, date: Option<&str>) -> Result<KrMarketCalendarResponse> {
        let req = RawRequest::get("/api/v1/market-calendar/KR", RateLimitGroup::MarketInfo)
            .query_opt("date", date.map(str::to_owned));
        self.call(req).await
    }

    /// US market calendar (`GET /api/v1/market-calendar/US`).
    pub async fn us_market_calendar(&self, date: Option<&str>) -> Result<UsMarketCalendarResponse> {
        let req = RawRequest::get("/api/v1/market-calendar/US", RateLimitGroup::MarketInfo)
            .query_opt("date", date.map(str::to_owned));
        self.call(req).await
    }

    /// List the authenticated user's accounts (`GET /api/v1/accounts`).
    pub async fn accounts(&self) -> Result<Vec<Account>> {
        let req = RawRequest::get("/api/v1/accounts", RateLimitGroup::Account);
        self.call(req).await
    }
}

// ── account-scoped endpoints (holdings, orders, order info) ──────────────────────────
impl AccountClient {
    /// Holdings overview (`GET /api/v1/holdings`). Optionally filter to one symbol.
    pub async fn holdings(&self, symbol: Option<&str>) -> Result<HoldingsOverview> {
        let req = RawRequest::get("/api/v1/holdings", RateLimitGroup::Asset)
            .query_opt("symbol", symbol.map(str::to_owned));
        self.call(req).await
    }

    /// Create an order (`POST /api/v1/orders`). Idempotent (and therefore retryable) only
    /// when the request carries a `client_order_id`.
    pub async fn create_order(&self, order: &OrderCreateRequest) -> Result<OrderResponse> {
        let retryable = has_client_order_id(order);
        let req = RawRequest::post("/api/v1/orders", RateLimitGroup::Order)
            .json_body(order)?
            .set_retryable(retryable);
        self.call(req).await
    }

    /// Modify an order (`POST /api/v1/orders/{id}/modify`). Returns a new order id.
    pub async fn modify_order(
        &self,
        order_id: &OrderId,
        request: &OrderModifyRequest,
    ) -> Result<OrderOperationResponse> {
        let req = RawRequest::post(
            format!("/api/v1/orders/{}/modify", order_id.as_str()),
            RateLimitGroup::Order,
        )
        .json_body(request)?;
        self.call(req).await
    }

    /// Cancel an order (`POST /api/v1/orders/{id}/cancel`). Returns a new order id.
    pub async fn cancel_order(&self, order_id: &OrderId) -> Result<OrderOperationResponse> {
        let req = RawRequest::post(
            format!("/api/v1/orders/{}/cancel", order_id.as_str()),
            RateLimitGroup::Order,
        );
        self.call(req).await
    }

    /// List orders (`GET /api/v1/orders`). `Closed` currently returns `closed-not-supported`.
    pub async fn orders(
        &self,
        filter: OrderListFilter,
        symbol: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<PaginatedOrderResponse> {
        let req = RawRequest::get("/api/v1/orders", RateLimitGroup::OrderHistory)
            .query("status", filter.as_wire())
            .query_opt("symbol", symbol.map(str::to_owned))
            .query_opt("from", from.map(str::to_owned))
            .query_opt("to", to.map(str::to_owned));
        self.call(req).await
    }

    /// Get a single order in any state (`GET /api/v1/orders/{id}`).
    pub async fn get_order(&self, order_id: &OrderId) -> Result<Order> {
        let req = RawRequest::get(
            format!("/api/v1/orders/{}", order_id.as_str()),
            RateLimitGroup::OrderHistory,
        );
        self.call(req).await
    }

    /// Cash buying power for a currency (`GET /api/v1/buying-power`).
    pub async fn buying_power(&self, currency: Currency) -> Result<BuyingPowerResponse> {
        let req = RawRequest::get("/api/v1/buying-power", RateLimitGroup::OrderInfo)
            .query("currency", currency.as_wire());
        self.call(req).await
    }

    /// Sellable quantity for a symbol (`GET /api/v1/sellable-quantity`).
    pub async fn sellable_quantity(&self, symbol: &str) -> Result<SellableQuantityResponse> {
        let req = RawRequest::get("/api/v1/sellable-quantity", RateLimitGroup::OrderInfo)
            .query("symbol", symbol);
        self.call(req).await
    }

    /// Trading commissions by market (`GET /api/v1/commissions`).
    pub async fn commissions(&self) -> Result<Vec<Commission>> {
        let req = RawRequest::get("/api/v1/commissions", RateLimitGroup::OrderInfo);
        self.call(req).await
    }
}

fn has_client_order_id(order: &OrderCreateRequest) -> bool {
    match order {
        OrderCreateRequest::QuantityBased(o) => o.client_order_id.is_some(),
        OrderCreateRequest::AmountBased(o) => o.client_order_id.is_some(),
    }
}
