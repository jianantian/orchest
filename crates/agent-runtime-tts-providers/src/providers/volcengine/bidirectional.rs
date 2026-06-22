use std::time::Instant;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{self, Message};

use crate::error::{TtsError, TtsErrorCode};
use crate::observability::TtsTelemetryBuilder;
use crate::streaming::{TtsDuplexStream, TtsStreamEvent};
use crate::types::{
    TextChunk, TtsOperation, TtsStreamSummary, TtsUsage, VoiceCatalogSource, VoiceInfo, VoiceKind,
};

use super::protocol;
use super::{
    audio_format_name, stream_send_error, volcengine_speech_rate, VolcengineSynthesisRequest,
};

pub fn spawn_duplex(request: VolcengineSynthesisRequest) -> TtsDuplexStream {
    let (input_tx, input_rx) = mpsc::channel(16);
    let (event_tx, event_rx) = mpsc::channel(32);
    tokio::spawn(async move {
        if let Err(error) = run_session(request, None, Some(input_rx), event_tx.clone()).await {
            let _ = event_tx
                .send(TtsStreamEvent::Error {
                    trace_id: "volcengine-duplex".to_owned(),
                    error,
                    fatal: true,
                })
                .await;
        }
    });
    TtsDuplexStream::new(input_tx, event_rx)
}

#[allow(clippy::too_many_lines)] // justified: bidirectional WebSocket session loop — event dispatch + reconnect logic is sequential and cannot be meaningfully split
async fn run_session(
    request: VolcengineSynthesisRequest,
    initial_text: Option<Vec<TextChunk>>,
    mut input_rx: Option<mpsc::Receiver<TextChunk>>,
    event_tx: mpsc::Sender<TtsStreamEvent>,
) -> Result<(), TtsError> {
    let ws_key = tungstenite::handshake::client::generate_key();
    let host = tungstenite::http::Uri::try_from(request.bidirectional_ws_url.as_str())
        .ok()
        .and_then(|u| u.authority().map(|a| a.as_str().to_string()))
        .unwrap_or_default();
    let ws_request = tungstenite::http::Request::builder()
        .uri(&request.bidirectional_ws_url)
        .header("Host", host)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", ws_key)
        .header("X-Api-Key", &request.api_key)
        .header("X-Api-Resource-Id", &request.resource_id)
        .header("X-Api-Connect-Id", uuid::Uuid::new_v4().to_string())
        .header("X-Control-Require-Usage-Tokens-Return", "text_words")
        .body(())
        .map_err(|e| {
            TtsError::new(
                TtsErrorCode::InvalidRequest,
                format!("build websocket request: {e}"),
            )
        })?;

    let (ws_stream, response) =
        tokio_tungstenite::connect_async(ws_request)
            .await
            .map_err(|e| {
                TtsError::new(
                    TtsErrorCode::ProviderStreamError,
                    format!("websocket connect failed: {e}"),
                )
            })?;
    let log_id = response
        .headers()
        .get("X-Tt-Logid")
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned);
    let (mut sink, mut source) = ws_stream.split();
    let session_id = uuid::Uuid::new_v4().simple().to_string();
    let provider_metadata = serde_json::json!({"x_tt_logid": log_id});

    // Step 1: send StartConnection, wait for ConnectionStarted (event 50).
    sink.send(Message::Binary(protocol::build_connect_frame(
        protocol::EVENT_START_CONNECTION,
        &serde_json::json!({}),
    )?))
    .await
    .map_err(stream_send_error)?;
    wait_for_connection_started(&mut source).await?;

    // Step 2: send StartSession, wait for SessionStarted (event 150).
    sink.send(Message::Binary(protocol::build_meta_frame(
        protocol::EVENT_START_SESSION,
        &session_id,
        &build_session_payload(&request),
    )?))
    .await
    .map_err(stream_send_error)?;
    wait_for_session_started(&mut source).await?;

    event_tx
        .send(TtsStreamEvent::Started {
            trace_id: request.trace_id.clone(),
            provider: "volcengine".to_owned(),
            model: request.model.clone(),
            voice: make_voice_info(&request),
        })
        .await
        .ok();

    // Step 3: send text (if stream mode) then FinishSession.
    if let Some(chunks) = initial_text {
        for chunk in chunks {
            send_text_chunk(&mut sink, &session_id, &request, &chunk).await?;
        }
        sink.send(Message::Binary(protocol::build_meta_frame(
            protocol::EVENT_FINISH_SESSION,
            &session_id,
            &serde_json::json!({}),
        )?))
        .await
        .map_err(stream_send_error)?;
    }

    let mut audio_seq = 0u64;
    let mut first_audio_at: Option<Instant> = None;
    let started = Instant::now();
    loop {
        tokio::select! {
            maybe_chunk = async {
                match input_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => None,
                }
            }, if input_rx.is_some() => {
                match maybe_chunk {
                    Some(chunk) => {
                        send_text_chunk(&mut sink, &session_id, &request, &chunk).await?;
                        if chunk.is_final {
                            sink.send(Message::Binary(protocol::build_meta_frame(
                                protocol::EVENT_FINISH_SESSION,
                                &session_id,
                                &serde_json::json!({}),
                            )?))
                            .await
                            .map_err(stream_send_error)?;
                        }
                    }
                    None => {
                        sink.send(Message::Binary(protocol::build_meta_frame(
                            protocol::EVENT_CANCEL_SESSION,
                            &session_id,
                            &serde_json::json!({}),
                        )?))
                        .await
                        .ok();
                    }
                }
            }
            next = source.next() => {
                let Some(message) = next else { break; };
                let message = message.map_err(|e| TtsError::new(
                    TtsErrorCode::ProviderStreamError,
                    format!("websocket receive failed: {e}"),
                ))?;
                if let Message::Binary(data) = message {
                    match protocol::parse_frame(&data)? {
                        protocol::VolcengineFrame::Audio { event, data, .. }
                            if event == protocol::EVENT_TTS_RESPONSE =>
                        {
                            if first_audio_at.is_none() {
                                first_audio_at = Some(Instant::now());
                            }
                            event_tx.send(TtsStreamEvent::AudioChunk {
                                trace_id: request.trace_id.clone(),
                                data: Bytes::from(data),
                                format: request.output_format.clone(),
                                sequence: audio_seq,
                            }).await.ok();
                            audio_seq += 1;
                        }
                        protocol::VolcengineFrame::Meta { event, payload, .. }
                            if event == protocol::EVENT_SESSION_FINISHED =>
                        {
                            let duration_ms = payload
                                .get("audio_info")
                                .and_then(|v| v.get("duration"))
                                .and_then(serde_json::Value::as_u64);
                            let input_chars = payload
                                .get("usage")
                                .and_then(|v| v.get("text_words"))
                                .and_then(serde_json::Value::as_u64)
                                .unwrap_or_default();
                            let mut telemetry = TtsTelemetryBuilder::new(
                                request.trace_id.clone(),
                                "volcengine",
                                request.model.clone(),
                                TtsOperation::DuplexStream,
                            )
                            .voice_id(Some(request.voice_id.clone()))
                            .input_chars(input_chars)
                            .build_with_final_latency(started.elapsed());
                            if let Some(first) = first_audio_at {
                                telemetry.first_audio_latency_ms =
                                    Some(first.duration_since(started).as_millis() as u64);
                            }
                            event_tx.send(TtsStreamEvent::Completed {
                                trace_id: request.trace_id.clone(),
                                summary: TtsStreamSummary {
                                    format: request.output_format,
                                    duration_ms,
                                    usage: TtsUsage {
                                        input_chars,
                                        billable_chars: Some(input_chars),
                                        audio_duration_ms: duration_ms,
                                        output_bytes: None,
                                        cost_estimate_micros: None,
                                    },
                                    option_adjustments: Vec::new(),
                                    provider_metadata,
                                    telemetry,
                                },
                            }).await.ok();
                            break;
                        }
                        protocol::VolcengineFrame::Meta { event, payload, .. }
                            if event == protocol::EVENT_SESSION_FAILED
                                || event == protocol::EVENT_CONNECTION_FAILED =>
                        {
                            return Err(TtsError::new(
                                TtsErrorCode::ProviderStreamError,
                                payload
                                    .get("message")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("Volcengine session failed"),
                            )
                            .with_upstream(
                                None,
                                payload.get("status_code").map(ToString::to_string),
                                None,
                                Some(payload),
                            ));
                        }
                        protocol::VolcengineFrame::Error { code, message } => {
                            return Err(TtsError::new(TtsErrorCode::ProviderStreamError, message)
                                .with_upstream(None, Some(code.to_string()), None, None));
                        }
                        _ => {}
                    }
                }
            }
            _ = tokio::time::sleep(request.timeout) => {
                return Err(TtsError::new(
                    TtsErrorCode::Timeout,
                    "Volcengine TTS stream timed out",
                ));
            }
        }
    }
    Ok(())
}

async fn wait_for_connection_started<S>(source: &mut S) -> Result<(), TtsError>
where
    S: futures_util::Stream<Item = Result<Message, tungstenite::Error>> + Unpin,
{
    loop {
        match source.next().await {
            None => {
                return Err(TtsError::new(
                    TtsErrorCode::ProviderStreamError,
                    "websocket closed before ConnectionStarted",
                ))
            }
            Some(Err(e)) => {
                return Err(TtsError::new(
                    TtsErrorCode::ProviderStreamError,
                    format!("ws error: {e}"),
                ))
            }
            Some(Ok(Message::Binary(data))) => match protocol::parse_frame(&data)? {
                protocol::VolcengineFrame::Meta { event, .. }
                    if event == protocol::EVENT_CONNECTION_STARTED =>
                {
                    return Ok(());
                }
                protocol::VolcengineFrame::Meta { event, payload, .. }
                    if event == protocol::EVENT_CONNECTION_FAILED =>
                {
                    return Err(TtsError::new(
                        TtsErrorCode::ProviderStreamError,
                        payload
                            .get("message")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("connection failed"),
                    )
                    .with_upstream(
                        None,
                        payload.get("status_code").map(ToString::to_string),
                        None,
                        Some(payload),
                    ));
                }
                protocol::VolcengineFrame::Error { code, message } => {
                    return Err(TtsError::new(TtsErrorCode::ProviderStreamError, message)
                        .with_upstream(None, Some(code.to_string()), None, None));
                }
                _ => {}
            },
            Some(Ok(_)) => {}
        }
    }
}

async fn wait_for_session_started<S>(source: &mut S) -> Result<(), TtsError>
where
    S: futures_util::Stream<Item = Result<Message, tungstenite::Error>> + Unpin,
{
    loop {
        match source.next().await {
            None => {
                return Err(TtsError::new(
                    TtsErrorCode::ProviderStreamError,
                    "websocket closed before SessionStarted",
                ))
            }
            Some(Err(e)) => {
                return Err(TtsError::new(
                    TtsErrorCode::ProviderStreamError,
                    format!("ws error: {e}"),
                ))
            }
            Some(Ok(Message::Binary(data))) => match protocol::parse_frame(&data)? {
                protocol::VolcengineFrame::Meta { event, .. }
                    if event == protocol::EVENT_SESSION_STARTED =>
                {
                    return Ok(());
                }
                protocol::VolcengineFrame::Meta { event, payload, .. }
                    if event == protocol::EVENT_SESSION_FAILED =>
                {
                    return Err(TtsError::new(
                        TtsErrorCode::ProviderStreamError,
                        payload
                            .get("message")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("session failed"),
                    )
                    .with_upstream(
                        None,
                        payload.get("status_code").map(ToString::to_string),
                        None,
                        Some(payload),
                    ));
                }
                protocol::VolcengineFrame::Error { code, message } => {
                    return Err(TtsError::new(TtsErrorCode::ProviderStreamError, message)
                        .with_upstream(None, Some(code.to_string()), None, None));
                }
                _ => {}
            },
            Some(Ok(_)) => {}
        }
    }
}

async fn send_text_chunk<S>(
    sink: &mut S,
    session_id: &str,
    request: &VolcengineSynthesisRequest,
    chunk: &TextChunk,
) -> Result<(), TtsError>
where
    S: futures_util::Sink<Message, Error = tungstenite::Error> + Unpin,
{
    if chunk.text.is_empty() {
        return Ok(());
    }
    let payload = build_task_request_payload(request, &chunk.text);
    let frame = protocol::build_meta_frame(protocol::EVENT_TASK_REQUEST, session_id, &payload)?;
    sink.send(Message::Binary(frame))
        .await
        .map_err(stream_send_error)
}

/// StartSession payload (no text — text arrives via TaskRequest).
pub fn build_session_payload(request: &VolcengineSynthesisRequest) -> serde_json::Value {
    let additions_json = serde_json::to_string(&serde_json::json!({
        "post_process": { "pitch": request.controls.pitch.round() as i64 }
    }))
    .unwrap_or_default();
    let mut req_params = serde_json::json!({
        "speaker": request.voice_id,
        "audio_params": {
            "format": audio_format_name(&request.output_format),
            "sample_rate": 24000,
            "speech_rate": volcengine_speech_rate(request.controls.speed),
        },
        "additions": additions_json,
    });
    if let Some(options) = request.provider_options.as_object() {
        for (key, value) in options {
            req_params[key] = value.clone();
        }
    }
    serde_json::json!({ "user": {"uid": "orchest-sdk"}, "req_params": req_params })
}

fn build_task_request_payload(
    request: &VolcengineSynthesisRequest,
    text: &str,
) -> serde_json::Value {
    let additions_json = serde_json::to_string(&serde_json::json!({
        "post_process": { "pitch": request.controls.pitch.round() as i64 }
    }))
    .unwrap_or_default();
    serde_json::json!({
        "user": {"uid": "orchest-sdk"},
        "namespace": "BidirectionalTTS",
        "event": protocol::EVENT_TASK_REQUEST,
        "req_params": {
            "text": text,
            "speaker": request.voice_id,
            "audio_params": {
                "format": audio_format_name(&request.output_format),
                "sample_rate": 24000,
                "speech_rate": volcengine_speech_rate(request.controls.speed),
            },
            "additions": additions_json,
        },
    })
}

pub fn make_voice_info(request: &VolcengineSynthesisRequest) -> VoiceInfo {
    VoiceInfo {
        provider: "volcengine".to_owned(),
        model: request.model.clone(),
        id: request.voice_id.clone(),
        display_name: request.voice_id.clone(),
        kind: VoiceKind::System,
        gender: None,
        languages: vec![],
        is_custom: false,
        supports_instruction: false,
        supports_emotion: false,
        supports_style: false,
        supports_cloning: false,
        supports_design: false,
        source: VoiceCatalogSource::ConservativeAssumption,
        provider_metadata: serde_json::Value::Null,
    }
}
