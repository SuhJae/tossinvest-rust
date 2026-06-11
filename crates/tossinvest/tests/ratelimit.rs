//! Integration tests for dynamic rate-limit adaptation against a wiremock server.

use serde_json::json;
use tossinvest::{Credentials, RateLimitGroup, TossClient};
use wiremock::matchers::{method, path};
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

fn client(server: &MockServer) -> TossClient {
    TossClient::builder(Credentials::new("id", "secret"))
        .base_url(server.uri())
        .build()
        .unwrap()
}

#[tokio::test]
async fn server_ratelimit_header_lowers_effective_tps() {
    let server = MockServer::start().await;
    mock_token(&server).await;
    // A successful response that carries an authoritative lower limit (2 < base 5 for STOCK).
    Mock::given(method("GET"))
        .and(path("/api/v1/stocks"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-ratelimit-limit", "2")
                .set_body_json(json!({ "result": [] })),
        )
        .mount(&server)
        .await;

    let c = client(&server);
    assert_eq!(c.rate_limiters().effective_tps(RateLimitGroup::Stock), 5.0);
    let _ = c.stocks(&["005930"]).await.unwrap();
    // The controller adopted the server's advertised limit.
    assert_eq!(c.rate_limiters().effective_tps(RateLimitGroup::Stock), 2.0);
}

#[tokio::test]
async fn ratelimit_429_then_retry_after_parks_and_throttles() {
    let server = MockServer::start().await;
    mock_token(&server).await;
    // 429 with Retry-After, then success on retry. Uses MarketData (base 10 > floor 1) so the
    // multiplicative decrease is observable.
    Mock::given(method("GET"))
        .and(path("/api/v1/prices"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "0")
                .set_body_json(
                    json!({"error": {"code": "rate-limit-exceeded", "message": "slow"}}),
                ),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/prices"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": [{"symbol": "005930", "timestamp": "2026-03-25T09:30:00+09:00", "lastPrice": "72000", "currency": "KRW"}]
        })))
        .mount(&server)
        .await;

    let c = client(&server);
    let before = c.rate_limiters().effective_tps(RateLimitGroup::MarketData);
    let prices = c.prices(&["005930"]).await.unwrap();
    assert_eq!(prices.len(), 1); // succeeded after the retry-after park + retry
    // The 429 multiplicatively decreased the MarketData group's effective rate.
    let after = c.rate_limiters().effective_tps(RateLimitGroup::MarketData);
    assert!(
        after < before,
        "429 should lower the effective rate ({after} !< {before})"
    );
}
