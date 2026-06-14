use std::sync::Arc;

use agent_runtime_asr_providers::providers::aliyun::{AliyunAsrAdapter, AliyunAsrConfig};
use agent_runtime_asr_providers::routing::{AsrGateway, AsrGatewayConfig, AsrRoute, AsrRouter};
use agent_runtime_asr_providers::types::*;
use bytes::Bytes;

#[tokio::main]
async fn main() {
    let api_key = std::env::var("DASHSCOPE_API_KEY").expect("set DASHSCOPE_API_KEY");

    let config = AliyunAsrConfig::fun_asr_realtime(api_key);
    let adapter = Arc::new(AliyunAsrAdapter::new(config));

    let mut router = AsrRouter::new();
    router.register_provider("aliyun/fun-asr-realtime".into(), adapter);
    router.set_routes(vec![AsrRoute {
        languages: vec![Language::new("zh-CN")],
        regions: vec![],
        max_latency_ms: None,
        max_cost_micros_per_minute: None,
        priority: 0,
        model: "aliyun/fun-asr-realtime".into(),
    }]);

    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let request = StreamingTranscribeRequest {
        model: Some("aliyun/fun-asr-realtime".into()),
        format: StreamingAudioFormat::Pcm16 {
            sample_rate_hz: 16000,
            channels: 1,
        },
        timeline: AudioTimelineMode::ContinuousRealtime,
        options: TranscribeOptions {
            trace_id: Some("segmented-example".into()),
            ..Default::default()
        },
        compatibility: CompatibilityPolicy::Coerce,
        provider_options: serde_json::Value::Null,
    };

    let mut stream = gateway.start_stream(request).await.unwrap();

    stream.events.next().await; // RouteSelected
    stream.events.next().await; // Started

    // Segment 1: 1 second of silence (simulating PTT press)
    // Replace with real microphone input for production use.
    let silence_1s = vec![0u8; 32000]; // 16kHz × 16-bit × 1s
    stream
        .input
        .send_audio(Bytes::from(silence_1s), None)
        .await
        .unwrap();

    let seg1 = stream.flush_and_wait_final().await.unwrap();
    println!("Segment 1: {:?}", seg1.result.text);

    // Segment 2: 2 seconds of silence
    let silence_2s = vec![0u8; 64000];
    stream
        .input
        .send_audio(Bytes::from(silence_2s), None)
        .await
        .unwrap();

    let seg2 = stream.flush_and_wait_final().await.unwrap();
    println!("Segment 2: {:?}", seg2.result.text);

    // Segment 3: final segment
    let silence_1s = vec![0u8; 32000];
    stream
        .input
        .send_audio(Bytes::from(silence_1s), None)
        .await
        .unwrap();

    let seg3 = stream.end_and_wait_final().await.unwrap();
    println!("Segment 3: {:?}", seg3.result.text);
    println!("Latency: {}ms", seg3.result.telemetry.latency_final_ms);
}
