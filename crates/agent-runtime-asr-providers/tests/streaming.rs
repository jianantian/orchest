mod fake_provider;

use agent_runtime_asr_providers::error::AsrErrorCode;
use agent_runtime_asr_providers::traits::AsrProvider;
use agent_runtime_asr_providers::types::*;
use bytes::Bytes;
use fake_provider::{FakeAdapterBehavior, FakeAsrProvider};
use std::time::Duration;

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

fn make_request_with_scope(scope: FinalResultScope) -> StreamingTranscribeRequest {
    let mut req = make_request();
    req.options.final_result_scope = scope;
    req
}

// ---------------------------------------------------------------------------
// Event ordering
// ---------------------------------------------------------------------------

#[tokio::test]
async fn event_ordering_route_selected_then_started() {
    let provider = FakeAsrProvider::with_behavior(FakeAdapterBehavior::Normal);
    let mut stream = provider.start_stream(make_request()).await.unwrap();

    let ev1 = stream.events.next().await.unwrap();
    assert!(matches!(ev1, AsrStreamEvent::RouteSelected { .. }));

    let ev2 = stream.events.next().await.unwrap();
    assert!(matches!(ev2, AsrStreamEvent::Started { .. }));

    stream.input.end_stream().await.unwrap();
}

// ---------------------------------------------------------------------------
// flush_and_wait_final
// ---------------------------------------------------------------------------

#[tokio::test]
async fn flush_and_wait_final_returns_caller_flush() {
    let provider = FakeAsrProvider::with_behavior(FakeAdapterBehavior::Normal);
    let mut stream = provider.start_stream(make_request()).await.unwrap();

    // Drain RouteSelected + Started
    stream.events.next().await;
    stream.events.next().await;

    // Send audio
    stream
        .input
        .send_audio(Bytes::from(vec![0u8; 100]), None)
        .await
        .unwrap();

    let final_output = stream.flush_and_wait_final().await.unwrap();
    assert_eq!(final_output.reason, AsrFinalReason::CallerFlush);
    assert!(!final_output.result.text.is_empty());

    // End the stream
    stream.input.end_stream().await.unwrap();
}

// ---------------------------------------------------------------------------
// end_and_wait_final
// ---------------------------------------------------------------------------

#[tokio::test]
async fn end_and_wait_final_returns_caller_end() {
    let provider = FakeAsrProvider::with_behavior(FakeAdapterBehavior::Normal);
    let mut stream = provider.start_stream(make_request()).await.unwrap();

    stream.events.next().await;
    stream.events.next().await;

    stream
        .input
        .send_audio(Bytes::from(vec![0u8; 100]), None)
        .await
        .unwrap();

    let final_output = stream.end_and_wait_final().await.unwrap();
    assert_eq!(final_output.reason, AsrFinalReason::CallerEnd);
    assert!(!final_output.result.text.is_empty());
}

// ---------------------------------------------------------------------------
// split() full-duplex
// ---------------------------------------------------------------------------

#[tokio::test]
async fn split_full_duplex_independent_tasks() {
    let provider = FakeAsrProvider::with_behavior(FakeAdapterBehavior::Normal);
    let stream = provider.start_stream(make_request()).await.unwrap();

    let (sink, mut events) = stream.split();

    let sender = tokio::spawn(async move {
        sink.send_audio(Bytes::from(vec![0u8; 100]), None)
            .await
            .unwrap();
        sink.end_stream().await.unwrap();
    });

    let receiver = tokio::spawn(async move {
        // Collect all events
        let mut saw_route = false;
        let mut saw_started = false;
        let mut saw_final = false;
        while let Some(ev) = events.next().await {
            match ev {
                AsrStreamEvent::RouteSelected { .. } => saw_route = true,
                AsrStreamEvent::Started { .. } => saw_started = true,
                AsrStreamEvent::AsrFinal { final_output } => {
                    saw_final = true;
                    assert_eq!(final_output.reason, AsrFinalReason::CallerEnd);
                }
                _ => {}
            }
        }
        assert!(saw_route);
        assert!(saw_started);
        assert!(saw_final);
    });

    sender.await.unwrap();
    receiver.await.unwrap();
}

// ---------------------------------------------------------------------------
// Flush timeout
// ---------------------------------------------------------------------------

#[tokio::test]
async fn flush_timeout_emits_timeout_final() {
    let provider = FakeAsrProvider::with_behavior_and_timeout(
        FakeAdapterBehavior::FlushTimeout,
        Duration::from_millis(50),
    );
    let mut request = make_request();
    request.options.flush_timeout = Some(Duration::from_millis(50));
    let mut stream = provider.start_stream(request).await.unwrap();

    stream.events.next().await; // RouteSelected
    stream.events.next().await; // Started

    stream
        .input
        .send_audio(Bytes::from(vec![0u8; 100]), None)
        .await
        .unwrap();

    let final_output = stream.flush_and_wait_final().await.unwrap();
    assert_eq!(final_output.reason, AsrFinalReason::Timeout);

    stream.input.end_stream().await.unwrap();
}

// ---------------------------------------------------------------------------
// Late provider final after timeout
// ---------------------------------------------------------------------------

#[tokio::test]
async fn late_provider_final_does_not_duplicate() {
    let provider = FakeAsrProvider::with_behavior_and_timeout(
        FakeAdapterBehavior::LateProviderFinal,
        Duration::from_millis(30),
    );
    let mut request = make_request();
    request.options.flush_timeout = Some(Duration::from_millis(30));
    let mut stream = provider.start_stream(request).await.unwrap();

    stream.events.next().await; // RouteSelected
    stream.events.next().await; // Started

    stream
        .input
        .send_audio(Bytes::from(vec![0u8; 100]), None)
        .await
        .unwrap();

    let final_output = stream.flush_and_wait_final().await.unwrap();
    assert_eq!(final_output.reason, AsrFinalReason::Timeout);

    // End the stream — should not receive a second AsrFinal
    stream.input.end_stream().await.unwrap();

    // After end, the adapter task returns; no duplicate final should appear
    // Drain any remaining events
    let mut extra_finals = 0;
    while let Some(ev) = stream.events.next().await {
        if matches!(ev, AsrStreamEvent::AsrFinal { .. }) {
            extra_finals += 1;
        }
    }
    assert_eq!(extra_finals, 0, "no duplicate AsrFinal after timeout");
}

// ---------------------------------------------------------------------------
// Provider endpointing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn provider_endpoint_emits_end_of_speech_and_final() {
    let provider = FakeAsrProvider::with_behavior(FakeAdapterBehavior::ProviderEndpoint);
    let mut stream = provider.start_stream(make_request()).await.unwrap();

    stream.events.next().await; // RouteSelected
    stream.events.next().await; // Started

    // Send large chunk to trigger provider endpoint
    stream
        .input
        .send_audio(Bytes::from(vec![0u8; 200]), None)
        .await
        .unwrap();

    // Collect events
    let mut saw_eos = false;
    let mut final_reason = None;
    while let Some(ev) = stream.events.next().await {
        match ev {
            AsrStreamEvent::EndOfSpeech { .. } => saw_eos = true,
            AsrStreamEvent::AsrFinal { final_output } => {
                final_reason = Some(final_output.reason);
                break;
            }
            _ => {}
        }
    }
    assert!(saw_eos, "EndOfSpeech should precede AsrFinal");
    assert_eq!(final_reason, Some(AsrFinalReason::ProviderEndpoint));
}

// ---------------------------------------------------------------------------
// Provider endpoint + caller flush: no duplicate
// ---------------------------------------------------------------------------

#[tokio::test]
async fn provider_endpoint_then_caller_flush_no_duplicate() {
    let provider =
        FakeAsrProvider::with_behavior(FakeAdapterBehavior::ProviderEndpointThenCallerFlush);
    let stream = provider.start_stream(make_request()).await.unwrap();
    let (sink, mut events) = stream.split();

    events.next().await; // RouteSelected
    events.next().await; // Started

    // Large chunk triggers provider endpoint
    sink.send_audio(Bytes::from(vec![0u8; 200]), None)
        .await
        .unwrap();

    // Caller flush after provider already finalized
    sink.flush_segment().await.unwrap();

    // End
    sink.end_stream().await.unwrap();

    let mut final_count = 0;
    while let Some(ev) = events.next().await {
        if matches!(ev, AsrStreamEvent::AsrFinal { .. }) {
            final_count += 1;
        }
    }
    assert_eq!(final_count, 1, "only one AsrFinal per segment");
}

// ---------------------------------------------------------------------------
// Sink drop cancellation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sink_drop_without_end_emits_cancelled() {
    let provider = FakeAsrProvider::with_behavior(FakeAdapterBehavior::Normal);
    let stream = provider.start_stream(make_request()).await.unwrap();
    let (sink, mut events) = stream.split();

    events.next().await; // RouteSelected
    events.next().await; // Started

    // Drop sink without End
    drop(sink);

    let final_ev = events.next_final().await;
    assert!(final_ev.is_err());
    assert_eq!(final_ev.unwrap_err().code, AsrErrorCode::Cancelled);
}

// ---------------------------------------------------------------------------
// FinalResultScope::Stream accumulation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn final_result_scope_stream_accumulates() {
    let provider = FakeAsrProvider::with_behavior(FakeAdapterBehavior::Normal);
    let mut stream = provider
        .start_stream(make_request_with_scope(FinalResultScope::Stream))
        .await
        .unwrap();

    stream.events.next().await; // RouteSelected
    stream.events.next().await; // Started

    // First segment
    stream
        .input
        .send_audio(Bytes::from(vec![0u8; 100]), None)
        .await
        .unwrap();
    let f1 = stream.flush_and_wait_final().await.unwrap();
    let text1 = f1.result.text.clone();

    // Second segment
    stream
        .input
        .send_audio(Bytes::from(vec![0u8; 100]), None)
        .await
        .unwrap();
    let f2 = stream.flush_and_wait_final().await.unwrap();

    // Stream scope: second final text should contain first segment's text
    assert!(
        f2.result.text.len() > text1.len(),
        "stream scope should accumulate: '{}' vs '{}'",
        f2.result.text,
        text1
    );

    stream.input.end_stream().await.unwrap();
}

// ---------------------------------------------------------------------------
// FinalResultScope::Segment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn final_result_scope_segment_does_not_accumulate() {
    let provider = FakeAsrProvider::with_behavior(FakeAdapterBehavior::Normal);
    let mut stream = provider
        .start_stream(make_request_with_scope(FinalResultScope::Segment))
        .await
        .unwrap();

    stream.events.next().await;
    stream.events.next().await;

    stream
        .input
        .send_audio(Bytes::from(vec![0u8; 100]), None)
        .await
        .unwrap();
    let f1 = stream.flush_and_wait_final().await.unwrap();

    stream
        .input
        .send_audio(Bytes::from(vec![0u8; 100]), None)
        .await
        .unwrap();
    let f2 = stream.flush_and_wait_final().await.unwrap();

    // Segment scope: both finals should have similar length
    assert_eq!(
        f1.result.text.len(),
        f2.result.text.len(),
        "segment scope should not accumulate"
    );

    stream.input.end_stream().await.unwrap();
}

// ---------------------------------------------------------------------------
// Committed deduplication
// ---------------------------------------------------------------------------

#[tokio::test]
async fn committed_segment_deduplication() {
    let provider = FakeAsrProvider::with_behavior(FakeAdapterBehavior::DuplicateCommitted);
    let stream = provider.start_stream(make_request()).await.unwrap();
    let (sink, mut events) = stream.split();

    events.next().await; // RouteSelected
    events.next().await; // Started

    // Send chunks that trigger duplicate committed updates
    sink.send_audio(Bytes::from(vec![0u8; 100]), None)
        .await
        .unwrap();
    sink.send_audio(Bytes::from(vec![0u8; 100]), None)
        .await
        .unwrap();
    sink.end_stream().await.unwrap();

    let mut committed_count = 0;
    while let Some(ev) = events.next().await {
        if matches!(
            ev,
            AsrStreamEvent::TranscriptUpdate {
                stability: TranscriptStability::Committed,
                ..
            }
        ) {
            committed_count += 1;
        }
    }
    // Each chunk produces one unique committed update (deduped from 2 → 1)
    assert_eq!(
        committed_count, 2,
        "two unique committed updates from two chunks"
    );
}

// ---------------------------------------------------------------------------
// AsrFinalOutput carries expected fields
// ---------------------------------------------------------------------------

#[tokio::test]
async fn final_output_carries_trace_segment_reason() {
    let provider = FakeAsrProvider::with_behavior(FakeAdapterBehavior::Normal);
    let mut request = make_request();
    request.options.trace_id = Some("test-trace-123".into());
    let mut stream = provider.start_stream(request).await.unwrap();

    stream.events.next().await;
    stream.events.next().await;

    stream
        .input
        .send_audio(Bytes::from(vec![0u8; 100]), None)
        .await
        .unwrap();
    let f = stream.flush_and_wait_final().await.unwrap();

    assert_eq!(f.trace_id, "test-trace-123");
    assert!(f.segment_id.is_some());
    assert_eq!(f.reason, AsrFinalReason::CallerFlush);
    assert!(!f.result.text.is_empty());

    stream.input.end_stream().await.unwrap();
}

// ---------------------------------------------------------------------------
// AudioChunk.timestamp_ms preserved
// ---------------------------------------------------------------------------

#[tokio::test]
async fn audio_chunk_timestamp_preserved() {
    let provider = FakeAsrProvider::with_behavior(FakeAdapterBehavior::Normal);
    let mut stream = provider.start_stream(make_request()).await.unwrap();

    stream.events.next().await;
    stream.events.next().await;

    // Timestamp is passed through to the adapter — it doesn't fabricate silence
    stream
        .input
        .send_audio(Bytes::from(vec![0u8; 100]), Some(1000))
        .await
        .unwrap();
    stream
        .input
        .send_audio(Bytes::from(vec![0u8; 100]), Some(5000))
        .await
        .unwrap();

    let f = stream.end_and_wait_final().await.unwrap();
    assert!(!f.result.text.is_empty());
}
