use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrTelemetry {
    pub trace_id: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub audio_duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_first_update_ms: Option<u64>,
    pub latency_final_ms: u64,
    #[serde(default)]
    pub update_rollback_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_avg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_estimate_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_status: Option<u16>,
    #[serde(default)]
    pub option_adjustment_count: u32,
}
