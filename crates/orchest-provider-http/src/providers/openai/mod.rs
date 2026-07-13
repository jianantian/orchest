//! OpenAI Chat profile + construction (ADR-0002 Phase 3).
//!
//! OpenAI is canonical Chat Completions over the shared [`ChatAdapter`]
//! (crate::chat). Its only deviations, carried here as [`OpenAiProfile`]:
//! - reasoning / thinking-budget option support (Strict errors), and
//! - capability facts — `openai_pricing` plus name-prefix fallbacks for
//!   reasoning support / context window when a model is absent from the catalog.

mod request;

use std::env;

use orchest_protocol::{ChatModel, ProtocolError};
use orchest_provider_core::registry::ProviderConfig;

use crate::protocol::{
    AdjustmentSpec, AppliedValue, OptionSupport, ProviderProfile, RequestOption, ResolvedModel,
};
use crate::{
    defaults, CacheCapability, CapabilitySource, ModelCapabilities, ModelError,
    ReasoningCapability, ThinkingLevel,
};

use request::normalize_chat_url;

/// The OpenAI Chat profile.
pub struct OpenAiProfile;

/// The singleton attached to the OpenAI `ProviderEntry`.
pub static OPENAI_PROFILE: OpenAiProfile = OpenAiProfile;

impl OpenAiProfile {
    /// Reasoning support: canonical from the catalog row; the name-prefix table is
    /// a documented fallback only for models absent from the catalog.
    fn supports_reasoning(cx: &ResolvedModel<'_>) -> bool {
        cx.catalog
            .map(|c| c.thinking.is_some())
            .unwrap_or_else(|| request::supports_reasoning_model(cx.model))
    }

    /// Context window: catalog first, name-prefix fallback otherwise.
    fn context_window(cx: &ResolvedModel<'_>) -> u64 {
        cx.catalog
            .map(|c| c.context_window)
            .unwrap_or_else(|| request::openai_context_window(cx.model))
    }
}

impl ProviderProfile for OpenAiProfile {
    fn option_support(&self, cx: &ResolvedModel<'_>, option: RequestOption) -> OptionSupport {
        match option {
            RequestOption::Reasoning => {
                if Self::supports_reasoning(cx) {
                    OptionSupport::Supported
                } else {
                    OptionSupport::Unsupported {
                        strict_error: Some((
                            "unsupported_reasoning_model",
                            "model does not declare OpenAI reasoning support",
                        )),
                        disables_thinking: true,
                        adjustment: Some(AdjustmentSpec {
                            option: "thinking",
                            applied: AppliedValue::Str("Off"),
                            reason: "unsupported_reasoning_model",
                        }),
                    }
                }
            }
            RequestOption::ThinkingBudget => OptionSupport::Unsupported {
                strict_error: Some((
                    "unsupported_thinking_budget",
                    "thinking_budget_tokens is not supported by OpenAI",
                )),
                disables_thinking: false,
                adjustment: Some(AdjustmentSpec {
                    option: "thinking_budget_tokens",
                    applied: AppliedValue::Null,
                    reason: "unsupported_by_provider",
                }),
            },
            RequestOption::ReasoningOutputExclusion => OptionSupport::Supported,
        }
    }

    fn capabilities(&self, cx: &ResolvedModel<'_>, max_output_tokens: u32) -> ModelCapabilities {
        let supports_reasoning = Self::supports_reasoning(cx);
        ModelCapabilities {
            streaming: true,
            tool_use: true,
            parallel_tool_use: true,
            reasoning: ReasoningCapability {
                supported: supports_reasoning,
                efforts: if supports_reasoning {
                    vec![
                        ThinkingLevel::Low,
                        ThinkingLevel::Medium,
                        ThinkingLevel::High,
                    ]
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
            context_window_size: Some(Self::context_window(cx)),
            source: CapabilitySource::Static,
            pricing: Some(crate::pricing::openai_pricing(cx.model)),
        }
    }
}

/// Resolve the OpenAI API key (explicit `config` or `OPENAI_API_KEY`).
fn resolve_api_key(config: &ProviderConfig) -> Result<String, ModelError> {
    config
        .api_key
        .clone()
        .or_else(|| env::var("OPENAI_API_KEY").ok())
        .ok_or_else(|| {
            ModelError::internal(
                "OPENAI_API_KEY not set and no api_key provided",
                "missing_api_key",
            )
        })
}

/// Resolve the OpenAI endpoint (config / `OPENAI_API_URL` / `OPENAI_BASE_URL` /
/// default) and append the canonical Chat path.
fn resolve_url(config_api_url: Option<&str>) -> Result<String, ModelError> {
    let api_url = config_api_url
        .map(String::from)
        .or_else(|| env::var("OPENAI_API_URL").ok())
        .or_else(|| env::var("OPENAI_BASE_URL").ok())
        .unwrap_or_else(|| defaults::openai::API_URL.to_string());
    if api_url.trim().is_empty() {
        return Err(ModelError::internal(
            "OpenAI API URL cannot be empty",
            "invalid_api_url",
        ));
    }
    Ok(normalize_chat_url(&api_url))
}

/// ADR-0002 Chat construction: builds the shared [`ChatAdapter`](crate::chat)
/// with OpenAI's resolved endpoint + profile (referenced as data by the OpenAI
/// `ProviderEntry.build_adapter`; no provider-name match in the core).
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (workspace convention)
pub fn build_chat_adapter(
    config: &ProviderConfig,
    resolved: &ResolvedModel<'_>,
) -> Result<Box<dyn ChatModel>, ProtocolError> {
    let api_key = resolve_api_key(config).map_err(ProtocolError::from)?;
    let api_url = resolve_url(config.api_url.as_deref()).map_err(ProtocolError::from)?;
    crate::chat::ChatAdapter::build(config, resolved, api_key, api_url)
}

#[cfg(test)]
mod tests;
