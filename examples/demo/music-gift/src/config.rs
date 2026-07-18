//! Environment-variable-driven configuration: chat model adapter + music gen task.

use std::sync::Arc;

use orchest::model::ModelAdapter;
use orchest::run::AgentConfig;
use orchest::tool::agent_as_tool::ContextMode;
use orchest::tool::registry::ToolRegistry;
use orchest::tool::Tool;
use orchest_protocol::{ChatModel, GenTask};
use orchest_provider::registry::Registry;
use orchest_provider::{create_adapter_from_config, ProviderConfig, ProviderRuntimeConfig};

use crate::error::{AppError, AppResult};
use crate::prompts::COUNTDOWN_TEMPLATE;
use crate::tools::countdown;


const CHAT_MODEL_ENV: &str = "MUSIC_GIFT_CHAT_MODEL";
const CHAT_API_KEY_ENV: &str = "MUSIC_GIFT_CHAT_API_KEY";
const CHAT_API_URL_ENV: &str = "MUSIC_GIFT_CHAT_API_URL";
const CHAT_MAX_TOKENS_ENV: &str = "MUSIC_GIFT_CHAT_MAX_TOKENS";

/// Per-component model overrides. Each falls back to the chat model env vars
/// if not set — most deployments use one provider for everything.
const COUNTDOWN_MODEL_ENV: &str = "MUSIC_GIFT_COUNTDOWN_MODEL";
const MUSIC_PROMPT_MODEL_ENV: &str = "MUSIC_GIFT_MUSIC_PROMPT_MODEL";

const MUSIC_PROVIDER_ENV: &str = "MUSIC_GIFT_MUSIC_PROVIDER";
const MUSIC_MODEL_ENV: &str = "MUSIC_GIFT_MUSIC_MODEL";
const MUSIC_API_KEY_ENV: &str = "MUSIC_GIFT_MUSIC_API_KEY";
const MUSIC_API_URL_ENV: &str = "MUSIC_GIFT_MUSIC_API_URL";

const PORT_ENV: &str = "MUSIC_GIFT_PORT";
const DEFAULT_PORT: u16 = 3000;

/// All configuration needed to start the server.
pub struct AppConfig {
    pub chat_model: Arc<dyn ChatModel>,
    pub music_prompt_model: Arc<dyn ChatModel>,
    pub gen_task: Arc<dyn GenTask>,
    /// Music provider identity (e.g. "suno"), resolved once here from
    /// `MUSIC_GIFT_MUSIC_PROVIDER`. Handlers must use this, never re-read env.
    pub music_provider: String,
    pub port: u16,
    pub countdown_tool: Option<Arc<dyn Tool>>,
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

/// Build a model adapter from a specific env var prefix, falling back to the
/// main chat model env vars. For optional per-component model configuration:
/// set `{prefix}_MODEL` to override, or leave unset to share the chat model.
fn build_model_or_default(model_env: &str) -> AppResult<Arc<dyn ChatModel>> {
    let model = std::env::var(model_env).or_else(|_| std::env::var(CHAT_MODEL_ENV)).map_err(|_| {
        AppError::Config(format!("no model configured: set {CHAT_MODEL_ENV} or {model_env}"))
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
        .map_err(|e| AppError::Config(format!("constructing model ({model_env}): {e}")))?;
    Ok(Arc::from(adapter))
}

/// Resolve the music provider identity from `MUSIC_GIFT_MUSIC_PROVIDER`.
/// Called once at startup; the result travels on `AppConfig`/`AppState`.
fn music_provider() -> String {
    std::env::var(MUSIC_PROVIDER_ENV).unwrap_or_else(|_| "suno".to_string())
}

/// Build the music generation task from `MUSIC_GIFT_MUSIC_*` env vars via the
/// registry.
fn build_gen_task(provider: &str) -> AppResult<Arc<dyn GenTask>> {
    let model = std::env::var(MUSIC_MODEL_ENV).unwrap_or_else(|_| "V5_5".to_string());
    let api_url = std::env::var(MUSIC_API_URL_ENV).ok();
    let api_key = std::env::var(MUSIC_API_KEY_ENV).ok();

    let registry = Registry::with_builtin();
    let mut config = ProviderConfig::new(provider, &model);
    if let Some(key) = api_key {
        config = config.with_api_key(key);
    }
    if let Some(url) = api_url {
        config = config.with_api_url(url);
    }

    let gen_task = registry
        .gen()
        .provider(provider)
        .build(&config)
        .map_err(|e| {
            AppError::Config(format!(
                "constructing music gen provider '{provider}': {e}. \
                 Set {MUSIC_API_KEY_ENV} and optionally {MUSIC_MODEL_ENV}."
            ))
        })?;
    Ok(Arc::from(gen_task))
}


/// Build the countdown subagent tool once at startup.
///
/// Returns `None` if the countdown prompt template is empty
/// (i.e., the prompt file is missing or blank).
pub fn build_countdown_tool(
    model: Arc<dyn ModelAdapter>,
) -> Option<Arc<dyn Tool>> {
    if COUNTDOWN_TEMPLATE.is_empty() {
        eprintln!("[music-gift] countdown disabled: template is empty");
        return None;
    }

    let config = match AgentConfig::builder("music-gift/countdown")
        .system_prompt(COUNTDOWN_TEMPLATE.as_str())
        .max_steps(1)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[music-gift] countdown disabled: building config: {e}");
            return None;
        }
    };

    config
        .as_tool("generate_countdown", "Generate a birthday countdown HTML block.")
        .model(model)
        .registry(ToolRegistry::new())
        .context_mode(ContextMode::Fresh)
        .input_mapper(|input: serde_json::Value| {
            let params = countdown::countdown_params_from_json(&input);
            let prompt = countdown::build_prompt(&params);
            Ok(prompt)
        })
        .output_extractor(|output: serde_json::Value| {
            let html = output.get("output").and_then(serde_json::Value::as_str).unwrap_or("");
            serde_json::json!({ "html": countdown::strip_code_fences(html) })
        })
        .build()
        .inspect_err(|e| eprintln!("[music-gift] countdown disabled: building tool: {e}"))
        .ok()
}
/// Load all configuration from environment variables.
pub fn load_config() -> AppResult<AppConfig> {
    let chat_model = build_chat_model()?;
    let countdown_model = build_model_or_default(COUNTDOWN_MODEL_ENV)?;
    let music_prompt_model = build_model_or_default(MUSIC_PROMPT_MODEL_ENV)?;
    let music_provider = music_provider();
    let gen_task = build_gen_task(&music_provider)?;
    let countdown_tool = build_countdown_tool(countdown_model.clone());
    let port = std::env::var(PORT_ENV)
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    Ok(AppConfig {
        chat_model,
        music_prompt_model,
        gen_task,
        music_provider,
        port,
        countdown_tool,
    })
}
