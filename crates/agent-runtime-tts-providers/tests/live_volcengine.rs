use std::env;

use agent_runtime_tts_providers::{
    create_tts_provider_from_config, AudioFormat, AudioOutputConfig, CompatibilityPolicy,
    SpeechControls, SynthesizeRequest, TtsInput, TtsProviderRuntimeConfig, VoiceSelection,
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
