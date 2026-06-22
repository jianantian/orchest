#![cfg(feature = "speechmatics")]

use agent_runtime_asr_providers::providers::speechmatics::{
    SpeechmaticsAsrAdapter, SpeechmaticsAsrConfig,
};
use agent_runtime_asr_providers::traits::AsrProvider;
use agent_runtime_asr_providers::types::*;

#[tokio::test]
#[ignore = "requires SPEECHMATICS_API_KEY and SPEECHMATICS_ASR_AUDIO_FILE; costs real provider usage"]
async fn live_speechmatics_transcribe_file() {
    let api_key = std::env::var("SPEECHMATICS_API_KEY").expect("SPEECHMATICS_API_KEY must be set");
    let audio_file = std::env::var("SPEECHMATICS_ASR_AUDIO_FILE")
        .expect("SPEECHMATICS_ASR_AUDIO_FILE must be set");
    let model = std::env::var("SPEECHMATICS_ASR_MODEL").unwrap_or_else(|_| "enhanced".into());
    let api_url = std::env::var("SPEECHMATICS_ASR_API_URL")
        .unwrap_or_else(|_| "https://asr.api.speechmatics.com/v2".into());

    let adapter = SpeechmaticsAsrAdapter::new(SpeechmaticsAsrConfig {
        model,
        api_key,
        api_url,
        poll_interval: std::time::Duration::from_secs(2),
        max_polls: 90,
    });

    let result = adapter
        .transcribe(TranscribeRequest {
            model: Some("speechmatics/enhanced".into()),
            audio: AudioInput::File {
                path: audio_file.into(),
                format: Some(AudioFormat::Wav),
            },
            options: TranscribeOptions {
                language: Some(Language::new("auto")),
                word_timestamps: true,
                speaker_diarization: true,
                ..Default::default()
            },
            timeout: Some(std::time::Duration::from_secs(180)),
            compatibility: CompatibilityPolicy::Coerce,
            provider_options: serde_json::json!({"operating_point": "enhanced"}),
        })
        .await
        .expect("Speechmatics transcription should succeed");

    assert!(!result.text.trim().is_empty());
}
