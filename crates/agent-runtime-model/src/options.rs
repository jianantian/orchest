//! Request options, capabilities, and pricing for model interactions.

use serde::{Deserialize, Serialize};

use crate::response::TokenUsage;

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    XHigh,
    Max,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum CachePolicy {
    None,
    #[default]
    Auto,
    Long,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum CompatibilityPolicy {
    #[default]
    Coerce,
    Strict,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum CapabilitySource {
    #[default]
    Static,
    ProviderMetadata,
    Assumed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestOptions {
    pub thinking: ThinkingLevel,
    pub thinking_budget_tokens: Option<u32>,
    pub include_thinking: bool,
    pub compatibility_policy: CompatibilityPolicy,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub cache_policy: CachePolicy,
    /// LLM provider 调用优先级,例如 Minimax `standard` / `priority`
    /// (`docs/external/minimax/llm/api.md:807`)。`None` 表示走 provider 默认级别。
    ///
    /// **命名冲突注**:`agent-runtime-aigc-providers::VideoGenerationConfig` 也有
    /// `service_tier` 字段,语义是"图片/视频生成调用优先级",与此处的 LLM 级别**不互通**。
    /// 跨 crate 时不要互相借用字符串值。
    #[serde(default)]
    pub service_tier: Option<String>,
}

impl Default for RequestOptions {
    fn default() -> Self {
        Self {
            thinking: ThinkingLevel::default(),
            thinking_budget_tokens: None,
            include_thinking: true,
            compatibility_policy: CompatibilityPolicy::default(),
            max_tokens: None,
            temperature: None,
            top_p: None,
            cache_policy: CachePolicy::default(),
            service_tier: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Capability metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCapabilities {
    pub streaming: bool,
    pub tool_use: bool,
    pub parallel_tool_use: bool,
    pub reasoning: ReasoningCapability,
    pub prompt_cache: CacheCapability,
    pub max_output_tokens: Option<u32>,
    pub context_window_size: Option<u64>,
    pub source: CapabilitySource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<ModelPricing>,
}

/// Pricing for a model, expressed as one or more input-length-keyed tiers.
///
/// Real-world providers charge differently by:
/// - **Input length tiers** (Volcengine 火山方舟 splits at ≤32K / 32K-128K / >128K).
/// - **Token modality** (audio/image/video inputs billed separately from text).
/// - **Cache hits vs. writes** (Anthropic prompt cache, Volcengine cache hit).
///
/// The catalog stores ALL tiers; `calculate()` picks the matching tier by
/// `usage.input_tokens` (the text input token count) and sums modality
/// contributions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub currency: String,
    /// Tiers ordered by ascending `max_input_tokens`. At least one entry is
    /// REQUIRED. The last entry typically has `max_input_tokens = None` to
    /// catch any input length.
    pub tiers: Vec<PricingTier>,
}

/// A single pricing tier keyed by an input-length ceiling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingTier {
    /// Upper bound (inclusive) on `usage.input_tokens` for this tier to match.
    /// `None` means "no upper bound" — the catch-all tier MUST be last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<u64>,
    pub rates: PricingRates,
}

/// Rate sheet for one tier, in units of `currency` per million tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingRates {
    pub text_input_per_million: f64,
    pub text_output_per_million: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_input_per_million: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_input_per_million: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_input_per_million: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_per_million: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_per_million: Option<f64>,
}

impl PricingRates {
    /// Text-only rate sheet — the common case. Use builder methods to add
    /// audio/image/video/cache rates as needed.
    pub fn text(input_per_million: f64, output_per_million: f64) -> Self {
        Self {
            text_input_per_million: input_per_million,
            text_output_per_million: output_per_million,
            audio_input_per_million: None,
            image_input_per_million: None,
            video_input_per_million: None,
            cache_read_per_million: None,
            cache_write_per_million: None,
        }
    }

    #[must_use]
    pub fn with_audio_input(mut self, rate: f64) -> Self {
        self.audio_input_per_million = Some(rate);
        self
    }

    #[must_use]
    pub fn with_image_input(mut self, rate: f64) -> Self {
        self.image_input_per_million = Some(rate);
        self
    }

    #[must_use]
    pub fn with_video_input(mut self, rate: f64) -> Self {
        self.video_input_per_million = Some(rate);
        self
    }

    #[must_use]
    pub fn with_cache(mut self, read: Option<f64>, write: Option<f64>) -> Self {
        self.cache_read_per_million = read;
        self.cache_write_per_million = write;
        self
    }
}

impl ModelPricing {
    /// Convenience constructor for a single flat tier (text-only).
    pub fn flat_text(
        currency: impl Into<String>,
        input_per_million: f64,
        output_per_million: f64,
    ) -> Self {
        Self::single_tier(
            currency,
            PricingRates::text(input_per_million, output_per_million),
        )
    }

    /// Convenience constructor for a single flat tier with arbitrary rates.
    pub fn single_tier(currency: impl Into<String>, rates: PricingRates) -> Self {
        Self {
            currency: currency.into(),
            tiers: vec![PricingTier {
                max_input_tokens: None,
                rates,
            }],
        }
    }

    /// Pick the tier whose `max_input_tokens` covers `input_tokens`. If no
    /// tier matches, returns the last tier (catch-all). Returns `None` only
    /// when the pricing has no tiers — which violates the type invariant
    /// but is handled defensively.
    pub fn tier_for_input(&self, input_tokens: u64) -> Option<&PricingTier> {
        for tier in &self.tiers {
            match tier.max_input_tokens {
                Some(cap) if input_tokens <= cap => return Some(tier),
                Some(_) => continue,
                None => return Some(tier),
            }
        }
        self.tiers.last()
    }

    /// Compute total billable cost in `currency` units, summing all modality
    /// inputs + output + cache against the matching tier.
    pub fn calculate(&self, usage: &TokenUsage) -> f64 {
        let Some(tier) = self.tier_for_input(usage.input_tokens) else {
            return 0.0;
        };
        let rates = &tier.rates;
        let per_mtok = |tokens: u64, rate: f64| (tokens as f64) * rate / 1_000_000.0;
        let mut total = per_mtok(usage.input_tokens, rates.text_input_per_million)
            + per_mtok(usage.output_tokens, rates.text_output_per_million);
        if let Some(r) = rates.audio_input_per_million {
            total += per_mtok(usage.audio_input_tokens, r);
        }
        if let Some(r) = rates.image_input_per_million {
            total += per_mtok(usage.image_input_tokens, r);
        }
        if let Some(r) = rates.video_input_per_million {
            total += per_mtok(usage.video_input_tokens, r);
        }
        if let Some(r) = rates.cache_read_per_million {
            total += per_mtok(usage.cache_read_tokens, r);
        }
        if let Some(r) = rates.cache_write_per_million {
            total += per_mtok(usage.cache_write_tokens, r);
        }
        total
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReasoningCapability {
    pub supported: bool,
    pub efforts: Vec<ThinkingLevel>,
    pub budget_tokens: bool,
    pub output_exclusion: bool,
    pub replay_metadata_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheCapability {
    pub supported: bool,
    pub explicit_breakpoints: bool,
    pub long_ttl: bool,
}
