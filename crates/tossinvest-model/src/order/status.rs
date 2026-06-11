//! Order status (the per-record finite-state machine) and the list-filter group label.

use crate::enum_macro::open_enum;
use serde::{Deserialize, Serialize};

open_enum! {
    /// The per-order status. This is the real state machine. Open enum — clients must
    /// tolerate unknown codes.
    ///
    /// `CANCEL_REJECTED` / `REPLACE_REJECTED` are emitted as separate sibling records
    /// while the original order reverts to its prior state; `REPLACED` means the original
    /// was superseded by a replacement under a new id. Terminal statuses may still carry
    /// partial fills — always read `execution.filled_quantity`.
    pub enum OrderStatus {
        Pending => "PENDING",
        PendingCancel => "PENDING_CANCEL",
        PendingReplace => "PENDING_REPLACE",
        PartialFilled => "PARTIAL_FILLED",
        Filled => "FILLED",
        Canceled => "CANCELED",
        Rejected => "REJECTED",
        CancelRejected => "CANCEL_REJECTED",
        ReplaceRejected => "REPLACE_REJECTED",
        Replaced => "REPLACED",
    }
}

/// The coarse lifecycle group an order falls into. Derived from [`OrderStatus`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LifecycleGroup {
    /// In-progress: `PENDING`, `PARTIAL_FILLED`, `PENDING_CANCEL`, `PENDING_REPLACE`.
    Open,
    /// Terminal / closed.
    Closed,
}

impl OrderStatus {
    /// `true` for the OPEN group (`PENDING`, `PARTIAL_FILLED`, `PENDING_CANCEL`, `PENDING_REPLACE`).
    pub fn is_open(&self) -> bool {
        matches!(
            self,
            Self::Pending | Self::PartialFilled | Self::PendingCancel | Self::PendingReplace
        )
    }

    /// `true` for terminal statuses (`FILLED`, `CANCELED`, `REJECTED`, `REPLACED`,
    /// `CANCEL_REJECTED`, `REPLACE_REJECTED`).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Filled
                | Self::Canceled
                | Self::Rejected
                | Self::Replaced
                | Self::CancelRejected
                | Self::ReplaceRejected
        )
    }

    /// The lifecycle group, or `None` for an unknown status (don't guess).
    pub fn group(&self) -> Option<LifecycleGroup> {
        if self.is_open() {
            Some(LifecycleGroup::Open)
        } else if matches!(self, Self::Unknown(_)) {
            None
        } else {
            Some(LifecycleGroup::Closed)
        }
    }
}

/// The `status` query-parameter filter for `GET /orders`. A deliberately separate type
/// from [`OrderStatus`] — it is a grouping label, not a state. `Closed` currently returns
/// `400 closed-not-supported`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum OrderListFilter {
    /// In-progress orders.
    Open,
    /// Terminal orders (currently unsupported by the API).
    Closed,
}

impl OrderListFilter {
    /// The wire value for the query parameter.
    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::Open => "OPEN",
            Self::Closed => "CLOSED",
        }
    }
}
