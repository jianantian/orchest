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
            trace_id: Some("full-duplex-example".into()),
            ..Default::default()
        },
        compatibility: CompatibilityPolicy::Coerce,
        provider_options: serde_json::Value::Null,
    };

    let stream = gateway.start_stream(request).await.unwrap();
    let (sink, mut events) = stream.split();

    // Producer task: send audio chunks.
    // Replace with real microphone input for production use.
    let producer = tokio::spawn(async move {
        let chunk_duration_ms = 100;
        let chunk_bytes = 16000 * 2 * chunk_duration_ms / 1000; // 16kHz × 16-bit × 100ms
        let silence = vec![0u8; chunk_bytes];

        for i in 0..30 {
            sink.send_audio(
                Bytes::from(silence.clone()),
                Some(i * chunk_duration_ms as u64),
            )
            .await
            .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(chunk_duration_ms as u64)).await;
        }

        sink.end_stream().await.unwrap();
    });

    // Consumer task: print events as they arrive.
    let consumer = tokio::spawn(async move {
        while let Some(event) = events.next().await {
            match &event {
                AsrStreamEvent::RouteSelected { model, .. } => {
                    println!("[route] {model}");
                }
                AsrStreamEvent::Started { trace_id, .. } => {
                    println!("[started] trace_id={trace_id}");
                }
                AsrStreamEvent::TranscriptUpdate {
                    text, stability, ..
                } => {
                    println!("[{stability:?}] {text}");
                }
                AsrStreamEvent::EndOfSpeech { .. } => {
                    println!("[end-of-speech]");
                }
                AsrStreamEvent::AsrFinal { final_output } => {
                    println!(
                        "[final] text={:?} latency={}ms",
                        final_output.result.text, final_output.result.telemetry.latency_final_ms
                    );
                    break;
                }
                AsrStreamEvent::Error { error, fatal, .. } => {
                    println!("[error] fatal={fatal} {error}");
                    if *fatal {
                        break;
                    }
                }
            }
        }
    });

    let _ = tokio::join!(producer, consumer);
}
