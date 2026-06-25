//! Static model catalog for discovery — no credentials needed.
//!
//! Use `list_providers()` to see all supported providers and their models,
//! or `list_models()` to iterate all individually enumerable models.
//!
//! OpenRouter and similar gateway providers cannot enumerate their full model
//! list statically; they appear as `LlmModelList::Dynamic` entries.

use std::sync::LazyLock;

use agent_runtime_model::{ModelPricing, PricingRates, PricingTier};
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
        // claude-fable-5 removed: temporarily delisted by Anthropic as of 2026-06-25.
        LlmModelEntry {
            model_id: "anthropic/claude-opus-4-8",
            provider: "anthropic",
            display_name: "Claude Opus 4.8",
            description: "Opus 主力模型，支持 extended/adaptive thinking",
            context_window: 1_000_000,
            max_input_tokens: Some(872_000),
            max_output_tokens: Some(128_000),
            thinking: Some(ThinkingSpec {
                max_thinking_tokens: None,
            }),
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[
                ModelScene::General,
                ModelScene::Coding,
                ModelScene::Agent,
                ModelScene::Reasoning,
            ],
            pricing: Some(ModelPricing::single_tier(
                "USD",
                PricingRates::text(5.0, 25.0).with_cache(Some(0.5), Some(6.25)),
            )),
        },
        LlmModelEntry {
            model_id: "anthropic/claude-sonnet-4-6",
            provider: "anthropic",
            display_name: "Claude Sonnet 4.6",
            description: "主力通用模型，平衡性能与成本，支持 extended thinking",
            context_window: 1_000_000,
            max_input_tokens: Some(936_000),
            max_output_tokens: Some(64_000),
            thinking: Some(ThinkingSpec {
                max_thinking_tokens: None,
            }),
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Coding, ModelScene::Agent],
            pricing: Some(ModelPricing::single_tier(
                "USD",
                PricingRates::text(3.0, 15.0).with_cache(Some(0.3), Some(3.75)),
            )),
        },
        LlmModelEntry {
            model_id: "anthropic/claude-haiku-4-5",
            provider: "anthropic",
            display_name: "Claude Haiku 4.5",
            description: "轻量快速模型，支持 extended thinking，200k 上下文",
            context_window: 200_000,
            max_input_tokens: Some(136_000),
            max_output_tokens: Some(64_000),
            thinking: Some(ThinkingSpec {
                max_thinking_tokens: None,
            }),
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General],
            pricing: Some(ModelPricing::single_tier(
                "USD",
                PricingRates::text(1.0, 5.0).with_cache(Some(0.1), Some(1.25)),
            )),
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
            thinking: Some(ThinkingSpec {
                max_thinking_tokens: None,
            }),
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[
                ModelScene::General,
                ModelScene::Coding,
                ModelScene::Agent,
                ModelScene::Reasoning,
            ],
            pricing: Some(ModelPricing::single_tier(
                "USD",
                PricingRates::text(5.0, 25.0).with_cache(Some(0.5), Some(6.25)),
            )),
        },
        LlmModelEntry {
            model_id: "anthropic/claude-opus-4-6",
            provider: "anthropic",
            display_name: "Claude Opus 4.6",
            description: "Opus 上代主力，支持 extended thinking",
            context_window: 1_000_000,
            max_input_tokens: Some(872_000),
            max_output_tokens: Some(128_000),
            thinking: Some(ThinkingSpec {
                max_thinking_tokens: None,
            }),
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[
                ModelScene::General,
                ModelScene::Coding,
                ModelScene::Agent,
                ModelScene::Reasoning,
            ],
            pricing: Some(ModelPricing::single_tier(
                "USD",
                PricingRates::text(5.0, 25.0).with_cache(Some(0.5), Some(6.25)),
            )),
        },
        LlmModelEntry {
            model_id: "anthropic/claude-sonnet-4-5",
            provider: "anthropic",
            display_name: "Claude Sonnet 4.5",
            description: "Sonnet 上代主力，支持 extended thinking",
            context_window: 1_000_000,
            max_input_tokens: Some(936_000),
            max_output_tokens: Some(64_000),
            thinking: Some(ThinkingSpec {
                max_thinking_tokens: None,
            }),
            input_modalities: &[Modality::Text, Modality::Image],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Coding, ModelScene::Agent],
            pricing: Some(ModelPricing::single_tier(
                "USD",
                PricingRates::text(3.0, 15.0).with_cache(Some(0.3), Some(3.75)),
            )),
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
            pricing: Some(ModelPricing::flat_text("USD", 5.0, 30.0)),
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
            pricing: Some(ModelPricing::flat_text("USD", 2.5, 15.0)),
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
            pricing: Some(ModelPricing::flat_text("USD", 0.75, 4.5)),
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
            pricing: Some(ModelPricing::flat_text("USD", 0.20, 1.25)),
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
            thinking: Some(ThinkingSpec {
                max_thinking_tokens: None,
            }),
            input_modalities: &[Modality::Text],
            output_modalities: &[Modality::Text],
            scenes: &[
                ModelScene::General,
                ModelScene::Reasoning,
                ModelScene::Coding,
            ],
            pricing: Some(ModelPricing::single_tier(
                "CNY",
                PricingRates::text(1.0, 2.0).with_cache(Some(0.02), None),
            )),
        },
        LlmModelEntry {
            model_id: "deepseek/deepseek-v4-pro",
            provider: "deepseek",
            display_name: "DeepSeek V4 Pro",
            description: "旗舰推理模型，支持 thinking 模式，极强代码与数学能力",
            context_window: 1_000_000,
            max_input_tokens: Some(616_000),
            max_output_tokens: Some(384_000),
            thinking: Some(ThinkingSpec {
                max_thinking_tokens: None,
            }),
            input_modalities: &[Modality::Text],
            output_modalities: &[Modality::Text],
            scenes: &[
                ModelScene::Reasoning,
                ModelScene::Coding,
                ModelScene::General,
            ],
            pricing: Some(ModelPricing::single_tier(
                "CNY",
                PricingRates::text(3.0, 6.0).with_cache(Some(0.025), None),
            )),
        },
    ];

    LlmProviderInfo {
        provider_id: "deepseek",
        display_name: "DeepSeek",
        models: LlmModelList::Known(models),
    }
}

fn doubao_seed_2_1_models() -> Vec<LlmModelEntry> {
    // Sources (Volcengine 火山方舟控制台 → 模型详情):
    //   https://console.volcengine.com/ark/region:cn-beijing/model/detail?name=doubao-seed-2-1-pro
    //   https://console.volcengine.com/ark/region:cn-beijing/model/detail?name=doubao-seed-2-1-turbo
    //   https://console.volcengine.com/ark/region:cn-beijing/model/detail?name=doubao-seed-character
    // Pro/Turbo: context_window = max_input = max_output = max_thinking = 256K
    //   (independent hard caps, not pooled). Pro ¥6 / ¥30 / cache ¥1.2;
    //   Turbo ¥3 / ¥15 / cache ¥0.6.
    // Character-260628: 128K ctx, no thinking, tiered text-only pricing.
    let multimodal_input: &[Modality] = &[
        Modality::Text,
        Modality::Image,
        Modality::Video,
        Modality::Audio,
    ];
    let coding_agent_scenes: &[ModelScene] = &[
        ModelScene::General,
        ModelScene::Reasoning,
        ModelScene::Coding,
        ModelScene::Agent,
    ];
    vec![
        LlmModelEntry {
            model_id: "volcengine/doubao-seed-2-1-pro-260628",
            provider: "volcengine",
            display_name: "Doubao Seed 2.1 Pro",
            description: "豆包 2.1 旗舰深度思考模型，面向复杂 Coding、长链路 Agent、多模态理解",
            context_window: 256_000,
            max_input_tokens: Some(256_000),
            max_output_tokens: Some(256_000),
            thinking: Some(ThinkingSpec {
                max_thinking_tokens: Some(256_000),
            }),
            input_modalities: multimodal_input,
            output_modalities: &[Modality::Text],
            scenes: coding_agent_scenes,
            pricing: Some(ModelPricing::single_tier(
                "CNY",
                PricingRates::text(6.0, 30.0).with_cache(Some(1.2), None),
            )),
        },
        LlmModelEntry {
            model_id: "volcengine/doubao-seed-2-1-turbo-260628",
            provider: "volcengine",
            display_name: "Doubao Seed 2.1 Turbo",
            description: "豆包 2.1 低成本低时延深度思考模型，面向规模化生产场景，效果与 Pro 相当",
            context_window: 256_000,
            max_input_tokens: Some(256_000),
            max_output_tokens: Some(256_000),
            thinking: Some(ThinkingSpec {
                max_thinking_tokens: Some(256_000),
            }),
            input_modalities: multimodal_input,
            output_modalities: &[Modality::Text],
            scenes: coding_agent_scenes,
            pricing: Some(ModelPricing::single_tier(
                "CNY",
                PricingRates::text(3.0, 15.0).with_cache(Some(0.6), None),
            )),
        },
        LlmModelEntry {
            model_id: "volcengine/doubao-seed-character-260628",
            provider: "volcengine",
            display_name: "Doubao Seed Character",
            description: "Seed 2.1 角色扮演/陪伴模型，多模态输入；按输入长度两档分段计价 —— 详见 pricing.tiers",
            context_window: 128_000,
            max_input_tokens: Some(96_000),
            max_output_tokens: Some(32_768),
            thinking: None,
            input_modalities: &[Modality::Text, Modality::Image, Modality::Audio],
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::Chat],
            pricing: Some(ModelPricing {
                currency: "CNY".into(),
                // Two-tier pricing by input length (Volcengine 控制台):
                //   ≤32k:  ¥0.8 in / ¥2.0 out
                //   >32k:  ¥1.2 in / ¥6.0 out
                tiers: vec![
                    PricingTier {
                        max_input_tokens: Some(32_000),
                        rates: PricingRates::text(0.8, 2.0),
                    },
                    PricingTier {
                        max_input_tokens: None,
                        rates: PricingRates::text(1.2, 6.0),
                    },
                ],
            }),
        },
    ]
}

fn doubao_seed_2_0_models() -> Vec<LlmModelEntry> {
    // Only the 260428 全模态升级版 remains live. Pro-260215 and the legacy
    // text-only Lite/Mini-260215 lines were retired upstream.
    // Per Volcengine 控制台 → 模型详情:
    //   Lite/Mini-260428: 256K ctx / 224K input / 128K output / 128K thinking,
    //   全模态 (text/image/video/audio in), 三档分段计费 — see description fields.
    // Three-tier pricing (≤32k / 32k-128k / >128k) and audio-input surcharge
    // are now expressed structurally via `PricingTier` + `PricingRates`.
    let multimodal_input: &[Modality] = &[
        Modality::Text,
        Modality::Image,
        Modality::Video,
        Modality::Audio,
    ];
    vec![
        LlmModelEntry {
            model_id: "volcengine/doubao-seed-2-0-lite-260428",
            provider: "volcengine",
            display_name: "Doubao Seed 2.0 Lite (260428)",
            description: "豆包 Lite 全模态深度思考（260428），含 thinking summary；按输入长度三档分段计价（¥/MTok），音频输入独立计费 —— 详见 pricing.tiers",
            context_window: 256_000,
            max_input_tokens: Some(224_000),
            max_output_tokens: Some(128_000),
            thinking: Some(ThinkingSpec {
                max_thinking_tokens: Some(128_000),
            }),
            input_modalities: multimodal_input,
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General, ModelScene::Coding],
            pricing: Some(ModelPricing {
                currency: "CNY".into(),
                // Three-tier pricing by input length (Volcengine 控制台).
                tiers: vec![
                    PricingTier {
                        max_input_tokens: Some(32_000),
                        rates: PricingRates::text(0.6, 3.6).with_audio_input(9.0),
                    },
                    PricingTier {
                        max_input_tokens: Some(128_000),
                        rates: PricingRates::text(0.9, 5.4).with_audio_input(13.5),
                    },
                    PricingTier {
                        max_input_tokens: None,
                        rates: PricingRates::text(1.8, 10.8).with_audio_input(27.0),
                    },
                ],
            }),
        },
        LlmModelEntry {
            model_id: "volcengine/doubao-seed-2-0-mini-260428",
            provider: "volcengine",
            display_name: "Doubao Seed 2.0 Mini (260428)",
            description: "豆包 Mini 全模态深度思考（260428），含 thinking summary；按输入长度三档分段计价（¥/MTok），音频输入独立计费 —— 详见 pricing.tiers",
            context_window: 256_000,
            max_input_tokens: Some(224_000),
            max_output_tokens: Some(128_000),
            thinking: Some(ThinkingSpec {
                max_thinking_tokens: Some(128_000),
            }),
            input_modalities: multimodal_input,
            output_modalities: &[Modality::Text],
            scenes: &[ModelScene::General],
            pricing: Some(ModelPricing {
                currency: "CNY".into(),
                // Three-tier pricing by input length (Volcengine 控制台).
                tiers: vec![
                    PricingTier {
                        max_input_tokens: Some(32_000),
                        rates: PricingRates::text(0.2, 2.0).with_audio_input(3.0),
                    },
                    PricingTier {
                        max_input_tokens: Some(128_000),
                        rates: PricingRates::text(0.4, 4.0).with_audio_input(6.0),
                    },
                    PricingTier {
                        max_input_tokens: None,
                        rates: PricingRates::text(0.8, 8.0).with_audio_input(12.0),
                    },
                ],
            }),
        },
    ]
}

fn volcengine_models() -> LlmProviderInfo {
    // API: https://ark.cn-beijing.volces.com/api/v3/chat/completions
    // Auth: ARK_API_KEY (火山方舟 API Key)
    // Pricing: CNY. Cache pricing: not published unless noted; entries use None.
    // max_input_tokens = context_window - max_output_tokens unless stated otherwise.
    let mut models = doubao_seed_2_1_models();
    models.extend(doubao_seed_2_0_models());

    LlmProviderInfo {
        provider_id: "volcengine",
        display_name: "Volcengine (火山引擎 / Doubao)",
        models: LlmModelList::Known(models),
    }
}

fn minimax_models() -> LlmProviderInfo {
    // Source: docs/external/minimax/llm/desc.md:21-29 (model table)
    // Pricing: pricing.md 缺失(研究文档 §八),pricing 字段为 0.0 占位待补。
    // Context: M3 1M, M2.x series 204_800.
    // M3 supports native multimodal (text + image + video) per
    // docs/external/minimax/llm/api.md:843. M2.x supports text + tools only.
    // Her (M2-her) excluded — uses non-Anthropic chat protocol (research §九, PRD non-goal).
    let pricing_placeholder = || ModelPricing::flat_text("USD", 0.0, 0.0);
    let m2_input_modalities: &[Modality] = &[Modality::Text];
    let m3_input_modalities: &[Modality] = &[Modality::Text, Modality::Image];
    let text_only_output: &[Modality] = &[Modality::Text];
    let coding_agent_scenes: &[ModelScene] =
        &[ModelScene::General, ModelScene::Coding, ModelScene::Agent];
    let m3_scenes: &[ModelScene] = &[
        ModelScene::General,
        ModelScene::Coding,
        ModelScene::Agent,
        ModelScene::Reasoning,
    ];
    let thinking_open = Some(ThinkingSpec {
        max_thinking_tokens: None,
    });

    let models = vec![
        LlmModelEntry {
            model_id: "minimax/MiniMax-M3",
            provider: "minimax",
            display_name: "MiniMax-M3",
            description:
                "Frontier coding 模型,1M 上下文,原生多模态(text/image/video),adaptive thinking。",
            context_window: 1_000_000,
            max_input_tokens: Some(936_000),
            max_output_tokens: Some(64_000),
            thinking: thinking_open,
            input_modalities: m3_input_modalities,
            output_modalities: text_only_output,
            scenes: m3_scenes,
            pricing: Some(pricing_placeholder()),
        },
        LlmModelEntry {
            model_id: "minimax/MiniMax-M2.7",
            provider: "minimax",
            display_name: "MiniMax-M2.7",
            description: "M2.7 主线,200K 上下文,thinking 始终开启;约 60 TPS。",
            context_window: 204_800,
            max_input_tokens: Some(140_800),
            max_output_tokens: Some(64_000),
            thinking: thinking_open,
            input_modalities: m2_input_modalities,
            output_modalities: text_only_output,
            scenes: coding_agent_scenes,
            pricing: Some(pricing_placeholder()),
        },
        LlmModelEntry {
            model_id: "minimax/MiniMax-M2.7-highspeed",
            provider: "minimax",
            display_name: "MiniMax-M2.7-highspeed",
            description: "M2.7 极速版,效果不变约 100 TPS;200K 上下文。",
            context_window: 204_800,
            max_input_tokens: Some(140_800),
            max_output_tokens: Some(64_000),
            thinking: thinking_open,
            input_modalities: m2_input_modalities,
            output_modalities: text_only_output,
            scenes: coding_agent_scenes,
            pricing: Some(pricing_placeholder()),
        },
        LlmModelEntry {
            model_id: "minimax/MiniMax-M2.5",
            provider: "minimax",
            display_name: "MiniMax-M2.5",
            description: "顶尖性能性价比,200K 上下文;约 60 TPS。",
            context_window: 204_800,
            max_input_tokens: Some(140_800),
            max_output_tokens: Some(64_000),
            thinking: thinking_open,
            input_modalities: m2_input_modalities,
            output_modalities: text_only_output,
            scenes: coding_agent_scenes,
            pricing: Some(pricing_placeholder()),
        },
        LlmModelEntry {
            model_id: "minimax/MiniMax-M2.5-highspeed",
            provider: "minimax",
            display_name: "MiniMax-M2.5-highspeed",
            description: "M2.5 极速版,约 100 TPS;200K 上下文。",
            context_window: 204_800,
            max_input_tokens: Some(140_800),
            max_output_tokens: Some(64_000),
            thinking: thinking_open,
            input_modalities: m2_input_modalities,
            output_modalities: text_only_output,
            scenes: coding_agent_scenes,
            pricing: Some(pricing_placeholder()),
        },
        LlmModelEntry {
            model_id: "minimax/MiniMax-M2.1",
            provider: "minimax",
            display_name: "MiniMax-M2.1",
            description: "多语言编程能力升级,200K 上下文;约 60 TPS。",
            context_window: 204_800,
            max_input_tokens: Some(140_800),
            max_output_tokens: Some(64_000),
            thinking: thinking_open,
            input_modalities: m2_input_modalities,
            output_modalities: text_only_output,
            scenes: coding_agent_scenes,
            pricing: Some(pricing_placeholder()),
        },
        LlmModelEntry {
            model_id: "minimax/MiniMax-M2.1-highspeed",
            provider: "minimax",
            display_name: "MiniMax-M2.1-highspeed",
            description: "M2.1 极速版,约 100 TPS;200K 上下文。",
            context_window: 204_800,
            max_input_tokens: Some(140_800),
            max_output_tokens: Some(64_000),
            thinking: thinking_open,
            input_modalities: m2_input_modalities,
            output_modalities: text_only_output,
            scenes: coding_agent_scenes,
            pricing: Some(pricing_placeholder()),
        },
        LlmModelEntry {
            model_id: "minimax/MiniMax-M2",
            provider: "minimax",
            display_name: "MiniMax-M2",
            description: "M2 系列基线,专为高效编码与 Agent 工作流;200K 上下文。",
            context_window: 204_800,
            max_input_tokens: Some(140_800),
            max_output_tokens: Some(64_000),
            thinking: thinking_open,
            input_modalities: m2_input_modalities,
            output_modalities: text_only_output,
            scenes: coding_agent_scenes,
            pricing: Some(pricing_placeholder()),
        },
    ];

    LlmProviderInfo {
        provider_id: "minimax",
        display_name: "MiniMax",
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
        minimax_models(),
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

/// Look up an enumerable model by its full `provider/model` id, or by the bare
/// model name when the provider is implicit (e.g. `"doubao-seed-2-1-pro-260628"`).
///
/// Returns `None` for dynamic-gateway providers or genuinely unknown models —
/// callers MUST handle that case (e.g. assume capabilities conservatively).
pub fn find_model(model_id: &str) -> Option<&'static LlmModelEntry> {
    list_models().find(|m| {
        m.model_id == model_id
            || m.model_id
                .rsplit_once('/')
                .is_some_and(|(_, bare)| bare == model_id)
    })
}

#[cfg(test)]
mod tests;
