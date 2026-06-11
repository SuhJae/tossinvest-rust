//! Account, holdings, and the order-support figures (buying power, sellable, commission).

use crate::enum_macro::open_enum;
use crate::enums::{Currency, MarketCountry};
use crate::newtype::AccountSeq;
use crate::scalar::{Dec, Ratio};
use crate::time::KstDate;
use serde::Deserialize;

open_enum! {
    /// Account type. Only `BROKERAGE` is currently returned.
    pub enum AccountType {
        Brokerage => "BROKERAGE",
        OverseasDerivatives => "OVERSEAS_DERIVATIVES",
        PensionSavings => "PENSION_SAVINGS",
        ReshoringInvestment => "RESHORING_INVESTMENT",
    }
}

/// A brokerage account.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// Human-facing account number (preserve leading zeros — keep as string).
    pub account_no: String,
    /// The account identity key — the `X-Tossinvest-Account` header value.
    pub account_seq: AccountSeq,
    /// Account type.
    pub account_type: AccountType,
}

/// Per-currency money bucket (the spec's `Price`). No cross-currency FX summation:
/// each field is the sum of holdings traded in that currency.
#[derive(Clone, Debug, Deserialize)]
pub struct CurrencyBucket {
    /// Sum of KRW-traded holdings; `"0"` if none.
    pub krw: Dec,
    /// Sum of USD-traded holdings; `None` if no overseas holdings.
    #[serde(default)]
    pub usd: Option<Dec>,
}

impl CurrencyBucket {
    /// The amount in the given currency, if present. Never treats a missing USD as zero.
    pub fn get(&self, currency: &Currency) -> Option<Dec> {
        match currency {
            Currency::Krw => Some(self.krw),
            Currency::Usd => self.usd,
            _ => None,
        }
    }
}

// ── per-holding figures (single-currency) ───────────────────────────────────────

/// Per-holding market valuation, in the holding's trading currency.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketValue {
    /// Cost basis (qty × average buy price).
    pub purchase_amount: Dec,
    /// Market valuation (gross).
    pub amount: Dec,
    /// Valuation after tax/commission.
    pub amount_after_cost: Dec,
}

/// Per-holding profit/loss, in the holding's trading currency.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitLoss {
    /// P/L amount (gross).
    pub amount: Dec,
    /// P/L amount after tax/commission.
    pub amount_after_cost: Dec,
    /// P/L rate as a ratio (`0.1077` = 10.77%).
    pub rate: Ratio,
    /// After-cost P/L rate as a ratio.
    pub rate_after_cost: Ratio,
}

/// Per-holding same-day profit/loss (no after-cost variant).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyProfitLoss {
    /// Daily P/L amount.
    pub amount: Dec,
    /// Daily P/L rate as a ratio.
    pub rate: Ratio,
}

/// Per-holding trading costs, in the holding's trading currency.
#[derive(Clone, Debug, Deserialize)]
pub struct Cost {
    /// Commission.
    pub commission: Dec,
    /// Tax; `None` when no tax applies (typically US holdings).
    #[serde(default)]
    pub tax: Option<Dec>,
}

/// One held position. All numeric fields are in the position's own `currency`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingsItem {
    /// Symbol.
    pub symbol: String,
    /// Security name.
    pub name: String,
    /// Market country.
    pub market_country: MarketCountry,
    /// Trading currency — the unit of every numeric field here.
    pub currency: Currency,
    /// Held quantity (decimal-capable; fractional US shares).
    pub quantity: Dec,
    /// Current price.
    pub last_price: Dec,
    /// Average purchase price.
    pub average_purchase_price: Dec,
    /// Market valuation.
    pub market_value: MarketValue,
    /// Profit/loss.
    pub profit_loss: ProfitLoss,
    /// Daily profit/loss.
    pub daily_profit_loss: DailyProfitLoss,
    /// Trading costs.
    pub cost: Cost,
}

// ── aggregate figures (currency-bucketed) ────────────────────────────────────────

/// Aggregate market valuation across the portfolio, per currency.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewMarketValue {
    /// Aggregate valuation (gross), per currency.
    pub amount: CurrencyBucket,
    /// Aggregate valuation after cost, per currency.
    pub amount_after_cost: CurrencyBucket,
}

/// Aggregate portfolio P/L. Amounts are per-currency; rates are FX-blended to KRW.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewProfitLoss {
    /// Aggregate P/L amount (gross), per currency.
    pub amount: CurrencyBucket,
    /// Aggregate P/L amount after cost, per currency.
    pub amount_after_cost: CurrencyBucket,
    /// Whole-portfolio P/L rate (KRW-converted basis), as a ratio.
    pub rate: Ratio,
    /// Whole-portfolio after-cost P/L rate (KRW-converted basis), as a ratio.
    pub rate_after_cost: Ratio,
}

/// Aggregate portfolio daily P/L. Amount per-currency; rate FX-blended to KRW.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewDailyProfitLoss {
    /// Aggregate daily P/L amount, per currency.
    pub amount: CurrencyBucket,
    /// Whole-portfolio daily P/L rate (KRW-converted basis), as a ratio.
    pub rate: Ratio,
}

/// The `GET /holdings` payload: portfolio summary plus per-holding rows.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingsOverview {
    /// Invested principal (cost basis), per currency.
    pub total_purchase_amount: CurrencyBucket,
    /// Aggregate market valuation.
    pub market_value: OverviewMarketValue,
    /// Aggregate profit/loss.
    pub profit_loss: OverviewProfitLoss,
    /// Aggregate daily profit/loss.
    pub daily_profit_loss: OverviewDailyProfitLoss,
    /// Per-holding rows (empty if no holdings).
    pub items: Vec<HoldingsItem>,
}

// ── order-support figures ────────────────────────────────────────────────────────

/// Cash-based buying power for a currency.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuyingPowerResponse {
    /// The currency this buying power is denominated in.
    pub currency: Currency,
    /// Cash-based buying power.
    pub cash_buying_power: Dec,
}

/// Sellable quantity for a symbol.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SellableQuantityResponse {
    /// Sellable quantity.
    pub sellable_quantity: Dec,
}

/// Trading commission for a market.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commission {
    /// Market country.
    pub market_country: MarketCountry,
    /// Commission rate as a ratio.
    pub commission_rate: Ratio,
    /// Effective start date.
    #[serde(default)]
    pub start_date: Option<KstDate>,
    /// Effective end date.
    #[serde(default)]
    pub end_date: Option<KstDate>,
}
