#![cfg(any(feature = "aliyun", feature = "volcengine"))]

use agent_runtime_tts_providers::{
    create_tts_provider_from_config, AudioFormat, AudioOutputConfig, CompatibilityPolicy,
    ListVoicesRequest, SpeechControls, SynthesizeRequest, TtsErrorCode, TtsInput,
    TtsProviderRuntimeConfig, VoiceSelection,
};

fn config(model: &str) -> TtsProviderRuntimeConfig {
    TtsProviderRuntimeConfig {
        model: model.to_owned(),
        api_key: Some("test-key".to_owned()),
        api_key_env: None,
        api_url: Some("https://example.test".to_owned()),
        region: Some("test-region".to_owned()),
        timeout: Some(std::time::Duration::from_secs(3)),
        provider_options: serde_json::json!({"fixture": "factory-only"}),
    }
}

fn request(model: &str) -> SynthesizeRequest {
    SynthesizeRequest {
        model: Some(model.to_owned()),
        input: TtsInput::Text("hi".to_owned()),
        voice: VoiceSelection::by_id("voice"),
        output: AudioOutputConfig::new(AudioFormat::Mp3),
        controls: SpeechControls::default(),
        compatibility: CompatibilityPolicy::Strict,
        trace_id: Some("adapter-trace".to_owned()),
        provider_options: serde_json::Value::Null,
    }
}

#[tokio::test]
async fn volcengine_factory_constructs_seed_tts_provider() {
    let provider = create_tts_provider_from_config(config("volcengine/seed-tts-2.0")).unwrap();
    assert_eq!(provider.provider_name(), "volcengine");
    assert_eq!(provider.model_name(), "seed-tts-2.0");
    assert!(provider.capabilities().duplex_streaming);
}

#[tokio::test]
async fn aliyun_factory_constructs_supported_models() {
    for model in [
        "aliyun/cosyvoice-v3-flash",
        "aliyun/qwen3-tts-flash-realtime",
        "aliyun/qwen3-tts-instruct-flash-realtime",
    ] {
        let provider = create_tts_provider_from_config(config(model)).unwrap();
        assert_eq!(provider.provider_name(), "aliyun");
    }
}

#[test]
fn explicit_api_key_env_missing_does_not_fallback() {
    let mut runtime = config("volcengine/seed-tts-2.0");
    runtime.api_key = None;
    runtime.api_key_env = Some("ORCHEST_TTS_TEST_MISSING_KEY".to_owned());
    std::env::remove_var("ORCHEST_TTS_TEST_MISSING_KEY");
    let err = match create_tts_provider_from_config(runtime) {
        Ok(_) => panic!("expected missing api key error"),
        Err(err) => err,
    };
    assert_eq!(err.code, TtsErrorCode::MissingApiKey);
}

#[tokio::test]
async fn non_instruct_aliyun_model_rejects_instruction_in_strict_mode() {
    let provider =
        create_tts_provider_from_config(config("aliyun/qwen3-tts-flash-realtime")).unwrap();
    let mut request = request("aliyun/qwen3-tts-flash-realtime");
    request.controls.instruction = Some("speak warmly".to_owned());
    let err = provider.synthesize(request).await.unwrap_err();
    assert_eq!(err.code, TtsErrorCode::UnsupportedOption);
}

#[tokio::test]
async fn list_voices_returns_static_catalog_without_live_call() {
    let provider = create_tts_provider_from_config(config("aliyun/cosyvoice-v3-flash")).unwrap();
    let voices = provider
        .list_voices(ListVoicesRequest {
            model: Some("aliyun/cosyvoice-v3-flash".to_owned()),
            language: None,
            kind: None,
            include_custom: false,
            trace_id: None,
        })
        .await
        .unwrap();
    assert!(!voices.is_empty());
    assert!(voices.iter().all(|voice| voice.provider == "aliyun"));
}
