//! LLM provider adapters: Anthropic, OpenAI, DeepSeek, OpenRouter, Volcengine.

pub mod catalog;
pub use catalog::{
    LlmModelEntry, LlmModelList, LlmProviderInfo, Modality, ModelScene, ThinkingSpec,
};

pub mod defaults;
pub mod pricing;
pub mod registry;
pub mod types;
pub use types::*;

pub mod providers;
pub use providers::{
    AnthropicAdapter, AnthropicConfig, DeepSeekAdapter, DeepSeekConfig, OpenAiAdapter,
    OpenAiConfig, OpenRouterAdapter, OpenRouterConfig, VolcengineAdapter, VolcengineConfig,
};

pub(crate) mod http;
pub(crate) mod sse;

pub mod telemetry;

pub use registry::{ProviderFactory, ProviderRegistry};

use std::future::Future;
use tokio::sync::mpsc;

pub fn create_adapter(
    model: &str,
    api_key: Option<String>,
) -> Result<Box<dyn ModelAdapter>, ModelError> {
    create_adapter_from_config(ProviderRuntimeConfig {
        model: model.to_string(),
        api_key,
        api_key_env: None,
        api_url: None,
        max_tokens: None,
    })
}

pub fn create_adapter_from_config(
    config: ProviderRuntimeConfig,
) -> Result<Box<dyn ModelAdapter>, ModelError> {
    let registry = ProviderRegistry::new();
    let normalized = normalize_provider_model(&config.model)?;

    let factory = registry
        .get(normalized.provider)
        .ok_or_else(|| unknown_provider(normalized.provider, &registry))?;

    let api_key = resolve_api_key(
        factory,
        config.api_key.as_deref(),
        config.api_key_env.as_deref(),
    )?;
    let max_tokens = config.max_tokens.unwrap_or(defaults::MAX_TOKENS);

    factory.create_adapter(normalized.model, max_tokens, api_key, config.api_url)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizedProviderModel<'a> {
    pub provider: &'a str,
    pub model: &'a str,
}

pub fn normalize_provider_model(model: &str) -> Result<NormalizedProviderModel<'_>, ModelError> {
    let registry = ProviderRegistry::new();
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return Err(ModelError::internal(
            "model string cannot be empty",
            "invalid_model",
        ));
    }

    let Some((provider, model_name)) = trimmed.split_once('/') else {
        return Ok(NormalizedProviderModel {
            provider: "anthropic",
            model: trimmed,
        });
    };

    if provider.is_empty() || model_name.is_empty() {
        return Err(ModelError::internal(
            format!("invalid model string '{model}': expected 'provider/model'"),
            "invalid_model",
        ));
    }

    if registry.get(provider).is_some() {
        Ok(NormalizedProviderModel {
            provider,
            model: model_name,
        })
    } else {
        Err(unknown_provider(provider, &registry))
    }
}

fn resolve_api_key(
    factory: &dyn ProviderFactory,
    explicit: Option<&str>,
    api_key_env: Option<&str>,
) -> Result<String, ModelError> {
    if let Some(value) = explicit {
        return non_empty_api_key(value);
    }
    if let Some(env_name) = api_key_env {
        if env_name.trim().is_empty() {
            return Err(ModelError::internal(
                "api_key_env cannot be empty",
                "invalid_api_key_env",
            ));
        }
        return match std::env::var(env_name) {
            Ok(value) => non_empty_api_key(&value),
            Err(_) => Err(ModelError::internal(
                format!("API key env var '{env_name}' is not set"),
                "missing_api_key",
            )),
        };
    }

    let env_name = factory.default_api_key_env();
    match std::env::var(env_name) {
        Ok(value) => non_empty_api_key(&value),
        Err(_) => Err(ModelError::internal(
            format!("{env_name} not set and no api_key provided"),
            "missing_api_key",
        )),
    }
}

fn non_empty_api_key(value: &str) -> Result<String, ModelError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(ModelError::internal(
            "API key cannot be empty",
            "invalid_api_key",
        ))
    } else {
        Ok(trimmed.to_string())
    }
}

fn unknown_provider(provider: &str, registry: &ProviderRegistry) -> ModelError {
    let supported = registry.supported_providers().join(", ");
    ModelError::internal(
        format!("unknown provider '{provider}': supported providers are {supported}"),
        "unknown_provider",
    )
}

pub fn stream_chat<'a>(
    adapter: &'a dyn ModelAdapter,
    messages: &'a [Message],
    tools: &'a [ToolDef],
    options: &'a RequestOptions,
) -> (
    impl Future<Output = Result<ModelResponse, ModelError>> + 'a,
    mpsc::Receiver<StreamEvent>,
) {
    let (tx, rx) = mpsc::channel(64);
    let future = adapter.complete(messages, tools, options, Some(tx));
    (future, rx)
}

pub async fn chat(
    adapter: &dyn ModelAdapter,
    messages: &[Message],
    tools: &[ToolDef],
    options: &RequestOptions,
) -> Result<ModelResponse, ModelError> {
    adapter.complete(messages, tools, options, None).await
}

#[cfg(test)]
mod tests;
