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

pub mod enums;
pub mod envelope;
pub mod error;
pub mod newtype;
pub mod scalar;

pub use enums::{Currency, MarketCountry, OrderType, RateChangeType, Side, TimeInForce};
pub use envelope::{ApiResponse, ErrorResponse};
pub use error::{ApiError, ErrorCode, ErrorData};
pub use newtype::{AccountSeq, ClientOrderId, Cursor, IsinCode, OrderId, RequestId, Symbol};
pub use scalar::{Dec, IntQty, Percent, Ratio, ValidationError};
