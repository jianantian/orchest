#![cfg(feature = "deepgram")]

use std::env;

use agent_runtime_asr_providers::providers::deepgram::{DeepgramAsrAdapter, DeepgramAsrConfig};
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

fn get_config() -> Option<DeepgramAsrConfig> {
    load_dotenv_if_present();
    let api_key = std::env::var("DEEPGRAM_API_KEY").ok()?;
    Some(DeepgramAsrConfig {
        model: std::env::var("DEEPGRAM_ASR_MODEL").unwrap_or_else(|_| "nova-3".into()),
        ws_url: std::env::var("DEEPGRAM_ASR_WS_URL")
            .unwrap_or_else(|_| "wss://api.deepgram.com/v1/listen".into()),
        api_key,
    })
}

#[ignore]
#[tokio::test]
async fn live_deepgram_streaming_silence() {
    let config = match get_config() {
        Some(c) => c,
        None => {
            eprintln!("skipping: DEEPGRAM_API_KEY not set");
            return;
        }
    };

    let model = config.model.clone();
    let adapter = DeepgramAsrAdapter::new(config);
    let request = StreamingTranscribeRequest {
        model: Some(format!("deepgram/{model}")),
        format: StreamingAudioFormat::Pcm16 {
            sample_rate_hz: 16000,
            channels: 1,
        },
        timeline: AudioTimelineMode::ContinuousRealtime,
        options: TranscribeOptions {
            trace_id: Some("live-test-deepgram-silence".into()),
            language: Some(Language::new("en")),
            word_timestamps: true,
            ..Default::default()
        },
        compatibility: CompatibilityPolicy::Coerce,
        provider_options: serde_json::json!({"smart_format": true}),
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
    assert_eq!(final_output.trace_id, "live-test-deepgram-silence");
    assert_eq!(
        final_output.result.telemetry.model,
        format!("deepgram/{model}")
    );
    assert!(final_output.result.telemetry.latency_final_ms > 0);
}
