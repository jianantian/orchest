//! Volcengine's Chat [`ProviderProfile`] (ADR-0002 Phase 3).
//!
//! Volcengine (Ark) is OpenAI-compatible Chat over the shared
//! [`ChatAdapter`](crate::chat) with a bare `thinking: {type}` dialect (no
//! `reasoning_effort`), `reasoning_content` replay, catalog-driven capability
//! probing, and no reasoning-output exclusion. Its reasoning gate is silent (an
//! unlisted model just disables thinking, with no error even under Strict).

use serde_json::{json, Value};

use crate::protocol::{
    AdjustmentSpec, AppliedValue, OptionSupport, ProviderProfile, RequestOption, ResolvedModel,
};
use crate::{
    CacheCapability, CachePolicy, CapabilitySource, ContentBlock, ModelCapabilities, ModelError,
    OptionAdjustment, ReasoningCapability, RequestOptions, ThinkingLevel, TokenUsage,
};

/// The Volcengine Chat profile.
pub struct VolcengineProfile;

/// The singleton attached to the Volcengine `ProviderEntry`.
pub static VOLCENGINE_PROFILE: VolcengineProfile = VolcengineProfile;

impl VolcengineProfile {
    fn supports_thinking(cx: &ResolvedModel<'_>) -> bool {
        cx.catalog.map(|m| m.thinking.is_some()).unwrap_or(false)
    }
}

impl ProviderProfile for VolcengineProfile {
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
        } else {
            body["thinking"] = json!({"type": "disabled"});
        }

        if options.cache_policy != CachePolicy::Auto && options.cache_policy != CachePolicy::None {
            adjustments.push(OptionAdjustment {
                option: "cache_policy".into(),
                requested: json!(format!("{:?}", options.cache_policy)),
                applied: json!("Auto"),
                reason: "volcengine_cache_automatic".into(),
            });
        }

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

    fn option_support(&self, cx: &ResolvedModel<'_>, option: RequestOption) -> OptionSupport {
        match option {
            // Reasoning is silently gated on catalog support (no error, no adjustment).
            RequestOption::Reasoning => {
                if Self::supports_thinking(cx) {
                    OptionSupport::Supported
                } else {
                    OptionSupport::Unsupported {
                        strict_error: None,
                        disables_thinking: true,
                        adjustment: None,
                    }
                }
            }
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
                    "Volcengine does not support output exclusion for reasoning",
                )),
                disables_thinking: true,
                adjustment: Some(AdjustmentSpec {
                    option: "include_thinking",
                    applied: AppliedValue::Bool(false),
                    reason: "thinking_disabled_for_output_exclusion",
                }),
            },
        }
    }

    fn chat_sse_reasoning(
        &self,
        _cx: &ResolvedModel<'_>,
    ) -> (Option<&'static str>, Option<&'static str>) {
        (Some("reasoning_content"), None)
    }

    /// Volcengine records no usage-missing adjustment (unlike OpenAI/DeepSeek/OpenRouter).
    fn interpret_usage(
        &self,
        _cx: &ResolvedModel<'_>,
        _raw: &Value,
        _usage: &mut TokenUsage,
    ) -> Vec<OptionAdjustment> {
        Vec::new()
    }

    fn capabilities(&self, cx: &ResolvedModel<'_>, max_output_tokens: u32) -> ModelCapabilities {
        let thinks = Self::supports_thinking(cx);
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
                replay_metadata_required: false,
            },
            prompt_cache: CacheCapability {
                supported: true,
                explicit_breakpoints: false,
                long_ttl: false,
            },
            max_output_tokens: Some(max_output_tokens),
            context_window_size: cx.catalog.map(|m| m.context_window).or(Some(128_000)),
            source: CapabilitySource::Static,
            pricing: cx
                .catalog
                .and_then(|m| m.pricing.clone())
                .or_else(|| Some(crate::pricing::volcengine_pricing(cx.model))),
        }
    }
}
