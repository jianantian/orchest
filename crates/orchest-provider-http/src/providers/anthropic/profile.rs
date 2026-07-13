//! Anthropic's Messages [`ProviderProfile`] (ADR-0002 Phase 3).
//!
//! Anthropic is the canonical Messages provider over the shared
//! [`MessagesAdapter`](crate::messages). It uses the default role mapping (drop
//! Minimax-only roles) and default `option_support`/`interpret_usage`; its
//! deviations are capability facts, `x-api-key` auth, adaptive-thinking
//! detection, and image-only multimodal content.

use serde_json::{json, Value};

use crate::defaults;
use crate::protocol::{ProviderProfile, ResolvedModel};
use crate::{
    CacheCapability, CapabilitySource, ContentBlock, MediaSource, ModelCapabilities,
    OptionAdjustment, ReasoningCapability, ThinkingLevel,
};

/// The Anthropic Messages profile.
pub struct AnthropicProfile;

/// The singleton attached to the Anthropic `ProviderEntry`.
pub static ANTHROPIC_PROFILE: AnthropicProfile = AnthropicProfile;

impl AnthropicProfile {
    fn supports_adaptive(model: &str) -> bool {
        model.starts_with("claude-fable-5")
            || model.starts_with("claude-mythos-5")
            || model.starts_with("claude-opus-4")
            || model.starts_with("claude-sonnet-4")
    }

    fn context_window(model: &str) -> u64 {
        if model.starts_with("claude-haiku-4") {
            200_000
        } else {
            1_000_000
        }
    }
}

impl ProviderProfile for AnthropicProfile {
    fn messages_supports_adaptive(&self, cx: &ResolvedModel<'_>) -> bool {
        Self::supports_adaptive(cx.model)
    }

    fn messages_auth_headers(
        &self,
        _cx: &ResolvedModel<'_>,
        api_key: &str,
    ) -> Vec<(&'static str, String)> {
        vec![
            ("x-api-key", api_key.to_string()),
            (
                "anthropic-version",
                defaults::anthropic::API_VERSION.to_string(),
            ),
            ("content-type", "application/json".to_string()),
        ]
    }

    fn encode_multimodal_block(
        &self,
        _cx: &ResolvedModel<'_>,
        block: &ContentBlock,
        adjustments: &mut Vec<OptionAdjustment>,
    ) -> Option<Value> {
        match block {
            // Anthropic Messages natively supports image (no `detail`).
            ContentBlock::Image { source, .. } => {
                let source_value = match source {
                    MediaSource::Url { url } => json!({"type": "url", "url": url}),
                    MediaSource::Base64 { media_type, data } => json!({
                        "type": "base64",
                        "media_type": media_type,
                        "data": data,
                    }),
                };
                Some(json!({"type": "image", "source": source_value}))
            }
            // Video / Audio / MidConvSystem are not accepted — drop + record.
            other => {
                let kind = match other {
                    ContentBlock::Video { .. } => "video",
                    ContentBlock::Audio { .. } => "audio",
                    ContentBlock::MidConvSystem(_) => "mid_conv_system",
                    _ => "unknown",
                };
                adjustments.push(OptionAdjustment {
                    option: "content_block".into(),
                    requested: json!(kind),
                    applied: json!(null),
                    reason: "anthropic_unsupported_content_block".into(),
                });
                None
            }
        }
    }

    fn capabilities(&self, cx: &ResolvedModel<'_>, max_output_tokens: u32) -> ModelCapabilities {
        let supports_adaptive = Self::supports_adaptive(cx.model);
        let efforts = if supports_adaptive {
            vec![
                ThinkingLevel::Off,
                ThinkingLevel::Minimal,
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
                ThinkingLevel::XHigh,
                ThinkingLevel::Max,
            ]
        } else {
            vec![
                ThinkingLevel::Off,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
                ThinkingLevel::Max,
            ]
        };
        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: true,
            reasoning: ReasoningCapability {
                supported: true,
                efforts,
                budget_tokens: !supports_adaptive,
                output_exclusion: true,
                replay_metadata_required: true,
            },
            prompt_cache: CacheCapability {
                supported: true,
                explicit_breakpoints: true,
                long_ttl: true,
            },
            max_output_tokens: Some(max_output_tokens),
            context_window_size: Some(Self::context_window(cx.model)),
            source: CapabilitySource::Static,
            pricing: Some(crate::pricing::anthropic_pricing(cx.model)),
        }
    }
}
