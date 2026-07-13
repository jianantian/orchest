//! Volcengine Ark endpoint normalization. The request body is built by the shared
//! [`ChatAdapter`](crate::chat).

pub(super) fn normalize_chat_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v3") || trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v3/chat/completions")
    }
}
