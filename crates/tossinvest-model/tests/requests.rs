//! Serialization tests for order request bodies (the `oneOf` create variants + modify).

use rust_decimal::Decimal;
use std::str::FromStr;
use tossinvest_model::{
    ClientOrderId, CreateTimeInForce, Dec, IntQty, OrderCreateRequest, OrderModifyRequest, Side,
    Symbol,
};

fn sym(s: &str) -> Symbol {
    Symbol::new(s).unwrap()
}

fn dec(s: &str) -> Dec {
    Dec(Decimal::from_str(s).unwrap())
}

#[test]
fn create_limit_serializes_quantity_based() {
    let req =
        OrderCreateRequest::limit(sym("005930"), Side::Buy, IntQty::from_u64(10), dec("70000"));
    let v: serde_json::Value = serde_json::to_value(&req).unwrap();
    assert_eq!(v["symbol"], "005930");
    assert_eq!(v["side"], "BUY");
    assert_eq!(v["orderType"], "LIMIT");
    assert_eq!(v["quantity"], "10");
    assert_eq!(v["price"], "70000");
    // Defaults are omitted.
    assert!(v.get("clientOrderId").is_none());
    assert!(v.get("timeInForce").is_none());
    assert!(v.get("confirmHighValueOrder").is_none());
}

#[test]
fn create_market_omits_price() {
    let req = OrderCreateRequest::market(sym("005930"), Side::Sell, IntQty::from_u64(5));
    let v: serde_json::Value = serde_json::to_value(&req).unwrap();
    assert_eq!(v["orderType"], "MARKET");
    assert!(v.get("price").is_none()); // MARKET must not carry a price
}

#[test]
fn create_amount_based_us_market() {
    let req = OrderCreateRequest::market_amount(sym("AAPL"), Side::Buy, dec("100.5"));
    let v: serde_json::Value = serde_json::to_value(&req).unwrap();
    assert_eq!(v["symbol"], "AAPL");
    assert_eq!(v["orderType"], "MARKET");
    assert_eq!(v["orderAmount"], "100.5");
    assert!(v.get("quantity").is_none());
}

#[test]
fn create_builders_compose() {
    let req = OrderCreateRequest::limit(sym("AAPL"), Side::Buy, IntQty::from_u64(1), dec("150"))
        .with_idempotency(ClientOrderId::new("my-order-001").unwrap())
        .with_time_in_force(CreateTimeInForce::Cls)
        .confirm_high_value();
    let v: serde_json::Value = serde_json::to_value(&req).unwrap();
    assert_eq!(v["clientOrderId"], "my-order-001");
    assert_eq!(v["timeInForce"], "CLS");
    assert_eq!(v["confirmHighValueOrder"], true);
}

#[test]
fn modify_kr_includes_quantity_us_omits() {
    let kr = serde_json::to_value(OrderModifyRequest::kr_limit(
        IntQty::from_u64(15),
        dec("71000"),
    ))
    .unwrap();
    assert_eq!(kr["quantity"], "15");
    assert_eq!(kr["price"], "71000");

    let us = serde_json::to_value(OrderModifyRequest::us_limit(dec("180.25"))).unwrap();
    assert!(us.get("quantity").is_none()); // US modify must not carry quantity
    assert_eq!(us["price"], "180.25");
}

#[test]
fn invalid_symbol_and_client_order_id_rejected() {
    assert!(Symbol::new("bad symbol!").is_err());
    assert!(ClientOrderId::new("has space").is_err());
    assert!(ClientOrderId::new("x".repeat(37)).is_err());
}
