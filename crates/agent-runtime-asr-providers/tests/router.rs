mod fake_provider;

use agent_runtime_asr_providers::compatibility::validate_streaming_request;
use agent_runtime_asr_providers::error::{AsrError, AsrErrorCode};
use agent_runtime_asr_providers::routing::{
    parse_route_config, AsrGateway, AsrGatewayConfig, AsrRoute, AsrRouter,
};
use agent_runtime_asr_providers::traits::AsrProvider;
use agent_runtime_asr_providers::types::*;
use fake_provider::FakeAsrProvider;
use serde_json::json;
use std::time::Duration;

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

fn setup_router_with_routes() -> AsrRouter {
    let volcengine = FakeAsrProvider::volcengine();
    let aliyun = FakeAsrProvider::aliyun();

    let mut router = AsrRouter::new();
    router.register_provider("volcengine/bigmodel_async".into(), volcengine);
    router.register_provider("aliyun/fun-asr-realtime".into(), aliyun);

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
"#,
    )
    .unwrap();
    router.set_routes(routes);
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
fn router_explicit_model_bypasses_routes() {
    let router = setup_router_with_routes();

    let mut request = make_streaming_request();
    request.model = Some("aliyun/fun-asr-realtime".into());

    let provider = router.select_for_streaming(&request).unwrap();
    assert_eq!(provider.provider_name(), "aliyun");
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
    let event = stream.events.next().await;
    assert!(matches!(event, Some(AsrStreamEvent::Started { .. })));
}
