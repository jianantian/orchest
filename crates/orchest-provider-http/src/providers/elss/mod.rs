//! Elss API gateway adapter.
//!
//! Elss (<https://elss.ai>) is an API aggregator that exposes a unified key
//! for Anthropic, OpenAI, and other providers. It supports two protocols:
//!
//! - **Anthropic** (`/v1/messages`): Claude models. Preserves prompt caching,
//!   thinking, and all Messages API features. Auth: `x-api-key` or
//!   `Authorization: Bearer`.
//! - **OpenAI** (`/v1/chat/completions`): GPT and other models. Auth:
//!   `Authorization: Bearer`.
//!
//! ## Model string routing
//!
//! The protocol is chosen by the model string's sub-prefix:
//!
//! - `elss/claude-sonnet-5` -> Anthropic protocol (default for Claude models)
//! - `elss/gpt-4.1` -> OpenAI protocol (default for non-Claude models)
//! - `elss/anthropic/claude-sonnet-5` -> explicit Anthropic protocol
//! - `elss/openai/gpt-4.1` -> explicit OpenAI protocol
//!
//! The explicit form lets users pick the protocol for models that both
//! endpoints support (e.g. `elss/anthropic/claude-sonnet-5` vs
//! `elss/openai/claude-sonnet-5`).

use crate::providers::anthropic::{AnthropicAdapter, AnthropicConfig};
use crate::providers::openai::{OpenAiAdapter, OpenAiConfig};
use crate::registry::ProviderFactory;
use orchest_protocol::{ModelAdapter, ModelError};

/// Factory that registers `"elss"` as a provider, routing to the Anthropic or
/// OpenAI adapter based on the model name.
pub struct ElssFactory;

impl ProviderFactory for ElssFactory {
    fn provider_name(&self) -> &'static str {
        "elss"
    }

    fn create_adapter(
        &self,
        model: &str,
        max_tokens: u32,
        api_key: String,
        api_url: Option<String>,
    ) -> Result<Box<dyn ModelAdapter>, ModelError> {
        let (protocol, model_name) = parse_elss_model(model);

        match protocol {
            Protocol::Anthropic => {
                let url = resolve_api_url(api_url, "v1/messages");
                let adapter = AnthropicAdapter::from_config(AnthropicConfig {
                    model: model_name.to_string(),
                    max_tokens,
                    api_key: Some(api_key),
                    api_url: Some(url),
                })?;
                Ok(Box::new(adapter))
            }
            Protocol::Openai => {
                let url = resolve_api_url(api_url, "v1/chat/completions");
                let adapter = OpenAiAdapter::from_config(OpenAiConfig {
                    model: model_name.to_string(),
                    max_tokens,
                    api_key: Some(api_key),
                    api_url: Some(url),
                })?;
                Ok(Box::new(adapter))
            }
        }
    }

    fn default_api_key_env(&self) -> &'static str {
        crate::defaults::elss::API_KEY_ENV
    }
}

/// The wire protocol to use for an Elss model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protocol {
    Anthropic,
    Openai,
}

/// Parse an Elss model string into (protocol, model_name).
///
/// - `anthropic/<model>` -> explicit Anthropic
/// - `openai/<model>` -> explicit OpenAI
/// - `claude-*` -> auto-detected Anthropic
/// - everything else -> auto-detected OpenAI
fn parse_elss_model(model: &str) -> (Protocol, &str) {
    // Explicit protocol override: `anthropic/...` or `openai/...`
    if let Some(rest) = model.strip_prefix("anthropic/") {
        return (Protocol::Anthropic, rest);
    }
    if let Some(rest) = model.strip_prefix("openai/") {
        return (Protocol::Openai, rest);
    }

    // Auto-detect: Claude models default to Anthropic protocol.
    let lower = model.to_ascii_lowercase();
    if lower.starts_with("claude") {
        (Protocol::Anthropic, model)
    } else {
        (Protocol::Openai, model)
    }
}

/// Resolve the Elss API URL for a given protocol path.
///
/// If the caller provided an explicit `api_url`, use it as-is. Otherwise
/// build `https://api.elss.ai/<path>` (or respect `ELSS_API_URL` env).
fn resolve_api_url(api_url: Option<String>, path: &str) -> String {
    if let Some(url) = api_url {
        return url;
    }

    std::env::var(crate::defaults::elss::API_URL_ENV)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|base| {
            let base = base.trim_end_matches('/');
            if base.ends_with("/v1/messages") || base.ends_with("/v1/chat/completions") {
                base.to_string()
            } else if base.ends_with("/v1") {
                format!("{base}/{path}")
            } else {
                format!("{base}/v1/{path}")
            }
        })
        .unwrap_or_else(|| format!("https://api.elss.ai/v1/{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_models_default_to_anthropic() {
        assert_eq!(
            parse_elss_model("claude-sonnet-5"),
            (Protocol::Anthropic, "claude-sonnet-5")
        );
        assert_eq!(
            parse_elss_model("claude-opus-4-7"),
            (Protocol::Anthropic, "claude-opus-4-7")
        );
    }

    #[test]
    fn non_claude_models_default_to_openai() {
        assert_eq!(parse_elss_model("gpt-4.1"), (Protocol::Openai, "gpt-4.1"));
        assert_eq!(
            parse_elss_model("gemini-2.5-flash"),
            (Protocol::Openai, "gemini-2.5-flash")
        );
    }

    #[test]
    fn explicit_anthropic_override() {
        assert_eq!(
            parse_elss_model("anthropic/gpt-4.1"),
            (Protocol::Anthropic, "gpt-4.1")
        );
    }

    #[test]
    fn explicit_openai_override() {
        assert_eq!(
            parse_elss_model("openai/claude-sonnet-5"),
            (Protocol::Openai, "claude-sonnet-5")
        );
    }

    // NOTE: env var tests use a unique key per test to avoid cross-test
    // pollution from parallel execution. The `ELSS_API_URL` env var is
    // shared global state; we remove it at the start of each test and only
    // set it in the test that needs it. But since tests run in parallel,
    // we use a separate approach: test `resolve_api_url` logic directly
    // without env var by passing explicit Some(url) where needed, and
    // accept that the None path reads the env.

    #[test]
    fn resolve_api_url_explicit_anthropic() {
        assert_eq!(
            resolve_api_url(
                Some("https://api.elss.ai/v1/messages".into()),
                "v1/messages"
            ),
            "https://api.elss.ai/v1/messages"
        );
    }

    #[test]
    fn resolve_api_url_explicit_openai() {
        assert_eq!(
            resolve_api_url(
                Some("https://api.elss.ai/v1/chat/completions".into()),
                "v1/chat/completions"
            ),
            "https://api.elss.ai/v1/chat/completions"
        );
    }

    #[test]
    fn resolve_api_url_respects_explicit_url() {
        assert_eq!(
            resolve_api_url(
                Some("https://custom.example.com/v1/messages".into()),
                "v1/messages"
            ),
            "https://custom.example.com/v1/messages"
        );
    }
}
