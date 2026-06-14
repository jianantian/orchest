/// Unidirectional streaming TTS: receive audio chunks as they are generated.
///
/// Prints each chunk's arrival time relative to the stream start, showing
/// the first-chunk latency and throughput characteristics.
///
/// Usage:
///   VOLCENGINE_API_KEY=<key> cargo run --example tts_stream
use std::time::Instant;

use agent_runtime_tts_providers::{
    create_tts_provider_from_config, AudioOutputConfig, CompatibilityPolicy, SpeechControls,
    SynthesizeRequest, TtsInput, TtsProviderRuntimeConfig, TtsStreamEvent, VoiceSelection,
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

    let mut stream = provider
        .stream_synthesize(SynthesizeRequest {
            model: None,
            input: TtsInput::text("流式合成示例：音频块在生成的同时逐步返回，首包延迟更低。"),
            voice: VoiceSelection::by_id("zh_female_wanwanxiaohe_moon_bigtts"),
            output: AudioOutputConfig::mp3(),
            controls: SpeechControls::default(),
            compatibility: CompatibilityPolicy::Strict,
            trace_id: Some("tts-stream-example".to_owned()),
            provider_options: serde_json::Value::Null,
        })
        .await?;

    let started = Instant::now();
    let mut all_audio: Vec<u8> = Vec::new();

    loop {
        match stream.events.next().await.unwrap() {
            TtsStreamEvent::Started { provider, model, .. } => {
                println!("[{:>5}ms] started  provider={provider} model={model}", started.elapsed().as_millis());
            }
            TtsStreamEvent::AudioChunk { data, sequence, .. } => {
                let elapsed = started.elapsed().as_millis();
                println!("[{elapsed:>5}ms] chunk #{sequence:02}  {} bytes", data.len());
                all_audio.extend_from_slice(&data);
            }
            TtsStreamEvent::Completed { summary, .. } => {
                println!(
                    "[{:>5}ms] completed  chunks={} total_bytes={} first_audio={}ms",
                    started.elapsed().as_millis(),
                    all_audio.len(),  // approximate
                    all_audio.len(),
                    summary.telemetry.first_audio_latency_ms.unwrap_or_default(),
                );
                break;
            }
            TtsStreamEvent::Error { error, .. } => {
                eprintln!("stream error: {error}");
                return Err(error.into());
            }
            _ => {}
        }
    }

    let path = "/tmp/tts_stream_output.mp3";
    std::fs::write(path, &all_audio)?;
    println!("wrote {path}");
    Ok(())
}
