//! Fireworks' Chat [`ProviderProfile`].
//!
//! OpenAI-compatible Chat. Reasoning is requested with the canonical
//! `reasoning_effort` (`low` / `medium` / `high`) and streamed back as
//! `delta.reasoning_content` (<https://docs.fireworks.ai/guides/reasoning>).
//! Fireworks has no token-budget or output-exclusion control on this surface,
//! and its model list is dynamic, so capabilities are assumed rather than read
//! from the catalog.

use crate::protocol::{
    AdjustmentSpec, AppliedValue, OptionSupport, ProviderProfile, RequestOption, ResolvedModel,
};
use crate::{
    CacheCapability, CapabilitySource, ModelCapabilities, ReasoningCapability, ThinkingLevel,
};

/// The Fireworks Chat profile.
pub struct FireworksProfile;

/// The singleton attached to the Fireworks `ProviderEntry`.
pub static FIREWORKS_PROFILE: FireworksProfile = FireworksProfile;

impl ProviderProfile for FireworksProfile {
    fn option_support(&self, _cx: &ResolvedModel<'_>, option: RequestOption) -> OptionSupport {
        match option {
            RequestOption::Reasoning => OptionSupport::Supported,
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
                    "Fireworks does not support output exclusion for reasoning",
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

    fn capabilities(&self, _cx: &ResolvedModel<'_>, max_output_tokens: u32) -> ModelCapabilities {
        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: true,
            reasoning: ReasoningCapability {
                supported: true,
                efforts: vec![
                    ThinkingLevel::Low,
                    ThinkingLevel::Medium,
                    ThinkingLevel::High,
                ],
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
            context_window_size: None,
            source: CapabilitySource::Assumed,
            pricing: None,
        }
    }
}
