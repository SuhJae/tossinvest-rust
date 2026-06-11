//! The two top-level response envelopes. They are mutually exclusive and selected by
//! HTTP status: 2xx carries [`ApiResponse`], 4xx/5xx carries [`ErrorResponse`].

use crate::error::ApiError;
use serde::Deserialize;

/// Success envelope: `{ "result": T }`. Each endpoint specializes `T`.
#[derive(Clone, Debug, Deserialize)]
pub struct ApiResponse<T> {
    /// The success payload.
    pub result: T,
}

/// Error envelope: `{ "error": ApiError }` (4xx / 5xx responses).
#[derive(Clone, Debug, Deserialize)]
pub struct ErrorResponse {
    /// The error payload.
    pub error: ApiError,
}
