#![cfg(feature = "aliyun")]

use agent_runtime_asr_providers::providers::aliyun::{AliyunAsrAdapter, AliyunAsrConfig};
use agent_runtime_asr_providers::traits::AsrProvider;
use agent_runtime_asr_providers::types::*;
use bytes::Bytes;

fn get_config() -> Option<AliyunAsrConfig> {
    let api_key = std::env::var("ALIYUN_ASR_API_KEY").ok()?;
    Some(AliyunAsrConfig::fun_asr_realtime(api_key))
}

#[ignore]
#[tokio::test]
async fn live_aliyun_streaming_silence() {
    let config = match get_config() {
        Some(c) => c,
        None => {
            eprintln!("skipping: ALIYUN_ASR_API_KEY not set");
            return;
        }
    };

    let adapter = AliyunAsrAdapter::new(config);
    let request = StreamingTranscribeRequest {
        model: Some("aliyun/fun-asr-realtime".into()),
        format: StreamingAudioFormat::Pcm16 {
            sample_rate_hz: 16000,
            channels: 1,
        },
        timeline: AudioTimelineMode::ContinuousRealtime,
        options: TranscribeOptions {
            trace_id: Some("live-test-aliyun-silence".into()),
            ..Default::default()
        },
        compatibility: CompatibilityPolicy::Coerce,
        provider_options: serde_json::Value::Null,
    };

    let mut stream = adapter.start_stream(request).await.unwrap();

    stream.events.next().await; // RouteSelected
    stream.events.next().await; // Started

    let silence = vec![0u8; 32000];
    stream
        .input
        .send_audio(Bytes::from(silence), None)
        .await
        .unwrap();

    let final_output = stream.end_and_wait_final().await.unwrap();
    assert_eq!(final_output.trace_id, "live-test-aliyun-silence");
    assert_eq!(
        final_output.result.telemetry.model,
        "aliyun/fun-asr-realtime"
    );
    assert!(final_output.result.telemetry.latency_final_ms > 0);
}
