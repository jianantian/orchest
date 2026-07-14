//! OpenAI Chat helpers: endpoint normalization and the documented name-prefix
//! capability fallbacks (used only when a model is absent from the catalog). The
//! request body itself is built by the shared [`ChatAdapter`](crate::chat).

pub(super) fn normalize_chat_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

/// **Fallback only** (ADR "Capability metadata"): the canonical source of
/// reasoning support is the catalog row. This name-prefix table is consulted
/// solely for models absent from the catalog (unlisted previews).
pub(super) fn supports_reasoning_model(model: &str) -> bool {
    let name = model.split_once('/').map_or(model, |(_, m)| m);
    name.starts_with("o1")
        || name.starts_with("o3")
        || name.starts_with("o4")
        || name.starts_with("gpt-5")
}

/// **Fallback only** (ADR "Capability metadata"): the canonical source of the
/// context window is the catalog row. This name-prefix table is consulted solely
/// for models absent from the catalog.
pub(super) fn openai_context_window(model: &str) -> u64 {
    let name = model.split_once('/').map_or(model, |(_, m)| m);
    if name.starts_with("gpt-5.5") || name == "gpt-5.4" {
        1_000_000
    } else if name.starts_with("gpt-5.4-mini") || name.starts_with("gpt-5.4-nano") {
        400_000
    } else {
        128_000
    }
}
