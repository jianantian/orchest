use agent_runtime_tts_providers::{
    create_tts_provider_from_config, AudioFormat, AudioOutputConfig, CompatibilityPolicy,
    SpeechControls, SynthesizeRequest, TtsInput, TtsProviderRuntimeConfig, VoiceSelection,
};

#[tokio::test]
#[ignore = "requires ALIYUN_TTS_API_KEY and real provider access"]
async fn live_aliyun_tiny_synthesis() {
    let provider = create_tts_provider_from_config(TtsProviderRuntimeConfig {
        model: "aliyun/cosyvoice-v3.5-flash".to_owned(),
        api_key: None,
        api_key_env: Some("ALIYUN_TTS_API_KEY".to_owned()),
        api_url: None,
        region: None,
        timeout: Some(std::time::Duration::from_secs(10)),
        provider_options: serde_json::Value::Null,
    })
    .unwrap();
    let result = provider
        .synthesize(SynthesizeRequest {
            model: Some("aliyun/cosyvoice-v3.5-flash".to_owned()),
            input: TtsInput::Text("你好世界".to_owned()),
            voice: VoiceSelection::by_id("longxiaochun"),
            output: AudioOutputConfig::new(AudioFormat::Mp3),
            controls: SpeechControls::default(),
            compatibility: CompatibilityPolicy::Strict,
            trace_id: Some("live-aliyun".to_owned()),
            provider_options: serde_json::Value::Null,
        })
        .await
        .unwrap();
    assert_eq!(result.telemetry.provider, "aliyun");
    assert!(result.usage.output_bytes.unwrap_or_default() > 0);
}
