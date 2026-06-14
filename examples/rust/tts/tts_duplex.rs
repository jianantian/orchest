/// Duplex streaming TTS: send text incrementally while receiving audio in parallel.
///
/// Models a real-time agent scenario where text is produced by an LLM
/// sentence by sentence. Audio starts arriving before all text is sent.
///
/// Usage:
///   VOLCENGINE_API_KEY=<key> cargo run --example tts_duplex
use std::time::Instant;

use agent_runtime_tts_providers::{
    create_tts_provider_from_config, AudioOutputConfig, CompatibilityPolicy,
    DuplexSynthesizeRequest, SpeechControls, TtsProviderRuntimeConfig, TtsStreamEvent,
    VoiceSelection,
};

const SENTENCES: &[&str] = &[
    "你好，我是你的 AI 助手。",
    "今天有什么我可以帮到你的吗？",
    "我可以回答问题、帮你写作，或者陪你聊天。",
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = create_tts_provider_from_config(TtsProviderRuntimeConfig {
        model: "volcengine/seed-tts-1.0".to_owned(),
        api_key: None,
        api_key_env: Some("VOLCENGINE_API_KEY".to_owned()),
        api_url: None,
        region: None,
        timeout: Some(std::time::Duration::from_secs(30)),
        provider_options: serde_json::Value::Null,
    })?;

    let mut stream = provider
        .start_duplex_stream(DuplexSynthesizeRequest {
            model: None,
            voice: VoiceSelection::by_id("zh_female_wanwanxiaohe_moon_bigtts"),
            output: AudioOutputConfig::mp3(),
            controls: SpeechControls::default(),
            compatibility: CompatibilityPolicy::Strict,
            trace_id: Some("tts-duplex-example".to_owned()),
            provider_options: serde_json::Value::Null,
        })
        .await?;

    let started = Instant::now();

    // Producer: simulate LLM output arriving sentence by sentence.
    let input = stream.input.clone();
    let producer = tokio::spawn(async move {
        for (i, sentence) in SENTENCES.iter().enumerate() {
            // Simulate LLM thinking time between sentences.
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            println!(
                "[{:>5}ms] sending sentence {}: {sentence:?}",
                started.elapsed().as_millis(),
                i + 1,
            );
            input.send_text(*sentence).await.unwrap();
        }
        input.finish().await.unwrap();
        println!("[{:>5}ms] all text sent", started.elapsed().as_millis());
    });

    // Consumer: collect audio chunks and print events.
    let mut all_audio: Vec<u8> = Vec::new();
    loop {
        match stream.events.next().await.unwrap() {
            TtsStreamEvent::Started { provider, model, .. } => {
                println!(
                    "[{:>5}ms] started  provider={provider} model={model}",
                    started.elapsed().as_millis()
                );
            }
            TtsStreamEvent::TextAccepted { chars, .. } => {
                println!("[{:>5}ms] text accepted  chars={chars}", started.elapsed().as_millis());
            }
            TtsStreamEvent::AudioChunk { data, sequence, .. } => {
                println!(
                    "[{:>5}ms] chunk #{sequence:02}  {} bytes",
                    started.elapsed().as_millis(),
                    data.len()
                );
                all_audio.extend_from_slice(&data);
            }
            TtsStreamEvent::Completed { summary, .. } => {
                println!(
                    "[{:>5}ms] completed  total_bytes={}  input_chars={}  first_audio={}ms",
                    started.elapsed().as_millis(),
                    all_audio.len(),
                    summary.usage.input_chars,
                    summary.telemetry.first_audio_latency_ms.unwrap_or_default(),
                );
                break;
            }
            TtsStreamEvent::Error { error, fatal, .. } => {
                eprintln!("stream error (fatal={fatal}): {error}");
                if fatal {
                    return Err(error.into());
                }
            }
            _ => {}
        }
    }

    producer.await?;

    let path = "/tmp/tts_duplex_output.mp3";
    std::fs::write(path, &all_audio)?;
    println!("wrote {path}");
    Ok(())
}
