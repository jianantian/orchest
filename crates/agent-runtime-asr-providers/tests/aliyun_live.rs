#![cfg(feature = "aliyun")]

use std::env;

use agent_runtime_asr_providers::providers::aliyun::{AliyunAsrAdapter, AliyunAsrConfig};
use agent_runtime_asr_providers::traits::AsrProvider;
use agent_runtime_asr_providers::types::*;
use bytes::Bytes;

fn load_dotenv_if_present() {
    let mut path = std::env::current_dir().ok();
    while let Some(dir) = path {
        if let Ok(contents) = std::fs::read_to_string(dir.join(".env")) {
            for line in contents.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    let key = key
                        .trim()
                        .strip_prefix("export ")
                        .unwrap_or(key.trim())
                        .trim();
                    if env::var_os(key).is_none() {
                        env::set_var(key, value.trim().trim_matches('"'));
                    }
                }
            }
            break;
        }
        path = dir.parent().map(|p| p.to_path_buf());
    }
}

fn get_config() -> Option<AliyunAsrConfig> {
    load_dotenv_if_present();
    let api_key = std::env::var("DASHSCOPE_API_KEY")
        .or_else(|_| std::env::var("ALIYUN_ASR_API_KEY"))
        .ok()?;
    Some(AliyunAsrConfig::fun_asr_realtime(api_key))
}

#[ignore]
#[tokio::test]
async fn live_aliyun_streaming_silence() {
    let config = match get_config() {
        Some(c) => c,
        None => {
            eprintln!("skipping: DASHSCOPE_API_KEY not set");
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
