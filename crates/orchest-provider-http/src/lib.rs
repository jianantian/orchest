//! `orchest-provider-http` — REST + SSE wire dialects (the light weight tier).
//!
//! Home of the LLM chat adapters (Anthropic, OpenAI, DeepSeek, OpenRouter,
//! Volcengine-Ark, Minimax), migrated here from `agent-runtime-providers` in
//! Issue 005. They implement [`ChatModel`] (the renamed `ModelAdapter`) over a
//! shared `reqwest` client + SSE decoder, and are registered through the wall
//! (`orchest-provider`) via [`chat_entries`]. The deprecated
//! `agent-runtime-providers` crate is now a thin re-export of this one, so
//! `core/node/py` keep their `create_adapter_from_config` /
//! `normalize_provider_model` / `ModelAdapter` entry points unchanged.
//!
//! REST/SSE one-shot ASR/TTS (Issue 006) and the minimax-music gen-task (Issue
//! 007) join the asr/tts/gen entry functions later.

pub mod catalog;
pub use catalog::{
    LlmModelEntry, LlmModelList, LlmProviderInfo, Modality, ModelScene, ThinkingSpec,
};

pub mod defaults;
pub mod pricing;
pub mod registry;
pub mod types;
pub use types::*;

pub mod asr;
pub mod gen;

pub mod providers;
pub use providers::{
    AnthropicAdapter, AnthropicConfig, DeepSeekAdapter, DeepSeekConfig, MinimaxAdapter,
    MinimaxConfig, OpenAiAdapter, OpenAiConfig, OpenRouterAdapter, OpenRouterConfig,
    VolcengineAdapter, VolcengineConfig,
};

pub(crate) mod http;
pub(crate) mod protocol;
pub(crate) mod role_compat;
pub(crate) mod sse;

pub mod telemetry;

pub use registry::{ProviderFactory, ProviderRegistry};

use std::future::Future;
use std::sync::Arc;
use tokio::sync::mpsc;

use orchest_protocol::{Asr, CatalogEntry, ChatModel, EventStream, GenTask, ProtocolError, Tts};
use orchest_provider_core::registry::{Entry, ProviderConfig};

// ---------------------------------------------------------------------------
// Wall registration (Issue 004 surface, filled here in Issue 005)
// ---------------------------------------------------------------------------

/// LLM chat dialects as registry entries. One [`Entry`] per **enumerable**
/// catalog model: the static [`CapabilityDescriptor`](orchest_protocol::CapabilityDescriptor)
/// (queried before instantiation) plus a factory that builds the concrete
/// adapter via [`create_adapter_from_config`]. Dynamic-gateway providers
/// (OpenRouter) cannot be enumerated statically and stay reachable only through
/// the free-function path (`create_adapter_from_config`), which `node/py` use.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn chat_entries() -> Vec<Entry<Box<dyn ChatModel>>> {
    catalog::list_models()
        .map(|m| {
            let model_id = m.model_id; // 'static "provider/model"
            Entry::new(CatalogEntry::descriptor(m), move |cfg: &ProviderConfig| {
                create_adapter_from_config(ProviderRuntimeConfig {
                    model: model_id.to_string(),
                    api_key: cfg.api_key.clone(),
                    api_key_env: None,
                    api_url: cfg.api_url.clone(),
                    max_tokens: cfg.max_tokens,
                })
                .map_err(ProtocolError::from)
            })
        })
        .collect()
}

/// REST/batch one-shot ASR dialects (AssemblyAI; Speechmatics next). Streaming
/// ASR lives in `orchest-provider-stream`.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn asr_entries() -> Vec<Entry<Box<dyn Asr>>> {
    vec![
        Entry::new(asr::assemblyai::entry_descriptor(), |cfg| {
            Ok(Box::new(asr::assemblyai::from_provider_config(cfg)?) as Box<dyn Asr>)
        }),
        Entry::new(asr::speechmatics::entry_descriptor(), |cfg| {
            Ok(Box::new(asr::speechmatics::from_provider_config(cfg)?) as Box<dyn Asr>)
        }),
    ]
}

/// REST TTS dialects. Filled in Issue 006.
pub fn tts_entries() -> Vec<Entry<Box<dyn Tts>>> {
    Vec::new()
}

/// REST gen-task dialects (music generation). Synchronous REST generation
/// (Minimax, Aliyun fun-music) is presented over the submit -> poll -> fetch
/// surface via `SyncGenCache`; asynchronous REST generation (Mureka, Suno)
/// uses real submit -> poll -> fetch. Construction is sync (fits the wall).
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn gen_entries() -> Vec<Entry<Box<dyn GenTask>>> {
    vec![
        Entry::new(gen::minimax_music::entry_descriptor(), |cfg| {
            Ok(Box::new(gen::minimax_music::from_provider_config(cfg)?) as Box<dyn GenTask>)
        }),
        Entry::new(gen::mureka::entry_descriptor(), |cfg| {
            Ok(Box::new(gen::mureka::from_provider_config(cfg)?) as Box<dyn GenTask>)
        }),
        Entry::new(gen::aliyun_music::entry_descriptor(), |cfg| {
            Ok(Box::new(gen::aliyun_music::from_provider_config(cfg)?) as Box<dyn GenTask>)
        }),
        Entry::new(gen::suno::entry_descriptor(), |cfg| {
            Ok(Box::new(gen::suno::from_provider_config(cfg)?) as Box<dyn GenTask>)
        }),
    ]
}

// ---------------------------------------------------------------------------
// Free-function construction API (preserved for core/node/py via the
// `agent-runtime-providers` re-export — same signatures as before the move).
// ---------------------------------------------------------------------------

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

    // ADR-0002 protocol-factory path (slice 001): providers with a migrated
    // `ProviderEntry` resolve through a `ProtocolFactory`; the rest stay on the
    // legacy `ProviderFactory` bridge below until their slice migrates them.
    if let Some(entry) = protocol::provider_entry(normalized.provider) {
        return create_adapter_via_protocol(entry, &normalized, &config);
    }

    let factory = registry
        .get(normalized.provider)
        .ok_or_else(|| unknown_provider(normalized.provider, &registry))?;

    let api_key = resolve_api_key(
        factory.default_api_key_env(),
        config.api_key.as_deref(),
        config.api_key_env.as_deref(),
    )?;
    let max_tokens = config.max_tokens.unwrap_or(defaults::MAX_TOKENS);

    factory.create_adapter(normalized.model, max_tokens, api_key, config.api_url)
}

/// Resolve `normalized` into a [`ResolvedModel`](protocol::ResolvedModel) and
/// construct through the protocol factory. The protocol is the explicit segment
/// if the model string carried one (erroring if the provider doesn't support
/// it), else auto-detected per the ADR precedence rules (slice 008).
fn create_adapter_via_protocol(
    entry: &'static protocol::ProviderEntry,
    normalized: &NormalizedProviderModel<'_>,
    config: &ProviderRuntimeConfig,
) -> Result<Box<dyn ModelAdapter>, ModelError> {
    let proto = match normalized.protocol {
        Some(explicit) => {
            if !entry.protocols.contains(&explicit) {
                return Err(ModelError::internal(
                    format!(
                        "protocol {explicit:?} is not supported by provider '{}'",
                        entry.name
                    ),
                    "unsupported_protocol",
                ));
            }
            explicit
        }
        None => protocol::auto_detect_protocol(entry, normalized.model).ok_or_else(|| {
            ModelError::internal(
                format!("provider '{}' declares no protocols", entry.name),
                "no_protocol",
            )
        })?,
    };

    let factory = protocol::protocol_factory(proto).ok_or_else(|| {
        ModelError::internal(
            format!("no protocol factory for {proto:?}"),
            "no_protocol_factory",
        )
    })?;

    let api_key = resolve_api_key(
        entry.default_api_key_env,
        config.api_key.as_deref(),
        config.api_key_env.as_deref(),
    )?;

    let resolved = protocol::ResolvedModel {
        provider: entry,
        protocol: proto,
        model: normalized.model,
        catalog: catalog::find_model(normalized.model),
    };

    let provider_config = ProviderConfig {
        provider: entry.name.to_string(),
        model: normalized.model.to_string(),
        api_key: Some(api_key),
        api_url: config.api_url.clone(),
        max_tokens: config.max_tokens,
        options: serde_json::Value::Null,
    };

    factory
        .create_adapter(&provider_config, &resolved)
        .map_err(ModelError::from)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizedProviderModel<'a> {
    pub provider: &'a str,
    /// The explicit protocol from a `provider/protocol/model` string, or `None`
    /// when the protocol is left to auto-detection (ADR-0002 slice 008). Added as
    /// a new field; existing callers that read `provider`/`model` are unaffected.
    pub protocol: Option<protocol::Protocol>,
    pub model: &'a str,
}

/// Parse a model string into provider + optional explicit protocol + model,
/// using the ADR-0002 vocabulary-based rule: the segment after the provider is a
/// protocol **only** if it is a canonical protocol name or a provider-declared
/// alias; otherwise it stays part of the model name (so multi-segment ids like
/// `openrouter/<vendor>/<model>` are preserved).
pub fn normalize_provider_model(model: &str) -> Result<NormalizedProviderModel<'_>, ModelError> {
    let registry = ProviderRegistry::new();
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return Err(ModelError::internal(
            "model string cannot be empty",
            "invalid_model",
        ));
    }

    let Some((provider, rest)) = trimmed.split_once('/') else {
        return Ok(NormalizedProviderModel {
            provider: "anthropic",
            protocol: None,
            model: trimmed,
        });
    };

    if provider.is_empty() || rest.is_empty() {
        return Err(ModelError::internal(
            format!("invalid model string '{model}': expected 'provider/model'"),
            "invalid_model",
        ));
    }

    if registry.get(provider).is_none() {
        return Err(unknown_provider(provider, &registry));
    }

    // Vocabulary-based protocol segment: recognize the next segment as a protocol
    // only if it is a canonical name or a provider alias, else it is model text.
    if let Some((maybe_protocol, model_rest)) = rest.split_once('/') {
        if !model_rest.is_empty() {
            if let Some(proto) = protocol::recognize_protocol(provider, maybe_protocol) {
                return Ok(NormalizedProviderModel {
                    provider,
                    protocol: Some(proto),
                    model: model_rest,
                });
            }
        }
    }

    Ok(NormalizedProviderModel {
        provider,
        protocol: None,
        model: rest,
    })
}

fn resolve_api_key(
    default_env: &str,
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

    match std::env::var(default_env) {
        Ok(value) => non_empty_api_key(&value),
        Err(_) => Err(ModelError::internal(
            format!("{default_env} not set and no api_key provided"),
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

/// Chat push→pull bridge (Issue 005). Drives a [`ChatModel`]'s retained push
/// completion (`complete(.., Some(tx))`) on a background task and hands back the
/// **pulled** [`EventStream`] — the unified delivery shape ASR/TTS/realtime
/// already speak (design §1.4). This is where chat "converges onto a pulled
/// `events()`": every LLM provider reaches the pull world through here while its
/// `ModelAdapter` push transport stays the working bridge underneath until Issue
/// 008. The completion's terminal `Result` is surfaced in-band as the stream's
/// trailing `Done`/`Error` events, so a pull-only consumer needs nothing else.
pub fn events(
    model: Arc<dyn ChatModel>,
    messages: Vec<Message>,
    tools: Vec<ToolDef>,
    options: RequestOptions,
) -> EventStream {
    let (tx, stream) = EventStream::channel(64);
    tokio::spawn(async move {
        if let Err(err) = model
            .complete(&messages, &tools, &options, Some(tx.clone()))
            .await
        {
            // The push path emits its own terminal events on success; on a hard
            // failure before any were sent, surface it so the puller still ends.
            let _ = tx
                .send(StreamEvent::Error {
                    error: err.into(),
                    fatal: true,
                })
                .await;
        }
    });
    stream
}

#[cfg(test)]
mod tests;
