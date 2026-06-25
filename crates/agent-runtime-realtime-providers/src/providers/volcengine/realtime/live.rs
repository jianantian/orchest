use std::time::Duration;

use futures_util::{SinkExt, Stream, StreamExt};
use serde_json::Value;
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest, http::HeaderValue, Message};

use super::*;

const START_ACK_TIMEOUT: Duration = Duration::from_secs(10);

impl VolcengineRealtimeSession {
    pub async fn connect_live(
        config: VolcengineRealtimeConfig,
    ) -> Result<(Self, mpsc::Receiver<VolcengineRealtimeEvent>), RealtimeError> {
        config.validate()?;
        let connect_id = config.effective_connect_id();
        let session_id = Uuid::new_v4().to_string();
        let request = build_realtime_request(&config, &connect_id)?;
        let (ws, response) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| {
                RealtimeError::new(
                    RealtimeErrorCode::ProviderStreamError,
                    format!("Volcengine realtime WebSocket connect failed: {e}"),
                )
            })?;
        let handshake_log_id = response
            .headers()
            .get("X-Tt-Logid")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let (mut write, mut read) = ws.split();
        let (events, rx) = mpsc::channel(64);

        events
            .send(VolcengineRealtimeEvent::Handshake {
                log_id: handshake_log_id.clone(),
            })
            .await
            .map_err(|_| {
                RealtimeError::new(
                    RealtimeErrorCode::ProviderStreamError,
                    "Volcengine realtime event receiver was dropped",
                )
            })?;

        write
            .send(Message::Binary(build_connect_json_frame(
                EVENT_START_CONNECTION,
                &serde_json::json!({}),
            )?))
            .await
            .map_err(stream_send_error)?;
        wait_for_lifecycle_event(&mut read, &session_id, &events, 50, "ConnectionStarted").await?;

        write
            .send(Message::Binary(build_session_json_frame(
                EVENT_START_SESSION,
                &session_id,
                &config.start_session_payload(),
            )?))
            .await
            .map_err(stream_send_error)?;
        wait_for_lifecycle_event(&mut read, &session_id, &events, 150, "SessionStarted").await?;

        let (commands, mut command_rx) = mpsc::channel(16);
        let read_events = events.clone();
        let read_session_id = session_id.clone();
        tokio::spawn(async move {
            while let Some(message) = read.next().await {
                let event = parse_realtime_message(message, &read_session_id);
                match event {
                    Ok(Some(event)) => {
                        if read_events.send(event).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        if read_events
                            .send(VolcengineRealtimeEvent::ProviderError {
                                message: err.message,
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });

        let command_session_id = session_id.clone();
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                let frame = match command {
                    VolcengineRealtimeCommand::Audio(bytes) => {
                        build_session_audio_frame(EVENT_TASK_REQUEST, &command_session_id, &bytes)
                    }
                    VolcengineRealtimeCommand::Interrupt => build_session_json_frame(
                        EVENT_CLIENT_INTERRUPT,
                        &command_session_id,
                        &serde_json::json!({}),
                    ),
                    VolcengineRealtimeCommand::CloseSession => build_session_json_frame(
                        EVENT_FINISH_SESSION,
                        &command_session_id,
                        &serde_json::json!({}),
                    ),
                    VolcengineRealtimeCommand::CloseConnection => {
                        build_connect_json_frame(EVENT_FINISH_CONNECTION, &serde_json::json!({}))
                    }
                };
                match frame {
                    Ok(frame) => {
                        if write.send(Message::Binary(frame)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok((
            Self {
                session_id,
                handshake_log_id,
                state: VolcengineRealtimeState::Started,
                events,
                commands: Some(commands),
            },
            rx,
        ))
    }
}

async fn wait_for_lifecycle_event<S>(
    read: &mut S,
    session_id: &str,
    events: &mpsc::Sender<VolcengineRealtimeEvent>,
    expected_event_id: u16,
    expected_name: &str,
) -> Result<(), RealtimeError>
where
    S: Stream<Item = Result<Message, tungstenite::Error>> + Unpin,
{
    loop {
        let event = tokio::time::timeout(
            START_ACK_TIMEOUT,
            read_next_realtime_event(read, session_id),
        )
        .await
        .map_err(|_| {
            RealtimeError::new(
                RealtimeErrorCode::ProviderStreamError,
                format!("timed out waiting for Volcengine realtime {expected_name}"),
            )
        })?
        .ok_or_else(|| {
            RealtimeError::new(
                RealtimeErrorCode::ProviderStreamError,
                format!("Volcengine realtime stream closed before {expected_name}"),
            )
        })??;

        let is_expected = matches!(
            &event,
            VolcengineRealtimeEvent::ServerEvent {
                event: VolcengineRealtimeMappedEvent::Lifecycle { event_id, .. },
            } if *event_id == expected_event_id
        );
        let error = match &event {
            VolcengineRealtimeEvent::ProviderError { message } => Some(message.clone()),
            VolcengineRealtimeEvent::ServerEvent {
                event: VolcengineRealtimeMappedEvent::Error { message, .. },
            } => Some(message.clone()),
            _ => None,
        };

        events.send(event).await.map_err(|_| {
            RealtimeError::new(
                RealtimeErrorCode::ProviderStreamError,
                "Volcengine realtime event receiver was dropped",
            )
        })?;

        if let Some(message) = error {
            return Err(RealtimeError::new(
                RealtimeErrorCode::ProviderStreamError,
                format!("Volcengine realtime failed before {expected_name}: {message}"),
            ));
        }
        if is_expected {
            return Ok(());
        }
    }
}

async fn read_next_realtime_event<S>(
    read: &mut S,
    session_id: &str,
) -> Option<Result<VolcengineRealtimeEvent, RealtimeError>>
where
    S: Stream<Item = Result<Message, tungstenite::Error>> + Unpin,
{
    read.next()
        .await
        .map(|message| parse_realtime_message(message, session_id).and_then(required_event))
}

fn required_event(
    event: Option<VolcengineRealtimeEvent>,
) -> Result<VolcengineRealtimeEvent, RealtimeError> {
    event.ok_or_else(|| {
        RealtimeError::new(
            RealtimeErrorCode::ProviderStreamError,
            "Volcengine realtime received a non-binary control message",
        )
    })
}

fn parse_realtime_message(
    message: Result<Message, tungstenite::Error>,
    session_id: &str,
) -> Result<Option<VolcengineRealtimeEvent>, RealtimeError> {
    match message {
        Ok(Message::Binary(bytes)) => parse_realtime_frame(&bytes),
        Ok(Message::Close(_)) => Ok(Some(VolcengineRealtimeEvent::SessionClosed {
            session_id: session_id.to_string(),
        })),
        Ok(_) => Ok(None),
        Err(err) => Ok(Some(VolcengineRealtimeEvent::ProviderError {
            message: format!("Volcengine realtime read failed: {err}"),
        })),
    }
}

pub(super) fn build_realtime_request(
    config: &VolcengineRealtimeConfig,
    connect_id: &str,
) -> Result<tungstenite::http::Request<()>, RealtimeError> {
    let mut request = config.ws_url.as_str().into_client_request().map_err(|e| {
        RealtimeError::new(
            RealtimeErrorCode::InvalidRequest,
            format!("Volcengine realtime request build failed: {e}"),
        )
    })?;
    insert_header(&mut request, "X-Api-App-ID", &config.app_id)?;
    insert_header(&mut request, "X-Api-Access-Key", &config.access_key)?;
    insert_header(&mut request, "X-Api-Resource-Id", &config.resource_id)?;
    insert_header(&mut request, "X-Api-App-Key", &config.app_key)?;
    insert_header(&mut request, "X-Api-Connect-Id", connect_id)?;
    Ok(request)
}

fn insert_header(
    request: &mut tungstenite::http::Request<()>,
    name: &'static str,
    value: &str,
) -> Result<(), RealtimeError> {
    let value = HeaderValue::from_str(value).map_err(|e| {
        RealtimeError::new(
            RealtimeErrorCode::InvalidRequest,
            format!("Volcengine realtime header {name} is invalid: {e}"),
        )
    })?;
    request.headers_mut().insert(name, value);
    Ok(())
}

fn stream_send_error(err: tungstenite::Error) -> RealtimeError {
    RealtimeError::new(
        RealtimeErrorCode::ProviderStreamError,
        format!("Volcengine realtime WebSocket send failed: {err}"),
    )
}

fn build_header(msg_type: u8, flags: u8, serialization: u8) -> [u8; 4] {
    [
        0x11,
        (msg_type << 4) | flags,
        (serialization << 4) | COMP_NONE,
        0x00,
    ]
}

pub(super) fn build_connect_json_frame(
    event_id: u32,
    payload: &Value,
) -> Result<Vec<u8>, RealtimeError> {
    let payload = serde_json::to_vec(payload).map_err(|e| {
        RealtimeError::new(
            RealtimeErrorCode::InvalidRequest,
            format!("Volcengine realtime JSON serialization failed: {e}"),
        )
    })?;
    let mut frame = Vec::with_capacity(12 + payload.len());
    frame.extend_from_slice(&build_header(MSG_FULL_CLIENT_REQUEST, FLAG_EVENT, SER_JSON));
    frame.extend_from_slice(&event_id.to_be_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub(super) fn build_session_json_frame(
    event_id: u32,
    session_id: &str,
    payload: &Value,
) -> Result<Vec<u8>, RealtimeError> {
    let payload = serde_json::to_vec(payload).map_err(|e| {
        RealtimeError::new(
            RealtimeErrorCode::InvalidRequest,
            format!("Volcengine realtime JSON serialization failed: {e}"),
        )
    })?;
    let mut frame = Vec::with_capacity(16 + session_id.len() + payload.len());
    frame.extend_from_slice(&build_header(MSG_FULL_CLIENT_REQUEST, FLAG_EVENT, SER_JSON));
    frame.extend_from_slice(&event_id.to_be_bytes());
    frame.extend_from_slice(&(session_id.len() as u32).to_be_bytes());
    frame.extend_from_slice(session_id.as_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

fn build_session_audio_frame(
    event_id: u32,
    session_id: &str,
    audio: &[u8],
) -> Result<Vec<u8>, RealtimeError> {
    if audio.is_empty() {
        return Err(RealtimeError::new(
            RealtimeErrorCode::InvalidAudio,
            "audio chunk must not be empty",
        ));
    }
    let mut frame = Vec::with_capacity(16 + session_id.len() + audio.len());
    frame.extend_from_slice(&build_header(MSG_AUDIO_ONLY_REQUEST, FLAG_EVENT, SER_NONE));
    frame.extend_from_slice(&event_id.to_be_bytes());
    frame.extend_from_slice(&(session_id.len() as u32).to_be_bytes());
    frame.extend_from_slice(session_id.as_bytes());
    frame.extend_from_slice(&(audio.len() as u32).to_be_bytes());
    frame.extend_from_slice(audio);
    Ok(frame)
}

fn parse_realtime_frame(data: &[u8]) -> Result<Option<VolcengineRealtimeEvent>, RealtimeError> {
    if data.len() < 12 {
        return Err(RealtimeError::new(
            RealtimeErrorCode::ProviderStreamError,
            "Volcengine realtime frame is too short",
        ));
    }
    let msg_type = (data[1] >> 4) & 0x0F;
    let flags = data[1] & 0x0F;
    let mut offset = 4;
    if msg_type == MSG_ERROR_RESPONSE {
        let code = read_u32(data, &mut offset)?;
        let payload = read_payload(data, &mut offset)?;
        let message = String::from_utf8_lossy(payload).to_string();
        return Ok(Some(VolcengineRealtimeEvent::ProviderError {
            message: format!("Volcengine realtime error {code}: {message}"),
        }));
    }
    if !matches!(msg_type, MSG_FULL_SERVER_RESPONSE | MSG_AUDIO_ONLY_RESPONSE) {
        return Err(RealtimeError::new(
            RealtimeErrorCode::ProviderStreamError,
            format!("unexpected Volcengine realtime message type: {msg_type}"),
        ));
    }
    let event_id = if flags == FLAG_EVENT {
        read_u32(data, &mut offset)? as u16
    } else if msg_type == MSG_AUDIO_ONLY_RESPONSE {
        352
    } else {
        0
    };
    skip_optional_session_id(data, &mut offset)?;
    let payload = read_payload(data, &mut offset)?;
    let mapped = if msg_type == MSG_AUDIO_ONLY_RESPONSE {
        map_realtime_server_event(
            event_id,
            realtime_event_name(event_id),
            Value::Null,
            Some(payload.to_vec()),
        )
    } else {
        let payload_json = if payload.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(payload).unwrap_or(Value::Null)
        };
        map_realtime_server_event(event_id, realtime_event_name(event_id), payload_json, None)
    };
    Ok(Some(VolcengineRealtimeEvent::from(mapped)))
}

fn read_u32(data: &[u8], offset: &mut usize) -> Result<u32, RealtimeError> {
    if data.len() < *offset + 4 {
        return Err(RealtimeError::new(
            RealtimeErrorCode::ProviderStreamError,
            "Volcengine realtime frame ended before u32 field",
        ));
    }
    let value = u32::from_be_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]);
    *offset += 4;
    Ok(value)
}

fn skip_optional_session_id(data: &[u8], offset: &mut usize) -> Result<(), RealtimeError> {
    if data.len() < *offset + 8 {
        return Ok(());
    }
    let mut probe = *offset;
    let session_len = read_u32(data, &mut probe)? as usize;
    if session_len == 0 || session_len > 128 || data.len() < probe + session_len + 4 {
        return Ok(());
    }
    *offset = probe + session_len;
    Ok(())
}

fn read_payload<'a>(data: &'a [u8], offset: &mut usize) -> Result<&'a [u8], RealtimeError> {
    let payload_len = read_u32(data, offset)? as usize;
    if data.len() < *offset + payload_len {
        return Err(RealtimeError::new(
            RealtimeErrorCode::ProviderStreamError,
            "Volcengine realtime frame payload is truncated",
        ));
    }
    let payload = &data[*offset..*offset + payload_len];
    *offset += payload_len;
    Ok(payload)
}

fn realtime_event_name(event_id: u16) -> String {
    match event_id {
        50 => "ConnectionStarted",
        51 => "ConnectionFailed",
        52 => "ConnectionFinished",
        150 => "SessionStarted",
        152 => "SessionFinished",
        153 => "SessionFailed",
        154 => "UsageResponse",
        350 => "TTSSentenceStart",
        351 => "TTSSentenceEnd",
        352 => "TTSResponse",
        359 => "TTSEnded",
        450 => "ASRInfo",
        451 => "ASRResponse",
        459 => "ASREnded",
        550 => "ChatResponse",
        559 => "ChatEnded",
        _ => "Unknown",
    }
    .to_string()
}

impl From<VolcengineRealtimeMappedEvent> for VolcengineRealtimeEvent {
    fn from(event: VolcengineRealtimeMappedEvent) -> Self {
        Self::ServerEvent { event }
    }
}
