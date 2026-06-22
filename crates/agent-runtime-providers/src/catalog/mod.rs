//! Static model catalog for discovery — no credentials needed.
//!
//! Use `list_providers()` to see all supported providers and their models,
//! or `list_models()` to iterate all individually enumerable models.
//!
//! OpenRouter and similar gateway providers cannot enumerate their full model
//! list statically; they appear as `LlmModelList::Dynamic` entries.

use std::sync::LazyLock;

use agent_runtime_model::ModelPricing;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// An individually enumerable LLM model.
///
/// - `description`: one-line technical characterization (Chinese).
/// - `max_input_tokens`: conservative upper bound on input assuming
///   `max_output_tokens` is fully used. For pool-type providers (Anthropic /
///   OpenAI) the constraint is `input + output ≤ context_window`.
/// - `thinking`: `None` = no thinking/reasoning mode; `Some` = supported,
///   with an optional token cap (`None` = vendor did not publish a cap).
/// - `input_modalities` / `output_modalities`: content types accepted/produced.
/// - `scenes`: workload classes the model is tuned for (can match several).
#[derive(Debug, Clone)]
pub struct LlmModelEntry {
    /// Full model ID for `ProviderRuntimeConfig { model: "..." }`.
    /// Always `"provider/model-name"` form, e.g. `"anthropic/claude-opus-4-8"`.
    pub model_id: &'static str,
    pub provider: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub context_window: u64,
    pub max_input_tokens: Option<u64>,
    pub max_output_tokens: Option<u32>,
    pub thinking: Option<ThinkingSpec>,
    pub input_modalities: &'static [Modality],
    pub output_modalities: &'static [Modality],
    pub scenes: &'static [ModelScene],
    pub pricing: Option<ModelPricing>,
}

/// A content modality a model can accept or produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Modality {
    Text,
    Image,
    Video,
    Audio,
}

/// A workload class a model is tuned for. A model can match several.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelScene {
    /// General-purpose tasks (default).
    General,
    /// Code generation / editing SOTA.
    Coding,
    /// Tool use / long-horizon agentic tasks.
    Agent,
    /// Role-play / companion / conversational.
    Chat,
    /// Math / logic / reasoning SOTA.
    Reasoning,
}

/// Thinking/reasoning mode specification for a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThinkingSpec {
    /// Maximum tokens the model can spend on thinking content.
    /// `None` = vendor did not publish a specific cap.
    pub max_thinking_tokens: Option<u32>,
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

fn anthropic_models() -> LlmProviderInfo {
    // Source: docs/external/anthropic/models.md
    // Pricing: USD. Context constraint: input + output ≤ context_window.
    // max_input_tokens = context_window - max_output_tokens (conservative).
    let models = vec![
        // --- Current models ---
        LlmModelEntry {
            model_id: "anthropic/claude-fable-5",
            provider: "anthropic",
            display_name: "Claude Fable 5",
            description: "旗舰推理模型，支持 adaptive thinking，百万 token 上下文",
            context_window: 1_000_000,
            max_input_tokens: Some(872_000),
            max_output_tokens: Some(128_000),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Coding, ModelScene::Agent, ModelScene::Reasoning],
            pricing: Some(ModelPricing {
                currency: "USD".into(),
                input_per_million: 10.0,
                output_per_million: 50.0,
                cache_read_per_million: Some(1.0),
                cache_write_per_million: Some(12.5),
            }),
        },
        LlmModelEntry {
            model_id: "anthropic/claude-opus-4-8",
            provider: "anthropic",
            display_name: "Claude Opus 4.8",
            description: "Opus 主力模型，支持 extended/adaptive thinking",
            context_window: 1_000_000,
            max_input_tokens: Some(872_000),
            max_output_tokens: Some(128_000),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Coding, ModelScene::Agent, ModelScene::Reasoning],
            pricing: Some(ModelPricing {
                currency: "USD".into(),
                input_per_million: 5.0,
                output_per_million: 25.0,
                cache_read_per_million: Some(0.5),
                cache_write_per_million: Some(6.25),
            }),
        },
        LlmModelEntry {
            model_id: "anthropic/claude-sonnet-4-6",
            provider: "anthropic",
            display_name: "Claude Sonnet 4.6",
            description: "主力通用模型，平衡性能与成本，支持 extended thinking",
            context_window: 1_000_000,
            max_input_tokens: Some(936_000),
            max_output_tokens: Some(64_000),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Coding, ModelScene::Agent],
            pricing: Some(ModelPricing {
                currency: "USD".into(),
                input_per_million: 3.0,
                output_per_million: 15.0,
                cache_read_per_million: Some(0.3),
                cache_write_per_million: Some(3.75),
            }),
        },
        LlmModelEntry {
            model_id: "anthropic/claude-haiku-4-5",
            provider: "anthropic",
            display_name: "Claude Haiku 4.5",
            description: "轻量快速模型，支持 extended thinking，200k 上下文",
            context_window: 200_000,
            max_input_tokens: Some(136_000),
            max_output_tokens: Some(64_000),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General],
            pricing: Some(ModelPricing {
                currency: "USD".into(),
                input_per_million: 1.0,
                output_per_million: 5.0,
                cache_read_per_million: Some(0.1),
                cache_write_per_million: Some(1.25),
            }),
        },
        // --- Legacy models (still available) ---
        LlmModelEntry {
            model_id: "anthropic/claude-opus-4-7",
            provider: "anthropic",
            display_name: "Claude Opus 4.7",
            description: "Opus 上代旗舰，支持 extended thinking",
            context_window: 1_000_000,
            max_input_tokens: Some(872_000),
            max_output_tokens: Some(128_000),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Coding, ModelScene::Agent, ModelScene::Reasoning],
            pricing: Some(ModelPricing {
                currency: "USD".into(),
                input_per_million: 5.0,
                output_per_million: 25.0,
                cache_read_per_million: Some(0.5),
                cache_write_per_million: Some(6.25),
            }),
        },
        LlmModelEntry {
            model_id: "anthropic/claude-opus-4-6",
            provider: "anthropic",
            display_name: "Claude Opus 4.6",
            description: "Opus 上代主力，支持 extended thinking",
            context_window: 1_000_000,
            max_input_tokens: Some(872_000),
            max_output_tokens: Some(128_000),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Coding, ModelScene::Agent, ModelScene::Reasoning],
            pricing: Some(ModelPricing {
                currency: "USD".into(),
                input_per_million: 5.0,
                output_per_million: 25.0,
                cache_read_per_million: Some(0.5),
                cache_write_per_million: Some(6.25),
            }),
        },
        LlmModelEntry {
            model_id: "anthropic/claude-sonnet-4-5",
            provider: "anthropic",
            display_name: "Claude Sonnet 4.5",
            description: "Sonnet 上代主力，支持 extended thinking",
            context_window: 1_000_000,
            max_input_tokens: Some(936_000),
            max_output_tokens: Some(64_000),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Coding, ModelScene::Agent],
            pricing: Some(ModelPricing {
                currency: "USD".into(),
                input_per_million: 3.0,
                output_per_million: 15.0,
                cache_read_per_million: Some(0.3),
                cache_write_per_million: Some(3.75),
            }),
        },
    ];

    LlmProviderInfo {
        provider_id: "anthropic",
        display_name: "Anthropic",
        models: LlmModelList::Known(models),
    }
}

fn openai_models() -> LlmProviderInfo {
    // Source: https://platform.openai.com/docs/models
    // Pricing: USD. max_input_tokens: None — OpenAI does not publish a separate
    // max-input limit distinct from context_window for these models.
    let models = vec![
        LlmModelEntry {
            model_id: "openai/gpt-5.5",
            provider: "openai",
            display_name: "GPT-5.5",
            description: "GPT 旗舰模型，综合能力最强，百万 token 上下文",
            context_window: 1_000_000,
            max_input_tokens: None,
            max_output_tokens: None,
            thinking: None,
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Coding, ModelScene::Agent],
            pricing: Some(ModelPricing {
                currency: "USD".into(),
                input_per_million: 5.0,
                output_per_million: 30.0,
                cache_read_per_million: None,
                cache_write_per_million: None,
            }),
        },
        LlmModelEntry {
            model_id: "openai/gpt-5.4",
            provider: "openai",
            display_name: "GPT-5.4",
            description: "GPT 主力模型，平衡能力与成本",
            context_window: 1_000_000,
            max_input_tokens: None,
            max_output_tokens: None,
            thinking: None,
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Coding],
            pricing: Some(ModelPricing {
                currency: "USD".into(),
                input_per_million: 2.5,
                output_per_million: 15.0,
                cache_read_per_million: None,
                cache_write_per_million: None,
            }),
        },
        LlmModelEntry {
            model_id: "openai/gpt-5.4-mini",
            provider: "openai",
            display_name: "GPT-5.4 mini",
            description: "GPT 轻量模型，高性价比",
            context_window: 400_000,
            max_input_tokens: None,
            max_output_tokens: None,
            thinking: None,
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General],
            pricing: Some(ModelPricing {
                currency: "USD".into(),
                input_per_million: 0.75,
                output_per_million: 4.5,
                cache_read_per_million: None,
                cache_write_per_million: None,
            }),
        },
        LlmModelEntry {
            model_id: "openai/gpt-5.4-nano",
            provider: "openai",
            display_name: "GPT-5.4 nano",
            description: "GPT 极轻量模型，最低延迟与成本",
            context_window: 400_000,
            max_input_tokens: None,
            max_output_tokens: None,
            thinking: None,
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General],
            pricing: Some(ModelPricing {
                currency: "USD".into(),
                input_per_million: 0.20,
                output_per_million: 1.25,
                cache_read_per_million: None,
                cache_write_per_million: None,
            }),
        },
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
    // Pricing: CNY. max_input_tokens = context_window - max_output_tokens.
    let models = vec![
        LlmModelEntry {
            model_id: "deepseek/deepseek-v4-flash",
            provider: "deepseek",
            display_name: "DeepSeek V4 Flash",
            description: "快速推理模型，支持 thinking 模式，超长上下文",
            context_window: 1_000_000,
            max_input_tokens: Some(616_000),
            max_output_tokens: Some(384_000),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Reasoning, ModelScene::Coding],
            pricing: Some(ModelPricing {
                currency: "CNY".into(),
                input_per_million: 1.0,
                output_per_million: 2.0,
                cache_read_per_million: Some(0.02),
                cache_write_per_million: None,
            }),
        },
        LlmModelEntry {
            model_id: "deepseek/deepseek-v4-pro",
            provider: "deepseek",
            display_name: "DeepSeek V4 Pro",
            description: "旗舰推理模型，支持 thinking 模式，极强代码与数学能力",
            context_window: 1_000_000,
            max_input_tokens: Some(616_000),
            max_output_tokens: Some(384_000),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::Reasoning, ModelScene::Coding, ModelScene::General],
            pricing: Some(ModelPricing {
                currency: "CNY".into(),
                input_per_million: 3.0,
                output_per_million: 6.0,
                cache_read_per_million: Some(0.025),
                cache_write_per_million: None,
            }),
        },
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
    // Pricing: CNY. Cache pricing: not published; entries use None.
    // max_input_tokens = context_window - max_output_tokens unless stated otherwise.
    let models = vec![
        // --- doubao-seed-2.0 series (thinking enabled by default) ---
        LlmModelEntry {
            model_id: "volcengine/doubao-seed-2-0-pro-260215",
            provider: "volcengine",
            display_name: "Doubao Seed 2.0 Pro",
            description: "豆包旗舰推理模型，默认开启 thinking",
            context_window: 128_000,
            max_input_tokens: Some(112_000),
            max_output_tokens: Some(16_384),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Reasoning, ModelScene::Coding],
            pricing: Some(ModelPricing {
                currency: "CNY".into(),
                input_per_million: 1.0,
                output_per_million: 5.0,
                cache_read_per_million: None,
                cache_write_per_million: None,
            }),
        },
        LlmModelEntry {
            model_id: "volcengine/doubao-seed-2-0-lite-260215",
            provider: "volcengine",
            display_name: "Doubao Seed 2.0 Lite",
            description: "豆包 Lite 推理模型，支持 thinking，低成本",
            context_window: 128_000,
            max_input_tokens: Some(112_000),
            max_output_tokens: Some(16_384),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Coding],
            pricing: Some(ModelPricing {
                currency: "CNY".into(),
                input_per_million: 0.5,
                output_per_million: 2.0,
                cache_read_per_million: None,
                cache_write_per_million: None,
            }),
        },
        LlmModelEntry {
            model_id: "volcengine/doubao-seed-2-0-mini-260215",
            provider: "volcengine",
            display_name: "Doubao Seed 2.0 Mini",
            description: "豆包 Mini 推理模型，最轻量 thinking",
            context_window: 128_000,
            max_input_tokens: Some(112_000),
            max_output_tokens: Some(16_384),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General],
            pricing: Some(ModelPricing {
                currency: "CNY".into(),
                input_per_million: 0.3,
                output_per_million: 1.5,
                cache_read_per_million: None,
                cache_write_per_million: None,
            }),
        },
        // --- doubao-seed-2.0 (428 series with thinking summary) ---
        LlmModelEntry {
            model_id: "volcengine/doubao-seed-2-0-lite-260428",
            provider: "volcengine",
            display_name: "Doubao Seed 2.0 Lite (260428)",
            description: "豆包 Lite 推理模型（带 thinking summary），低成本",
            context_window: 128_000,
            max_input_tokens: Some(112_000),
            max_output_tokens: Some(16_384),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Coding],
            pricing: Some(ModelPricing {
                currency: "CNY".into(),
                input_per_million: 0.5,
                output_per_million: 2.0,
                cache_read_per_million: None,
                cache_write_per_million: None,
            }),
        },
        LlmModelEntry {
            model_id: "volcengine/doubao-seed-2-0-mini-260428",
            provider: "volcengine",
            display_name: "Doubao Seed 2.0 Mini (260428)",
            description: "豆包 Mini 推理模型（带 thinking summary），极轻量",
            context_window: 128_000,
            max_input_tokens: Some(112_000),
            max_output_tokens: Some(16_384),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General],
            pricing: Some(ModelPricing {
                currency: "CNY".into(),
                input_per_million: 0.3,
                output_per_million: 1.5,
                cache_read_per_million: None,
                cache_write_per_million: None,
            }),
        },
        // --- doubao-seed-character series (roleplay / character dialogue) ---
        LlmModelEntry {
            model_id: "volcengine/doubao-seed-character-251128",
            provider: "volcengine",
            display_name: "Doubao Seed Character",
            description: "角色扮演专用模型，适合对话与角色扮演场景，不支持 thinking",
            context_window: 128_000,
            max_input_tokens: Some(96_000),
            max_output_tokens: Some(32_768),
            thinking: None,
            input_modalities: &[Modality::Text],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::Chat],
            pricing: Some(ModelPricing {
                currency: "CNY".into(),
                input_per_million: 0.8,
                output_per_million: 2.0,
                cache_read_per_million: None,
                cache_write_per_million: None,
            }),
        },
        // --- doubao-seed-1.x series ---
        LlmModelEntry {
            model_id: "volcengine/doubao-seed-1-6-flash-250615",
            provider: "volcengine",
            display_name: "Doubao Seed 1.6 Flash",
            description: "豆包 1.6 快速模型，支持 thinking",
            context_window: 128_000,
            max_input_tokens: Some(112_000),
            max_output_tokens: Some(16_384),
            thinking: Some(ThinkingSpec { max_thinking_tokens: None }),
            input_modalities: &[Modality::Text],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General],
            pricing: Some(ModelPricing {
                currency: "CNY".into(),
                input_per_million: 0.5,
                output_per_million: 2.0,
                cache_read_per_million: None,
                cache_write_per_million: None,
            }),
        },
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
