//! Minimax's Messages [`ProviderProfile`] (ADR-0002 Phase 3).
//!
//! Minimax is Anthropic-Messages-compatible but forks the most on the Messages
//! side — the stress test for the profile model. It deviates on **roles** (native
//! Minimax-only roles instead of a downgrade), **multimodal content encoding**
//! (real Image/Video/MidConvSystem serialization, Audio dropped), adaptive-thinking
//! detection, `Bearer` auth, and capability facts. Crucially it does NOT change the
//! stream-event shape or the `thinking: {type, display}` dialect, so per the ADR
//! "dialect-fork threshold" it stays a profile, not a new protocol.

use serde_json::{json, Value};

use crate::protocol::{ProviderProfile, ResolvedModel};
use crate::{
    CacheCapability, CapabilitySource, ContentBlock, MediaSource, ModelCapabilities, ModelPricing,
    OptionAdjustment, ReasoningCapability, Role, ThinkingLevel,
};

/// The Minimax Messages profile. Zero-sized; behavior is in the hook impls.
pub struct MinimaxProfile;

/// The singleton attached to the Minimax `ProviderEntry`.
pub static MINIMAX_PROFILE: MinimaxProfile = MinimaxProfile;

impl MinimaxProfile {
    fn supports_adaptive(model: &str) -> bool {
        model.starts_with("MiniMax-M3")
    }

    fn context_window(model: &str) -> u64 {
        // `llm/desc.md:21-29` 模型表:M3 1M,M2 系列 204_800。
        if model.starts_with("MiniMax-M3") {
            1_000_000
        } else {
            204_800
        }
    }

    /// `None` for source kinds added to the protocol later (`MediaSource` is
    /// `#[non_exhaustive]`); the caller drops the block and records it.
    fn media_source_value(source: &MediaSource) -> Option<Value> {
        match source {
            MediaSource::Url { url } => Some(json!({"type": "url", "url": url})),
            MediaSource::Base64 { media_type, data } => Some(json!({
                "type": "base64",
                "media_type": media_type,
                "data": data,
            })),
            _ => None,
        }
    }

    fn record_dropped_block(adjustments: &mut Vec<OptionAdjustment>, kind: &str, reason: &str) {
        adjustments.push(OptionAdjustment {
            option: "content_block".into(),
            requested: json!(kind),
            applied: json!(null),
            reason: reason.into(),
        });
    }
}

impl ProviderProfile for MinimaxProfile {
    /// Minimax accepts its own roles natively (rather than downgrading them like
    /// the Chat providers do). `System` is handled by the core as a top-level
    /// `system` field, so it is not produced here.
    fn messages_wire_role(
        &self,
        _cx: &ResolvedModel<'_>,
        role: &Role,
        _adjustments: &mut Vec<OptionAdjustment>,
    ) -> &'static str {
        match role {
            Role::User | Role::Tool => "user",
            Role::Assistant => "assistant",
            Role::System => "system", // handled as top-level system; unreachable here
            Role::UserSystem => "user_system",
            Role::Group => "group",
            Role::SampleMessageUser => "sample_message_user",
            Role::SampleMessageAi => "sample_message_ai",
            // Roles added to the protocol later fall back to `user`.
            _ => "user",
        }
    }

    fn messages_supports_adaptive(&self, cx: &ResolvedModel<'_>) -> bool {
        Self::supports_adaptive(cx.model)
    }

    /// `Authorization: Bearer ${api_key}`(锚点 `llm/api.md:1354-1362`)。
    fn messages_auth_headers(
        &self,
        _cx: &ResolvedModel<'_>,
        api_key: &str,
    ) -> Vec<(&'static str, String)> {
        vec![
            ("authorization", format!("Bearer {api_key}")),
            ("content-type", "application/json".to_string()),
        ]
    }

    /// `Image`/`Video`/`MidConvSystem` 走真实序列化(锚点 `llm/api.md:1136-1321`);
    /// `Audio` 在当前 LLM API 不被接受(Step 2 omni 占位),丢弃并记录 OptionAdjustment。
    fn encode_multimodal_block(
        &self,
        _cx: &ResolvedModel<'_>,
        block: &ContentBlock,
        adjustments: &mut Vec<OptionAdjustment>,
    ) -> Option<Value> {
        match block {
            // Minimax 原生支持 image,与 Anthropic 同 schema(`llm/api.md:1215-1305`)。
            ContentBlock::Image { source, detail } => {
                let Some(source_value) = Self::media_source_value(source) else {
                    Self::record_dropped_block(
                        adjustments,
                        "image",
                        "minimax_unsupported_media_source",
                    );
                    return None;
                };
                let mut obj = json!({"type": "image", "source": source_value});
                if let Some(d) = detail {
                    obj["detail"] = json!(d);
                }
                Some(obj)
            }
            // Minimax 视频 block,专属字段 fps / max_long_side_pixel(`llm/api.md:1334-1343`)。
            ContentBlock::Video {
                source,
                fps,
                detail,
                max_long_side_pixel,
            } => {
                let Some(source_value) = Self::media_source_value(source) else {
                    Self::record_dropped_block(
                        adjustments,
                        "video",
                        "minimax_unsupported_media_source",
                    );
                    return None;
                };
                let mut obj = json!({"type": "video", "source": source_value});
                if let Some(f) = fps {
                    obj["fps"] = json!(f);
                }
                if let Some(d) = detail {
                    obj["detail"] = json!(d);
                }
                if let Some(m) = max_long_side_pixel {
                    obj["max_long_side_pixel"] = json!(m);
                }
                Some(obj)
            }
            // Minimax 对话中途插入的系统指令(`llm/api.md:1202-1211`)。
            ContentBlock::MidConvSystem(text) => {
                Some(json!({"type": "mid_conv_system", "text": text}))
            }
            // Minimax LLM API 当前不接 audio block(Step 2 omni 占位);记录后丢弃。
            ContentBlock::Audio { .. } => {
                adjustments.push(OptionAdjustment {
                    option: "content_block".into(),
                    requested: json!("audio"),
                    applied: json!(null),
                    reason: "minimax_audio_block_unsupported_in_llm_api".into(),
                });
                None
            }
            // Block kinds added to the protocol later — drop + record.
            _ => {
                Self::record_dropped_block(
                    adjustments,
                    "unknown",
                    "minimax_unsupported_content_block",
                );
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
            // Minimax catalog 暂无定价数据;返回零成本占位。
            pricing: Some(ModelPricing::flat_text("USD", 0.0, 0.0)),
        }
    }
}
