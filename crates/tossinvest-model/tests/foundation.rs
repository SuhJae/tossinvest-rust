//! Foundation round-trip tests for the model scalars, open enums, and error envelope.

use tossinvest_model::{ApiError, Currency, Dec, ErrorCode, ErrorResponse, IntQty, Side};

#[test]
fn open_enum_preserves_known_and_unknown() {
    // Known value serializes to its wire form.
    assert_eq!(serde_json::to_string(&Currency::Krw).unwrap(), "\"KRW\"");

    // Unknown value round-trips verbatim and is flagged.
    let u: Currency = serde_json::from_str("\"GBP\"").unwrap();
    assert_eq!(u, Currency::Unknown("GBP".to_owned()));
    assert!(!u.is_known());
    assert_eq!(u.as_wire(), "GBP");
    assert_eq!(serde_json::to_string(&u).unwrap(), "\"GBP\"");
}

#[test]
fn closed_enum_uses_screaming_snake() {
    assert_eq!(serde_json::to_string(&Side::Buy).unwrap(), "\"BUY\"");
    let s: Side = serde_json::from_str("\"SELL\"").unwrap();
    assert_eq!(s, Side::Sell);
}

#[test]
fn dec_serializes_as_string_and_keeps_scale() {
    let d: Dec = serde_json::from_str("\"70000.50\"").unwrap();
    assert_eq!(serde_json::to_string(&d).unwrap(), "\"70000.50\"");
}

#[test]
fn intqty_rejects_non_integer_and_negative() {
    assert!(serde_json::from_str::<IntQty>("\"10\"").is_ok());
    assert!(serde_json::from_str::<IntQty>("\"10.5\"").is_err());
    assert!(serde_json::from_str::<IntQty>("\"-1\"").is_err());
}

#[test]
fn error_code_unknown_is_preserved() {
    let c: ErrorCode = serde_json::from_str("\"some-future-code\"").unwrap();
    assert_eq!(c, ErrorCode::Unknown("some-future-code".to_owned()));
    assert_eq!(c.as_wire(), "some-future-code");
}

#[test]
fn error_envelope_deserializes_with_data() {
    let body = r#"{
        "error": {
            "requestId": "01HXYZ",
            "code": "invalid-request",
            "message": "주문 방향이 올바르지 않습니다.",
            "data": { "field": "side", "allowedValues": ["BUY", "SELL"] }
        }
    }"#;
    let resp: ErrorResponse = serde_json::from_str(body).unwrap();
    let ApiError {
        code,
        data,
        request_id,
        ..
    } = resp.error;
    assert_eq!(code, ErrorCode::InvalidRequest);
    assert_eq!(request_id.unwrap().as_str(), "01HXYZ");
    let data = data.unwrap();
    assert_eq!(data.field.as_deref(), Some("side"));
    assert_eq!(data.allowed_values.unwrap(), vec!["BUY", "SELL"]);
}

#[test]
fn error_envelope_without_optional_fields() {
    let resp: ErrorResponse = serde_json::from_str(r#"{"error":{"code":"maintenance"}}"#).unwrap();
    assert_eq!(resp.error.code, ErrorCode::Maintenance);
    assert!(resp.error.message.is_empty());
    assert!(resp.error.data.is_none());
    assert!(resp.error.request_id.is_none());
}
