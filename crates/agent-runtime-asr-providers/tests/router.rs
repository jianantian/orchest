mod fake_provider;

use agent_runtime_asr_providers::compatibility::{
    validate_streaming_request, validate_transcribe_request,
};
use agent_runtime_asr_providers::error::{AsrError, AsrErrorCode};
use agent_runtime_asr_providers::routing::{
    parse_route_config, AsrGateway, AsrGatewayConfig, AsrRoute, AsrRouter,
};
use agent_runtime_asr_providers::traits::AsrProvider;
use agent_runtime_asr_providers::types::*;
use fake_provider::FakeAsrProvider;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::oneshot;

fn expect_err<T>(result: Result<T, AsrError>) -> AsrError {
    match result {
        Err(e) => e,
        Ok(_) => panic!("expected error"),
    }
}

fn make_streaming_request() -> StreamingTranscribeRequest {
    StreamingTranscribeRequest {
        model: None,
        format: StreamingAudioFormat::Pcm16 {
            sample_rate_hz: 16000,
            channels: 1,
        },
        timeline: AudioTimelineMode::ContinuousRealtime,
        options: TranscribeOptions {
            language: Some(Language::new("zh-CN")),
            ..Default::default()
        },
        compatibility: CompatibilityPolicy::Strict,
        provider_options: serde_json::Value::Null,
    }
}

fn make_transcribe_request(audio: AudioInput) -> TranscribeRequest {
    TranscribeRequest {
        model: None,
        audio,
        options: TranscribeOptions {
            language: Some(Language::new("zh-CN")),
            ..Default::default()
        },
        timeout: None,
        compatibility: CompatibilityPolicy::Strict,
        provider_options: serde_json::Value::Null,
    }
}

fn setup_router_with_routes() -> AsrRouter {
    let volcengine = FakeAsrProvider::volcengine();
    let aliyun = FakeAsrProvider::aliyun();
    let deepgram = FakeAsrProvider::deepgram();
    let elevenlabs = FakeAsrProvider::elevenlabs();
    let soniox = FakeAsrProvider::soniox();

    let mut router = AsrRouter::new();
    router.register_provider("volcengine/bigmodel_async".into(), volcengine);
    router.register_provider("aliyun/fun-asr-realtime".into(), aliyun);
    router.register_provider("deepgram/nova-3".into(), deepgram);
    router.register_provider("elevenlabs/scribe_v2_realtime".into(), elevenlabs);
    router.register_provider("soniox/stt-rt-v5".into(), soniox);

    let routes = parse_route_config(
        r#"
[[routes]]
model = "volcengine/bigmodel_async"
priority = 10
languages = ["zh-CN"]
regions = ["cn"]

[[routes]]
model = "aliyun/fun-asr-realtime"
priority = 20
languages = ["zh-CN", "en", "ja"]
regions = ["cn"]

[[routes]]
model = "deepgram/nova-3"
priority = 5
languages = ["en", "es"]
regions = ["global"]

[[routes]]
model = "elevenlabs/scribe_v2_realtime"
priority = 6
languages = ["en", "es", "auto"]
regions = ["global"]

[[routes]]
model = "soniox/stt-rt-v5"
priority = 4
languages = ["mixed:en,es"]
regions = ["global"]
"#,
    )
    .unwrap();
    router.set_routes(routes);
    router
}

fn setup_batch_router_with_routes(provider: std::sync::Arc<FakeAsrProvider>) -> AsrRouter {
    let mut router = AsrRouter::new();
    router.register_provider("fake/batch".into(), provider);
    router.set_routes(vec![AsrRoute {
        model: "fake/batch".into(),
        priority: 10,
        languages: vec![Language::new("zh-CN"), Language::new("en")],
        regions: vec![],
        max_latency_ms: None,
        max_cost_micros_per_minute: None,
    }]);
    router
}

fn setup_global_batch_router() -> AsrRouter {
    let mut router = AsrRouter::new();
    router.register_provider("assemblyai/universal".into(), FakeAsrProvider::assemblyai());
    router.register_provider(
        "speechmatics/enhanced".into(),
        FakeAsrProvider::speechmatics(),
    );
    router.set_routes(vec![
        AsrRoute {
            model: "assemblyai/universal".into(),
            priority: 10,
            languages: vec![Language::new("auto"), Language::new("en")],
            regions: vec![],
            max_latency_ms: None,
            max_cost_micros_per_minute: None,
        },
        AsrRoute {
            model: "speechmatics/enhanced".into(),
            priority: 20,
            languages: vec![
                Language::new("auto"),
                Language::new("de"),
                Language::new("en"),
            ],
            regions: vec![],
            max_latency_ms: None,
            max_cost_micros_per_minute: None,
        },
    ]);
    router
}

// ---------------------------------------------------------------------------
// Router determinism
// ---------------------------------------------------------------------------

#[test]
fn router_selects_lowest_priority() {
    let router = setup_router_with_routes();
    let request = make_streaming_request();
    let provider = router.select_for_streaming(&request).unwrap();
    assert_eq!(provider.provider_name(), "volcengine");
}

#[test]
fn router_tiebreak_by_model_string() {
    let volcengine = FakeAsrProvider::volcengine();
    let aliyun = FakeAsrProvider::aliyun();

    let mut router = AsrRouter::new();
    router.register_provider("volcengine/bigmodel_async".into(), volcengine);
    router.register_provider("aliyun/fun-asr-realtime".into(), aliyun);

    router.set_routes(vec![
        AsrRoute {
            model: "volcengine/bigmodel_async".into(),
            priority: 10,
            languages: vec![Language::new("zh-CN")],
            regions: vec![],
            max_latency_ms: None,
            max_cost_micros_per_minute: None,
        },
        AsrRoute {
            model: "aliyun/fun-asr-realtime".into(),
            priority: 10,
            languages: vec![Language::new("zh-CN")],
            regions: vec![],
            max_latency_ms: None,
            max_cost_micros_per_minute: None,
        },
    ]);

    let request = make_streaming_request();
    let provider = router.select_for_streaming(&request).unwrap();
    assert_eq!(provider.provider_name(), "aliyun");
}

#[test]
fn router_language_filter() {
    let router = setup_router_with_routes();

    let mut request = make_streaming_request();
    request.options.language = Some(Language::new("ja"));

    let provider = router.select_for_streaming(&request).unwrap();
    assert_eq!(provider.provider_name(), "aliyun");
}

#[test]
fn router_selects_deepgram_for_english_route() {
    let router = setup_router_with_routes();

    let mut request = make_streaming_request();
    request.options.language = Some(Language::new("en"));

    let provider = router.select_for_streaming(&request).unwrap();
    assert_eq!(provider.provider_name(), "deepgram");
    assert_eq!(provider.model_name(), "nova-3");
}

#[test]
fn router_explicit_model_bypasses_routes() {
    let router = setup_router_with_routes();

    let mut request = make_streaming_request();
    request.model = Some("aliyun/fun-asr-realtime".into());

    let provider = router.select_for_streaming(&request).unwrap();
    assert_eq!(provider.provider_name(), "aliyun");
}

#[test]
fn router_explicit_deepgram_model_bypasses_routes() {
    let router = setup_router_with_routes();

    let mut request = make_streaming_request();
    request.model = Some("deepgram/nova-3".into());

    let provider = router.select_for_streaming(&request).unwrap();
    assert_eq!(provider.provider_name(), "deepgram");
    assert_eq!(provider.model_name(), "nova-3");
}

#[test]
fn router_explicit_elevenlabs_model_bypasses_routes() {
    let router = setup_router_with_routes();

    let mut request = make_streaming_request();
    request.model = Some("elevenlabs/scribe_v2_realtime".into());

    let provider = router.select_for_streaming(&request).unwrap();
    assert_eq!(provider.provider_name(), "elevenlabs");
    assert_eq!(provider.model_name(), "scribe_v2_realtime");
}

#[test]
fn router_handles_arbitrary_mixed_language_tag_for_soniox() {
    let router = setup_router_with_routes();

    let mut request = make_streaming_request();
    request.options.language = Some(Language::new("mixed:en,es"));

    let provider = router.select_for_streaming(&request).unwrap();
    assert_eq!(provider.provider_name(), "soniox");
    assert_eq!(provider.model_name(), "stt-rt-v5");
}

#[test]
fn router_explicit_soniox_model_bypasses_routes() {
    let router = setup_router_with_routes();

    let mut request = make_streaming_request();
    request.model = Some("soniox/stt-rt-v5".into());
    request.options.language = Some(Language::new("xx-custom"));

    let provider = router.select_for_streaming(&request).unwrap();
    assert_eq!(provider.provider_name(), "soniox");
    assert_eq!(provider.model_name(), "stt-rt-v5");
}

#[test]
fn router_transcribe_explicit_model_bypasses_routes() {
    let router = setup_batch_router_with_routes(FakeAsrProvider::batch());

    let mut request = make_transcribe_request(AudioInput::Bytes {
        data: vec![0; 100],
        format: AudioFormat::Pcm,
        sample_rate_hz: Some(16000),
    });
    request.model = Some("fake/batch".into());

    let provider = router.select_for_transcribe(&request).unwrap();
    assert_eq!(provider.provider_name(), "fake");
}

#[test]
fn router_transcribe_explicit_assemblyai_model_bypasses_routes() {
    let router = setup_global_batch_router();
    let mut request = make_transcribe_request(AudioInput::Url {
        url: "https://example.com/audio.mp3".into(),
        format: Some(AudioFormat::Mp3),
    });
    request.model = Some("assemblyai/universal".into());
    request.options.language = Some(Language::new("xx-custom"));

    let provider = router.select_for_transcribe(&request).unwrap();
    assert_eq!(provider.provider_name(), "assemblyai");
    assert_eq!(provider.model_name(), "universal");
}

#[test]
fn router_transcribe_routes_german_to_speechmatics() {
    let router = setup_global_batch_router();
    let mut request = make_transcribe_request(AudioInput::Bytes {
        data: vec![0; 100],
        format: AudioFormat::Wav,
        sample_rate_hz: None,
    });
    request.options.language = Some(Language::new("de"));

    let provider = router.select_for_transcribe(&request).unwrap();
    assert_eq!(provider.provider_name(), "speechmatics");
    assert_eq!(provider.model_name(), "enhanced");
}

#[test]
fn router_transcribe_auto_selects_route() {
    let router = setup_batch_router_with_routes(FakeAsrProvider::batch());

    let request = make_transcribe_request(AudioInput::Bytes {
        data: vec![0; 100],
        format: AudioFormat::Pcm,
        sample_rate_hz: Some(16000),
    });

    let provider = router.select_for_transcribe(&request).unwrap();
    assert_eq!(provider.model_name(), "batch");
}

#[test]
fn router_explicit_model_not_registered() {
    let router = setup_router_with_routes();

    let mut request = make_streaming_request();
    request.model = Some("deepgram/nova-2".into());

    let err = expect_err(router.select_for_streaming(&request));
    assert_eq!(err.code, AsrErrorCode::NoMatchingProvider);
}

#[test]
fn router_no_model_no_routes() {
    let router = AsrRouter::new();
    let request = make_streaming_request();
    let err = expect_err(router.select_for_streaming(&request));
    assert_eq!(err.code, AsrErrorCode::NoMatchingProvider);
}

#[test]
fn router_no_language_match() {
    let router = setup_router_with_routes();

    let mut request = make_streaming_request();
    request.options.language = Some(Language::new("fr"));

    let err = expect_err(router.select_for_streaming(&request));
    assert_eq!(err.code, AsrErrorCode::NoMatchingProvider);
}

// ---------------------------------------------------------------------------
// Gateway validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gateway_rejects_provider_options_without_model() {
    let router = setup_router_with_routes();
    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let mut request = make_streaming_request();
    request.provider_options = json!({"resource_id": "test"});
    request.model = None;

    let err = expect_err(gateway.start_stream(request).await);
    assert_eq!(err.code, AsrErrorCode::InvalidRequest);
    assert!(err.message.contains("provider_options"));
}

#[tokio::test]
async fn gateway_allows_provider_options_with_model() {
    let router = setup_router_with_routes();
    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let mut request = make_streaming_request();
    request.provider_options = json!({"resource_id": "test"});
    request.model = Some("volcengine/bigmodel_async".into());

    let result = gateway.start_stream(request).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn gateway_auto_generates_trace_id() {
    let router = setup_router_with_routes();
    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let mut request = make_streaming_request();
    request.model = Some("volcengine/bigmodel_async".into());
    assert!(request.options.trace_id.is_none());

    let _stream = gateway.start_stream(request).await.unwrap();
}

#[tokio::test]
async fn gateway_transcribe_bytes_success_with_explicit_model() {
    let router = setup_batch_router_with_routes(FakeAsrProvider::batch());
    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let mut request = make_transcribe_request(AudioInput::Bytes {
        data: vec![0; 100],
        format: AudioFormat::Pcm,
        sample_rate_hz: Some(16000),
    });
    request.model = Some("fake/batch".into());

    let result = gateway.transcribe(request).await.unwrap();
    assert_eq!(result.text, "one-shot transcript");
    assert_eq!(result.language, Some(Language::new("zh-CN")));
}

#[tokio::test]
async fn gateway_transcribe_file_success_with_explicit_format() {
    let router = setup_batch_router_with_routes(FakeAsrProvider::batch());
    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let request = make_transcribe_request(AudioInput::File {
        path: PathBuf::from("fixtures/audio.mp3"),
        format: Some(AudioFormat::Mp3),
    });

    let result = gateway.transcribe(request).await.unwrap();
    assert_eq!(result.text, "one-shot transcript");
}

#[tokio::test]
async fn gateway_transcribe_url_success_with_explicit_format() {
    let router = setup_batch_router_with_routes(FakeAsrProvider::batch());
    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let request = make_transcribe_request(AudioInput::Url {
        url: "https://example.com/audio.wav".into(),
        format: Some(AudioFormat::Wav),
    });

    let result = gateway.transcribe(request).await.unwrap();
    assert_eq!(result.text, "one-shot transcript");
}

#[tokio::test]
async fn gateway_transcribe_file_without_format_requires_inference_support() {
    let router = setup_batch_router_with_routes(FakeAsrProvider::batch());
    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let request = make_transcribe_request(AudioInput::File {
        path: PathBuf::from("fixtures/audio.mp3"),
        format: None,
    });

    let err = expect_err(gateway.transcribe(request).await);
    assert_eq!(err.code, AsrErrorCode::InvalidRequest);
    assert!(err.message.contains("explicit format"));
}

#[tokio::test]
async fn gateway_transcribe_url_without_format_allowed_with_inference_support() {
    let router = setup_batch_router_with_routes(FakeAsrProvider::batch_with_format_inference(true));
    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let request = make_transcribe_request(AudioInput::Url {
        url: "https://example.com/audio".into(),
        format: None,
    });

    let result = gateway.transcribe(request).await.unwrap();
    assert_eq!(result.text, "one-shot transcript");
}

#[tokio::test]
async fn gateway_transcribe_realtime_only_provider_returns_unsupported() {
    let router = setup_router_with_routes();
    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let mut request = make_transcribe_request(AudioInput::Bytes {
        data: vec![0; 100],
        format: AudioFormat::Pcm,
        sample_rate_hz: Some(16000),
    });
    request.model = Some("volcengine/bigmodel_async".into());

    let err = expect_err(gateway.transcribe(request).await);
    assert_eq!(err.code, AsrErrorCode::UnsupportedOperation);
}

#[tokio::test]
async fn gateway_transcribe_rejects_empty_byte_input() {
    let router = setup_batch_router_with_routes(FakeAsrProvider::batch());
    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let request = make_transcribe_request(AudioInput::Bytes {
        data: vec![],
        format: AudioFormat::Pcm,
        sample_rate_hz: Some(16000),
    });

    let err = expect_err(gateway.transcribe(request).await);
    assert_eq!(err.code, AsrErrorCode::InvalidAudio);
}

#[tokio::test]
async fn gateway_transcribe_requires_pcm_sample_rate() {
    let router = setup_batch_router_with_routes(FakeAsrProvider::batch());
    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let request = make_transcribe_request(AudioInput::Bytes {
        data: vec![0; 100],
        format: AudioFormat::Pcm,
        sample_rate_hz: None,
    });

    let err = expect_err(gateway.transcribe(request).await);
    assert_eq!(err.code, AsrErrorCode::InvalidRequest);
    assert!(err.message.contains("sample_rate_hz"));
}

#[tokio::test]
async fn gateway_transcribe_times_out_provider_execution() {
    let (entered_tx, _entered_rx) = oneshot::channel();
    let (cancelled_tx, _cancelled_rx) = oneshot::channel();
    let router = setup_batch_router_with_routes(FakeAsrProvider::batch_never_transcribes(
        entered_tx,
        cancelled_tx,
    ));
    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let mut request = make_transcribe_request(AudioInput::Bytes {
        data: vec![0; 100],
        format: AudioFormat::Pcm,
        sample_rate_hz: Some(16000),
    });
    request.timeout = Some(Duration::from_millis(10));

    let err = expect_err(gateway.transcribe(request).await);
    assert_eq!(err.code, AsrErrorCode::Timeout);
}

#[tokio::test]
async fn gateway_transcribe_future_cancellation_drops_provider_future() {
    let (entered_tx, entered_rx) = oneshot::channel();
    let (cancelled_tx, cancelled_rx) = oneshot::channel();
    let router = setup_batch_router_with_routes(FakeAsrProvider::batch_never_transcribes(
        entered_tx,
        cancelled_tx,
    ));
    let gateway = AsrGateway::new(
        router,
        AsrGatewayConfig {
            route_config_path: None,
        },
    );

    let request = make_transcribe_request(AudioInput::Bytes {
        data: vec![0; 100],
        format: AudioFormat::Pcm,
        sample_rate_hz: Some(16000),
    });

    let task = tokio::spawn(async move { gateway.transcribe(request).await });
    tokio::time::timeout(Duration::from_secs(1), entered_rx)
        .await
        .expect("provider future should start")
        .expect("entered sender should not drop before start");
    task.abort();
    tokio::time::timeout(Duration::from_secs(1), cancelled_rx)
        .await
        .expect("provider future should be dropped on cancellation")
        .expect("cancelled sender should not drop before cancellation");
}

// ---------------------------------------------------------------------------
// Compatibility: Strict mode
// ---------------------------------------------------------------------------

#[test]
fn strict_rejects_unsupported_audio_format() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.format = StreamingAudioFormat::Encoded {
        format: AudioFormat::Mp3,
    };

    let err = validate_streaming_request(&request, &caps).unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedAudioFormat);
}

#[test]
fn strict_rejects_unsupported_sample_rate() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.format = StreamingAudioFormat::Pcm16 {
        sample_rate_hz: 44100,
        channels: 1,
    };

    let err = validate_streaming_request(&request, &caps).unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedAudioFormat);
}

#[test]
fn strict_rejects_unsupported_channels() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.format = StreamingAudioFormat::Pcm16 {
        sample_rate_hz: 16000,
        channels: 2,
    };

    let err = validate_streaming_request(&request, &caps).unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedAudioFormat);
}

#[test]
fn strict_rejects_sparse_timeline_for_continuous_only() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.timeline = AudioTimelineMode::SparseSpeechOnly;

    let err = validate_streaming_request(&request, &caps).unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedOption);
}

#[test]
fn strict_rejects_unsupported_endpointing_mode() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.options.endpointing = Some(EndpointingOptions {
        mode: EndpointingMode::Semantic,
        silence_timeout: None,
    });

    let err = validate_streaming_request(&request, &caps).unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedOption);
}

#[test]
fn strict_allows_provider_default_endpointing() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.options.endpointing = Some(EndpointingOptions {
        mode: EndpointingMode::ProviderDefault,
        silence_timeout: None,
    });

    assert!(validate_streaming_request(&request, &caps).is_ok());
}

#[test]
fn strict_rejects_silence_timeout_without_acoustic_silence_mode() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.options.endpointing = Some(EndpointingOptions {
        mode: EndpointingMode::NaturalSegmenting,
        silence_timeout: Some(Duration::from_millis(500)),
    });

    let err = validate_streaming_request(&request, &caps).unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedOption);
    assert!(err.message.contains("silence_timeout"));
}

#[test]
fn strict_allows_silence_timeout_with_acoustic_silence() {
    let caps = FakeAsrProvider::aliyun().capabilities();
    let mut request = make_streaming_request();
    request.options.endpointing = Some(EndpointingOptions {
        mode: EndpointingMode::AcousticSilence,
        silence_timeout: Some(Duration::from_millis(500)),
    });

    assert!(validate_streaming_request(&request, &caps).is_ok());
}

#[test]
fn strict_allows_deepgram_pcm_range_word_timestamps_and_endpointing() {
    let caps = FakeAsrProvider::deepgram().capabilities();
    let mut request = make_streaming_request();
    request.model = Some("deepgram/nova-3".into());
    request.options.language = Some(Language::new("en"));
    request.options.word_timestamps = true;
    request.options.endpointing = Some(EndpointingOptions {
        mode: EndpointingMode::AcousticSilence,
        silence_timeout: Some(Duration::from_millis(750)),
    });
    request.provider_options = json!({"smart_format": true});

    assert!(validate_streaming_request(&request, &caps).is_ok());
}

#[test]
fn strict_allows_elevenlabs_keyterms_language_detection_and_timestamps() {
    let caps = FakeAsrProvider::elevenlabs().capabilities();
    let mut request = make_streaming_request();
    request.model = Some("elevenlabs/scribe_v2_realtime".into());
    request.options.language = Some(Language::new("en"));
    request.options.word_timestamps = true;
    request.options.hot_words = vec!["orchest".into()];
    request.options.endpointing = Some(EndpointingOptions {
        mode: EndpointingMode::AcousticSilence,
        silence_timeout: Some(Duration::from_millis(750)),
    });
    request.provider_options = json!({
        "include_language_detection": true,
        "keyterms": ["runtime"],
        "commit_strategy": "vad",
        "vad_silence_threshold_secs": 0.75
    });

    assert!(validate_streaming_request(&request, &caps).is_ok());
}

#[test]
fn strict_allows_soniox_code_switching_and_arbitrary_language_hint() {
    let caps = FakeAsrProvider::soniox().capabilities();
    let mut request = make_streaming_request();
    request.model = Some("soniox/stt-rt-v5".into());
    request.options.language = Some(Language::new("mixed:en,es"));
    request.options.code_switching = true;
    request.options.word_timestamps = true;
    request.options.hot_words = vec!["orchest".into()];
    request.options.context_prompt = Some("agent runtime vocabulary".into());
    request.options.endpointing = Some(EndpointingOptions {
        mode: EndpointingMode::AcousticSilence,
        silence_timeout: Some(Duration::from_millis(750)),
    });
    request.provider_options = json!({
        "language_hints": ["en", "es"],
        "language_hints_strict": false,
        "enable_language_identification": true,
        "max_endpoint_delay_ms": 750
    });

    assert!(validate_streaming_request(&request, &caps).is_ok());
}

#[test]
fn strict_allows_assemblyai_batch_metadata_options() {
    let caps = FakeAsrProvider::assemblyai().capabilities();
    let mut request = make_transcribe_request(AudioInput::Url {
        url: "https://example.com/audio.mp3".into(),
        format: Some(AudioFormat::Mp3),
    });
    request.model = Some("assemblyai/universal".into());
    request.options.language = Some(Language::new("auto"));
    request.options.speaker_diarization = true;
    request.options.word_timestamps = true;
    request.options.hot_words = vec!["orchest".into()];
    request.provider_options = json!({
        "speaker_labels": true,
        "language_detection": true,
        "language_confidence_threshold": 0.7,
        "speech_model": "universal",
        "format_text": true
    });

    assert!(validate_transcribe_request(&request, &caps).is_ok());
}

#[test]
fn strict_allows_speechmatics_batch_metadata_options() {
    let caps = FakeAsrProvider::speechmatics().capabilities();
    let mut request = make_transcribe_request(AudioInput::Bytes {
        data: vec![0; 100],
        format: AudioFormat::Wav,
        sample_rate_hz: None,
    });
    request.model = Some("speechmatics/enhanced".into());
    request.options.language = Some(Language::new("de"));
    request.options.speaker_diarization = true;
    request.options.word_timestamps = true;
    request.options.hot_words = vec!["orchest".into()];
    request.provider_options = json!({
        "operating_point": "enhanced",
        "diarization": "speaker",
        "additional_vocab": [{"content": "runtime"}],
        "enable_entities": true
    });

    assert!(validate_transcribe_request(&request, &caps).is_ok());
}

#[test]
fn strict_rejects_streaming_request_for_batch_only_provider() {
    let caps = FakeAsrProvider::assemblyai().capabilities();
    let mut request = make_streaming_request();
    request.model = Some("assemblyai/universal".into());
    request.options.language = Some(Language::new("en"));

    let err = validate_streaming_request(&request, &caps).unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedAudioFormat);
}

#[test]
fn strict_rejects_word_timestamps() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.options.word_timestamps = true;

    let err = validate_streaming_request(&request, &caps).unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedOption);
}

#[test]
fn strict_rejects_speaker_diarization() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.options.speaker_diarization = true;

    let err = validate_streaming_request(&request, &caps).unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedOption);
}

#[test]
fn strict_rejects_context_prompt_when_unsupported() {
    let caps = FakeAsrProvider::aliyun().capabilities();
    let mut request = make_streaming_request();
    request.options.context_prompt = Some("test context".into());

    let err = validate_streaming_request(&request, &caps).unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedOption);
}

#[test]
fn strict_allows_context_prompt_when_supported() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.options.context_prompt = Some("test context".into());

    assert!(validate_streaming_request(&request, &caps).is_ok());
}

#[test]
fn strict_rejects_hot_words_when_unsupported() {
    let mut caps = FakeAsrProvider::volcengine().capabilities();
    caps.hot_words = false;
    let mut request = make_streaming_request();
    request.options.hot_words = vec!["test".into()];

    let err = validate_streaming_request(&request, &caps).unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedOption);
}

#[test]
fn strict_rejects_unknown_provider_option_keys() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.model = Some("volcengine/bigmodel_async".into());
    request.provider_options = json!({"unknown_key": "value"});

    let err = validate_streaming_request(&request, &caps).unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedOption);
    assert!(err.message.contains("unknown_key"));
}

#[test]
fn strict_allows_known_provider_option_keys() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.model = Some("volcengine/bigmodel_async".into());
    request.provider_options = json!({"resource_id": "test-resource"});

    assert!(validate_streaming_request(&request, &caps).is_ok());
}

#[test]
fn strict_rejects_code_switching_when_unsupported() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.options.code_switching = true;

    let err = validate_streaming_request(&request, &caps).unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedOption);
}

// ---------------------------------------------------------------------------
// Compatibility: Coerce mode
// ---------------------------------------------------------------------------

#[test]
fn coerce_records_word_timestamps_adjustment() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.compatibility = CompatibilityPolicy::Coerce;
    request.options.word_timestamps = true;

    let result = validate_streaming_request(&request, &caps).unwrap();
    assert_eq!(result.adjustments.len(), 1);
    assert_eq!(result.adjustments[0].option, "word_timestamps");
}

#[test]
fn coerce_records_multiple_adjustments() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.compatibility = CompatibilityPolicy::Coerce;
    request.options.word_timestamps = true;
    request.options.speaker_diarization = true;
    request.options.code_switching = true;

    let result = validate_streaming_request(&request, &caps).unwrap();
    assert_eq!(result.adjustments.len(), 3);
    let options: Vec<&str> = result
        .adjustments
        .iter()
        .map(|a| a.option.as_str())
        .collect();
    assert!(options.contains(&"word_timestamps"));
    assert!(options.contains(&"speaker_diarization"));
    assert!(options.contains(&"code_switching"));
}

#[test]
fn coerce_still_rejects_unsupported_audio_format() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.compatibility = CompatibilityPolicy::Coerce;
    request.format = StreamingAudioFormat::Encoded {
        format: AudioFormat::Mp3,
    };

    let err = validate_streaming_request(&request, &caps).unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedAudioFormat);
}

#[test]
fn coerce_records_unsupported_provider_options() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.compatibility = CompatibilityPolicy::Coerce;
    request.model = Some("volcengine/bigmodel_async".into());
    request.provider_options = json!({"unknown_key": "value"});

    let result = validate_streaming_request(&request, &caps).unwrap();
    assert_eq!(result.adjustments.len(), 1);
    assert!(result.adjustments[0].option.contains("unknown_key"));
}

#[test]
fn coerce_records_endpointing_mode_adjustment() {
    let caps = FakeAsrProvider::volcengine().capabilities();
    let mut request = make_streaming_request();
    request.compatibility = CompatibilityPolicy::Coerce;
    request.options.endpointing = Some(EndpointingOptions {
        mode: EndpointingMode::Semantic,
        silence_timeout: None,
    });

    let result = validate_streaming_request(&request, &caps).unwrap();
    assert!(result
        .adjustments
        .iter()
        .any(|a| a.option == "endpointing.mode"));
}

#[test]
fn coerce_records_deepgram_unsupported_options() {
    let caps = FakeAsrProvider::deepgram().capabilities();
    let mut request = make_streaming_request();
    request.compatibility = CompatibilityPolicy::Coerce;
    request.model = Some("deepgram/nova-3".into());
    request.options.speaker_diarization = true;
    request.options.code_switching = true;
    request.options.context_prompt = Some("domain hints".into());
    request.provider_options = json!({"unsupported": true});

    let result = validate_streaming_request(&request, &caps).unwrap();
    let options: Vec<&str> = result
        .adjustments
        .iter()
        .map(|a| a.option.as_str())
        .collect();
    assert!(options.contains(&"speaker_diarization"));
    assert!(options.contains(&"code_switching"));
    assert!(options.contains(&"context_prompt"));
    assert!(options.contains(&"provider_options.unsupported"));
}

#[test]
fn coerce_records_elevenlabs_unsupported_diarization() {
    let caps = FakeAsrProvider::elevenlabs().capabilities();
    let mut request = make_streaming_request();
    request.compatibility = CompatibilityPolicy::Coerce;
    request.model = Some("elevenlabs/scribe_v2_realtime".into());
    request.options.speaker_diarization = true;
    request.provider_options = json!({"unknown": true});

    let result = validate_streaming_request(&request, &caps).unwrap();
    let options: Vec<&str> = result
        .adjustments
        .iter()
        .map(|a| a.option.as_str())
        .collect();
    assert!(options.contains(&"speaker_diarization"));
    assert!(options.contains(&"provider_options.unknown"));
}

// ---------------------------------------------------------------------------
// Bare model rejection
// ---------------------------------------------------------------------------

#[test]
fn router_rejects_bare_model_string() {
    let router = setup_router_with_routes();

    let mut request = make_streaming_request();
    request.model = Some("bigmodel_async".into());

    let err = expect_err(router.select_for_streaming(&request));
    assert_eq!(err.code, AsrErrorCode::InvalidRequest);
}

// ---------------------------------------------------------------------------
// Fake provider behavior
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fake_provider_transcribe_returns_unsupported() {
    let provider = FakeAsrProvider::volcengine();

    let request = TranscribeRequest {
        model: Some("volcengine/bigmodel_async".into()),
        audio: AudioInput::Bytes {
            data: vec![0; 100],
            format: AudioFormat::Pcm,
            sample_rate_hz: Some(16000),
        },
        options: TranscribeOptions::default(),
        timeout: None,
        compatibility: CompatibilityPolicy::Strict,
        provider_options: serde_json::Value::Null,
    };

    let err = provider.transcribe(request).await.unwrap_err();
    assert_eq!(err.code, AsrErrorCode::UnsupportedOperation);
}

#[tokio::test]
async fn fake_provider_start_stream_succeeds() {
    let provider = FakeAsrProvider::volcengine();

    let request = StreamingTranscribeRequest {
        model: Some("volcengine/bigmodel_async".into()),
        format: StreamingAudioFormat::Pcm16 {
            sample_rate_hz: 16000,
            channels: 1,
        },
        timeline: AudioTimelineMode::ContinuousRealtime,
        options: TranscribeOptions::default(),
        compatibility: CompatibilityPolicy::Strict,
        provider_options: serde_json::Value::Null,
    };

    let mut stream = provider.start_stream(request).await.unwrap();

    let ev1 = stream.events.next().await;
    assert!(matches!(ev1, Some(AsrStreamEvent::RouteSelected { .. })));

    let ev2 = stream.events.next().await;
    assert!(matches!(ev2, Some(AsrStreamEvent::Started { .. })));

    stream.input.end_stream().await.unwrap();
}
