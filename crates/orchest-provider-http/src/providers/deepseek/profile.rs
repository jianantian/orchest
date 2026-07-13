//! DeepSeek's Chat [`ProviderProfile`] (ADR-0002 Phase 3).
//!
//! DeepSeek is OpenAI-compatible Chat over the shared [`ChatAdapter`](crate::chat)
//! with a reasoning-dialect deviation: a top-level `thinking: {type}` +
//! `reasoning_effort`, `reasoning_content` replay, a `reasoning` SSE field, forced
//! `cache_write_tokens = 0`, and its own capability facts. Reasoning-output
//! exclusion and thinking-budget support are declared as data on `option_support`.

use serde_json::{json, Value};

use crate::protocol::{
    AdjustmentSpec, AppliedValue, OptionSupport, ProviderProfile, RequestOption, ResolvedModel,
};
use crate::{
    CacheCapability, CachePolicy, CapabilitySource, ContentBlock, ModelCapabilities, ModelError,
    OptionAdjustment, ReasoningCapability, RequestOptions, ThinkingLevel, TokenUsage,
};

/// The DeepSeek Chat profile.
pub struct DeepSeekProfile;

/// The singleton attached to the DeepSeek `ProviderEntry`.
pub static DEEPSEEK_PROFILE: DeepSeekProfile = DeepSeekProfile;

impl DeepSeekProfile {
    /// v4-flash / v4-pro (and legacy deepseek-reasoner) support thinking.
    fn supports_thinking(model: &str) -> bool {
        model.starts_with("deepseek-v4-flash")
            || model.starts_with("deepseek-v4-pro")
            || model.starts_with("deepseek-reasoner")
    }
}

impl ProviderProfile for DeepSeekProfile {
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

    fn replay_reasoning(
        &self,
        _cx: &ResolvedModel<'_>,
        assistant_msg: &mut Value,
        blocks: &[ContentBlock],
    ) -> Result<(), ModelError> {
        let has_tool_calls = blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolUse { .. }));
        if !has_tool_calls {
            return Ok(());
        }
        let reasoning = blocks.iter().rev().find_map(|b| match b {
            ContentBlock::Thinking { text: Some(t), .. } => Some(t.clone()),
            _ => None,
        });
        if let Some(text) = reasoning {
            assistant_msg["reasoning_content"] = json!(text);
        }
        Ok(())
    }

    fn option_support(&self, _cx: &ResolvedModel<'_>, option: RequestOption) -> OptionSupport {
        match option {
            // DeepSeek always accepts thinking in the request (no model gate).
            RequestOption::Reasoning => OptionSupport::Supported,
            // Budget is not supported — degrade in both modes, never a Strict error.
            RequestOption::ThinkingBudget => OptionSupport::Unsupported {
                strict_error: None,
                disables_thinking: false,
                adjustment: Some(AdjustmentSpec {
                    option: "thinking_budget_tokens",
                    applied: AppliedValue::Null,
                    reason: "unsupported_by_provider",
                }),
            },
            RequestOption::ReasoningOutputExclusion => OptionSupport::Unsupported {
                strict_error: Some((
                    "unsupported_reasoning_output_exclusion",
                    "DeepSeek does not support output exclusion for reasoning",
                )),
                disables_thinking: true,
                adjustment: Some(AdjustmentSpec {
                    option: "include_thinking",
                    applied: AppliedValue::Str("thinking_disabled"),
                    reason: "output_exclusion_unsupported_disables_reasoning",
                }),
            },
        }
    }

    fn chat_sse_reasoning(
        &self,
        _cx: &ResolvedModel<'_>,
    ) -> (Option<&'static str>, Option<&'static str>) {
        (Some("reasoning"), None)
    }

    fn interpret_usage(
        &self,
        cx: &ResolvedModel<'_>,
        _raw: &Value,
        usage: &mut TokenUsage,
    ) -> Vec<OptionAdjustment> {
        usage.cache_write_tokens = 0;
        if usage.input_tokens == 0 && usage.output_tokens == 0 {
            crate::telemetry::record_usage_missing(cx.provider.name, cx.model);
            vec![OptionAdjustment {
                option: "usage".into(),
                requested: json!(null),
                applied: json!(null),
                reason: "usage_not_reported".into(),
            }]
        } else {
            Vec::new()
        }
    }

    fn capabilities(&self, cx: &ResolvedModel<'_>, max_output_tokens: u32) -> ModelCapabilities {
        let context_window = if cx.model.starts_with("deepseek-v4") {
            1_000_000
        } else {
            64_000
        };
        let thinks = Self::supports_thinking(cx.model);
        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: true,
            reasoning: ReasoningCapability {
                supported: thinks,
                efforts: if thinks {
                    vec![ThinkingLevel::High, ThinkingLevel::Max]
                } else {
                    vec![]
                },
                budget_tokens: false,
                output_exclusion: false,
                replay_metadata_required: true,
            },
            prompt_cache: CacheCapability {
                supported: true,
                explicit_breakpoints: false,
                long_ttl: false,
            },
            max_output_tokens: Some(max_output_tokens),
            context_window_size: Some(context_window),
            source: CapabilitySource::Static,
            pricing: Some(crate::pricing::deepseek_pricing(cx.model)),
        }
    }
}
