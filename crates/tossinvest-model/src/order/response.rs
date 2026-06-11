//! Order response models: the order record, its execution, and operation responses.

use crate::enums::{Currency, OrderType, Side, TimeInForce};
use crate::newtype::{ClientOrderId, Cursor, OrderId};
use crate::order::status::OrderStatus;
use crate::scalar::Dec;
use crate::time::{KstDate, KstDateTime};
use rust_decimal::Decimal;
use serde::Deserialize;

/// Execution result attached to an order. Fills are present even on terminal statuses;
/// `filled_quantity` is always present (`"0"` when nothing filled).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderExecution {
    /// Filled quantity (`"0"` if none).
    pub filled_quantity: Dec,
    /// Average fill price; `None` if nothing filled.
    pub average_filled_price: Option<Dec>,
    /// Total filled amount; `None` if nothing filled.
    pub filled_amount: Option<Dec>,
    /// Total commission; `None` if not applicable.
    pub commission: Option<Dec>,
    /// Total tax; `None` if not applicable.
    pub tax: Option<Dec>,
    /// Last fill time; `None` if nothing filled.
    pub filled_at: Option<KstDateTime>,
    /// Settlement date; `None` if not yet settled.
    pub settlement_date: Option<KstDate>,
}

/// A full order record (from `GET /orders` or `GET /orders/{id}`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    /// Server order identifier.
    pub order_id: OrderId,
    /// Symbol.
    pub symbol: String,
    /// Side.
    pub side: Side,
    /// Price type.
    pub order_type: OrderType,
    /// Time-in-force (response side; superset including `OPG`).
    pub time_in_force: TimeInForce,
    /// Current status.
    pub status: OrderStatus,
    /// Order price (native currency); `None` for `MARKET`.
    #[serde(default)]
    pub price: Option<Dec>,
    /// Order quantity.
    pub quantity: Dec,
    /// Order amount in USD (amount-based US market buys only); `None` otherwise.
    #[serde(default)]
    pub order_amount: Option<Dec>,
    /// Currency.
    pub currency: Currency,
    /// Order time.
    pub ordered_at: KstDateTime,
    /// Cancel time; `None` if not canceled.
    #[serde(default)]
    pub canceled_at: Option<KstDateTime>,
    /// Execution result.
    pub execution: OrderExecution,
}

impl Order {
    /// `true` if any quantity has filled (orthogonal to status).
    pub fn has_fills(&self) -> bool {
        self.execution.filled_quantity.get() > Decimal::ZERO
    }

    /// `true` if the entire quantity has filled.
    pub fn is_fully_filled(&self) -> bool {
        self.execution.filled_quantity == self.quantity
    }
}

/// Response to a create-order request.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponse {
    /// Server-generated order id (used for modify/cancel).
    pub order_id: OrderId,
    /// The `client_order_id` from the request, echoed back; `None` if not provided.
    #[serde(default)]
    pub client_order_id: Option<ClientOrderId>,
}

/// Response to a modify/cancel request. The `order_id` is a **new** identifier, distinct
/// from the original order's id.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderOperationResponse {
    /// Newly issued order id for the operation.
    pub order_id: OrderId,
}

/// A page of orders. For `status=OPEN` the whole open set is returned in one page
/// (`next_cursor` is `None`, `has_next` is `false`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedOrderResponse {
    /// The orders on this page.
    pub orders: Vec<Order>,
    /// Cursor for the next page; `None` if none.
    #[serde(default)]
    pub next_cursor: Option<Cursor>,
    /// Whether a next page exists.
    pub has_next: bool,
}
