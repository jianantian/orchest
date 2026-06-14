use std::sync::Arc;

use agent_runtime_tts_providers::{
    normalize_tts_provider_model, AudioFormat, AudioOutputConfig, CompatibilityPolicy,
    DuplexSynthesizeRequest, Language, ListVoicesRequest, SpeechControls, SynthesizeRequest,
    TtsErrorCode, TtsGateway, TtsGatewayConfig, TtsInput, TtsOperation, TtsProvider,
    TtsProviderRuntimeConfig, TtsRoute, TtsRouter, TtsStreamEvent, VoiceKind, VoiceSelection,
};

mod support;

use support::FakeTtsProvider;

fn text_request(model: Option<&str>) -> SynthesizeRequest {
    SynthesizeRequest {
        model: model.map(ToOwned::to_owned),
        input: TtsInput::Text("hello".to_owned()),
        voice: VoiceSelection::by_id("voice-a"),
        output: AudioOutputConfig::new(AudioFormat::Mp3),
        controls: SpeechControls::default(),
        compatibility: CompatibilityPolicy::Strict,
        trace_id: Some("trace-test".to_owned()),
        provider_options: serde_json::Value::Null,
    }
}

fn gateway() -> TtsGateway {
    let mut router = TtsRouter::new();
    router.register_provider(
        "fake/batch".to_owned(),
        Arc::new(FakeTtsProvider::new("fake", "batch").with_batch_only()),
    );
    router.register_provider(
        "fake/stream".to_owned(),
        Arc::new(FakeTtsProvider::new("fake", "stream").with_streams()),
    );
    router.register_provider(
        "fake/duplex".to_owned(),
        Arc::new(FakeTtsProvider::new("fake", "duplex").with_duplex()),
    );
    router.set_routes(vec![
        TtsRoute {
            model: "fake/stream".to_owned(),
            languages: vec![Language::new("en-US")],
            voice_kinds: vec![VoiceKind::System],
            output_formats: vec![AudioFormat::Mp3],
            max_latency_ms: Some(200),
            max_cost_micros_per_char: Some(10),
            priority: 2,
        },
        TtsRoute {
            model: "fake/duplex".to_owned(),
            languages: vec![Language::new("en-US")],
            voice_kinds: vec![VoiceKind::System],
            output_formats: vec![AudioFormat::Mp3],
            max_latency_ms: Some(100),
            max_cost_micros_per_char: Some(20),
            priority: 1,
        },
        TtsRoute {
            model: "fake/batch".to_owned(),
            languages: vec![Language::new("en-US")],
            voice_kinds: vec![VoiceKind::System],
            output_formats: vec![AudioFormat::Mp3],
            max_latency_ms: Some(50),
            max_cost_micros_per_char: Some(5),
            priority: 0,
        },
    ]);
    TtsGateway::new(router, TtsGatewayConfig::default())
}

#[test]
fn normalize_rejects_bare_models() {
    let err = normalize_tts_provider_model("seed-tts-2.0").unwrap_err();
    assert_eq!(err.code, TtsErrorCode::InvalidRequest);
}

#[tokio::test]
async fn gateway_routes_batch_to_registered_provider() {
    let result = gateway().synthesize(text_request(None)).await.unwrap();
    assert_eq!(result.telemetry.provider, "fake");
    assert_eq!(result.telemetry.model, "batch");
    assert_eq!(result.telemetry.trace_id, "trace-test");
}

#[tokio::test]
async fn router_tiebreaks_by_normalized_model_string() {
    let mut router = TtsRouter::new();
    router.register_provider(
        "fake/beta".to_owned(),
        Arc::new(FakeTtsProvider::new("fake", "beta").with_batch_only()),
    );
    router.register_provider(
        "fake/alpha".to_owned(),
        Arc::new(FakeTtsProvider::new("fake", "alpha").with_batch_only()),
    );
    router.set_routes(vec![
        TtsRoute {
            model: "fake/beta".to_owned(),
            languages: vec![Language::new("en-US")],
            voice_kinds: vec![VoiceKind::System],
            output_formats: vec![AudioFormat::Mp3],
            max_latency_ms: None,
            max_cost_micros_per_char: None,
            priority: 0,
        },
        TtsRoute {
            model: "fake/alpha".to_owned(),
            languages: vec![Language::new("en-US")],
            voice_kinds: vec![VoiceKind::System],
            output_formats: vec![AudioFormat::Mp3],
            max_latency_ms: None,
            max_cost_micros_per_char: None,
            priority: 0,
        },
    ]);

    let result = TtsGateway::new(router, TtsGatewayConfig::default())
        .synthesize(text_request(None))
        .await
        .unwrap();

    assert_eq!(result.telemetry.model, "alpha");
}

#[tokio::test]
async fn gateway_coerce_strips_unsupported_semantic_controls_before_provider_call() {
    let mut router = TtsRouter::new();
    router.register_provider(
        "fake/batch".to_owned(),
        Arc::new(
            FakeTtsProvider::new("fake", "batch")
                .with_batch_only()
                .reject_semantic_controls(),
        ),
    );
    let gateway = TtsGateway::new(router, TtsGatewayConfig::default());
    let mut request = text_request(Some("fake/batch"));
    request.compatibility = CompatibilityPolicy::Coerce;
    request.controls.instruction = Some("speak warmly".to_owned());

    let result = gateway.synthesize(request).await.unwrap();

    assert_eq!(result.option_adjustments.len(), 1);
    assert_eq!(result.option_adjustments[0].option, "instruction");
    assert!(result.option_adjustments[0].applied.is_null());
}

#[tokio::test]
async fn explicit_model_restricts_routing() {
    let result = gateway()
        .synthesize(text_request(Some("fake/stream")))
        .await
        .unwrap();
    assert_eq!(result.telemetry.model, "stream");
}

#[tokio::test]
async fn unprefixed_request_model_is_rejected() {
    let err = gateway()
        .synthesize(text_request(Some("stream")))
        .await
        .unwrap_err();
    assert_eq!(err.code, TtsErrorCode::InvalidRequest);
}

#[tokio::test]
async fn direct_provider_selector_mismatch_is_rejected() {
    let provider = FakeTtsProvider::new("fake", "batch").with_batch_only();
    let err = provider
        .synthesize(text_request(Some("other/model")))
        .await
        .unwrap_err();
    assert_eq!(err.code, TtsErrorCode::UnknownModel);
}

#[tokio::test]
async fn list_voices_filters_language_kind_and_custom_flag() {
    let voices = gateway()
        .list_voices(ListVoicesRequest {
            model: None,
            language: Some(Language::new("en-US")),
            kind: Some(VoiceKind::System),
            include_custom: false,
            trace_id: Some("voices".to_owned()),
        })
        .await
        .unwrap();

    assert!(!voices.is_empty());
    assert!(voices.iter().all(|v| v.kind == VoiceKind::System));
    assert!(voices.iter().all(|v| !v.is_custom));
}

#[tokio::test]
async fn list_voices_with_explicit_model_uses_selected_provider() {
    let voices = gateway()
        .list_voices(ListVoicesRequest {
            model: Some("fake/batch".to_owned()),
            language: None,
            kind: None,
            include_custom: true,
            trace_id: Some("voices".to_owned()),
        })
        .await
        .unwrap();

    assert!(voices.iter().all(|voice| voice.model == "batch"));
    assert!(voices.iter().any(|voice| voice.kind == VoiceKind::Custom));
}

#[tokio::test]
async fn stream_gateway_emits_route_selected_first() {
    let mut stream = gateway()
        .stream_synthesize(SynthesizeRequest {
            model: Some("fake/stream".to_owned()),
            input: TtsInput::Text("hello".to_owned()),
            voice: VoiceSelection::by_id("voice-a"),
            output: AudioOutputConfig::new(AudioFormat::Mp3),
            controls: SpeechControls::default(),
            compatibility: CompatibilityPolicy::Strict,
            trace_id: None,
            provider_options: serde_json::Value::Null,
        })
        .await
        .unwrap();

    let first = stream.events.next().await.unwrap();
    let trace_id = first.trace_id().to_owned();
    assert!(!trace_id.is_empty());
    assert!(first.is_route_selected());
    let second = stream.events.next().await.unwrap();
    assert_eq!(second.trace_id(), trace_id);
    assert!(second.is_started());
    let terminal = stream.events.collect_until_terminal().await.unwrap();
    assert_eq!(terminal.trace_id(), trace_id);
    assert!(terminal.is_completed());
}

#[tokio::test]
async fn gateway_duplex_routes_and_forwards_terminal_summary() {
    let mut stream = gateway()
        .start_duplex_stream(DuplexSynthesizeRequest {
            model: Some("fake/duplex".to_owned()),
            voice: VoiceSelection::by_id("voice-a"),
            output: AudioOutputConfig::new(AudioFormat::Mp3),
            controls: SpeechControls::default(),
            compatibility: CompatibilityPolicy::Strict,
            trace_id: Some("duplex-trace".to_owned()),
            provider_options: serde_json::Value::Null,
        })
        .await
        .unwrap();

    let first = stream.events.next().await.unwrap();
    assert!(first.is_route_selected());
    let second = stream.events.next().await.unwrap();
    assert!(second.is_started());

    stream.input.send_text("hello").await.unwrap();
    stream.input.finish().await.unwrap();

    let mut saw_text_accepted = false;
    let mut saw_audio_zero = false;
    loop {
        match stream.events.next().await.unwrap() {
            TtsStreamEvent::TextAccepted { trace_id, chars } => {
                assert_eq!(trace_id, "duplex-trace");
                assert_eq!(chars, 5);
                saw_text_accepted = true;
            }
            TtsStreamEvent::AudioChunk {
                trace_id, sequence, ..
            } => {
                assert_eq!(trace_id, "duplex-trace");
                assert_eq!(sequence, 0);
                saw_audio_zero = true;
            }
            TtsStreamEvent::Completed { trace_id, summary } => {
                assert_eq!(trace_id, "duplex-trace");
                assert_eq!(summary.telemetry.operation, TtsOperation::DuplexStream);
                assert_eq!(summary.usage.input_chars, 5);
                break;
            }
            event => assert_eq!(event.trace_id(), "duplex-trace"),
        }
    }
    assert!(saw_text_accepted);
    assert!(saw_audio_zero);
}

#[tokio::test]
async fn gateway_duplex_input_drop_before_final_emits_cancelled() {
    let mut stream = gateway()
        .start_duplex_stream(DuplexSynthesizeRequest {
            model: Some("fake/duplex".to_owned()),
            voice: VoiceSelection::by_id("voice-a"),
            output: AudioOutputConfig::new(AudioFormat::Mp3),
            controls: SpeechControls::default(),
            compatibility: CompatibilityPolicy::Strict,
            trace_id: Some("duplex-cancel".to_owned()),
            provider_options: serde_json::Value::Null,
        })
        .await
        .unwrap();
    drop(stream.input.clone());
    drop(stream.input);

    let _ = stream.events.next().await.unwrap();
    let _ = stream.events.next().await.unwrap();
    let terminal = stream.events.collect_until_terminal().await.unwrap();
    match terminal {
        TtsStreamEvent::Error { error, fatal, .. } => {
            assert!(fatal);
            assert_eq!(error.code, TtsErrorCode::Cancelled);
        }
        other => panic!("expected cancellation error, got {other:?}"),
    }
}

#[tokio::test]
async fn strict_rejects_ssml_and_out_of_range_controls() {
    let mut request = text_request(Some("fake/batch"));
    request.input = TtsInput::Ssml("<speak>hello</speak>".to_owned());
    let err = gateway().synthesize(request).await.unwrap_err();
    assert_eq!(err.code, TtsErrorCode::UnsupportedOption);

    let mut request = text_request(Some("fake/batch"));
    request.controls.speed = 2.5;
    let err = gateway().synthesize(request).await.unwrap_err();
    assert_eq!(err.code, TtsErrorCode::InvalidRequest);
}

#[tokio::test]
async fn coerce_clamps_numeric_controls_and_records_adjustments() {
    let mut request = text_request(Some("fake/batch"));
    request.compatibility = CompatibilityPolicy::Coerce;
    request.controls.speed = 3.0;
    request.controls.pitch = -20.0;
    request.controls.volume = -1.0;

    let result = gateway().synthesize(request).await.unwrap();

    assert_eq!(result.option_adjustments.len(), 3);
    assert_eq!(result.telemetry.option_adjustment_count, 3);
    assert!(result
        .option_adjustments
        .iter()
        .any(|adjustment| adjustment.option == "speed"
            && adjustment.applied == serde_json::json!(2.0)));
}

#[tokio::test]
async fn coerce_drops_unsupported_semantic_controls() {
    let mut request = text_request(Some("fake/batch"));
    request.compatibility = CompatibilityPolicy::Coerce;
    request.controls.instruction = Some("speak warmly".to_owned());

    let result = gateway().synthesize(request).await.unwrap();

    assert_eq!(result.option_adjustments.len(), 1);
    assert_eq!(result.option_adjustments[0].option, "instruction");
    assert!(result.option_adjustments[0].applied.is_null());
}

#[tokio::test]
async fn duplex_rejects_single_stream_only_provider() {
    let err = gateway()
        .start_duplex_stream(DuplexSynthesizeRequest {
            model: Some("fake/stream".to_owned()),
            voice: VoiceSelection::by_id("voice-a"),
            output: AudioOutputConfig::new(AudioFormat::Mp3),
            controls: SpeechControls::default(),
            compatibility: CompatibilityPolicy::Coerce,
            trace_id: None,
            provider_options: serde_json::Value::Null,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, TtsErrorCode::UnsupportedOperation);
}

#[test]
fn provider_runtime_config_preserves_fields() {
    let config = TtsProviderRuntimeConfig {
        model: "volcengine/seed-tts-2.0".to_owned(),
        api_key: Some("explicit".to_owned()),
        api_key_env: None,
        api_url: Some("https://example.test".to_owned()),
        region: Some("cn-north-1".to_owned()),
        timeout: Some(std::time::Duration::from_secs(5)),
        provider_options: serde_json::json!({"mode": "fixture"}),
    };

    assert_eq!(config.model, "volcengine/seed-tts-2.0");
    assert_eq!(config.provider_options["mode"], "fixture");
}
