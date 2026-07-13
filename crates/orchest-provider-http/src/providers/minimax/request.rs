//! Minimax Messages endpoint normalization. The request body is built by the
//! shared [`MessagesAdapter`](crate::messages); Minimax's divergence lives in
//! [`MinimaxProfile`](super::profile::MinimaxProfile).

pub(super) fn normalize_messages_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.ends_with("/anthropic/v1/messages") || trimmed.ends_with("/v1/messages") {
        trimmed.to_string()
    } else {
        // Minimax 的 Anthropic 兼容路径是 `/anthropic/v1/messages`(`llm/api.md:42`),
        // 与 Anthropic 自家的 `/v1/messages` 不同。用户给 base URL 时自动补全。
        format!("{trimmed}/anthropic/v1/messages")
    }
}
