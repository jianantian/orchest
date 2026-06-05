use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

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

// ---------------------------------------------------------------------------
// Telemetry builder
// ---------------------------------------------------------------------------

pub struct AsrTelemetryBuilder {
    trace_id: String,
    model: String,
    language: Option<String>,
    started_at: Instant,
    first_update_at: Option<Instant>,
    rollback_count: u32,
    confidences: Vec<f64>,
    audio_duration_ms: u64,
    cost_estimate_micros: Option<u64>,
    network_region: Option<String>,
    upstream_status: Option<u16>,
    option_adjustment_count: u32,
}

impl AsrTelemetryBuilder {
    pub fn new(trace_id: String, model: String) -> Self {
        Self {
            trace_id,
            model,
            language: None,
            started_at: Instant::now(),
            first_update_at: None,
            rollback_count: 0,
            confidences: Vec::new(),
            audio_duration_ms: 0,
            cost_estimate_micros: None,
            network_region: None,
            upstream_status: None,
            option_adjustment_count: 0,
        }
    }

    pub fn on_started(&mut self) {
        self.started_at = Instant::now();
    }

    pub fn on_transcript_update(&mut self, confidence: Option<f64>, is_rollback: bool) {
        if self.first_update_at.is_none() {
            self.first_update_at = Some(Instant::now());
        }
        if is_rollback {
            self.rollback_count += 1;
        }
        if let Some(c) = confidence {
            self.confidences.push(c);
        }
    }

    pub fn set_language(&mut self, language: String) {
        self.language = Some(language);
    }

    pub fn set_audio_duration_ms(&mut self, ms: u64) {
        self.audio_duration_ms = ms;
    }

    pub fn set_cost_estimate_micros(&mut self, micros: u64) {
        self.cost_estimate_micros = Some(micros);
    }

    pub fn set_network_region(&mut self, region: String) {
        self.network_region = Some(region);
    }

    pub fn set_upstream_status(&mut self, status: u16) {
        self.upstream_status = Some(status);
    }

    pub fn set_option_adjustment_count(&mut self, count: u32) {
        self.option_adjustment_count = count;
    }

    pub fn build(self) -> AsrTelemetry {
        let now = Instant::now();
        let latency_final_ms = now.duration_since(self.started_at).as_millis() as u64;
        let latency_first_update_ms = self
            .first_update_at
            .map(|t| t.duration_since(self.started_at).as_millis() as u64);

        let confidence_avg = if self.confidences.is_empty() {
            None
        } else {
            let sum: f64 = self.confidences.iter().sum();
            Some(sum / self.confidences.len() as f64)
        };

        AsrTelemetry {
            trace_id: self.trace_id,
            model: self.model,
            language: self.language,
            audio_duration_ms: self.audio_duration_ms,
            latency_first_update_ms,
            latency_final_ms,
            update_rollback_count: self.rollback_count,
            confidence_avg,
            cost_estimate_micros: self.cost_estimate_micros,
            network_region: self.network_region,
            upstream_status: self.upstream_status,
            option_adjustment_count: self.option_adjustment_count,
        }
    }
}

// ---------------------------------------------------------------------------
// Tracing spans
// ---------------------------------------------------------------------------

pub fn gateway_transcribe_span(trace_id: &str, model: &str) -> tracing::Span {
    tracing::info_span!("asr.gateway.transcribe", trace_id, model)
}

pub fn gateway_stream_span(trace_id: &str, model: &str) -> tracing::Span {
    tracing::info_span!("asr.gateway.stream", trace_id, model)
}

pub fn provider_request_span(trace_id: &str, model: &str) -> tracing::Span {
    tracing::info_span!("asr.provider.request", trace_id, model)
}

pub fn provider_stream_span(trace_id: &str, model: &str) -> tracing::Span {
    tracing::info_span!("asr.provider.stream", trace_id, model)
}

pub fn router_select_span(trace_id: &str) -> tracing::Span {
    tracing::info_span!("asr.router.select", trace_id)
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

pub fn record_request_duration(model: &str, duration: Duration) {
    metrics::histogram!("asr.request.duration_ms", "model" => model.to_string())
        .record(duration.as_millis() as f64);
}

pub fn record_first_update_latency(model: &str, latency_ms: u64) {
    metrics::histogram!("asr.first_update.latency_ms", "model" => model.to_string())
        .record(latency_ms as f64);
}

pub fn record_final_latency(model: &str, latency_ms: u64) {
    metrics::histogram!("asr.final.latency_ms", "model" => model.to_string())
        .record(latency_ms as f64);
}

pub fn record_audio_duration(model: &str, duration_ms: u64) {
    metrics::histogram!("asr.audio.duration_ms", "model" => model.to_string())
        .record(duration_ms as f64);
}

pub fn record_upstream_error(model: &str, code: &str, status: Option<u16>) {
    let status_str = status.map_or_else(|| "none".to_string(), |s| s.to_string());
    metrics::counter!("asr.upstream.error_count", "model" => model.to_string(), "code" => code.to_string(), "status" => status_str)
        .increment(1);
}

pub fn record_rollback_count(model: &str, count: u32) {
    metrics::counter!("asr.transcript.rollback_count", "model" => model.to_string())
        .increment(count as u64);
}

pub fn record_option_adjustment_count(model: &str, count: u32) {
    metrics::counter!("asr.option.adjustment_count", "model" => model.to_string())
        .increment(count as u64);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn telemetry_builder_basic_flow() {
        let mut builder = AsrTelemetryBuilder::new("trace-1".into(), "fake/test".into());
        builder.on_started();

        thread::sleep(Duration::from_millis(5));
        builder.on_transcript_update(Some(0.9), false);

        thread::sleep(Duration::from_millis(5));
        builder.on_transcript_update(Some(0.8), true);
        builder.on_transcript_update(None, false);

        builder.set_audio_duration_ms(3000);
        builder.set_language("zh-CN".into());

        let telemetry = builder.build();
        assert_eq!(telemetry.trace_id, "trace-1");
        assert_eq!(telemetry.model, "fake/test");
        assert_eq!(telemetry.language, Some("zh-CN".into()));
        assert_eq!(telemetry.audio_duration_ms, 3000);
        assert!(telemetry.latency_first_update_ms.is_some());
        assert!(telemetry.latency_final_ms >= 10);
        assert_eq!(telemetry.update_rollback_count, 1);
        let avg = telemetry.confidence_avg.unwrap();
        assert!((avg - 0.85).abs() < 0.01);
    }

    #[test]
    fn telemetry_builder_no_updates() {
        let builder = AsrTelemetryBuilder::new("trace-2".into(), "fake/test".into());
        let telemetry = builder.build();
        assert!(telemetry.latency_first_update_ms.is_none());
        assert_eq!(telemetry.update_rollback_count, 0);
        assert!(telemetry.confidence_avg.is_none());
    }

    #[test]
    fn telemetry_builder_option_adjustments() {
        let mut builder = AsrTelemetryBuilder::new("trace-3".into(), "fake/test".into());
        builder.set_option_adjustment_count(3);
        builder.set_upstream_status(200);
        builder.set_network_region("cn-east".into());
        builder.set_cost_estimate_micros(500);

        let telemetry = builder.build();
        assert_eq!(telemetry.option_adjustment_count, 3);
        assert_eq!(telemetry.upstream_status, Some(200));
        assert_eq!(telemetry.network_region, Some("cn-east".into()));
        assert_eq!(telemetry.cost_estimate_micros, Some(500));
    }
}
