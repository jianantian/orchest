//! Volcengine's Chat [`ProviderProfile`] (ADR-0002 slice 003).
//!
//! Volcengine (Ark) is OpenAI-compatible Chat with two deviations: its thinking
//! field is a bare `thinking: {type}` (no `reasoning_effort`), and it cannot
//! exclude reasoning output. `lower_options` owns the first; `option_support`
//! declares the second so the shared `CompatibilityPolicy` handling
//! ([`resolve_reasoning_exclusion`](crate::protocol::resolve_reasoning_exclusion))
//! degrades/errors uniformly instead of a private adapter branch.

use serde_json::{json, Value};

use crate::protocol::{OptionSupport, ProviderProfile, RequestOption, ResolvedModel};
use crate::{CachePolicy, OptionAdjustment, RequestOptions, ThinkingLevel};

/// The Volcengine Chat profile. Zero-sized; all behavior is in the hook impls.
pub struct VolcengineProfile;

/// The singleton attached to the Volcengine `ProviderEntry`.
pub static VOLCENGINE_PROFILE: VolcengineProfile = VolcengineProfile;

impl ProviderProfile for VolcengineProfile {
    /// Volcengine expresses reasoning as a bare top-level `thinking: {type}`
    /// (no `reasoning_effort`) and omits sampling when thinking is enabled.
    /// `options.thinking == Off` here reflects the effective decision (the shared
    /// exclusion handling ran before this hook).
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

        if options.thinking_budget_tokens.is_some() {
            adjustments.push(OptionAdjustment {
                option: "thinking_budget_tokens".into(),
                requested: json!(options.thinking_budget_tokens),
                applied: json!(null),
                reason: "unsupported_by_provider".into(),
            });
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

    /// Volcengine cannot exclude reasoning output. Everything else is canonical.
    fn option_support(&self, _cx: &ResolvedModel<'_>, option: RequestOption) -> OptionSupport {
        match option {
            RequestOption::ReasoningOutputExclusion => OptionSupport::Unsupported {
                code: "unsupported_reasoning_output_exclusion",
                message: "Volcengine does not support output exclusion for reasoning",
            },
        }
    }
}
