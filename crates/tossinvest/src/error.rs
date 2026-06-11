//! Error types and the control-flow classification of API errors.

use std::time::Duration;
use tossinvest_model::{ErrorCode, RequestId};

/// A low-level transport failure (before any HTTP response was interpreted).
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// The request timed out.
    #[error("request timed out")]
    Timeout,
    /// A connection-level failure.
    #[error("connection error: {0}")]
    Connect(String),
    /// Any other HTTP/transport error.
    #[error("transport error: {0}")]
    Other(String),
    /// The base URL or path could not be resolved.
    #[error("invalid url: {0}")]
    Url(String),
}

impl TransportError {
    /// Whether retrying the request might succeed.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Timeout | Self::Connect(_))
    }
}

/// A control-flow classification of an API error, derived from the HTTP status and code.
/// Callers branch on this small set; the raw [`ErrorCode`] and request id are still carried.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApiErrorKind {
    /// Rate limited (429). Honor `retry_after` if present.
    RateLimited {
        /// Suggested wait before retrying.
        retry_after: Option<Duration>,
    },
    /// The access token is invalid/expired (401) — refresh and retry.
    AuthExpired,
    /// The target order is already in a terminal state (`already-filled/canceled/...`).
    AlreadyTerminal,
    /// A cancel/modify is already in flight (`already-processing`, `request-in-progress`).
    OperationInProgress,
    /// Outside order-acceptance hours.
    OrderHoursClosed,
    /// The operation is blocked by an instrument/account rule (`*-restricted`).
    Restricted,
    /// Insufficient buying power.
    InsufficientFunds,
    /// The resource was not found (404).
    NotFound,
    /// A malformed or invalid request (400 / 422 validation).
    BadRequest,
    /// A server-side error (5xx).
    ServerError,
    /// An unrecognized or uncategorized error (forward-compatible).
    Other,
}

impl ApiErrorKind {
    /// Classify an error from its HTTP status and code.
    pub fn classify(status: u16, code: &ErrorCode, retry_after: Option<Duration>) -> Self {
        use ErrorCode::*;
        match code {
            EdgeRateLimitExceeded | RateLimitExceeded => Self::RateLimited { retry_after },
            ExpiredToken | InvalidToken => Self::AuthExpired,
            AlreadyFilled | AlreadyCanceled | AlreadyModified | AlreadyRejected => {
                Self::AlreadyTerminal
            }
            AlreadyProcessing | RequestInProgress => Self::OperationInProgress,
            OrderHoursClosed => Self::OrderHoursClosed,
            ModifyRestricted | CancelRestricted | StockRestricted => Self::Restricted,
            InsufficientBuyingPower => Self::InsufficientFunds,
            StockNotFound | OrderNotFound | AccountNotFound | ExchangeRateNotFound => {
                Self::NotFound
            }
            InternalError | Maintenance => Self::ServerError,
            InvalidRequest
            | ConfirmHighValueRequired
            | AccountHeaderRequired
            | PriceOutOfRange
            | OrderTypeNotAllowed
            | InvalidTickSize
            | MaxOrderAmountExceeded => Self::BadRequest,
            _ => match status {
                404 => Self::NotFound,
                429 => Self::RateLimited { retry_after },
                500..=599 => Self::ServerError,
                400 | 422 => Self::BadRequest,
                _ => Self::Other,
            },
        }
    }
}

/// The crate's primary error type.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A transport-level failure.
    #[error("transport: {0}")]
    Transport(#[from] TransportError),

    /// A structured API error response.
    #[error("api error {status} [{code}]: {message}")]
    Api {
        /// HTTP status code.
        status: u16,
        /// The API error code.
        code: ErrorCode,
        /// Control-flow classification.
        kind: ApiErrorKind,
        /// Human-readable message.
        message: String,
        /// Request id, if present.
        request_id: Option<RequestId>,
    },

    /// An OAuth2 token-endpoint error (uses the OAuth2 format, not the API envelope).
    #[error("oauth2 error: {error}")]
    OAuth2 {
        /// The OAuth2 error code.
        error: String,
        /// Optional description.
        description: Option<String>,
    },

    /// A response body could not be decoded into the expected type.
    #[error("failed to decode response: {0}")]
    Decode(String),

    /// An argument failed validation before the request was sent.
    #[error("invalid argument `{field}`: {reason}")]
    InvalidArg {
        /// The offending field.
        field: &'static str,
        /// Why it was rejected.
        reason: String,
    },
}

impl Error {
    /// Whether retrying this request might succeed.
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Transport(t) => t.is_transient(),
            Error::Api { kind, .. } => matches!(
                kind,
                ApiErrorKind::RateLimited { .. }
                    | ApiErrorKind::OperationInProgress
                    | ApiErrorKind::ServerError
            ),
            _ => false,
        }
    }

    /// The classification, if this is an API error.
    pub fn kind(&self) -> Option<&ApiErrorKind> {
        match self {
            Error::Api { kind, .. } => Some(kind),
            _ => None,
        }
    }
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, Error>;
