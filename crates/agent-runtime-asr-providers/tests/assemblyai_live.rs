#![cfg(feature = "assemblyai")]

use agent_runtime_asr_providers::providers::assemblyai::{
    AssemblyAiAsrAdapter, AssemblyAiAsrConfig,
};
use agent_runtime_asr_providers::traits::AsrProvider;
use agent_runtime_asr_providers::types::*;

#[tokio::test]
#[ignore = "requires ASSEMBLYAI_API_KEY and ASSEMBLYAI_ASR_AUDIO_URL; costs real provider usage"]
async fn live_assemblyai_transcribe_url() {
    let api_key = std::env::var("ASSEMBLYAI_API_KEY").expect("ASSEMBLYAI_API_KEY must be set");
    let audio_url =
        std::env::var("ASSEMBLYAI_ASR_AUDIO_URL").expect("ASSEMBLYAI_ASR_AUDIO_URL must be set");
    let model = std::env::var("ASSEMBLYAI_ASR_MODEL").unwrap_or_else(|_| "universal".into());
    let api_url = std::env::var("ASSEMBLYAI_ASR_API_URL")
        .unwrap_or_else(|_| "https://api.assemblyai.com/v2".into());
    let upload_url = std::env::var("ASSEMBLYAI_ASR_UPLOAD_URL")
        .unwrap_or_else(|_| "https://api.assemblyai.com/v2/upload".into());

    let adapter = AssemblyAiAsrAdapter::new(AssemblyAiAsrConfig {
        model,
        api_key,
        api_url,
        upload_url,
        poll_interval: std::time::Duration::from_secs(2),
        max_polls: 90,
    });

    let result = adapter
        .transcribe(TranscribeRequest {
            model: Some("assemblyai/universal".into()),
            audio: AudioInput::Url {
                url: audio_url,
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
            provider_options: serde_json::json!({"language_detection": true}),
        })
        .await
        .expect("AssemblyAI transcription should succeed");

    assert!(!result.text.trim().is_empty());
}
