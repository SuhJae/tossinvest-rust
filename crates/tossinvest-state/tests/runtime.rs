//! Integration tests for the tokio reconciler against a wiremock server.

#![cfg(feature = "tokio")]

use serde_json::json;
use std::time::Duration;
use tossinvest::{Credentials, TossClient};
use tossinvest_state::core::Lifecycle;
use tossinvest_state::{RefreshTarget, SchedulerConfig, StateHandle};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn mock_token(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "tok", "token_type": "Bearer", "expires_in": 3600
        })))
        .mount(server)
        .await;
}

fn order_json(id: &str, status: &str, qty: &str, filled: &str) -> serde_json::Value {
    json!({
        "orderId": id, "symbol": "005930", "side": "BUY", "orderType": "LIMIT",
        "timeInForce": "DAY", "status": status, "price": "70000", "quantity": qty,
        "orderAmount": null, "currency": "KRW", "orderedAt": "2026-03-28T09:30:00+09:00",
        "canceledAt": null,
        "execution": {"filledQuantity": filled, "averageFilledPrice": null, "filledAmount": null,
            "commission": null, "tax": null, "filledAt": null, "settlementDate": null}
    })
}

fn handle(server: &MockServer) -> StateHandle {
    let client = TossClient::builder(Credentials::new("id", "secret"))
        .base_url(server.uri())
        .build()
        .unwrap();
    StateHandle::spawn(client.account(7.into()), SchedulerConfig::default())
}

#[tokio::test]
async fn refresh_pulls_open_orders_into_snapshot() {
    let server = MockServer::start().await;
    mock_token(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/orders"))
        .and(query_param("status", "OPEN"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": {"orders": [order_json("A", "PENDING", "10", "0")], "nextCursor": null, "hasNext": false}
        })))
        .mount(&server)
        .await;

    let h = handle(&server);
    // On-demand refresh resolves once the data is folded into the snapshot.
    h.refresh(RefreshTarget::Orders).await;

    let snap = h.snapshot();
    assert_eq!(snap.orders.len(), 1);
    let view = snap.orders.values().next().unwrap();
    assert_eq!(view.symbol, "005930");
    assert_eq!(view.lifecycle, Lifecycle::Working);
    h.shutdown().await;
}

#[tokio::test]
async fn drop_from_sweep_triggers_terminal_capture() {
    let server = MockServer::start().await;
    mock_token(&server).await;

    // First the order is OPEN; then it disappears from the sweep.
    Mock::given(method("GET"))
        .and(path("/api/v1/orders"))
        .and(query_param("status", "OPEN"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": {"orders": [order_json("A", "PENDING", "10", "0")], "nextCursor": null, "hasNext": false}
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/orders"))
        .and(query_param("status", "OPEN"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": {"orders": [], "nextCursor": null, "hasNext": false}
        })))
        .mount(&server)
        .await;
    // The dropped order is fetched individually and turns out FILLED.
    Mock::given(method("GET"))
        .and(path("/api/v1/orders/A"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": order_json("A", "FILLED", "10", "10")
        })))
        .mount(&server)
        .await;

    let h = handle(&server);
    h.refresh(RefreshTarget::Orders).await; // sees PENDING

    // Let the reconciler run a couple of sweep ticks to detect the drop + capture terminal.
    let mut terminal = false;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if h.snapshot().orders.values().any(|v| v.is_terminal()) {
            terminal = true;
            break;
        }
    }
    assert!(terminal, "dropped order should be captured as terminal");
    h.shutdown().await;
}

#[tokio::test]
async fn fill_invalidates_holdings_when_leased() {
    let server = MockServer::start().await;
    mock_token(&server).await;

    // Sweep #1: order is PENDING. Sweep #2+: order is FILLED (a fill happened).
    Mock::given(method("GET"))
        .and(path("/api/v1/orders"))
        .and(query_param("status", "OPEN"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": {"orders": [order_json("A", "PENDING", "10", "0")], "nextCursor": null, "hasNext": false}
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/orders"))
        .and(query_param("status", "OPEN"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": {"orders": [order_json("A", "FILLED", "10", "10")], "nextCursor": null, "hasNext": false}
        })))
        .mount(&server)
        .await;
    // Holdings refetch triggered by the fill — assert it is actually called.
    Mock::given(method("GET"))
        .and(path("/api/v1/holdings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": {
                "totalPurchaseAmount": {"krw": "700000", "usd": null},
                "marketValue": {"amount": {"krw": "720000"}, "amountAfterCost": {"krw": "718000"}},
                "profitLoss": {"amount": {"krw": "20000"}, "amountAfterCost": {"krw": "18000"}, "rate": "0.02", "rateAfterCost": "0.018"},
                "dailyProfitLoss": {"amount": {"krw": "20000"}, "rate": "0.02"},
                "items": []
            }
        })))
        .expect(1..) // must be fetched at least once
        .mount(&server)
        .await;

    let h = handle(&server);
    let _lease = h.watch_holdings(); // express demand for holdings
    h.refresh(RefreshTarget::Orders).await; // sees PENDING

    // Run sweeps: the FILLED transition should mark holdings dirty → refetch.
    let mut got_holdings = false;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if h.snapshot().holdings.is_some() {
            got_holdings = true;
            break;
        }
    }
    assert!(
        got_holdings,
        "a fill should invalidate + refetch holdings while leased"
    );
    h.shutdown().await;
}

#[tokio::test]
async fn optimistic_create_then_confirm() {
    let server = MockServer::start().await;
    mock_token(&server).await;
    // Empty sweeps.
    Mock::given(method("GET"))
        .and(path("/api/v1/orders"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": {"orders": [], "nextCursor": null, "hasNext": false}
        })))
        .mount(&server)
        .await;
    // The create returns a server id.
    Mock::given(method("POST"))
        .and(path("/api/v1/orders"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": {"orderId": "SVR-1", "clientOrderId": null}
        })))
        .mount(&server)
        .await;

    let h = handle(&server);
    // Subscribe BEFORE submitting so both deltas are observed regardless of timing.
    let mut rx = h.subscribe();
    let _outcome = h.create_order(tossinvest::OrderCreateRequest::limit(
        tossinvest::Symbol::new("005930").unwrap(),
        tossinvest::Side::Buy,
        tossinvest_model::IntQty::from_u64(10),
        tossinvest_model::Dec("70000".parse().unwrap()),
    ));

    use tossinvest_state::core::Delta;
    let mut saw_optimistic = false;
    let mut saw_confirmed = false;
    while let Ok(Ok(delta)) = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await {
        match delta {
            Delta::OrderSubmitted { .. } => saw_optimistic = true,
            Delta::OrderConfirmed { key } => {
                assert!(
                    matches!(&key, tossinvest_state::core::OrderKey::Server(id) if id.as_str() == "SVR-1")
                );
                saw_confirmed = true;
                break;
            }
            _ => {}
        }
    }
    assert!(
        saw_optimistic,
        "optimistic OrderSubmitted delta should fire"
    );
    assert!(
        saw_confirmed,
        "OrderConfirmed delta should re-key to the server id"
    );
    h.shutdown().await;
}
