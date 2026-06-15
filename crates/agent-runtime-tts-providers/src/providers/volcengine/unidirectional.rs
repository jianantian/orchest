use std::time::Instant;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{self, Message};

use crate::error::{TtsError, TtsErrorCode};
use crate::observability::TtsTelemetryBuilder;
use crate::streaming::{TtsOutputStream, TtsStreamEvent};
use crate::types::{TtsOperation, TtsStreamSummary, TtsUsage};

use super::bidirectional::make_voice_info;
use super::protocol;
use super::{
    audio_format_name, stream_send_error, volcengine_speech_rate, VolcengineSynthesisRequest,
};

pub fn spawn_stream(request: VolcengineSynthesisRequest, text: String) -> TtsOutputStream {
    let (event_tx, event_rx) = mpsc::channel(32);
    tokio::spawn(async move {
        if let Err(error) = run_stream(request, text, event_tx.clone()).await {
            let _ = event_tx
                .send(TtsStreamEvent::Error {
                    trace_id: "volcengine-unistream".to_owned(),
                    error,
                    fatal: true,
                })
                .await;
        }
    });
    TtsOutputStream::new(event_rx)
}

async fn run_stream(
    request: VolcengineSynthesisRequest,
    text: String,
    event_tx: mpsc::Sender<TtsStreamEvent>,
) -> Result<(), TtsError> {
    let ws_key = tungstenite::handshake::client::generate_key();
    let host = tungstenite::http::Uri::try_from(request.unidirectional_ws_url.as_str())
        .ok()
        .and_then(|u| u.authority().map(|a| a.as_str().to_string()))
        .unwrap_or_default();
    let ws_request = tungstenite::http::Request::builder()
        .uri(&request.unidirectional_ws_url)
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
    let provider_metadata = serde_json::json!({"x_tt_logid": log_id});
    let (mut sink, mut source) = ws_stream.split();

    // Send text: unidirectional frame has no event number in byte[1].
    let payload = build_payload(&request, &text);
    let payload_bytes = serde_json::to_vec(&payload).map_err(|e| {
        TtsError::new(
            TtsErrorCode::InvalidRequest,
            format!("serialize unidirectional payload: {e}"),
        )
    })?;
    sink.send(Message::Binary(build_send_frame(&payload_bytes)))
        .await
        .map_err(stream_send_error)?;

    event_tx
        .send(TtsStreamEvent::Started {
            trace_id: request.trace_id.clone(),
            provider: "volcengine".to_owned(),
            model: request.model.clone(),
            voice: make_voice_info(&request),
        })
        .await
        .ok();

    let mut audio_seq = 0u64;
    let mut first_audio_at: Option<Instant> = None;
    let started = Instant::now();

    loop {
        tokio::select! {
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
                                .unwrap_or_else(|| text.chars().count() as u64);
                            let mut telemetry = TtsTelemetryBuilder::new(
                                request.trace_id.clone(),
                                "volcengine",
                                request.model.clone(),
                                TtsOperation::SingleStream,
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
                        protocol::VolcengineFrame::Meta { event, .. }
                            if event == protocol::EVENT_CONNECTION_FINISHED =>
                        {
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
                                    .unwrap_or("Volcengine unidirectional stream failed"),
                            )
                            .with_upstream(
                                None,
                                payload.get("status_code").map(ToString::to_string),
                                None,
                                Some(payload),
                            ));
                        }
                        protocol::VolcengineFrame::Error { code, message } => {
                            return Err(
                                TtsError::new(TtsErrorCode::ProviderStreamError, message)
                                    .with_upstream(None, Some(code.to_string()), None, None),
                            );
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

/// Unidirectional send frame: byte[1] = MSG_FULL_CLIENT_REQUEST << 4 (no FLAG_WITH_EVENT).
/// Layout: 4-byte header | 4-byte payload length | payload
fn build_send_frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(&[
        0b0001_0001,
        protocol::MSG_FULL_CLIENT_REQUEST << 4, // 0x10 — no event flag
        protocol::SER_JSON << 4,
        0,
    ]);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

fn build_payload(request: &VolcengineSynthesisRequest, text: &str) -> serde_json::Value {
    let additions_json = serde_json::to_string(&serde_json::json!({
        "post_process": { "pitch": request.controls.pitch.round() as i64 }
    }))
    .unwrap_or_default();
    let mut req_params = serde_json::json!({
        "text": text,
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
