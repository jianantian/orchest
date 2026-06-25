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

    assert_eq!(err.code, AsrErrorCode::InvalidRequest);
}

#[test]
fn fixture_event_sequence_maps_core_event_classes() {
    let fixture =
        include_str!("../../../../../docs/iteration/v0_9_11/fixtures/fake_event_sequence.json");
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

#[tokio::test]
async fn fake_session_interrupt_is_push_to_talk_only() {
    let (mut session, mut events) = VolcengineRealtimeSession::fake("session-1");
    session.start().await.expect("session starts");

    let unsupported = session
        .interrupt(VolcengineRealtimeInputMode::AudioFile)
        .await
        .expect_err("audio_file interrupt is unsupported");
    assert_eq!(unsupported.code, AsrErrorCode::UnsupportedOperation);

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
