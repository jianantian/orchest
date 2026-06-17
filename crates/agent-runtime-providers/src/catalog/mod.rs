//! Static model catalog for discovery — no credentials needed.
//!
//! Use `list_providers()` to see all supported providers and their models,
//! or `list_models()` to iterate all individually enumerable models.
//!
//! OpenRouter and similar gateway providers cannot enumerate their full model
//! list statically; they appear as `LlmModelList::Dynamic` entries.

use std::sync::LazyLock;

use agent_runtime_model::ModelPricing;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// An individually enumerable LLM model.
#[derive(Debug, Clone)]
pub struct LlmModelEntry {
    /// Full model ID to put in `ProviderRuntimeConfig { model: "..." }`.
    /// Always in `"provider/model-name"` form, e.g. `"anthropic/claude-opus-4-8"`.
    pub model_id: &'static str,
    pub provider: &'static str,
    pub display_name: &'static str,
    pub context_window: u64,
    pub max_output_tokens: Option<u32>,
    pub pricing: Option<ModelPricing>,
}

/// How a provider exposes its model list.
#[derive(Debug, Clone)]
pub enum LlmModelList {
    /// The provider has a fixed, known set of models.
    Known(Vec<LlmModelEntry>),
    /// The provider is a dynamic gateway (e.g. OpenRouter). Models cannot be
    /// enumerated here; see `description` for usage guidance.
    Dynamic {
        /// Human-readable explanation of what this provider is and how to use it.
        description: &'static str,
        /// Pattern for model IDs accepted by this provider.
        model_id_format: &'static str,
        /// A concrete example model ID the user can pass directly.
        model_id_example: &'static str,
    },
}

/// Top-level metadata about a provider.
#[derive(Debug, Clone)]
pub struct LlmProviderInfo {
    pub provider_id: &'static str,
    pub display_name: &'static str,
    pub models: LlmModelList,
}

// ---------------------------------------------------------------------------
// Static catalog
// ---------------------------------------------------------------------------

static LLM_PROVIDERS: LazyLock<Vec<LlmProviderInfo>> = LazyLock::new(build_catalog);

#[allow(clippy::too_many_arguments)] // justified: catalog builder needs all pricing/capability fields; a struct would be more verbose with no clarity gain
fn usd_model(
    model_id: &'static str,
    provider: &'static str,
    display_name: &'static str,
    context_window: u64,
    max_output_tokens: Option<u32>,
    input: f64,
    output: f64,
    cache_read: Option<f64>,
    cache_write: Option<f64>,
) -> LlmModelEntry {
    LlmModelEntry {
        model_id,
        provider,
        display_name,
        context_window,
        max_output_tokens,
        pricing: Some(ModelPricing {
            currency: "USD".into(),
            input_per_million: input,
            output_per_million: output,
            cache_read_per_million: cache_read,
            cache_write_per_million: cache_write,
        }),
    }
}

#[allow(clippy::too_many_arguments)] // justified: same as usd_model — all fields required, no meaningful grouping
fn cny_model(
    model_id: &'static str,
    provider: &'static str,
    display_name: &'static str,
    context_window: u64,
    max_output_tokens: Option<u32>,
    input: f64,
    output: f64,
    cache_read: Option<f64>,
    cache_write: Option<f64>,
) -> LlmModelEntry {
    LlmModelEntry {
        model_id,
        provider,
        display_name,
        context_window,
        max_output_tokens,
        pricing: Some(ModelPricing {
            currency: "CNY".into(),
            input_per_million: input,
            output_per_million: output,
            cache_read_per_million: cache_read,
            cache_write_per_million: cache_write,
        }),
    }
}

fn anthropic_models() -> LlmProviderInfo {
    // Source: docs/external/anthropic/models.md
    let models = vec![
        // --- Current models ---
        usd_model(
            "anthropic/claude-fable-5",
            "anthropic",
            "Claude Fable 5",
            1_000_000,
            Some(128_000),
            10.0,
            50.0,
            Some(1.0),
            Some(12.5),
        ),
        usd_model(
            "anthropic/claude-opus-4-8",
            "anthropic",
            "Claude Opus 4.8",
            1_000_000,
            Some(128_000),
            5.0,
            25.0,
            Some(0.5),
            Some(6.25),
        ),
        usd_model(
            "anthropic/claude-sonnet-4-6",
            "anthropic",
            "Claude Sonnet 4.6",
            1_000_000,
            Some(64_000),
            3.0,
            15.0,
            Some(0.3),
            Some(3.75),
        ),
        usd_model(
            "anthropic/claude-haiku-4-5",
            "anthropic",
            "Claude Haiku 4.5",
            200_000,
            Some(64_000),
            1.0,
            5.0,
            Some(0.1),
            Some(1.25),
        ),
        // --- Legacy models (still available) ---
        usd_model(
            "anthropic/claude-opus-4-7",
            "anthropic",
            "Claude Opus 4.7",
            1_000_000,
            Some(128_000),
            5.0,
            25.0,
            Some(0.5),
            Some(6.25),
        ),
        usd_model(
            "anthropic/claude-opus-4-6",
            "anthropic",
            "Claude Opus 4.6",
            1_000_000,
            Some(128_000),
            5.0,
            25.0,
            Some(0.5),
            Some(6.25),
        ),
        usd_model(
            "anthropic/claude-sonnet-4-5",
            "anthropic",
            "Claude Sonnet 4.5",
            1_000_000,
            Some(64_000),
            3.0,
            15.0,
            Some(0.3),
            Some(3.75),
        ),
    ];

    LlmProviderInfo {
        provider_id: "anthropic",
        display_name: "Anthropic",
        models: LlmModelList::Known(models),
    }
}

fn openai_models() -> LlmProviderInfo {
    // Source: https://developers.openai.com/api/docs/models/all
    let models = vec![
        usd_model(
            "openai/gpt-5.5",
            "openai",
            "GPT-5.5",
            1_000_000,
            None,
            5.0,
            30.0,
            None,
            None,
        ),
        usd_model(
            "openai/gpt-5.4",
            "openai",
            "GPT-5.4",
            1_000_000,
            None,
            2.5,
            15.0,
            None,
            None,
        ),
        usd_model(
            "openai/gpt-5.4-mini",
            "openai",
            "GPT-5.4 mini",
            400_000,
            None,
            0.75,
            4.5,
            None,
            None,
        ),
        usd_model(
            "openai/gpt-5.4-nano",
            "openai",
            "GPT-5.4 nano",
            400_000,
            None,
            0.20,
            1.25,
            None,
            None,
        ),
    ];

    LlmProviderInfo {
        provider_id: "openai",
        display_name: "OpenAI",
        models: LlmModelList::Known(models),
    }
}

fn deepseek_models() -> LlmProviderInfo {
    // Source: https://api-docs.deepseek.com/zh-cn/quick_start/pricing
    // Legacy model IDs (deepseek-chat, deepseek-reasoner) deprecated 2026-07-24.
    let models = vec![
        cny_model(
            "deepseek/deepseek-v4-flash",
            "deepseek",
            "DeepSeek V4 Flash",
            1_000_000,
            Some(384_000),
            1.0,
            2.0,
            Some(0.02), // cache hit price; see source URL above
            None,       // DeepSeek does not charge for cache writes
        ),
        cny_model(
            "deepseek/deepseek-v4-pro",
            "deepseek",
            "DeepSeek V4 Pro",
            1_000_000,
            Some(384_000),
            3.0,
            6.0,
            Some(0.025),
            None,
        ),
    ];

    LlmProviderInfo {
        provider_id: "deepseek",
        display_name: "DeepSeek",
        models: LlmModelList::Known(models),
    }
}

fn volcengine_models() -> LlmProviderInfo {
    // Source: docs/external/volceengine/llm/
    // API: https://ark.cn-beijing.volces.com/api/v3/chat/completions
    // Auth: ARK_API_KEY (火山方舟 API Key)
    // Cache pricing: Volcengine has not published per-model cache rates;
    // entries pass None until the upstream pricing page lists them.
    let models = vec![
        // --- doubao-seed-2.0 series (thinking enabled by default) ---
        cny_model(
            "volcengine/doubao-seed-2-0-pro-260215",
            "volcengine",
            "Doubao Seed 2.0 Pro",
            128_000,
            Some(16_384),
            1.0,
            5.0,
            None,
            None,
        ),
        cny_model(
            "volcengine/doubao-seed-2-0-lite-260215",
            "volcengine",
            "Doubao Seed 2.0 Lite",
            128_000,
            Some(16_384),
            0.5,
            2.0,
            None,
            None,
        ),
        cny_model(
            "volcengine/doubao-seed-2-0-mini-260215",
            "volcengine",
            "Doubao Seed 2.0 Mini",
            128_000,
            Some(16_384),
            0.3,
            1.5,
            None,
            None,
        ),
        // --- doubao-seed-2.0 (428 series with thinking summary) ---
        cny_model(
            "volcengine/doubao-seed-2-0-lite-260428",
            "volcengine",
            "Doubao Seed 2.0 Lite (260428)",
            128_000,
            Some(16_384),
            0.5,
            2.0,
            None,
            None,
        ),
        cny_model(
            "volcengine/doubao-seed-2-0-mini-260428",
            "volcengine",
            "Doubao Seed 2.0 Mini (260428)",
            128_000,
            Some(16_384),
            0.3,
            1.5,
            None,
            None,
        ),
        // --- doubao-seed-1.x series ---
        cny_model(
            "volcengine/doubao-seed-1-6-flash-250615",
            "volcengine",
            "Doubao Seed 1.6 Flash",
            128_000,
            Some(16_384),
            0.5,
            2.0,
            None,
            None,
        ),
    ];

    LlmProviderInfo {
        provider_id: "volcengine",
        display_name: "Volcengine (火山引擎 / Doubao)",
        models: LlmModelList::Known(models),
    }
}

fn openrouter_provider() -> LlmProviderInfo {
    LlmProviderInfo {
        provider_id: "openrouter",
        display_name: "OpenRouter",
        models: LlmModelList::Dynamic {
            description: "OpenRouter is a gateway that routes to 300+ models from many providers \
                (Anthropic, OpenAI, Google, Meta, etc.). It does not have a fixed model list. \
                Pass any model available on openrouter.ai as the model ID.",
            model_id_format: "openrouter/<upstream-provider>/<model-name>",
            model_id_example: "openrouter/anthropic/claude-opus-4-8",
        },
    }
}

fn build_catalog() -> Vec<LlmProviderInfo> {
    vec![
        anthropic_models(),
        openai_models(),
        deepseek_models(),
        volcengine_models(),
        openrouter_provider(),
    ]
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns metadata for all known LLM providers, including their model lists.
pub fn list_providers() -> &'static [LlmProviderInfo] {
    &LLM_PROVIDERS
}

/// Returns all individually enumerable LLM models across all providers.
/// Dynamic providers (e.g. OpenRouter) are excluded; use `list_providers()` to see them.
pub fn list_models() -> impl Iterator<Item = &'static LlmModelEntry> {
    LLM_PROVIDERS.iter().flat_map(|p| match &p.models {
        LlmModelList::Known(models) => models.as_slice(),
        LlmModelList::Dynamic { .. } => &[],
    })
}

#[cfg(test)]
mod tests;
