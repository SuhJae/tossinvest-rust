//! Identity keys and the delta events the store emits to subscribers.

use tossinvest_model::{OrderId, OrderStatus};

/// A stable correlation key for one logical order across its many server ids. Optimistic
/// rows start as [`OrderKey::Local`] and are re-keyed to [`OrderKey::Server`] on confirm.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum OrderKey {
    /// A pre-confirmation placeholder id minted by the store.
    Local(u64),
    /// A server-issued order id.
    Server(OrderId),
}

impl OrderKey {
    /// The server id, if this key has been confirmed.
    pub fn server_id(&self) -> Option<&OrderId> {
        match self {
            OrderKey::Server(id) => Some(id),
            OrderKey::Local(_) => None,
        }
    }
}

/// The kind of in-flight operation against an order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpKind {
    /// A cancel request.
    Cancel,
    /// A modify request.
    Modify,
}

/// A semantic change applied to the store, delivered to subscribers as a delta. Each
/// variant is a fact that already happened (the snapshot is updated *before* it is sent).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Delta {
    /// An order appeared or changed (status/fields). `key` identifies it in the snapshot.
    OrderUpserted {
        /// The order's key.
        key: OrderKey,
    },
    /// New quantity filled. Useful for animations/sounds a snapshot diff can't recover.
    FillAdded {
        /// The order's key.
        key: OrderKey,
        /// Newly filled quantity since the last observation.
        delta: String,
        /// Total filled quantity.
        cumulative: String,
    },
    /// An order reached a terminal status.
    OrderClosed {
        /// The order's key.
        key: OrderKey,
        /// The terminal status.
        status: OrderStatus,
    },
    /// A locally-submitted order is provisionally present (optimistic).
    OrderSubmitted {
        /// The local key.
        key: OrderKey,
    },
    /// A submitted order was confirmed and re-keyed to its server id.
    OrderConfirmed {
        /// The (now server) key.
        key: OrderKey,
    },
    /// A submission failed before/at the server.
    SubmitFailed {
        /// The order's key.
        key: OrderKey,
        /// Why it failed.
        message: String,
    },
    /// An in-flight cancel/modify was recorded against an order.
    OpSubmitted {
        /// The order's key.
        key: OrderKey,
        /// The operation kind.
        kind: OpKind,
    },
    /// The holdings overview changed.
    HoldingsUpdated,
    /// A symbol's price changed.
    PriceUpdated {
        /// The symbol.
        symbol: String,
    },
    /// The reconciler stopped; the snapshot is final.
    Stopped,
}
