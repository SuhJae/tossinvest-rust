//! End-to-end pipeline tests against a `wiremock` server using the real reqwest transport.

use serde_json::json;
use tossinvest::{ApiErrorKind, Credentials, Error, OrderListFilter, TossClient};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn mock_token(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "tok-123",
            "token_type": "Bearer",
            "expires_in": 3600
        })))
        .mount(server)
        .await;
}

fn client(server: &MockServer) -> TossClient {
    TossClient::builder(Credentials::new("cid", "csecret"))
        .base_url(server.uri())
        .build()
        .unwrap()
}

#[tokio::test]
async fn prices_fetches_token_then_sends_bearer() {
    let server = MockServer::start().await;
    mock_token(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/prices"))
        .and(query_param("symbols", "005930,AAPL"))
        .and(header("authorization", "Bearer tok-123")) // proves auth injection
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": [{"symbol": "005930", "timestamp": "2026-03-25T09:30:00+09:00",
                        "lastPrice": "72000", "currency": "KRW"}]
        })))
        .mount(&server)
        .await;

    let prices = client(&server).prices(&["005930", "AAPL"]).await.unwrap();
    assert_eq!(prices.len(), 1);
    assert_eq!(prices[0].symbol, "005930");
    assert_eq!(prices[0].last_price.to_string(), "72000");
}

#[tokio::test]
async fn error_envelope_maps_to_typed_api_error() {
    let server = MockServer::start().await;
    mock_token(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/orderbook"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": {"requestId": "req-1", "code": "stock-not-found", "message": "not found"}
        })))
        .mount(&server)
        .await;

    let err = client(&server).orderbook("ZZZZZZ").await.unwrap_err();
    match err {
        Error::Api {
            status,
            kind,
            request_id,
            ..
        } => {
            assert_eq!(status, 404);
            assert_eq!(kind, ApiErrorKind::NotFound);
            assert_eq!(request_id.unwrap().as_str(), "req-1");
        }
        other => panic!("expected Error::Api, got {other:?}"),
    }
}

#[tokio::test]
async fn account_client_injects_account_header() {
    let server = MockServer::start().await;
    mock_token(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/holdings"))
        .and(header("x-tossinvest-account", "7")) // proves account scoping
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": {
                "totalPurchaseAmount": {"krw": "0", "usd": null},
                "marketValue": {"amount": {"krw": "0"}, "amountAfterCost": {"krw": "0"}},
                "profitLoss": {"amount": {"krw": "0"}, "amountAfterCost": {"krw": "0"}, "rate": "0", "rateAfterCost": "0"},
                "dailyProfitLoss": {"amount": {"krw": "0"}, "rate": "0"},
                "items": []
            }
        })))
        .mount(&server)
        .await;

    let acct = client(&server).account(7.into());
    let holdings = acct.holdings(None).await.unwrap();
    assert!(holdings.items.is_empty());
}

#[tokio::test]
async fn retries_on_429_then_succeeds() {
    let server = MockServer::start().await;
    mock_token(&server).await;

    // First call: 429. wiremock serves mounts in order with `up_to_n_times`.
    Mock::given(method("GET"))
        .and(path("/api/v1/accounts"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "0")
                .set_body_json(json!({
                    "error": {"code": "rate-limit-exceeded", "message": "slow down"}
                })),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    // Second call: 200.
    Mock::given(method("GET"))
        .and(path("/api/v1/accounts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": [{"accountNo": "123", "accountSeq": 7, "accountType": "BROKERAGE"}]
        })))
        .mount(&server)
        .await;

    let accounts = client(&server).accounts().await.unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].account_seq.get(), 7);
}

#[tokio::test]
async fn orders_filter_sends_status_param() {
    let server = MockServer::start().await;
    mock_token(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/orders"))
        .and(query_param("status", "OPEN"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": {"orders": [], "nextCursor": null, "hasNext": false}
        })))
        .mount(&server)
        .await;

    let page = client(&server)
        .account(7.into())
        .orders(OrderListFilter::Open, None, None, None)
        .await
        .unwrap();
    assert!(page.orders.is_empty());
    assert!(!page.has_next);
}
