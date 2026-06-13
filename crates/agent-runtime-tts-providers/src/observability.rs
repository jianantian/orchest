use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::Span;

use crate::types::TtsOperation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsTelemetry {
    pub trace_id: String,
    pub provider: String,
    pub model: String,
    pub operation: TtsOperation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_id: Option<String>,
    pub input_chars: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_audio_latency_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_latency_ms: Option<u64>,
    pub option_adjustment_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_code: Option<String>,
}

pub struct TtsTelemetryBuilder {
    start: Instant,
    first_audio_at: Option<Instant>,
    trace_id: String,
    provider: String,
    model: String,
    operation: TtsOperation,
    voice_id: Option<String>,
    input_chars: u64,
    output_bytes: Option<u64>,
    option_adjustment_count: u64,
    status: Option<u16>,
    upstream_code: Option<String>,
}

impl TtsTelemetryBuilder {
    pub fn new(
        trace_id: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
        operation: TtsOperation,
    ) -> Self {
        Self {
            start: Instant::now(),
            first_audio_at: None,
            trace_id: trace_id.into(),
            provider: provider.into(),
            model: model.into(),
            operation,
            voice_id: None,
            input_chars: 0,
            output_bytes: None,
            option_adjustment_count: 0,
            status: None,
            upstream_code: None,
        }
    }

    pub fn voice_id(mut self, voice_id: Option<String>) -> Self {
        self.voice_id = voice_id;
        self
    }

    pub fn input_chars(mut self, input_chars: u64) -> Self {
        self.input_chars = input_chars;
        self
    }

    pub fn output_bytes(mut self, output_bytes: Option<u64>) -> Self {
        self.output_bytes = output_bytes;
        self
    }

    pub fn option_adjustment_count(mut self, count: usize) -> Self {
        self.option_adjustment_count = count as u64;
        self
    }

    pub fn mark_first_audio(&mut self) {
        if self.first_audio_at.is_none() {
            self.first_audio_at = Some(Instant::now());
        }
    }

    pub fn build(&self) -> TtsTelemetry {
        let first_audio_latency_ms = self
            .first_audio_at
            .map(|instant| elapsed_ms(self.start.elapsed().saturating_sub(instant.elapsed())));
        TtsTelemetry {
            trace_id: self.trace_id.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            operation: self.operation.clone(),
            voice_id: self.voice_id.clone(),
            input_chars: self.input_chars,
            output_bytes: self.output_bytes,
            first_audio_latency_ms,
            final_latency_ms: Some(elapsed_ms(self.start.elapsed())),
            option_adjustment_count: self.option_adjustment_count,
            status: self.status,
            upstream_code: self.upstream_code.clone(),
        }
    }

    pub fn build_with_final_latency(&self, final_latency: Duration) -> TtsTelemetry {
        let mut telemetry = self.build();
        telemetry.final_latency_ms = Some(elapsed_ms(final_latency));
        telemetry
    }
}

fn elapsed_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

pub fn gateway_synthesize_span(trace_id: &str, provider: &str, model: &str) -> Span {
    tracing::info_span!(
        "tts.gateway.synthesize",
        trace_id = trace_id,
        provider = provider,
        model = model
    )
}

pub fn gateway_stream_span(trace_id: &str, provider: &str, model: &str) -> Span {
    tracing::info_span!(
        "tts.gateway.stream",
        trace_id = trace_id,
        provider = provider,
        model = model
    )
}

pub fn provider_request_span(trace_id: &str, provider: &str, model: &str) -> Span {
    tracing::info_span!(
        "tts.provider.request",
        trace_id = trace_id,
        provider = provider,
        model = model
    )
}

pub fn provider_stream_span(trace_id: &str, provider: &str, model: &str) -> Span {
    tracing::info_span!(
        "tts.provider.stream",
        trace_id = trace_id,
        provider = provider,
        model = model
    )
}

pub fn router_select_span(trace_id: &str) -> Span {
    tracing::info_span!("tts.router.select", trace_id = trace_id)
}

pub fn voices_list_span(trace_id: &str) -> Span {
    tracing::info_span!("tts.voices.list", trace_id = trace_id)
}

pub fn record_request_duration(provider: &str, model: &str, duration: Duration) {
    metrics::histogram!(
        "tts.request.duration_ms",
        "provider" => provider.to_owned(),
        "model" => model.to_owned()
    )
    .record(duration.as_millis() as f64);
}

pub fn record_first_audio_latency(provider: &str, model: &str, duration: Duration) {
    metrics::histogram!(
        "tts.first_audio_latency_ms",
        "provider" => provider.to_owned(),
        "model" => model.to_owned()
    )
    .record(duration.as_millis() as f64);
}

pub fn record_final_latency(provider: &str, model: &str, duration: Duration) {
    metrics::histogram!(
        "tts.final_latency_ms",
        "provider" => provider.to_owned(),
        "model" => model.to_owned()
    )
    .record(duration.as_millis() as f64);
}

pub fn record_input_chars(provider: &str, model: &str, chars: u64) {
    metrics::counter!(
        "tts.input_chars",
        "provider" => provider.to_owned(),
        "model" => model.to_owned()
    )
    .increment(chars);
}

pub fn record_output_bytes(provider: &str, model: &str, bytes: u64) {
    metrics::counter!(
        "tts.output_bytes",
        "provider" => provider.to_owned(),
        "model" => model.to_owned()
    )
    .increment(bytes);
}

pub fn record_audio_duration(provider: &str, model: &str, duration: Duration) {
    metrics::histogram!(
        "tts.audio_duration_ms",
        "provider" => provider.to_owned(),
        "model" => model.to_owned()
    )
    .record(duration.as_millis() as f64);
}

pub fn record_option_adjustments(provider: &str, model: &str, count: u64) {
    metrics::counter!(
        "tts.option_adjustment_count",
        "provider" => provider.to_owned(),
        "model" => model.to_owned()
    )
    .increment(count);
}

pub fn record_provider_error(provider: &str, model: &str, code: &str) {
    metrics::counter!(
        "tts.provider.error_count",
        "provider" => provider.to_owned(),
        "model" => model.to_owned(),
        "code" => code.to_owned()
    )
    .increment(1);
}
