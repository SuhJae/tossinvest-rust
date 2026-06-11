//! The UI-facing order view and its flat lifecycle enum.

use super::event::{OpKind, OrderKey};
use tossinvest_model::{Currency, Order, OrderId, OrderStatus, Side};

/// A flat lifecycle classification a renderer can match on directly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    /// Optimistically submitted; no server confirmation yet.
    SubmittingPending,
    /// Working: `PENDING` or `PARTIAL_FILLED`.
    Working,
    /// A cancel/modify is in progress (`PENDING_CANCEL` / `PENDING_REPLACE`).
    Closing {
        /// Which operation is in flight.
        kind: OpKind,
    },
    /// Terminal.
    Closed {
        /// Why it closed.
        reason: ClosedReason,
    },
    /// A status the spec doesn't define (shown verbatim, still polled).
    UnknownStatus,
    /// A submission that the server rejected outright.
    SubmitRejected {
        /// Why it was rejected.
        message: String,
    },
}

/// Why an order reached a terminal state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosedReason {
    /// Fully filled.
    Filled,
    /// Canceled.
    Canceled,
    /// Rejected by the broker.
    Rejected,
    /// Superseded by a replacement.
    Replaced,
    /// A reject of a cancel/modify operation (sibling record).
    OperationRejected,
    /// Any other terminal status.
    Other,
}

/// Cross-id links discovered during reconciliation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrderLinks {
    /// For a `REPLACED` order: the working replacement's id.
    pub replaced_by: Option<OrderId>,
    /// For a replacement: the original order's id.
    pub replaces: Option<OrderId>,
}

/// The order as a UI consumes it: status, derived lifecycle, fills, and links.
#[derive(Clone, Debug)]
pub struct OrderView {
    /// Stable correlation key.
    pub key: OrderKey,
    /// Server id, once known.
    pub server_id: Option<OrderId>,
    /// Symbol.
    pub symbol: String,
    /// Side.
    pub side: Side,
    /// Raw status (verbatim, including `Unknown`).
    pub status: OrderStatus,
    /// Flat lifecycle for rendering.
    pub lifecycle: Lifecycle,
    /// Ordered quantity (string form, exact).
    pub quantity: String,
    /// Filled quantity (string form, exact) — always read from execution, not inferred.
    pub filled: String,
    /// Limit price, if any.
    pub price: Option<String>,
    /// Currency.
    pub currency: Currency,
    /// Cross-id links.
    pub links: OrderLinks,
    /// Whether this row is still optimistic (no server confirmation).
    pub optimistic: bool,
    /// Monotonic revision, bumped on each change.
    pub revision: u64,
}

impl OrderView {
    /// `true` if any quantity has filled.
    pub fn has_fills(&self) -> bool {
        self.filled.parse::<f64>().map(|f| f > 0.0).unwrap_or(false)
    }

    /// `true` if the order is in a terminal lifecycle.
    pub fn is_terminal(&self) -> bool {
        matches!(self.lifecycle, Lifecycle::Closed { .. })
    }

    /// `true` if the order is working or has a cancel/modify in flight.
    pub fn is_working(&self) -> bool {
        matches!(
            self.lifecycle,
            Lifecycle::Working | Lifecycle::Closing { .. }
        )
    }

    pub(crate) fn from_order(key: OrderKey, order: &Order) -> Self {
        let mut v = Self {
            key,
            server_id: Some(order.order_id.clone()),
            symbol: order.symbol.clone(),
            side: order.side,
            status: order.status.clone(),
            lifecycle: Lifecycle::Working,
            quantity: order.quantity.to_string(),
            filled: order.execution.filled_quantity.to_string(),
            price: order.price.as_ref().map(|p| p.to_string()),
            currency: order.currency.clone(),
            links: OrderLinks::default(),
            optimistic: false,
            revision: 0,
        };
        v.lifecycle = lifecycle_for(&v.status, false);
        v
    }

    pub(crate) fn absorb(&mut self, order: &Order) {
        self.server_id = Some(order.order_id.clone());
        self.symbol = order.symbol.clone();
        self.status = order.status.clone();
        self.quantity = order.quantity.to_string();
        self.filled = order.execution.filled_quantity.to_string();
        self.price = order.price.as_ref().map(|p| p.to_string());
        self.optimistic = false;
        self.lifecycle = lifecycle_for(&self.status, false);
    }
}

pub(crate) fn lifecycle_for(status: &OrderStatus, optimistic: bool) -> Lifecycle {
    if optimistic {
        return Lifecycle::SubmittingPending;
    }
    match status {
        OrderStatus::Pending | OrderStatus::PartialFilled => Lifecycle::Working,
        OrderStatus::PendingCancel => Lifecycle::Closing {
            kind: OpKind::Cancel,
        },
        OrderStatus::PendingReplace => Lifecycle::Closing {
            kind: OpKind::Modify,
        },
        OrderStatus::Filled => Lifecycle::Closed {
            reason: ClosedReason::Filled,
        },
        OrderStatus::Canceled => Lifecycle::Closed {
            reason: ClosedReason::Canceled,
        },
        OrderStatus::Rejected => Lifecycle::Closed {
            reason: ClosedReason::Rejected,
        },
        OrderStatus::Replaced => Lifecycle::Closed {
            reason: ClosedReason::Replaced,
        },
        OrderStatus::CancelRejected | OrderStatus::ReplaceRejected => Lifecycle::Closed {
            reason: ClosedReason::OperationRejected,
        },
        OrderStatus::Unknown(_) => Lifecycle::UnknownStatus,
    }
}

pub(crate) fn closed_reason(status: &OrderStatus) -> ClosedReason {
    match lifecycle_for(status, false) {
        Lifecycle::Closed { reason } => reason,
        _ => ClosedReason::Other,
    }
}
