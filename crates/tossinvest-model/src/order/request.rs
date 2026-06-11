//! Order creation and modification request bodies.

use crate::enums::{OrderType, Side};
use crate::newtype::{ClientOrderId, Symbol};
use crate::scalar::{Dec, IntQty};
use serde::Serialize;

/// Time-in-force allowed at order **creation** (`OPG` is response-only / unsupported).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CreateTimeInForce {
    /// Valid for the day.
    Day,
    /// At-the-close (US `LIMIT` only → LOC).
    Cls,
}

/// A create-order request. Exactly one of the two variants: quantity-based (KR + US) or
/// amount-based (US `MARKET` only). Serialized untagged — the body is the inner object.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum OrderCreateRequest {
    /// Quantity-based order.
    QuantityBased(OrderCreateQuantityBased),
    /// Amount-based order (US `MARKET` only).
    AmountBased(OrderCreateAmountBased),
}

/// Quantity-based create body.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderCreateQuantityBased {
    /// Idempotency key (10-minute window); omitted ⇒ no idempotency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<ClientOrderId>,
    /// Symbol.
    pub symbol: Symbol,
    /// Side.
    pub side: Side,
    /// Price type.
    pub order_type: OrderType,
    /// Time-in-force; omitted ⇒ `DAY`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<CreateTimeInForce>,
    /// Order quantity (integer shares).
    pub quantity: IntQty,
    /// Limit price (required for `LIMIT`, forbidden for `MARKET`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<Dec>,
    /// Acknowledge a high-value (≥ 1억원) order.
    #[serde(skip_serializing_if = "is_false")]
    pub confirm_high_value_order: bool,
}

/// Amount-based create body (US `MARKET` only).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderCreateAmountBased {
    /// Idempotency key (10-minute window); omitted ⇒ no idempotency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<ClientOrderId>,
    /// US symbol.
    pub symbol: Symbol,
    /// Side.
    pub side: Side,
    /// Price type — always `MARKET` for amount-based orders.
    pub order_type: OrderType,
    /// Order amount in USD (the filled quantity is determined by the market price).
    pub order_amount: Dec,
    /// Acknowledge a high-value (≥ 1억원) order.
    #[serde(skip_serializing_if = "is_false")]
    pub confirm_high_value_order: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl OrderCreateRequest {
    /// A limit order (KR or US): `quantity` shares at `price`.
    pub fn limit(symbol: Symbol, side: Side, quantity: IntQty, price: Dec) -> Self {
        Self::QuantityBased(OrderCreateQuantityBased {
            client_order_id: None,
            symbol,
            side,
            order_type: OrderType::Limit,
            time_in_force: None,
            quantity,
            price: Some(price),
            confirm_high_value_order: false,
        })
    }

    /// A market order (KR or US): `quantity` shares at market.
    pub fn market(symbol: Symbol, side: Side, quantity: IntQty) -> Self {
        Self::QuantityBased(OrderCreateQuantityBased {
            client_order_id: None,
            symbol,
            side,
            order_type: OrderType::Market,
            time_in_force: None,
            quantity,
            price: None,
            confirm_high_value_order: false,
        })
    }

    /// An amount-based US market order: spend `amount` USD.
    pub fn market_amount(symbol: Symbol, side: Side, amount: Dec) -> Self {
        Self::AmountBased(OrderCreateAmountBased {
            client_order_id: None,
            symbol,
            side,
            order_type: OrderType::Market,
            order_amount: amount,
            confirm_high_value_order: false,
        })
    }

    /// Attach a client-supplied idempotency key.
    pub fn with_idempotency(mut self, id: ClientOrderId) -> Self {
        match &mut self {
            Self::QuantityBased(o) => o.client_order_id = Some(id),
            Self::AmountBased(o) => o.client_order_id = Some(id),
        }
        self
    }

    /// Set the time-in-force (quantity-based orders only; ignored for amount-based).
    pub fn with_time_in_force(mut self, tif: CreateTimeInForce) -> Self {
        if let Self::QuantityBased(o) = &mut self {
            o.time_in_force = Some(tif);
        }
        self
    }

    /// Acknowledge that this is a high-value order (≥ 1억원).
    pub fn confirm_high_value(mut self) -> Self {
        match &mut self {
            Self::QuantityBased(o) => o.confirm_high_value_order = true,
            Self::AmountBased(o) => o.confirm_high_value_order = true,
        }
        self
    }
}

/// A modify-order request. KR requires `quantity`; US forbids it (price-only).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderModifyRequest {
    /// New price type.
    pub order_type: OrderType,
    /// New quantity (KR: required; US: must be omitted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<IntQty>,
    /// New limit price (for `LIMIT`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<Dec>,
    /// Acknowledge a high-value order.
    #[serde(skip_serializing_if = "is_false")]
    pub confirm_high_value_order: bool,
}

impl OrderModifyRequest {
    /// Modify a KR order (quantity required).
    pub fn kr_limit(quantity: IntQty, price: Dec) -> Self {
        Self {
            order_type: OrderType::Limit,
            quantity: Some(quantity),
            price: Some(price),
            confirm_high_value_order: false,
        }
    }

    /// Modify a US limit order's price (no quantity).
    pub fn us_limit(price: Dec) -> Self {
        Self {
            order_type: OrderType::Limit,
            quantity: None,
            price: Some(price),
            confirm_high_value_order: false,
        }
    }

    /// Acknowledge that this is a high-value order.
    pub fn confirm_high_value(mut self) -> Self {
        self.confirm_high_value_order = true;
        self
    }
}
