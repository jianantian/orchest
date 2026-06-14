use std::env;

use agent_runtime_tts_providers::{
    create_tts_provider_from_config, AudioFormat, AudioOutputConfig, CompatibilityPolicy,
    SpeechControls, SynthesizeRequest, TtsInput, TtsOperation, TtsProviderRuntimeConfig,
    TtsStreamEvent, VoiceSelection,
};

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

#[tokio::test]
#[ignore = "requires VOLCENGINE_API_KEY and real provider access"]
async fn live_volcengine_tiny_synthesis() {
    load_dotenv_if_present();
    let provider = create_tts_provider_from_config(TtsProviderRuntimeConfig {
        model: "volcengine/seed-tts-1.0".to_owned(),
        api_key: None,
        api_key_env: Some("VOLCENGINE_API_KEY".to_owned()),
        api_url: None,
        region: None,
        timeout: Some(std::time::Duration::from_secs(10)),
        provider_options: serde_json::Value::Null,
    })
    .unwrap();
    let result = provider
        .synthesize(SynthesizeRequest {
            model: Some("volcengine/seed-tts-1.0".to_owned()),
            input: TtsInput::Text("你好世界".to_owned()),
            voice: VoiceSelection::by_id("zh_female_wanwanxiaohe_moon_bigtts"),
            output: AudioOutputConfig::new(AudioFormat::Mp3),
            controls: SpeechControls::default(),
            compatibility: CompatibilityPolicy::Strict,
            trace_id: Some("live-volcengine".to_owned()),
            provider_options: serde_json::Value::Null,
        })
        .await
        .unwrap();
    assert_eq!(result.telemetry.provider, "volcengine");
    assert!(result.usage.output_bytes.unwrap_or_default() > 0);
}

#[tokio::test]
#[ignore = "requires VOLCENGINE_API_KEY and real provider access"]
async fn live_volcengine_unidirectional_stream() {
    load_dotenv_if_present();
    let provider = create_tts_provider_from_config(TtsProviderRuntimeConfig {
        model: "volcengine/seed-tts-1.0".to_owned(),
        api_key: None,
        api_key_env: Some("VOLCENGINE_API_KEY".to_owned()),
        api_url: None,
        region: None,
        timeout: Some(std::time::Duration::from_secs(15)),
        provider_options: serde_json::Value::Null,
    })
    .unwrap();
    let mut stream = provider
        .stream_synthesize(SynthesizeRequest {
            model: Some("volcengine/seed-tts-1.0".to_owned()),
            input: TtsInput::Text("你好世界".to_owned()),
            voice: VoiceSelection::by_id("zh_female_wanwanxiaohe_moon_bigtts"),
            output: AudioOutputConfig::new(AudioFormat::Mp3),
            controls: SpeechControls::default(),
            compatibility: CompatibilityPolicy::Strict,
            trace_id: Some("live-volcengine-stream".to_owned()),
            provider_options: serde_json::Value::Null,
        })
        .await
        .unwrap();

    let mut saw_started = false;
    let mut audio_chunks = 0u64;
    let mut total_bytes = 0usize;
    loop {
        match stream.events.next().await.unwrap() {
            TtsStreamEvent::Started { provider, .. } => {
                assert_eq!(provider, "volcengine");
                saw_started = true;
            }
            TtsStreamEvent::AudioChunk { data, .. } => {
                total_bytes += data.len();
                audio_chunks += 1;
            }
            TtsStreamEvent::Completed { summary, .. } => {
                assert_eq!(summary.telemetry.operation, TtsOperation::SingleStream);
                break;
            }
            TtsStreamEvent::Error { error, .. } => panic!("stream error: {error:?}"),
            _ => {}
        }
    }
    assert!(saw_started);
    assert!(audio_chunks > 0, "expected at least one audio chunk");
    assert!(total_bytes > 0, "expected non-empty audio");
}
