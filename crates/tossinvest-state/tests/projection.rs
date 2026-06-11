//! Replay tests for the pure projection fold — no network, no clock.

use tossinvest_model::{
    Currency, Order, OrderExecution, OrderId, OrderStatus, OrderType, Side, TimeInForce,
};
use tossinvest_state::core::{
    ClosedReason, Delta, Lifecycle, OrderKey, ProjectedState, state::OptimisticOrder,
};

fn dec(s: &str) -> tossinvest_model::Dec {
    tossinvest_model::Dec(s.parse().unwrap())
}

/// Build an order with a given id/status/filled for tests.
fn order(id: &str, status: OrderStatus, qty: &str, filled: &str) -> Order {
    Order {
        order_id: OrderId(id.to_owned()),
        symbol: "005930".to_owned(),
        side: Side::Buy,
        order_type: OrderType::Limit,
        time_in_force: TimeInForce::Day,
        status,
        price: Some(dec("70000")),
        quantity: dec(qty),
        order_amount: None,
        currency: Currency::Krw,
        ordered_at: "2026-03-28T09:30:00+09:00".parse().unwrap(),
        canceled_at: None,
        execution: OrderExecution {
            filled_quantity: dec(filled),
            average_filled_price: None,
            filled_amount: None,
            commission: None,
            tax: None,
            filled_at: None,
            settlement_date: None,
        },
    }
}

#[test]
fn new_open_order_then_partial_fill_then_filled() {
    let mut s = ProjectedState::new();

    let d = s.observe_order(order("A", OrderStatus::Pending, "10", "0"));
    assert!(matches!(d.as_slice(), [Delta::OrderUpserted { .. }]));
    let key = OrderKey::Server(OrderId("A".to_owned()));
    assert_eq!(s.orders[&key].lifecycle, Lifecycle::Working);

    // Partial fill emits a FillAdded with the delta.
    let d = s.observe_order(order("A", OrderStatus::PartialFilled, "10", "3"));
    assert!(
        d.iter()
            .any(|x| matches!(x, Delta::FillAdded { delta, .. } if delta == "3"))
    );
    assert_eq!(s.orders[&key].filled, "3");

    // Second partial: delta is the increment only.
    let d = s.observe_order(order("A", OrderStatus::PartialFilled, "10", "5"));
    assert!(d.iter().any(|x| matches!(x, Delta::FillAdded { delta, cumulative, .. } if delta == "2" && cumulative == "5")));

    // Full fill closes it.
    let d = s.observe_order(order("A", OrderStatus::Filled, "10", "10"));
    assert!(d.iter().any(|x| matches!(x, Delta::OrderClosed { .. })));
    assert!(s.orders[&key].is_terminal());
}

#[test]
fn terminal_latch_ignores_late_observation() {
    let mut s = ProjectedState::new();
    s.observe_order(order("A", OrderStatus::Filled, "10", "10"));
    let gen0 = s.generation;
    // A stale sweep re-reports it as working — must be ignored.
    let d = s.observe_order(order("A", OrderStatus::Pending, "10", "0"));
    assert!(d.is_empty());
    assert_eq!(s.generation, gen0);
    let key = OrderKey::Server(OrderId("A".to_owned()));
    assert!(s.orders[&key].is_terminal());
}

#[test]
fn canceled_with_partial_fill_is_orthogonal() {
    let mut s = ProjectedState::new();
    s.observe_order(order("A", OrderStatus::PartialFilled, "10", "3"));
    let d = s.observe_order(order("A", OrderStatus::Canceled, "10", "3"));
    assert!(d.iter().any(|x| matches!(x, Delta::OrderClosed { .. })));
    let key = OrderKey::Server(OrderId("A".to_owned()));
    let v = &s.orders[&key];
    assert!(v.is_terminal());
    assert!(v.has_fills()); // partial fill survives the cancel
    assert_eq!(s.closed_reason_of(&key), Some(ClosedReason::Canceled));
}

#[test]
fn duplicate_observation_is_noop() {
    let mut s = ProjectedState::new();
    s.observe_order(order("A", OrderStatus::Pending, "10", "0"));
    let gen0 = s.generation;
    let d = s.observe_order(order("A", OrderStatus::Pending, "10", "0"));
    assert!(d.is_empty());
    assert_eq!(s.generation, gen0);
}

#[test]
fn optimistic_submit_then_confirm_rekeys() {
    let mut s = ProjectedState::new();
    let (local, d) = s.submit_optimistic(
        1,
        OptimisticOrder {
            symbol: "005930".to_owned(),
            side: Side::Buy,
            quantity: "10".to_owned(),
            price: Some("70000".to_owned()),
            currency: Currency::Krw,
        },
    );
    assert!(matches!(d.as_slice(), [Delta::OrderSubmitted { .. }]));
    assert!(matches!(local, OrderKey::Local(1)));
    assert_eq!(s.orders[&local].lifecycle, Lifecycle::SubmittingPending);
    assert!(s.orders[&local].optimistic);

    // Confirm re-keys local → server.
    let d = s.confirm(1, OrderId("A".to_owned()));
    assert!(matches!(d.as_slice(), [Delta::OrderConfirmed { .. }]));
    assert!(!s.orders.contains_key(&local));
    let server = OrderKey::Server(OrderId("A".to_owned()));
    assert!(s.orders.contains_key(&server));
    assert!(!s.orders[&server].optimistic);

    // A subsequent observe updates the now-server-keyed order in place.
    let d = s.observe_order(order("A", OrderStatus::Filled, "10", "10"));
    assert!(d.iter().any(|x| matches!(x, Delta::OrderClosed { .. })));
    assert_eq!(s.orders.len(), 1); // no duplicate row
}

#[test]
fn confirm_collapses_when_sweep_arrived_first() {
    let mut s = ProjectedState::new();
    // Sweep observed the server order before our create response returned.
    s.observe_order(order("A", OrderStatus::Pending, "10", "0"));
    let (local, _) = s.submit_optimistic(
        1,
        OptimisticOrder {
            symbol: "005930".to_owned(),
            side: Side::Buy,
            quantity: "10".to_owned(),
            price: None,
            currency: Currency::Krw,
        },
    );
    // Confirm collapses the optimistic row into the already-tracked server one.
    s.confirm(1, OrderId("A".to_owned()));
    assert!(!s.orders.contains_key(&local));
    assert_eq!(s.orders.len(), 1);
}

#[test]
fn cancel_rejected_sibling_is_separate_record() {
    let mut s = ProjectedState::new();
    // Original working order.
    s.observe_order(order("A", OrderStatus::Pending, "10", "0"));
    // The broker rejects a cancel: a separate sibling record appears (terminal),
    // and the original reverts/stays open (observed by the next sweep).
    let d = s.observe_order(order("B", OrderStatus::CancelRejected, "10", "0"));
    assert!(d.iter().any(|x| matches!(x, Delta::OrderClosed { .. })));
    let orig = OrderKey::Server(OrderId("A".to_owned()));
    let sib = OrderKey::Server(OrderId("B".to_owned()));
    assert!(!s.orders[&orig].is_terminal()); // original still open
    assert_eq!(
        s.closed_reason_of(&sib),
        Some(ClosedReason::OperationRejected)
    );
}

#[test]
fn unknown_status_is_tolerated_and_not_terminal() {
    let mut s = ProjectedState::new();
    let d = s.observe_order(order(
        "A",
        OrderStatus::Unknown("PENDING_SETTLEMENT".to_owned()),
        "10",
        "0",
    ));
    assert!(matches!(d.as_slice(), [Delta::OrderUpserted { .. }]));
    let key = OrderKey::Server(OrderId("A".to_owned()));
    assert_eq!(s.orders[&key].lifecycle, Lifecycle::UnknownStatus);
    assert!(!s.orders[&key].is_terminal());
    // It stays in the open-id set (still polled).
    assert!(s.open_server_ids().contains(&OrderId("A".to_owned())));
}

#[test]
fn open_server_ids_excludes_terminal_and_optimistic() {
    let mut s = ProjectedState::new();
    s.observe_order(order("A", OrderStatus::Pending, "10", "0"));
    s.observe_order(order("B", OrderStatus::Filled, "10", "10"));
    s.submit_optimistic(
        1,
        OptimisticOrder {
            symbol: "005930".to_owned(),
            side: Side::Buy,
            quantity: "1".to_owned(),
            price: None,
            currency: Currency::Krw,
        },
    );
    let open = s.open_server_ids();
    assert_eq!(open.len(), 1);
    assert!(open.contains(&OrderId("A".to_owned())));
}

#[test]
fn stop_is_idempotent() {
    let mut s = ProjectedState::new();
    let d = s.stop();
    assert!(matches!(d.as_slice(), [Delta::Stopped]));
    assert!(s.stopped);
    assert!(s.stop().is_empty()); // second stop is a no-op
}
