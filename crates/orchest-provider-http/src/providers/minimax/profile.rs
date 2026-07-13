//! Minimax's Messages [`ProviderProfile`] (ADR-0002 slice 006).
//!
//! Minimax is Anthropic-Messages-compatible but forks the most on the Messages
//! side — the stress test for the profile model. It deviates on **roles** (native
//! Minimax-only roles instead of a downgrade) and **option lowering** (the
//! `thinking: {type, display}` dialect + `service_tier`). Crucially it does NOT
//! change content-block encoding or stream-event shape, so per the ADR
//! "dialect-fork threshold" it stays a profile, not a new protocol.

use serde_json::{json, Value};

use crate::protocol::{ProviderProfile, ResolvedModel, WireRole};
use crate::{CachePolicy, OptionAdjustment, RequestOptions, Role, ThinkingLevel};

/// The Minimax Messages profile. Zero-sized; behavior is in the hook impls.
pub struct MinimaxProfile;

/// The singleton attached to the Minimax `ProviderEntry`.
pub static MINIMAX_PROFILE: MinimaxProfile = MinimaxProfile;

impl ProviderProfile for MinimaxProfile {
    /// Minimax accepts its own roles natively (rather than downgrading them like
    /// the Chat providers do). `System` is handled by the adapter as a top-level
    /// `system` field, so it is not produced here.
    fn map_role(&self, _cx: &ResolvedModel<'_>, role: &Role) -> WireRole {
        WireRole(match role {
            Role::User | Role::Tool => "user",
            Role::Assistant => "assistant",
            Role::System => "system", // handled as top-level system; unreachable here
            Role::UserSystem => "user_system",
            Role::Group => "group",
            Role::SampleMessageUser => "sample_message_user",
            Role::SampleMessageAi => "sample_message_ai",
        })
    }

    /// Minimax's `thinking: {type, display}` dialect (adaptive vs budget_tokens),
    /// cache control, sampling, and `service_tier` pass-through. Reads the already
    /// serialized `max_tokens` from `body` for the budget-token ceiling. Adaptive
    /// support is derived from the resolved model id.
    fn lower_options(
        &self,
        cx: &ResolvedModel<'_>,
        options: &RequestOptions,
        body: &mut Value,
    ) -> Vec<OptionAdjustment> {
        let mut adjustments = Vec::new();
        let supports_adaptive = cx.model.starts_with("MiniMax-M3");
        let effective_max_tokens = body["max_tokens"].as_u64().unwrap_or(0) as u32;

        match options.thinking {
            ThinkingLevel::Off => {
                body["thinking"] = json!({"type": "disabled"});
            }
            level => {
                if supports_adaptive {
                    let effort = match level {
                        ThinkingLevel::Minimal | ThinkingLevel::Low => "low",
                        ThinkingLevel::Medium => "medium",
                        ThinkingLevel::High => "high",
                        ThinkingLevel::XHigh => "xhigh",
                        ThinkingLevel::Max => "max",
                        ThinkingLevel::Off => unreachable!(),
                    };
                    body["thinking"] = json!({"type": "adaptive"});
                    body["thinking"]["display"] = if options.include_thinking {
                        json!("summarized")
                    } else {
                        json!("omitted")
                    };
                    body["output_config"] = json!({"effort": effort});

                    if options.thinking_budget_tokens.is_some() {
                        adjustments.push(OptionAdjustment {
                            option: "thinking_budget_tokens".into(),
                            requested: json!(options.thinking_budget_tokens),
                            applied: json!(null),
                            reason: "unsupported_in_adaptive_thinking".into(),
                        });
                    }
                } else {
                    let budget = options.thinking_budget_tokens.unwrap_or(match level {
                        ThinkingLevel::Minimal => 1024,
                        ThinkingLevel::Low => 4096,
                        ThinkingLevel::Medium => 10240,
                        ThinkingLevel::High => 32768,
                        ThinkingLevel::XHigh => 65536,
                        ThinkingLevel::Max => effective_max_tokens,
                        ThinkingLevel::Off => unreachable!(),
                    });
                    body["thinking"] = json!({
                        "type": "enabled",
                        "budget_tokens": budget,
                    });
                    body["thinking"]["display"] = if options.include_thinking {
                        json!("summarized")
                    } else {
                        json!("omitted")
                    };
                }
            }
        }

        match options.cache_policy {
            CachePolicy::Auto => {
                body["cache_control"] = json!({"type": "ephemeral"});
            }
            CachePolicy::Long => {
                body["cache_control"] = json!({"type": "ephemeral", "ttl": "1h"});
            }
            CachePolicy::None => {}
        }

        if let Some(temp) = options.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(tp) = options.top_p {
            body["top_p"] = json!(tp);
        }

        // service_tier pass-through. Kept on RequestOptions for now (moving
        // gateway meta-options onto ProviderConfig::options per ADR Q10 is a
        // consumer-surface change, deferred out of this non-breaking hotfix).
        if let Some(tier) = &options.service_tier {
            body["service_tier"] = json!(tier);
        }

        adjustments
    }
}
