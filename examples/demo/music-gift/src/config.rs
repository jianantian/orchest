//! Environment-variable-driven configuration: chat model adapter + music gen task.

use std::sync::Arc;

use orchest_protocol::{ChatModel, GenTask};
use orchest_provider::registry::Registry;
use orchest_provider::{create_adapter_from_config, ProviderConfig, ProviderRuntimeConfig};

use crate::error::{AppError, AppResult};

const CHAT_MODEL_ENV: &str = "MUSIC_GIFT_CHAT_MODEL";
const CHAT_API_KEY_ENV: &str = "MUSIC_GIFT_CHAT_API_KEY";
const CHAT_API_URL_ENV: &str = "MUSIC_GIFT_CHAT_API_URL";
const CHAT_MAX_TOKENS_ENV: &str = "MUSIC_GIFT_CHAT_MAX_TOKENS";

const MUSIC_PROVIDER_ENV: &str = "MUSIC_GIFT_MUSIC_PROVIDER";
const MUSIC_MODEL_ENV: &str = "MUSIC_GIFT_MUSIC_MODEL";
const MUSIC_API_KEY_ENV: &str = "MUSIC_GIFT_MUSIC_API_KEY";
const MUSIC_API_URL_ENV: &str = "MUSIC_GIFT_MUSIC_API_URL";

const PORT_ENV: &str = "MUSIC_GIFT_PORT";
const DEFAULT_PORT: u16 = 3000;

/// All configuration needed to start the server.
pub struct AppConfig {
    pub chat_model: Arc<dyn ChatModel>,
    pub gen_task: Arc<dyn GenTask>,
    pub port: u16,
}

/// Build the chat model adapter from `MUSIC_GIFT_CHAT_*` env vars.
///
/// Same pattern as briefing-desk: `MUSIC_GIFT_CHAT_MODEL` is a `provider/model`
/// string; the API key falls back to the provider's default env var if
/// `_API_KEY` is unset.
fn build_chat_model() -> AppResult<Arc<dyn ChatModel>> {
    let model = std::env::var(CHAT_MODEL_ENV).map_err(|_| {
        AppError::Config(format!(
            "no chat model configured: set {CHAT_MODEL_ENV} to a provider/model string \
             (e.g. anthropic/claude-sonnet-4-6). See .env.example."
        ))
    })?;
    let config = ProviderRuntimeConfig {
        model,
        api_key: std::env::var(CHAT_API_KEY_ENV).ok(),
        api_key_env: None,
        api_url: std::env::var(CHAT_API_URL_ENV).ok(),
        max_tokens: std::env::var(CHAT_MAX_TOKENS_ENV)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok()),
    };
    let adapter = create_adapter_from_config(config)
        .map_err(|e| AppError::Config(format!("constructing chat model: {e}")))?;
    // Box<dyn ModelAdapter> -> Arc<dyn ChatModel>: ModelAdapter is an alias for
    // ChatModel, so the box already is a Box<dyn ChatModel>.
    Ok(Arc::from(adapter))
}

/// Build the music generation task from `MUSIC_GIFT_MUSIC_*` env vars via the
/// registry.
fn build_gen_task() -> AppResult<Arc<dyn GenTask>> {
    let provider = std::env::var(MUSIC_PROVIDER_ENV).unwrap_or_else(|_| "suno".to_string());
    let model = std::env::var(MUSIC_MODEL_ENV).unwrap_or_else(|_| "V5_5".to_string());
    let api_url = std::env::var(MUSIC_API_URL_ENV).ok();
    let api_key = std::env::var(MUSIC_API_KEY_ENV).ok();

    let registry = Registry::with_builtin();
    let mut config = ProviderConfig::new(&provider, &model);
    if let Some(key) = api_key {
        config = config.with_api_key(key);
    }
    if let Some(url) = api_url {
        config = config.with_api_url(url);
    }

    let gen_task = registry
        .gen()
        .provider(&provider)
        .build(&config)
        .map_err(|e| {
            AppError::Config(format!(
                "constructing music gen provider '{provider}': {e}. \
                 Set {MUSIC_API_KEY_ENV} and optionally {MUSIC_MODEL_ENV}."
            ))
        })?;
    Ok(Arc::from(gen_task))
}

/// Load all configuration from environment variables.
pub fn load_config() -> AppResult<AppConfig> {
    let chat_model = build_chat_model()?;
    let gen_task = build_gen_task()?;
    let port = std::env::var(PORT_ENV)
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    Ok(AppConfig {
        chat_model,
        gen_task,
        port,
    })
}
