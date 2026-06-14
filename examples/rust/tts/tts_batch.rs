/// Batch TTS: synthesize a phrase and write the audio to disk.
///
/// Usage:
///   VOLCENGINE_API_KEY=<key> cargo run --example tts_batch
use agent_runtime_tts_providers::{
    create_tts_provider_from_config, AudioOutputConfig, CompatibilityPolicy, SpeechControls,
    SynthesizeRequest, TtsInput, TtsProviderRuntimeConfig, VoiceSelection,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = create_tts_provider_from_config(TtsProviderRuntimeConfig {
        model: "volcengine/seed-tts-1.0".to_owned(),
        api_key: None,
        api_key_env: Some("VOLCENGINE_API_KEY".to_owned()),
        api_url: None,
        region: None,
        timeout: Some(std::time::Duration::from_secs(15)),
        provider_options: serde_json::Value::Null,
    })?;

    let result = provider
        .synthesize(SynthesizeRequest {
            model: None,
            input: TtsInput::text("今天天气真不错，我们出去走走吧。"),
            voice: VoiceSelection::by_id("zh_female_wanwanxiaohe_moon_bigtts"),
            output: AudioOutputConfig::mp3(),
            controls: SpeechControls::default(),
            compatibility: CompatibilityPolicy::Strict,
            trace_id: Some("tts-batch-example".to_owned()),
            provider_options: serde_json::Value::Null,
        })
        .await?;

    let path = "/tmp/tts_batch_output.mp3";
    match &result.audio {
        agent_runtime_tts_providers::AudioData::Bytes(bytes) => {
            std::fs::write(path, bytes)?;
            println!(
                "wrote {} bytes to {path}  ({}ms, {} input chars)",
                bytes.len(),
                result.duration_ms.unwrap_or_default(),
                result.usage.input_chars,
            );
        }
        agent_runtime_tts_providers::AudioData::Url { url, .. } => {
            println!("audio url: {url}");
        }
    }

    println!(
        "provider={} model={} latency={}ms",
        result.telemetry.provider,
        result.telemetry.model,
        result.telemetry.final_latency_ms.unwrap_or_default(),
    );
    Ok(())
}
