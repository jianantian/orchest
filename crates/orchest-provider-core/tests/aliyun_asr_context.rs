use orchest_protocol::ErrorCode;
use orchest_provider_core::aliyun_asr::{parse_context, ContextRole};
use serde_json::json;

#[test]
fn parses_valid_simplified_context() {
    let messages = parse_context(&json!({
        "context": [
            {"role": "user", "text": "Orchest"},
            {"role": "assistant", "text": "好的"}
        ]
    }))
    .expect("valid context");

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, ContextRole::User);
}

#[test]
fn rejects_invalid_order_counts_and_round_length() {
    let orphan = parse_context(&json!({
        "context": [{"role": "assistant", "text": "orphan"}]
    }))
    .expect_err("assistant cannot start a round");
    assert_eq!(orphan.code, ErrorCode::InvalidRequest);

    let consecutive_users = parse_context(&json!({
        "context": [
            {"role": "user", "text": "one"},
            {"role": "user", "text": "two"}
        ]
    }))
    .expect_err("rounds must preserve user/assistant ordering");
    assert_eq!(consecutive_users.code, ErrorCode::InvalidRequest);

    let too_many = (0..6)
        .map(|index| json!({"role": "user", "text": index.to_string()}))
        .collect::<Vec<_>>();
    assert!(parse_context(&json!({"context": too_many})).is_err());

    let too_long = "你".repeat(401);
    assert!(parse_context(&json!({
        "context": [{"role": "user", "text": too_long}]
    }))
    .is_err());
}

#[test]
fn missing_context_is_empty_and_malformed_context_fails() {
    assert!(parse_context(&json!({}))
        .expect("missing is empty")
        .is_empty());
    assert!(parse_context(&json!({"context": "bad"})).is_err());
    assert!(parse_context(&json!({"context": [{"role": "user", "text": ""}]})).is_err());
}
