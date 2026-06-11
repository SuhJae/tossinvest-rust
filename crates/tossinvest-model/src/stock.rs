//! Stock reference data and buy-warning information.

use crate::enum_macro::open_enum;
use crate::enums::Currency;
use crate::scalar::Dec;
use crate::time::KstDate;
use serde::Deserialize;

open_enum! {
    /// Listing market segment.
    pub enum Market {
        Kospi => "KOSPI",
        Kosdaq => "KOSDAQ",
        Nyse => "NYSE",
        Nasdaq => "NASDAQ",
        Amex => "AMEX",
        KrEtc => "KR_ETC",
        UsEtc => "US_ETC",
    }
}

open_enum! {
    /// Security type.
    pub enum SecurityType {
        Stock => "STOCK",
        ForeignStock => "FOREIGN_STOCK",
        DepositaryReceipt => "DEPOSITARY_RECEIPT",
        InfrastructureFund => "INFRASTRUCTURE_FUND",
        Reit => "REIT",
        Etf => "ETF",
        ForeignEtf => "FOREIGN_ETF",
        Etn => "ETN",
        StockWarrants => "STOCK_WARRANTS",
    }
}

open_enum! {
    /// Listing status.
    pub enum ListingStatus {
        Scheduled => "SCHEDULED",
        Active => "ACTIVE",
        Delisted => "DELISTED",
    }
}

open_enum! {
    /// Buy-warning category.
    pub enum WarningType {
        LiquidationTrading => "LIQUIDATION_TRADING",
        Overheated => "OVERHEATED",
        InvestmentWarning => "INVESTMENT_WARNING",
        InvestmentRisk => "INVESTMENT_RISK",
        ViStaticAndDynamic => "VI_STATIC_AND_DYNAMIC",
        ViStatic => "VI_STATIC",
        ViDynamic => "VI_DYNAMIC",
        StockWarrants => "STOCK_WARRANTS",
    }
}

/// KR-specific per-stock trading-status flags (`StockInfo.koreanMarketDetail`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KrMarketDetail {
    /// Liquidation/delisting-procedure trading in progress.
    pub liquidation_trading: bool,
    /// Whether the symbol is supported on the NXT ATS.
    pub nxt_supported: bool,
    /// KRX trading suspended.
    pub krx_trading_suspended: bool,
    /// NXT trading suspended; `None` when NXT is unsupported (N/A).
    #[serde(default)]
    pub nxt_trading_suspended: Option<bool>,
}

/// Reference data for one symbol.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockInfo {
    /// Symbol. KR: 6-digit numeric; US: ticker.
    pub symbol: String,
    /// Korean name.
    pub name: String,
    /// English name.
    pub english_name: String,
    /// ISIN code (ISO 6166).
    pub isin_code: String,
    /// Listing market segment.
    pub market: Market,
    /// Security type.
    pub security_type: SecurityType,
    /// `true` for common shares, `false` for preferred shares.
    pub is_common_share: bool,
    /// Listing status.
    pub status: ListingStatus,
    /// Trading currency.
    pub currency: Currency,
    /// Listing date; `None` if unavailable.
    #[serde(default)]
    pub list_date: Option<KstDate>,
    /// Delisting date; `None` for active stocks.
    #[serde(default)]
    pub delist_date: Option<KstDate>,
    /// Shares outstanding.
    pub shares_outstanding: Dec,
    /// Leverage factor for leveraged/inverse products; `None` otherwise.
    #[serde(default)]
    pub leverage_factor: Option<Dec>,
    /// KR per-stock trading-status flags; `None` for US stocks.
    #[serde(default)]
    pub korean_market_detail: Option<KrMarketDetail>,
}

/// A single buy-warning entry for a stock.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockWarning {
    /// Warning category.
    pub warning_type: WarningType,
    /// The exchange the warning applies to; `None` if not exchange-specific.
    #[serde(default)]
    pub exchange: Option<String>,
    /// Warning start date.
    #[serde(default)]
    pub start_date: Option<KstDate>,
    /// Warning end date.
    #[serde(default)]
    pub end_date: Option<KstDate>,
}
