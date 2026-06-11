//! The API error envelope payload and the full error-code taxonomy.

use crate::enum_macro::open_enum;
use crate::newtype::RequestId;
use crate::scalar::Dec;
use serde::Deserialize;

open_enum! {
    /// A Toss Open API error code. Open enum — clients must tolerate unknown codes.
    ///
    /// Covers the documented error table plus a few codes that appear only in schema
    /// descriptions (`us-modify-quantity-not-supported`, `max-order-amount-exceeded`,
    /// `invalid-tick-size`).
    pub enum ErrorCode {
        InvalidRequest => "invalid-request",
        ConfirmHighValueRequired => "confirm-high-value-required",
        ClosedNotSupported => "closed-not-supported",
        AccountHeaderRequired => "account-header-required",
        InvalidToken => "invalid-token",
        EdgeBlocked => "edge-blocked",
        ExpiredToken => "expired-token",
        LoginUserNotFound => "login-user-not-found",
        Forbidden => "forbidden",
        StockNotFound => "stock-not-found",
        ExchangeRateNotFound => "exchange-rate-not-found",
        AccountNotFound => "account-not-found",
        OrderNotFound => "order-not-found",
        RequestInProgress => "request-in-progress",
        AlreadyFilled => "already-filled",
        AlreadyCanceled => "already-canceled",
        AlreadyModified => "already-modified",
        AlreadyRejected => "already-rejected",
        AlreadyProcessing => "already-processing",
        InsufficientBuyingPower => "insufficient-buying-power",
        OrderHoursClosed => "order-hours-closed",
        StockRestricted => "stock-restricted",
        PriceOutOfRange => "price-out-of-range",
        OppositePendingOrderExists => "opposite-pending-order-exists",
        OrderTypeNotAllowed => "order-type-not-allowed",
        PrerequisiteRequired => "prerequisite-required",
        MarketNotSupportedForStock => "market-not-supported-for-stock",
        InvestorExchangeNotIntegrated => "investor-exchange-not-integrated",
        AmountOrderOutsideRegularHours => "amount-order-outside-regular-hours",
        ModifyRestricted => "modify-restricted",
        CancelRestricted => "cancel-restricted",
        EdgeRateLimitExceeded => "edge-rate-limit-exceeded",
        RateLimitExceeded => "rate-limit-exceeded",
        InternalError => "internal-error",
        Maintenance => "maintenance",
        UsModifyQuantityNotSupported => "us-modify-quantity-not-supported",
        MaxOrderAmountExceeded => "max-order-amount-exceeded",
        InvalidTickSize => "invalid-tick-size",
    }
}

/// The error payload inside an [`ErrorResponse`](crate::envelope::ErrorResponse).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    /// Unique request id (equal to the `X-Request-Id` response header). May be absent.
    #[serde(default)]
    pub request_id: Option<RequestId>,
    /// The stable error code.
    pub code: ErrorCode,
    /// A human-readable message (may be empty).
    #[serde(default)]
    pub message: String,
    /// Per-code resolution hints; key set varies by code.
    #[serde(default)]
    pub data: Option<ErrorData>,
}

/// Resolution hints attached to some errors. All fields are optional; which appear
/// depends on the [`ErrorCode`].
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorData {
    /// The offending request field.
    #[serde(default)]
    pub field: Option<String>,
    /// The set of allowed values for `field`.
    #[serde(default)]
    pub allowed_values: Option<Vec<String>>,
    /// The correct tick size (for `invalid-tick-size` / price errors).
    #[serde(default)]
    pub tick_size: Option<Dec>,
    /// Nearest valid prices.
    #[serde(default)]
    pub nearest_prices: Option<Vec<Dec>>,
    /// Suggested wait before retrying, in seconds.
    #[serde(default)]
    pub retry_after_seconds: Option<u64>,
    /// Suggested retry time (ISO 8601), e.g. for `order-hours-closed`.
    #[serde(default)]
    pub retry_after_at: Option<String>,
}
