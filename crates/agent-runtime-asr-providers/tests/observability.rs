mod fake_provider;

use agent_runtime_asr_providers::error::{redact_secrets, AsrError, AsrErrorCode};
use agent_runtime_asr_providers::traits::AsrProvider;
use agent_runtime_asr_providers::types::*;
use bytes::Bytes;
use fake_provider::{FakeAdapterBehavior, FakeAsrProvider};

fn make_request() -> StreamingTranscribeRequest {
    StreamingTranscribeRequest {
        model: Some("fake/test".into()),
        format: StreamingAudioFormat::Pcm16 {
            sample_rate_hz: 16000,
            channels: 1,
        },
        timeline: AudioTimelineMode::ContinuousRealtime,
        options: TranscribeOptions::default(),
        compatibility: CompatibilityPolicy::Coerce,
        provider_options: serde_json::Value::Null,
    }
}

// ---------------------------------------------------------------------------
// Trace ID propagation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn trace_id_propagates_through_all_events() {
    let provider = FakeAsrProvider::with_behavior(FakeAdapterBehavior::Normal);
    let mut request = make_request();
    request.options.trace_id = Some("trace-propagation-test".into());

    let stream = provider.start_stream(request).await.unwrap();
    let (sink, mut events) = stream.split();

    sink.send_audio(Bytes::from(vec![0u8; 100]), None)
        .await
        .unwrap();
    sink.end_stream().await.unwrap();

    let mut trace_ids = Vec::new();
    while let Some(ev) = events.next().await {
        match ev {
            AsrStreamEvent::RouteSelected { trace_id, .. } => trace_ids.push(trace_id),
            AsrStreamEvent::Started { trace_id, .. } => trace_ids.push(trace_id),
            AsrStreamEvent::TranscriptUpdate { trace_id, .. } => trace_ids.push(trace_id),
            AsrStreamEvent::EndOfSpeech { trace_id, .. } => trace_ids.push(trace_id),
            AsrStreamEvent::AsrFinal { final_output } => {
                trace_ids.push(final_output.trace_id.clone())
            }
            AsrStreamEvent::Error { trace_id, .. } => trace_ids.push(trace_id),
        }
    }

    assert!(!trace_ids.is_empty());
    for tid in &trace_ids {
        assert_eq!(tid, "trace-propagation-test");
    }
}

#[tokio::test]
async fn auto_trace_id_is_generated_when_none() {
    let provider = FakeAsrProvider::with_behavior(FakeAdapterBehavior::Normal);
    let request = make_request(); // trace_id is None by default

    let mut stream = provider.start_stream(request).await.unwrap();
    let ev = stream.events.next().await.unwrap();

    let trace_id = match ev {
        AsrStreamEvent::RouteSelected { trace_id, .. } => trace_id,
        _ => panic!("expected RouteSelected"),
    };
    assert!(!trace_id.is_empty());

    stream.input.end_stream().await.unwrap();
}

// ---------------------------------------------------------------------------
// Secret redaction
// ---------------------------------------------------------------------------

#[test]
fn redact_secrets_in_json_body() {
    let mut body = serde_json::json!({
        "api_key": "sk-12345",
        "access_token": "tok-abc",
        "data": "visible",
        "nested": {
            "password": "hunter2",
            "info": "ok"
        },
        "list": [
            {"secret_value": "hidden", "name": "test"}
        ]
    });

    redact_secrets(&mut body);

    assert_eq!(body["api_key"], "[REDACTED]");
    assert_eq!(body["access_token"], "[REDACTED]");
    assert_eq!(body["data"], "visible");
    assert_eq!(body["nested"]["password"], "[REDACTED]");
    assert_eq!(body["nested"]["info"], "ok");
    assert_eq!(body["list"][0]["secret_value"], "[REDACTED]");
    assert_eq!(body["list"][0]["name"], "test");
}

#[test]
fn with_upstream_auto_redacts() {
    let body = serde_json::json!({
        "api_key": "sk-12345",
        "message": "error details"
    });

    let err = AsrError::new(AsrErrorCode::ProviderHttpError, "upstream failed").with_upstream(
        Some(401),
        Some("auth_error".into()),
        Some("unauthorized".into()),
        Some(body),
    );

    let upstream_body = err.upstream_body.unwrap();
    assert_eq!(upstream_body["api_key"], "[REDACTED]");
    assert_eq!(upstream_body["message"], "error details");
    assert_eq!(err.status, Some(401));
}

// ---------------------------------------------------------------------------
// AsrTelemetry in final output
// ---------------------------------------------------------------------------

#[tokio::test]
async fn final_output_contains_telemetry() {
    let provider = FakeAsrProvider::with_behavior(FakeAdapterBehavior::Normal);
    let mut request = make_request();
    request.options.trace_id = Some("tel-test".into());

    let mut stream = provider.start_stream(request).await.unwrap();
    stream.events.next().await; // RouteSelected
    stream.events.next().await; // Started

    stream
        .input
        .send_audio(Bytes::from(vec![0u8; 100]), None)
        .await
        .unwrap();

    let final_output = stream.flush_and_wait_final().await.unwrap();
    assert_eq!(final_output.result.telemetry.trace_id, "tel-test");
    assert_eq!(
        final_output.result.telemetry.model,
        "volcengine/bigmodel_async"
    );

    stream.input.end_stream().await.unwrap();
}
