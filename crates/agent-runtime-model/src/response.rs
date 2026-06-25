//! Model response types: content blocks, token usage, and stop reasons.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::ContentBlock;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelResponse {
    pub content: Vec<ContentBlock>,
    pub usage: TokenUsage,
    pub stop_reason: StopReason,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub option_adjustments: Vec<OptionAdjustment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TokenUsage {
    /// Total input tokens (text-equivalent). For multimodal usage this is the
    /// **text** input count; per-modality input counts live in the dedicated
    /// fields below and MUST NOT be double-counted in `input_tokens`.
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// Audio input tokens (separately billed by some providers, e.g. Volcengine
    /// Lite/Mini-260428 charges ¥9/MTok for audio input vs ¥0.6 for text).
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub audio_input_tokens: u64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub image_input_tokens: u64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub video_input_tokens: u64,
    #[serde(default)]
    pub details: HashMap<String, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde-required signature
fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    ContentFilter,
    Refusal,
    ContextWindowExceeded,
    Pause,
    Interrupted,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptionAdjustment {
    pub option: String,
    pub requested: Value,
    pub applied: Value,
    pub reason: String,
}
