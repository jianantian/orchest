//! Usage and cost estimates for speech recognition and synthesis.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VoiceOutcome {
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VoiceUsageSource {
    /// Usage reported by the provider in its protocol.
    ProviderReported,
    /// Quantity explicitly identified as billable by the provider.
    ProviderBilled,
    /// Derived locally from audio bytes or the submitted text.
    InputEstimate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceQuantity {
    pub value: u64,
    pub source: VoiceUsageSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VoiceRateUnit {
    AudioSecond,
    BilledCharacter,
}

/// A published rate and the conditions under which it applies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceRate {
    pub currency: String,
    pub unit: VoiceRateUnit,
    pub price_per_unit: f64,
    pub scope: String,
    pub source_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceCostEstimate {
    pub amount: f64,
    pub rate: VoiceRate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceUsage {
    pub provider: String,
    pub model: String,
    pub outcome: VoiceOutcome,
    /// In milliseconds. The source identifies how the duration was obtained.
    pub audio_duration_ms: Option<VoiceQuantity>,
    /// Submitted Unicode scalar values, including whitespace and punctuation.
    pub input_characters: Option<u64>,
    /// Provider-defined billable characters, if reported.
    pub billed_characters: Option<u64>,
    pub cost_estimate: Option<VoiceCostEstimate>,
}

impl VoiceUsage {
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        outcome: VoiceOutcome,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            outcome,
            audio_duration_ms: None,
            input_characters: None,
            billed_characters: None,
            cost_estimate: None,
        }
    }
}
