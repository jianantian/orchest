use std::sync::Arc;
use std::time::Duration;

use agent_runtime_asr_providers::config::{
    create_asr_provider_from_config, AsrProviderRuntimeConfig,
};
use agent_runtime_asr_providers::routing::{AsrGateway, AsrGatewayConfig, AsrRouter};
use agent_runtime_asr_providers::types::*;

#[tokio::main]
async fn main() {
    let model = std::env::var("ASR_MODEL").unwrap_or_else(|_| "assemblyai/universal".into());
    let provider = create_asr_provider_from_config(AsrProviderRuntimeConfig {
        model: model.clone(),
        api_key: std::env::var("ASR_API_KEY").ok(),
        api_key_env: std::env::var("ASR_API_KEY_ENV").ok(),
        api_url: std::env::var("ASR_API_URL").ok(),
        region: None,
        timeout: Some(Duration::from_secs(180)),
        provider_options: serde_json::Value::Null,
    })
    .expect("provider feature must be enabled and credentials must be configured");

    let mut router = AsrRouter::new();
    router.register_provider(model.clone(), Arc::clone(&provider));
    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let request = TranscribeRequest {
        model: Some(model),
        audio: audio_from_env().await,
        options: TranscribeOptions {
            language: std::env::var("ASR_LANGUAGE").ok().map(Language::new),
            word_timestamps: env_bool("ASR_WORD_TIMESTAMPS"),
            speaker_diarization: env_bool("ASR_SPEAKER_DIARIZATION"),
            trace_id: Some("asr-transcribe-example".into()),
            ..Default::default()
        },
        timeout: Some(Duration::from_secs(180)),
        compatibility: CompatibilityPolicy::Coerce,
        provider_options: provider_options_from_env(),
    };

    match gateway.transcribe(request).await {
        Ok(result) => {
            println!("text: {}", result.text);
            println!("language: {:?}", result.language);
            println!("words: {}", result.words.len());
            println!("speakers: {}", result.speakers.len());
        }
        Err(error)
            if error.code == agent_runtime_asr_providers::AsrErrorCode::UnsupportedOperation =>
        {
            println!("selected provider/model does not support one-shot transcribe(): {error}");
        }
        Err(error) => panic!("transcribe failed: {error}"),
    }
}

async fn audio_from_env() -> AudioInput {
    let format = std::env::var("ASR_AUDIO_FORMAT")
        .ok()
        .as_deref()
        .map(parse_audio_format)
        .unwrap_or(AudioFormat::Wav);
    if let Ok(path) = std::env::var("ASR_AUDIO_BYTES_FILE") {
        return AudioInput::Bytes {
            data: tokio::fs::read(path)
                .await
                .expect("ASR_AUDIO_BYTES_FILE must be readable"),
            format,
            sample_rate_hz: std::env::var("ASR_SAMPLE_RATE_HZ")
                .ok()
                .and_then(|v| v.parse().ok()),
        };
    }
    if let Ok(path) = std::env::var("ASR_AUDIO_FILE") {
        return AudioInput::File {
            path: path.into(),
            format: Some(format),
        };
    }
    AudioInput::Url {
        url: std::env::var("ASR_AUDIO_URL")
            .expect("set one of ASR_AUDIO_URL, ASR_AUDIO_FILE, or ASR_AUDIO_BYTES_FILE"),
        format: Some(format),
    }
}

fn parse_audio_format(value: &str) -> AudioFormat {
    match value {
        "pcm" => AudioFormat::Pcm,
        "wav" => AudioFormat::Wav,
        "opus" => AudioFormat::Opus,
        "mp3" => AudioFormat::Mp3,
        "ogg" => AudioFormat::Ogg,
        "flac" => AudioFormat::Flac,
        other => panic!("unsupported ASR_AUDIO_FORMAT: {other}"),
    }
}

fn provider_options_from_env() -> serde_json::Value {
    std::env::var("ASR_PROVIDER_OPTIONS_JSON")
        .ok()
        .map(|raw| serde_json::from_str(&raw).expect("ASR_PROVIDER_OPTIONS_JSON must be JSON"))
        .unwrap_or(serde_json::Value::Null)
}

fn env_bool(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}
