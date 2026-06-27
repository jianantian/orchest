use super::*;

#[test]
fn config_defaults_match_issue_001_decision() {
    let config = VolcengineRealtimeConfig::new("app", "access");

    assert_eq!(config.ws_url, DEFAULT_REALTIME_WS_URL);
    assert_eq!(config.resource_id, DEFAULT_REALTIME_RESOURCE_ID);
    assert_eq!(config.app_key, DEFAULT_REALTIME_APP_KEY);
    assert_eq!(config.model, DEFAULT_REALTIME_MODEL);
    assert_eq!(config.speaker, DEFAULT_REALTIME_SPEAKER);
    assert!(config.validate().is_ok());
}

#[test]
fn start_session_payload_uses_audio_file_and_pcm_output() {
    let config = VolcengineRealtimeConfig::new("app", "access");
    let payload = config.start_session_payload();

    assert_eq!(payload["dialog"]["extra"]["input_mod"], "audio_file");
    assert_eq!(payload["dialog"]["extra"]["model"], DEFAULT_REALTIME_MODEL);
    assert_eq!(payload["tts"]["audio_config"]["format"], "pcm_s16le");
    assert_eq!(payload["tts"]["audio_config"]["sample_rate"], 24000);
}

#[test]
fn config_from_env_accepts_shared_volcengine_aliases() {
    let config = config_from_env_lookup(|name| match name {
        "VOLCENGINE_APP_ID" => Some("shared-app".to_string()),
        "VOLCENGINE_ACCESS_TOKEN" => Some("shared-token".to_string()),
        "VOLCENGINE_REALTIME_RESOURCE_ID" => Some("custom-resource".to_string()),
        _ => None,
    })
    .expect("shared Volcengine env aliases are accepted");

    assert_eq!(config.app_id, "shared-app");
    assert_eq!(config.access_key, "shared-token");
    assert_eq!(config.resource_id, "custom-resource");
}

#[test]
fn config_from_env_rejects_invented_realtime_app_credentials() {
    let err = config_from_env_lookup(|name| match name {
        "VOLCENGINE_REALTIME_APP_ID" => Some("invented-app".to_string()),
        "VOLCENGINE_REALTIME_ACCESS_KEY" => Some("invented-token".to_string()),
        _ => None,
    })
    .expect_err("invented realtime credential names must not be accepted");

    assert!(err.message.contains("VOLCENGINE_APP_ID"));
    assert!(!err.message.contains("VOLCENGINE_REALTIME_APP_ID"));
}

#[test]
fn start_connection_frame_matches_vendor_doc_example() {
    let frame = live::build_connect_json_frame(EVENT_START_CONNECTION, &serde_json::json!({}))
        .expect("StartConnection frame builds");

    assert_eq!(frame, vec![17, 20, 16, 0, 0, 0, 0, 1, 0, 0, 0, 2, 123, 125]);
}

#[test]
fn start_session_frame_places_session_id_before_payload() {
    let session_id = "75a6126e-427f-49a1-a2c1-621143cb9db3";
    let payload = serde_json::json!({
        "dialog": {
            "bot_name": "豆包",
            "dialog_id": "",
            "extra": null
        }
    });

    let frame = live::build_session_json_frame(EVENT_START_SESSION, session_id, &payload)
        .expect("StartSession frame builds");

    assert_eq!(&frame[..8], &[17, 20, 16, 0, 0, 0, 0, 100]);
    assert_eq!(u32::from_be_bytes(frame[8..12].try_into().unwrap()), 36);
    assert_eq!(&frame[12..48], session_id.as_bytes());

    let payload_len = u32::from_be_bytes(frame[48..52].try_into().unwrap()) as usize;
    let parsed: serde_json::Value =
        serde_json::from_slice(&frame[52..52 + payload_len]).expect("payload parses");
    assert_eq!(parsed, payload);
}

#[test]
fn realtime_request_includes_websocket_handshake_and_volcengine_headers() {
    let config = VolcengineRealtimeConfig::new("app", "access");

    let request = live::build_realtime_request(&config, "connect-1").expect("request builds");

    assert!(request.headers().contains_key("sec-websocket-key"));
    assert!(request.headers().contains_key("sec-websocket-version"));
    assert_eq!(request.headers()["X-Api-App-ID"], "app");
    assert_eq!(request.headers()["X-Api-Access-Key"], "access");
    assert_eq!(
        request.headers()["X-Api-Resource-Id"],
        DEFAULT_REALTIME_RESOURCE_ID
    );
    assert_eq!(request.headers()["X-Api-App-Key"], DEFAULT_REALTIME_APP_KEY);
    assert_eq!(request.headers()["X-Api-Connect-Id"], "connect-1");
}

#[tokio::test]
async fn fake_session_accepts_audio_and_closes() {
    let (mut session, mut events) = VolcengineRealtimeSession::fake("session-1");
    let audio = [0_u8; 640];

    session.start().await.expect("session starts");
    session
        .send_audio_chunk(&audio)
        .await
        .expect("audio chunk accepted");
    session.close().await.expect("session closes");

    assert_eq!(session.state(), &VolcengineRealtimeState::Closed);
    assert_eq!(
        events.recv().await,
        Some(VolcengineRealtimeEvent::SessionStarted {
            session_id: "session-1".to_string(),
        })
    );
    assert_eq!(
        events.recv().await,
        Some(VolcengineRealtimeEvent::AudioInputAccepted { bytes: 640 })
    );
    assert_eq!(
        events.recv().await,
        Some(VolcengineRealtimeEvent::SessionClosed {
            session_id: "session-1".to_string(),
        })
    );
}

#[tokio::test]
async fn fake_session_rejects_audio_before_start() {
    let (session, _events) = VolcengineRealtimeSession::fake("session-1");

    let err = session
        .send_audio_chunk(&[1, 2, 3])
        .await
        .expect_err("audio before start fails");

    assert_eq!(err.code, RealtimeErrorCode::InvalidRequest);
}

#[tokio::test]
async fn audio_input_ack_does_not_block_when_receiver_lags() {
    let (mut session, _events) = VolcengineRealtimeSession::fake("session-1");
    let audio = [0_u8; 640];
    session.start().await.expect("session starts");

    let result = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        for _ in 0..80 {
            session
                .send_audio_chunk(&audio)
                .await
                .expect("audio chunk accepted without receiver progress");
        }
    })
    .await;

    assert!(result.is_ok(), "audio send should not block on ack events");
}

#[tokio::test]
async fn live_session_send_audio_does_not_emit_local_ack_events() {
    let (events, mut rx) = tokio::sync::mpsc::channel(16);
    let (commands, _command_rx) = tokio::sync::mpsc::channel(16);
    let session = VolcengineRealtimeSession {
        session_id: "session-1".to_string(),
        handshake_log_id: None,
        state: VolcengineRealtimeState::Started,
        events,
        commands: Some(commands),
    };

    session
        .send_audio_chunk(&[0_u8; 640])
        .await
        .expect("live audio chunk sends");

    assert!(
        rx.try_recv().is_err(),
        "live audio send should not emit local ack"
    );
}

#[test]
fn fixture_event_sequence_maps_core_event_classes() {
    let fixture = include_str!(
        "../../../../../../docs/archive/iteration/v0_9_11/fixtures/fake_event_sequence.json"
    );
    let events: Vec<VolcengineRealtimeFixtureEvent> =
        serde_json::from_str(fixture).expect("fixture event JSON parses");
    let mapped: Vec<VolcengineRealtimeMappedEvent> = events
        .into_iter()
        .map(|event| {
            let audio = (event.event_id == 352).then(|| vec![0_u8; 640]);
            map_realtime_server_event(event.event_id, event.name, event.payload, audio)
        })
        .collect();

    assert!(matches!(
        mapped.first(),
        Some(VolcengineRealtimeMappedEvent::Lifecycle { event_id: 50, .. })
    ));
    assert!(mapped.iter().any(|event| matches!(
        event,
        VolcengineRealtimeMappedEvent::Transcript {
            text,
            is_interim: true,
            ..
        } if text == "你好"
    )));
    assert!(mapped.iter().any(|event| matches!(
        event,
        VolcengineRealtimeMappedEvent::ModelText { content, .. }
            if content.contains("Orchest realtime fixture")
    )));
    assert!(mapped.iter().any(|event| matches!(
        event,
        VolcengineRealtimeMappedEvent::AudioOutput { bytes, .. } if bytes.len() == 640
    )));
}

#[test]
fn usage_response_maps_to_metadata() {
    let mapped = map_realtime_server_event(
        154,
        "UsageResponse",
        serde_json::json!({
            "usage": {
                "input_text_tokens": 0,
                "input_audio_tokens": 5,
                "output_text_tokens": 3,
                "output_audio_tokens": 8
            }
        }),
        None,
    );

    assert!(matches!(
        mapped,
        VolcengineRealtimeMappedEvent::Metadata {
            event_id: 154,
            name,
            payload,
        } if name == "UsageResponse" && payload["usage"]["input_audio_tokens"] == 5
    ));
}

#[tokio::test]
async fn fake_session_interrupt_is_push_to_talk_only() {
    let (mut session, mut events) = VolcengineRealtimeSession::fake("session-1");
    session.start().await.expect("session starts");

    let unsupported = session
        .interrupt(VolcengineRealtimeInputMode::AudioFile)
        .await
        .expect_err("audio_file interrupt is unsupported");
    assert_eq!(unsupported.code, RealtimeErrorCode::UnsupportedOperation);

    session
        .interrupt(VolcengineRealtimeInputMode::PushToTalk)
        .await
        .expect("push_to_talk interrupt is accepted");

    assert_eq!(
        events.recv().await,
        Some(VolcengineRealtimeEvent::SessionStarted {
            session_id: "session-1".to_string(),
        })
    );
    assert_eq!(
        events.recv().await,
        Some(VolcengineRealtimeEvent::ClientInterrupted {
            session_id: "session-1".to_string(),
        })
    );
}

#[test]
fn realtime_error_classification_is_debuggable() {
    assert_eq!(
        classify_realtime_error(Some(401), "invalid token"),
        VolcengineRealtimeErrorCategory::Authentication
    );
    assert_eq!(
        classify_realtime_error(None, "websocket timeout"),
        VolcengineRealtimeErrorCategory::Transport
    );
    assert_eq!(
        classify_realtime_error(None, "bad frame payload"),
        VolcengineRealtimeErrorCategory::Protocol
    );
    assert_eq!(
        classify_realtime_error(Some(50000000), "AudioQueryError"),
        VolcengineRealtimeErrorCategory::Provider
    );
}

#[tokio::test]
#[ignore = "requires Volcengine realtime credentials and network access"]
async fn live_realtime_session_can_send_audio_and_receive_events() {
    let config = VolcengineRealtimeConfig::from_env().expect("Volcengine realtime env configured");
    let (mut session, mut events) = VolcengineRealtimeSession::connect_live(config)
        .await
        .expect("live realtime session connects");
    let audio = [0_u8; 640];

    session
        .send_audio_chunk(&audio)
        .await
        .expect("live audio chunk sent");

    let event = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
        .await
        .expect("live realtime event received within timeout");
    assert!(event.is_some(), "provider returned at least one event");

    session.close().await.expect("live session closes");
}
