use agent_runtime_tts_providers::{
    create_tts_provider_from_config, AudioFormat, AudioOutputConfig, CompatibilityPolicy,
    SpeechControls, SynthesizeRequest, TtsInput, TtsProviderRuntimeConfig, VoiceSelection,
};

#[tokio::test]
#[ignore = "requires VOLCENGINE_TTS_API_KEY and real provider access"]
async fn live_volcengine_tiny_synthesis() {
    let provider = create_tts_provider_from_config(TtsProviderRuntimeConfig {
        model: "volcengine/seed-tts-2.0".to_owned(),
        api_key: None,
        api_key_env: Some("VOLCENGINE_TTS_API_KEY".to_owned()),
        api_url: None,
        region: None,
        timeout: Some(std::time::Duration::from_secs(10)),
        provider_options: serde_json::Value::Null,
    })
    .unwrap();
    let result = provider
        .synthesize(SynthesizeRequest {
            model: Some("volcengine/seed-tts-2.0".to_owned()),
            input: TtsInput::Text("hi".to_owned()),
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
