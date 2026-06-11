//! `tossinvest-model` — pure, runtime-free data types for the Toss Securities Open API.
//!
//! This crate has no I/O, no async runtime, and no HTTP dependency. It contains the
//! serde-(de)serializable request/response models, the domain newtypes
//! ([`Symbol`], [`AccountSeq`], [`OrderId`], [`Dec`], …), open enums that tolerate
//! unknown values, and the order-lifecycle types.
//!
//! See `DESIGN.md` at the repository root for the full data model, the order
//! finite-state machine, and the crate-family architecture.

mod enum_macro;

pub mod account;
pub mod enums;
pub mod envelope;
pub mod error;
pub mod market_data;
pub mod market_info;
pub mod newtype;
pub mod order;
pub mod scalar;
pub mod stock;
pub mod time;

pub use account::{
    Account, AccountType, BuyingPowerResponse, Commission, Cost, CurrencyBucket, DailyProfitLoss,
    HoldingsItem, HoldingsOverview, MarketValue, OverviewDailyProfitLoss, OverviewMarketValue,
    OverviewProfitLoss, ProfitLoss, SellableQuantityResponse,
};
pub use enums::{Currency, MarketCountry, OrderType, RateChangeType, Side, TimeInForce};
pub use envelope::{ApiResponse, ErrorResponse};
pub use error::{ApiError, ErrorCode, ErrorData};
pub use market_data::{
    Candle, CandlePageResponse, OrderbookEntry, OrderbookResponse, PriceLimitResponse,
    PriceResponse, Trade,
};
pub use market_info::{
    AfterMarketSession, ExchangeRateResponse, IntegratedHour, KrMarketCalendarResponse,
    KrMarketDay, PreMarketSession, RegularMarketSession, UsMarketCalendarResponse, UsMarketDay,
    UsSession,
};
pub use newtype::{AccountSeq, ClientOrderId, Cursor, IsinCode, OrderId, RequestId, Symbol};
pub use order::{
    CreateTimeInForce, LifecycleGroup, Order, OrderCreateAmountBased, OrderCreateQuantityBased,
    OrderCreateRequest, OrderExecution, OrderListFilter, OrderModifyRequest,
    OrderOperationResponse, OrderResponse, OrderStatus, PaginatedOrderResponse,
};
pub use scalar::{Dec, IntQty, Percent, Ratio, ValidationError};
pub use stock::{
    KrMarketDetail, ListingStatus, Market, SecurityType, StockInfo, StockWarning, WarningType,
};
pub use time::{KstDate, KstDateTime};
