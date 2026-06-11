//! Golden tests: deserialize verbatim response examples taken from the OpenAPI spec
//! (`.docs/openapi.json`) and assert the model captures them faithfully.

use rust_decimal::Decimal;
use std::str::FromStr;
use tossinvest_model::{
    ApiResponse, Currency, HoldingsOverview, MarketCountry, Order, OrderStatus, PriceResponse,
};

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

#[test]
fn golden_holdings_with_holdings() {
    // From GET /api/v1/holdings example "withHoldings".
    let body = r#"{"result":{"totalPurchaseAmount":{"krw":"6500000","usd":"1553"},
        "marketValue":{"amount":{"krw":"7200000","usd":"1785"},"amountAfterCost":{"krw":"7050000","usd":"1771.43"}},
        "profitLoss":{"amount":{"krw":"700000","usd":"232"},"amountAfterCost":{"krw":"550000","usd":"218.43"},"rate":"0.1179","rateAfterCost":"0.0983"},
        "dailyProfitLoss":{"amount":{"krw":"100000","usd":"25"},"rate":"0.0141"},
        "items":[
            {"symbol":"005930","name":"삼성전자","marketCountry":"KR","currency":"KRW","quantity":"100","lastPrice":"72000","averagePurchasePrice":"65000",
             "marketValue":{"purchaseAmount":"6500000","amount":"7200000","amountAfterCost":"7050000"},
             "profitLoss":{"amount":"700000","amountAfterCost":"550000","rate":"0.1077","rateAfterCost":"0.0846"},
             "dailyProfitLoss":{"amount":"100000","rate":"0.0141"},
             "cost":{"commission":"14400","tax":"135600"}},
            {"symbol":"AAPL","name":"Apple Inc.","marketCountry":"US","currency":"USD","quantity":"10","lastPrice":"178.5","averagePurchasePrice":"155.3",
             "marketValue":{"purchaseAmount":"1553","amount":"1785","amountAfterCost":"1771.43"},
             "profitLoss":{"amount":"232","amountAfterCost":"218.43","rate":"0.1494","rateAfterCost":"0.1406"},
             "dailyProfitLoss":{"amount":"25","rate":"0.0142"},
             "cost":{"commission":"3.57","tax":"10"}}
        ]}}"#;
    let resp: ApiResponse<HoldingsOverview> = serde_json::from_str(body).unwrap();
    let h = resp.result;

    // Currency bucket: krw present, usd present.
    assert_eq!(h.total_purchase_amount.krw.get(), dec("6500000"));
    assert_eq!(h.total_purchase_amount.usd.unwrap().get(), dec("1553"));

    // Two holdings, KR then US.
    assert_eq!(h.items.len(), 2);
    let kr = &h.items[0];
    assert_eq!(kr.market_country, MarketCountry::Kr);
    assert_eq!(kr.currency, Currency::Krw);
    assert_eq!(kr.cost.tax.unwrap().get(), dec("135600")); // KR has tax

    let us = &h.items[1];
    assert_eq!(us.market_country, MarketCountry::Us);
    assert_eq!(us.profit_loss.rate.get(), dec("0.1494")); // ratio, not percent
}

#[test]
fn golden_holdings_empty_and_null_usd() {
    // Empty holdings: krw "0", usd null, items [].
    let body = r#"{"result":{"totalPurchaseAmount":{"krw":"0","usd":null},
        "marketValue":{"amount":{"krw":"0"},"amountAfterCost":{"krw":"0"}},
        "profitLoss":{"amount":{"krw":"0"},"amountAfterCost":{"krw":"0"},"rate":"0","rateAfterCost":"0"},
        "dailyProfitLoss":{"amount":{"krw":"0"},"rate":"0"},
        "items":[]}}"#;
    let resp: ApiResponse<HoldingsOverview> = serde_json::from_str(body).unwrap();
    let h = resp.result;
    assert!(h.items.is_empty());
    // krw present as "0"; usd both null-valued and absent-key forms map to None.
    assert_eq!(h.total_purchase_amount.krw.get(), dec("0"));
    assert!(h.total_purchase_amount.usd.is_none());
    assert!(h.market_value.amount.usd.is_none()); // absent key → None via serde(default)
}

#[test]
fn golden_order_kr_limit_filled() {
    // From GET /api/v1/orders/{orderId} example "krLimitFilled".
    let body = r#"{"result":{"orderId":"0d5QIHjmtksbsmM","symbol":"005930","side":"BUY","orderType":"LIMIT",
        "timeInForce":"DAY","status":"FILLED","price":"70000","quantity":"10","orderAmount":null,"currency":"KRW",
        "orderedAt":"2026-03-28T09:30:00+09:00","canceledAt":null,
        "execution":{"filledQuantity":"10","averageFilledPrice":"70000","filledAmount":"700000",
            "commission":"1400","tax":"0","filledAt":"2026-03-28T09:31:15+09:00","settlementDate":"2026-03-30"}}}"#;
    let resp: ApiResponse<Order> = serde_json::from_str(body).unwrap();
    let o = resp.result;
    assert_eq!(o.status, OrderStatus::Filled);
    assert!(o.status.is_terminal());
    assert!(!o.status.is_open());
    assert!(o.is_fully_filled());
    assert!(o.has_fills());
    assert!(o.price.is_some());
    assert!(o.order_amount.is_none());
    assert_eq!(
        o.execution.settlement_date.unwrap().to_string(),
        "2026-03-30"
    );
}

#[test]
fn golden_order_canceled_with_partial_fill() {
    // Terminal CANCELED that still carries a partial fill (fills orthogonal to status).
    let body = r#"{"result":{"orderId":"X","symbol":"AAPL","side":"BUY","orderType":"LIMIT",
        "timeInForce":"DAY","status":"CANCELED","price":"150","quantity":"10","orderAmount":null,"currency":"USD",
        "orderedAt":"2026-03-28T22:30:00+09:00","canceledAt":"2026-03-28T22:35:00+09:00",
        "execution":{"filledQuantity":"3","averageFilledPrice":"150","filledAmount":"450",
            "commission":"1","tax":null,"filledAt":"2026-03-28T22:31:00+09:00","settlementDate":null}}}"#;
    let resp: ApiResponse<Order> = serde_json::from_str(body).unwrap();
    let o = resp.result;
    assert_eq!(o.status, OrderStatus::Canceled);
    assert!(o.status.is_terminal());
    assert!(o.has_fills()); // partial fill survives the cancel
    assert!(!o.is_fully_filled());
    assert!(o.canceled_at.is_some());
    assert!(o.execution.tax.is_none());
    assert!(o.execution.settlement_date.is_none());
}

#[test]
fn golden_prices_array_with_fractional_timestamp() {
    // From GET /api/v1/prices example "krStock" (note millisecond timestamp).
    let body = r#"{"result":[{"symbol":"005930","timestamp":"2026-03-25T09:30:00.123+09:00","lastPrice":"72000","currency":"KRW"}]}"#;
    let resp: ApiResponse<Vec<PriceResponse>> = serde_json::from_str(body).unwrap();
    assert_eq!(resp.result.len(), 1);
    assert_eq!(resp.result[0].symbol, "005930");
    assert_eq!(resp.result[0].last_price.get(), dec("72000"));
    assert!(resp.result[0].timestamp.is_some());
}

#[test]
fn golden_unknown_status_tolerated() {
    // A status the spec does not define must round-trip without error.
    let body = r#"{"result":{"orderId":"Z","symbol":"005930","side":"SELL","orderType":"MARKET",
        "timeInForce":"DAY","status":"PENDING_SETTLEMENT","price":null,"quantity":"5","currency":"KRW",
        "orderedAt":"2026-03-28T09:30:00+09:00",
        "execution":{"filledQuantity":"0","averageFilledPrice":null,"filledAmount":null,
            "commission":null,"tax":null,"filledAt":null,"settlementDate":null}}}"#;
    let resp: ApiResponse<Order> = serde_json::from_str(body).unwrap();
    let o = resp.result;
    assert_eq!(
        o.status,
        OrderStatus::Unknown("PENDING_SETTLEMENT".to_owned())
    );
    assert!(o.status.group().is_none()); // don't guess the group for unknowns
    assert!(o.price.is_none()); // MARKET → null price
}
