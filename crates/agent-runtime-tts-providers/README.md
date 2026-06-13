# agent-runtime-tts-providers

Provider-neutral text-to-speech gateway for Orchest applications.

This crate is independent from `agent-runtime-core`: it provides typed TTS
requests, provider routing, voice catalogs, streaming contracts, telemetry, and
feature-gated provider adapters. TTS is an output rendering layer, not a core
runtime tool or skill.

## Providers

Default features include:

- `volcengine`: `volcengine/seed-tts-2.0`
- `aliyun`: `aliyun/cosyvoice-v3-flash`,
  `aliyun/qwen3-tts-flash-realtime`,
  `aliyun/qwen3-tts-instruct-flash-realtime`

Provider-specific WebSocket dependencies are optional feature dependencies.

## Gateway Setup

```rust
use agent_runtime_tts_providers::{
    create_tts_provider_from_config, AudioFormat, Language, TtsGateway,
    TtsGatewayConfig, TtsProviderRuntimeConfig, TtsRoute, TtsRouter, VoiceKind,
};

let provider = create_tts_provider_from_config(TtsProviderRuntimeConfig {
    model: "volcengine/seed-tts-2.0".to_owned(),
    api_key: None,
    api_key_env: Some("VOLCENGINE_TTS_API_KEY".to_owned()),
    api_url: None,
    region: None,
    timeout: None,
    provider_options: serde_json::Value::Null,
})?;

let mut router = TtsRouter::new();
router.register_provider("volcengine/seed-tts-2.0".to_owned(), provider);
router.set_routes(vec![TtsRoute {
    model: "volcengine/seed-tts-2.0".to_owned(),
    languages: vec![Language::new("zh-CN")],
    voice_kinds: vec![VoiceKind::System],
    output_formats: vec![AudioFormat::Mp3],
    max_latency_ms: Some(300),
    max_cost_micros_per_char: None,
    priority: 0,
}]);

let gateway = TtsGateway::new(router, TtsGatewayConfig::default());
```

## Voice Listing

```rust
use agent_runtime_tts_providers::{Language, ListVoicesRequest, VoiceKind};

let voices = gateway
    .list_voices(ListVoicesRequest {
        model: None,
        language: Some(Language::new("zh-CN")),
        kind: Some(VoiceKind::System),
        include_custom: false,
        trace_id: Some("voice-list".to_owned()),
    })
    .await?;
```

## Batch Synthesis

```rust
use agent_runtime_tts_providers::{
    AudioFormat, AudioOutputConfig, CompatibilityPolicy, SpeechControls,
    SynthesizeRequest, TtsInput, VoiceSelection,
};

let result = gateway
    .synthesize(SynthesizeRequest {
        model: Some("volcengine/seed-tts-2.0".to_owned()),
        input: TtsInput::Text("hello".to_owned()),
        voice: VoiceSelection::by_id("zh_female_wanwanxiaohe_moon_bigtts"),
        output: AudioOutputConfig::new(AudioFormat::Mp3),
        controls: SpeechControls::default(),
        compatibility: CompatibilityPolicy::Strict,
        trace_id: Some("batch".to_owned()),
        provider_options: serde_json::Value::Null,
    })
    .await?;
```

## Single Stream

```rust
use agent_runtime_tts_providers::{
    AudioFormat, AudioOutputConfig, CompatibilityPolicy, SpeechControls,
    StreamSynthesizeRequest, TtsInput, VoiceSelection,
};

let mut stream = gateway
    .stream_synthesize(StreamSynthesizeRequest {
        model: Some("volcengine/seed-tts-2.0".to_owned()),
        input: TtsInput::Text("stream this".to_owned()),
        voice: VoiceSelection::by_id("zh_female_wanwanxiaohe_moon_bigtts"),
        output: AudioOutputConfig::new(AudioFormat::Mp3),
        controls: SpeechControls::default(),
        compatibility: CompatibilityPolicy::Strict,
        trace_id: Some("stream".to_owned()),
        provider_options: serde_json::Value::Null,
    })
    .await?;

let terminal = stream.events.collect_until_terminal().await?;
```

## Duplex Stream

```rust
use agent_runtime_tts_providers::{
    AudioFormat, AudioOutputConfig, CompatibilityPolicy, DuplexSynthesizeRequest,
    SpeechControls, VoiceSelection,
};

let mut stream = gateway
    .start_duplex_stream(DuplexSynthesizeRequest {
        model: Some("volcengine/seed-tts-2.0".to_owned()),
        voice: VoiceSelection::by_id("zh_female_wanwanxiaohe_moon_bigtts"),
        output: AudioOutputConfig::new(AudioFormat::Mp3),
        controls: SpeechControls::default(),
        compatibility: CompatibilityPolicy::Strict,
        trace_id: Some("duplex".to_owned()),
        provider_options: serde_json::Value::Null,
    })
    .await?;

stream.input.send_text("first chunk").await?;
stream.input.send_text("second chunk").await?;
stream.input.finish().await?;
let terminal = stream.events.collect_until_terminal().await?;
```
