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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub input_per_million_usd: f64,
    pub output_per_million_usd: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_per_million_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_per_million_usd: Option<f64>,
}

impl ModelPricing {
    pub fn calculate(&self, usage: &TokenUsage) -> f64 {
        let base = usage.input_tokens as f64 * self.input_per_million_usd / 1_000_000.0
            + usage.output_tokens as f64 * self.output_per_million_usd / 1_000_000.0;
        let cache_read = usage.cache_read_tokens as f64
            * self.cache_read_per_million_usd.unwrap_or(0.0)
            / 1_000_000.0;
        let cache_write = usage.cache_write_tokens as f64
            * self.cache_write_per_million_usd.unwrap_or(0.0)
            / 1_000_000.0;
        base + cache_read + cache_write
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
