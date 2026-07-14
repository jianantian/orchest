//! OpenRouter's Chat [`ProviderProfile`] (ADR-0002 Phase 3).
//!
//! OpenRouter is a multi-provider gateway, OpenAI-compatible Chat over the shared
//! [`ChatAdapter`](crate::chat) with its own `reasoning` object dialect and
//! `reasoning_details` replay (fallible — invalid replay data is an error). Its
//! routing headers ride on the entry (`HeaderValue::Env`); usage-missing handling
//! is the canonical default. It supports reasoning, budget, and exclusion.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::protocol::{OptionSupport, ProviderProfile, RequestOption, ResolvedModel};
use crate::{
    CacheCapability, CapabilitySource, ContentBlock, ModelCapabilities, ModelError,
    OptionAdjustment, ReasoningCapability, RequestOptions, ThinkingLevel, UpstreamErrorDetail,
};

/// The OpenRouter Chat profile.
pub struct OpenRouterProfile;

/// The singleton attached to the OpenRouter `ProviderEntry`.
pub static OPENROUTER_PROFILE: OpenRouterProfile = OpenRouterProfile;

fn append_reasoning_details(target: &mut Vec<Value>, details: &Value) -> Result<(), ModelError> {
    match details {
        Value::Array(items) => {
            target.extend(items.iter().cloned());
            Ok(())
        }
        Value::Object(_) => {
            target.push(details.clone());
            Ok(())
        }
        other => Err(ModelError {
            message: format!(
                "OpenRouter reasoning replay details must be object or array, got {other}"
            ),
            code: Some("invalid_reasoning_replay".into()),
            provider: Some("openrouter".into()),
            status: None,
            retry_after_secs: None,
            upstream: Some(Arc::new(UpstreamErrorDetail {
                code: None,
                message: None,
                body: Some(other.clone()),
            })),
        }),
    }
}

impl ProviderProfile for OpenRouterProfile {
    fn lower_options(
        &self,
        _cx: &ResolvedModel<'_>,
        options: &RequestOptions,
        body: &mut Value,
    ) -> Vec<OptionAdjustment> {
        if options.thinking != ThinkingLevel::Off {
            let mut reasoning = json!({});
            if let Some(budget) = options.thinking_budget_tokens {
                reasoning["max_tokens"] = json!(budget);
            } else {
                let effort = match options.thinking {
                    ThinkingLevel::Off => "none",
                    ThinkingLevel::Minimal => "minimal",
                    ThinkingLevel::Low => "low",
                    ThinkingLevel::Medium => "medium",
                    ThinkingLevel::High => "high",
                    ThinkingLevel::XHigh => "xhigh",
                    ThinkingLevel::Max => "max",
                };
                reasoning["effort"] = json!(effort);
            }
            if !options.include_thinking {
                reasoning["exclude"] = json!(true);
            }
            body["reasoning"] = reasoning;
        }

        if let Some(temp) = options.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(tp) = options.top_p {
            body["top_p"] = json!(tp);
        }
        Vec::new()
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
        let mut reasoning_text: Option<String> = None;
        let mut reasoning_details: Vec<Value> = Vec::new();
        for block in blocks {
            if let ContentBlock::Thinking {
                text,
                provider_details,
                ..
            } = block
            {
                if let Some(details) = provider_details {
                    append_reasoning_details(&mut reasoning_details, details)?;
                } else if reasoning_details.is_empty() {
                    if let Some(t) = text {
                        reasoning_text = Some(match reasoning_text {
                            Some(existing) => format!("{existing}{t}"),
                            None => t.clone(),
                        });
                    }
                }
            }
        }
        if !reasoning_details.is_empty() {
            assistant_msg["reasoning_details"] = Value::Array(reasoning_details);
        } else if let Some(reasoning) = reasoning_text {
            assistant_msg["reasoning"] = json!(reasoning);
        }
        Ok(())
    }

    fn option_support(&self, _cx: &ResolvedModel<'_>, _option: RequestOption) -> OptionSupport {
        // OpenRouter supports reasoning, budget, and output exclusion.
        OptionSupport::Supported
    }

    fn chat_sse_reasoning(
        &self,
        _cx: &ResolvedModel<'_>,
    ) -> (Option<&'static str>, Option<&'static str>) {
        (Some("reasoning"), Some("reasoning_details"))
    }

    fn capabilities(&self, _cx: &ResolvedModel<'_>, max_output_tokens: u32) -> ModelCapabilities {
        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: true,
            reasoning: ReasoningCapability {
                supported: true,
                efforts: vec![
                    ThinkingLevel::Minimal,
                    ThinkingLevel::Low,
                    ThinkingLevel::Medium,
                    ThinkingLevel::High,
                    ThinkingLevel::XHigh,
                    ThinkingLevel::Max,
                ],
                budget_tokens: true,
                output_exclusion: true,
                replay_metadata_required: true,
            },
            prompt_cache: CacheCapability {
                supported: true,
                explicit_breakpoints: false,
                long_ttl: false,
            },
            max_output_tokens: Some(max_output_tokens),
            context_window_size: None,
            source: CapabilitySource::Assumed,
            pricing: None,
        }
    }
}
