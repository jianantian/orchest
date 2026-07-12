//! OpenRouter's Chat [`ProviderProfile`] (ADR-0002 slice 004).
//!
//! OpenRouter is a multi-provider gateway, OpenAI-compatible Chat with its own
//! `reasoning` object dialect. `lower_options` owns that. Usage-missing handling
//! is the canonical default ([`ProviderProfile::interpret_usage`]) shared with
//! OpenAI/DeepSeek, so it is not overridden here. Reasoning replay
//! (`reasoning_details`) stays inline in the adapter for now — it is fallible
//! (invalid replay is an error), which the `replay_reasoning` hook signature does
//! not carry; it migrates when the adapters collapse in v0.12.

use serde_json::{json, Value};

use crate::protocol::{ProviderProfile, ResolvedModel};
use crate::{OptionAdjustment, RequestOptions, ThinkingLevel};

/// The OpenRouter Chat profile. Zero-sized; behavior is in the hook impls.
pub struct OpenRouterProfile;

/// The singleton attached to the OpenRouter `ProviderEntry`.
pub static OPENROUTER_PROFILE: OpenRouterProfile = OpenRouterProfile;

impl ProviderProfile for OpenRouterProfile {
    /// OpenRouter expresses reasoning as a `reasoning` object: `max_tokens` when
    /// a budget is set, otherwise a granular `effort`, plus `exclude: true` when
    /// the caller wants reasoning off the response. Sampling is always forwarded.
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
}
