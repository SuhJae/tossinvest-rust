//! The projected state: a pure, replay-testable fold over observed orders and events.
//!
//! Every mutation method returns the list of [`Delta`]s it produced (for the subscriber
//! stream) and bumps [`ProjectedState::generation`]. There is no I/O here.

use super::event::{Delta, OpKind, OrderKey};
use super::view::{Lifecycle, OrderView, closed_reason, lifecycle_for};
use rust_decimal::Decimal;
use std::collections::{HashMap, HashSet};
use tossinvest_model::{HoldingsOverview, Order, OrderId, PriceResponse, Side};

/// A provisional order the UI submitted, shown before the server confirms.
#[derive(Clone, Debug)]
pub struct OptimisticOrder {
    /// Symbol.
    pub symbol: String,
    /// Side.
    pub side: Side,
    /// Quantity (string form).
    pub quantity: String,
    /// Limit price, if any.
    pub price: Option<String>,
    /// Currency.
    pub currency: tossinvest_model::Currency,
}

/// The full reconciled state the UI reads. Cloned into an `Arc` snapshot on each change.
#[derive(Clone, Debug, Default)]
pub struct ProjectedState {
    /// Tracked orders, keyed by correlation key.
    pub orders: HashMap<OrderKey, OrderView>,
    /// The latest holdings overview, if fetched.
    pub holdings: Option<HoldingsOverview>,
    /// Latest prices, by symbol.
    pub prices: HashMap<String, PriceResponse>,
    /// Monotonic generation; equals the count of applied changes.
    pub generation: u64,
    /// Whether the reconciler has stopped.
    pub stopped: bool,
    /// server-id → key routing index (private to reconciliation).
    server_index: HashMap<OrderId, OrderKey>,
}

impl ProjectedState {
    /// A fresh, empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// The server ids of all currently-open (non-terminal) tracked orders. The driver
    /// diffs this against an OPEN sweep to detect drops that need a terminal fetch.
    pub fn open_server_ids(&self) -> HashSet<OrderId> {
        self.orders
            .values()
            .filter(|v| !v.is_terminal() && !v.optimistic)
            .filter_map(|v| v.server_id.clone())
            .collect()
    }

    fn bump(&mut self, deltas: Vec<Delta>) -> Vec<Delta> {
        if !deltas.is_empty() {
            self.generation += 1;
        }
        deltas
    }

    /// Observe an order (from an OPEN sweep or a single `get_order`). Upserts the view,
    /// emits fill/transition/close deltas, and latches terminal states (late or duplicate
    /// observations of a closed order are ignored).
    pub fn observe_order(&mut self, order: Order) -> Vec<Delta> {
        let id = order.order_id.clone();
        if let Some(key) = self.server_index.get(&id).cloned() {
            let view = self.orders.get_mut(&key).expect("indexed key exists");
            if view.is_terminal() {
                return Vec::new(); // monotone terminal latch
            }
            let prev_filled = parse_dec(&view.filled);
            let prev_status = view.status.clone();
            view.absorb(&order);
            view.revision += 1;

            let mut deltas = Vec::new();
            let new_filled = order.execution.filled_quantity.get();
            if new_filled > prev_filled {
                deltas.push(Delta::FillAdded {
                    key: key.clone(),
                    delta: (new_filled - prev_filled).to_string(),
                    cumulative: new_filled.to_string(),
                });
            }
            if order.status != prev_status {
                deltas.push(Delta::OrderUpserted { key: key.clone() });
                if order.status.is_terminal() {
                    deltas.push(Delta::OrderClosed {
                        key,
                        status: order.status.clone(),
                    });
                }
            } else if deltas.is_empty() {
                // Nothing observable changed.
                return Vec::new();
            }
            self.bump(deltas)
        } else {
            // First time we see this server id.
            let key = OrderKey::Server(id.clone());
            let view = OrderView::from_order(key.clone(), &order);
            let terminal = view.is_terminal();
            self.server_index.insert(id, key.clone());
            self.orders.insert(key.clone(), view);
            let mut deltas = vec![Delta::OrderUpserted { key: key.clone() }];
            if terminal {
                deltas.push(Delta::OrderClosed {
                    key,
                    status: order.status.clone(),
                });
            }
            self.bump(deltas)
        }
    }

    /// Insert a provisional optimistic order, returning its local key.
    pub fn submit_optimistic(
        &mut self,
        local_id: u64,
        intent: OptimisticOrder,
    ) -> (OrderKey, Vec<Delta>) {
        let key = OrderKey::Local(local_id);
        let view = OrderView {
            key: key.clone(),
            server_id: None,
            symbol: intent.symbol,
            side: intent.side,
            status: tossinvest_model::OrderStatus::Pending,
            lifecycle: Lifecycle::SubmittingPending,
            quantity: intent.quantity,
            filled: "0".to_owned(),
            price: intent.price,
            currency: intent.currency,
            links: Default::default(),
            optimistic: true,
            revision: 0,
        };
        self.orders.insert(key.clone(), view);
        let deltas = vec![Delta::OrderSubmitted { key: key.clone() }];
        (key.clone(), self.bump(deltas))
    }

    /// Confirm an optimistic order: re-key it from local to its server id.
    pub fn confirm(&mut self, local_id: u64, server_id: OrderId) -> Vec<Delta> {
        let local = OrderKey::Local(local_id);
        // If the server id is already tracked (e.g. a sweep arrived first), drop the local.
        if self.server_index.contains_key(&server_id) {
            self.orders.remove(&local);
            return self.bump(vec![Delta::OrderConfirmed {
                key: OrderKey::Server(server_id),
            }]);
        }
        let Some(mut view) = self.orders.remove(&local) else {
            return Vec::new();
        };
        let key = OrderKey::Server(server_id.clone());
        view.key = key.clone();
        view.server_id = Some(server_id.clone());
        view.optimistic = false;
        view.lifecycle = lifecycle_for(&view.status, false);
        view.revision += 1;
        self.server_index.insert(server_id, key.clone());
        self.orders.insert(key.clone(), view);
        self.bump(vec![Delta::OrderConfirmed { key }])
    }

    /// Mark a submission as failed (rejected before/at the server).
    pub fn submit_failed(&mut self, key: OrderKey, message: String) -> Vec<Delta> {
        if let Some(view) = self.orders.get_mut(&key) {
            view.lifecycle = Lifecycle::SubmitRejected {
                message: message.clone(),
            };
            view.revision += 1;
        }
        self.bump(vec![Delta::SubmitFailed { key, message }])
    }

    /// Record an in-flight cancel/modify (optimistic `Closing` hint until the next observe).
    pub fn op_submitted(&mut self, server_id: &OrderId, kind: OpKind) -> Vec<Delta> {
        let Some(key) = self.server_index.get(server_id).cloned() else {
            return Vec::new();
        };
        if let Some(view) = self.orders.get_mut(&key)
            && !view.is_terminal()
        {
            view.lifecycle = Lifecycle::Closing { kind };
            view.revision += 1;
        }
        self.bump(vec![Delta::OpSubmitted { key, kind }])
    }

    /// Update the holdings overview.
    pub fn update_holdings(&mut self, overview: HoldingsOverview) -> Vec<Delta> {
        self.holdings = Some(overview);
        self.bump(vec![Delta::HoldingsUpdated])
    }

    /// Update a symbol's price.
    pub fn price_tick(&mut self, price: PriceResponse) -> Vec<Delta> {
        let symbol = price.symbol.clone();
        self.prices.insert(symbol.clone(), price);
        self.bump(vec![Delta::PriceUpdated { symbol }])
    }

    /// Mark the reconciler stopped (final snapshot).
    pub fn stop(&mut self) -> Vec<Delta> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        self.bump(vec![Delta::Stopped])
    }

    /// The closed reason for a terminal order, for convenience.
    pub fn closed_reason_of(&self, key: &OrderKey) -> Option<super::view::ClosedReason> {
        self.orders
            .get(key)
            .filter(|v| v.is_terminal())
            .map(|v| closed_reason(&v.status))
    }
}

fn parse_dec(s: &str) -> Decimal {
    s.parse::<Decimal>().unwrap_or(Decimal::ZERO)
}
