//! DeepSeek's Chat [`ProviderProfile`] (ADR-0002 slice 002).
//!
//! DeepSeek is OpenAI-compatible Chat with one behavioral deviation: its
//! reasoning dialect. This profile is the named home of that deviation —
//! `lower_options` emits DeepSeek's top-level `thinking: {type}` + `reasoning_effort`
//! (and gates sampling on it), and `replay_reasoning` re-injects prior assistant
//! reasoning as `reasoning_content`. Everything else DeepSeek does is
//! protocol-canonical, so it is not represented here.

use serde_json::{json, Value};

use crate::protocol::{ProviderProfile, ResolvedModel};
use crate::{CachePolicy, ContentBlock, OptionAdjustment, RequestOptions, ThinkingLevel};

/// The DeepSeek Chat profile. Zero-sized; all behavior is in the hook impls.
pub struct DeepSeekProfile;

/// The singleton attached to the DeepSeek `ProviderEntry`.
pub static DEEPSEEK_PROFILE: DeepSeekProfile = DeepSeekProfile;

impl ProviderProfile for DeepSeekProfile {
    /// DeepSeek expresses reasoning as a top-level `thinking: {type: enabled|disabled}`
    /// plus a coarse `reasoning_effort` (`high`/`max`), and omits sampling when
    /// thinking is enabled. `options.thinking == Off` here already reflects the
    /// effective decision (the adapter forces it off for the coerce/exclude
    /// case before calling this hook), so this reads `options` directly.
    fn lower_options(
        &self,
        _cx: &ResolvedModel<'_>,
        options: &RequestOptions,
        body: &mut Value,
    ) -> Vec<OptionAdjustment> {
        let mut adjustments = Vec::new();
        let thinking_enabled = options.thinking != ThinkingLevel::Off;

        if thinking_enabled {
            body["thinking"] = json!({"type": "enabled"});
            let effort = match options.thinking {
                ThinkingLevel::XHigh | ThinkingLevel::Max => "max",
                _ => "high",
            };
            body["reasoning_effort"] = json!(effort);
        } else {
            body["thinking"] = json!({"type": "disabled"});
        }

        // thinking_budget_tokens is not supported by DeepSeek.
        if options.thinking_budget_tokens.is_some() {
            adjustments.push(OptionAdjustment {
                option: "thinking_budget_tokens".into(),
                requested: json!(options.thinking_budget_tokens),
                applied: json!(null),
                reason: "unsupported_by_provider".into(),
            });
        }

        // DeepSeek caching is fully automatic — any explicit policy degrades.
        if options.cache_policy != CachePolicy::Auto && options.cache_policy != CachePolicy::None {
            adjustments.push(OptionAdjustment {
                option: "cache_policy".into(),
                requested: json!(format!("{:?}", options.cache_policy)),
                applied: json!("Auto"),
                reason: "deepseek_cache_automatic".into(),
            });
        }

        // Sampling parameters — omit when thinking is enabled.
        if !thinking_enabled {
            if let Some(temp) = options.temperature {
                body["temperature"] = json!(temp);
            }
            if let Some(tp) = options.top_p {
                body["top_p"] = json!(tp);
            }
        }

        adjustments
    }

    /// On a replayed assistant turn that made tool calls, re-inject the prior
    /// reasoning as `reasoning_content` (DeepSeek requires reasoning replay for
    /// tool-call continuity). Last Thinking block wins, matching the prior
    /// inline behavior.
    fn replay_reasoning(
        &self,
        _cx: &ResolvedModel<'_>,
        assistant_msg: &mut Value,
        blocks: &[ContentBlock],
    ) {
        let has_tool_calls = blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolUse { .. }));
        if !has_tool_calls {
            return;
        }
        let reasoning = blocks.iter().rev().find_map(|b| match b {
            ContentBlock::Thinking { text: Some(t), .. } => Some(t.clone()),
            _ => None,
        });
        if let Some(text) = reasoning {
            assistant_msg["reasoning_content"] = json!(text);
        }
    }
}
