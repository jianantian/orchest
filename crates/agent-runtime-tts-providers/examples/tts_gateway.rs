use agent_runtime_tts_providers::{
    create_tts_provider_from_config, AudioFormat, AudioOutputConfig, CompatibilityPolicy, Language,
    ListVoicesRequest, SpeechControls, StreamSynthesizeRequest, SynthesizeRequest, TtsGateway,
    TtsGatewayConfig, TtsInput, TtsProviderRuntimeConfig, TtsRoute, TtsRouter, VoiceKind,
    VoiceSelection,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = create_tts_provider_from_config(TtsProviderRuntimeConfig {
        model: "volcengine/seed-tts-2.0".to_owned(),
        api_key: Some("example-key".to_owned()),
        api_key_env: None,
        api_url: None,
        region: None,
        timeout: Some(std::time::Duration::from_secs(5)),
        provider_options: serde_json::Value::Null,
    })?;

    let mut router = TtsRouter::new();
    router.register_provider("volcengine/seed-tts-2.0".to_owned(), provider);
    router.set_routes(vec![TtsRoute {
        model: "volcengine/seed-tts-2.0".to_owned(),
        languages: vec![Language::new("zh-CN"), Language::new("en-US")],
        voice_kinds: vec![VoiceKind::System],
        output_formats: vec![AudioFormat::Mp3],
        max_latency_ms: Some(300),
        max_cost_micros_per_char: None,
        priority: 0,
    }]);

    let gateway = TtsGateway::new(router, TtsGatewayConfig::default());
    let voices = gateway
        .list_voices(ListVoicesRequest {
            model: None,
            language: Some(Language::new("zh-CN")),
            kind: Some(VoiceKind::System),
            include_custom: false,
            trace_id: None,
        })
        .await?;

    let result = gateway
        .synthesize(SynthesizeRequest {
            model: None,
            input: TtsInput::Text("你好".to_owned()),
            voice: VoiceSelection::by_id(&voices[0].id)
                .with_kind(VoiceKind::System)
                .with_language(Language::new("zh-CN")),
            output: AudioOutputConfig::new(AudioFormat::Mp3),
            controls: SpeechControls::default(),
            compatibility: CompatibilityPolicy::Strict,
            trace_id: Some("example-tts".to_owned()),
            provider_options: serde_json::Value::Null,
        })
        .await?;

    println!(
        "synthesized {} bytes with {}",
        result.usage.output_bytes.unwrap_or_default(),
        result.telemetry.provider
    );

    stream_once(&gateway, &voices[0].id).await?;
    duplex_once(&gateway, &voices[0].id).await?;
    Ok(())
}

async fn stream_once(
    gateway: &TtsGateway,
    voice_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = gateway
        .stream_synthesize(StreamSynthesizeRequest {
            model: Some("volcengine/seed-tts-2.0".to_owned()),
            input: TtsInput::Text("streaming text".to_owned()),
            voice: VoiceSelection::by_id(voice_id),
            output: AudioOutputConfig::new(AudioFormat::Mp3),
            controls: SpeechControls::default(),
            compatibility: CompatibilityPolicy::Strict,
            trace_id: Some("example-tts-stream".to_owned()),
            provider_options: serde_json::Value::Null,
        })
        .await?;

    let terminal = stream.events.collect_until_terminal().await?;
    println!("single-stream terminal event: {terminal:?}");
    Ok(())
}

async fn duplex_once(
    gateway: &TtsGateway,
    voice_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = gateway
        .start_duplex_stream(agent_runtime_tts_providers::DuplexSynthesizeRequest {
            model: Some("volcengine/seed-tts-2.0".to_owned()),
            voice: VoiceSelection::by_id(voice_id),
            output: AudioOutputConfig::new(AudioFormat::Mp3),
            controls: SpeechControls::default(),
            compatibility: CompatibilityPolicy::Strict,
            trace_id: Some("example-tts-duplex".to_owned()),
            provider_options: serde_json::Value::Null,
        })
        .await?;

    stream.input.send_text("first chunk").await?;
    stream.input.send_text("second chunk").await?;
    stream.input.finish().await?;
    let terminal = stream.events.collect_until_terminal().await?;
    println!("duplex terminal event: {terminal:?}");
    Ok(())
}
