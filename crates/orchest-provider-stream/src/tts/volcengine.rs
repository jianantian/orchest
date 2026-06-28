//! Volcengine openspeech **TTS** event-framing codec, on the shared
//! [`crate::openspeech`] core (Issue 006). Ported from
//! `agent-runtime-tts-providers`'s `providers/volcengine/protocol.rs` — same
//! event-tagged frame layout, now over the unified header/constants and the
//! spine [`ProtocolError`] instead of a provider-local protocol + `TtsError`.
//!
//! TTS uses the `FLAG_WITH_EVENT` framing: after the 4-byte header come an
//! `event` (i32), then — for session/task frames — a length-prefixed
//! `session_id`, then the length-prefixed payload (or raw audio).

use bytes::Bytes;
use orchest_protocol::{AudioFormat, ErrorCode, ProtocolError, StreamEvent, TokenUsage};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::asr::volcengine::ByteDuplex;
use crate::openspeech::{
    build_header, parse_header, COMP_NONE, FLAG_WITH_EVENT, MSG_AUDIO_ONLY_RESPONSE,
    MSG_ERROR_RESPONSE, MSG_FULL_CLIENT_REQUEST, MSG_FULL_SERVER_RESPONSE, SER_JSON,
};

// openspeech TTS event numbers.
pub const EVENT_START_CONNECTION: i32 = 1;
pub const EVENT_CONNECTION_STARTED: i32 = 50;
pub const EVENT_CONNECTION_FAILED: i32 = 51;
pub const EVENT_CONNECTION_FINISHED: i32 = 52;
pub const EVENT_START_SESSION: i32 = 100;
pub const EVENT_CANCEL_SESSION: i32 = 101;
pub const EVENT_FINISH_SESSION: i32 = 102;
pub const EVENT_SESSION_STARTED: i32 = 150;
pub const EVENT_SESSION_FINISHED: i32 = 152;
pub const EVENT_SESSION_FAILED: i32 = 153;
pub const EVENT_TASK_REQUEST: i32 = 200;
pub const EVENT_TTS_RESPONSE: i32 = 352;

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn stream_err(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(ErrorCode::ProviderStreamError, message)
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn serialize(payload: &Value) -> Result<Vec<u8>, ProtocolError> {
    serde_json::to_vec(payload)
        .map_err(|e| ProtocolError::new(ErrorCode::InvalidRequest, format!("serialize: {e}")))
}

#[derive(Debug)]
pub enum VolcengineFrame {
    Meta {
        event: i32,
        session_id: String,
        payload: Value,
    },
    Audio {
        event: i32,
        session_id: String,
        data: Vec<u8>,
    },
    Error {
        code: u32,
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Frame building
// ---------------------------------------------------------------------------

/// Connection-level frame (StartConnection / FinishConnection): event, no session_id.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn build_connect_frame(event: i32, payload: &Value) -> Result<Vec<u8>, ProtocolError> {
    let payload = serialize(payload)?;
    let mut out = build_header(
        MSG_FULL_CLIENT_REQUEST,
        FLAG_WITH_EVENT,
        SER_JSON,
        COMP_NONE,
    )
    .to_vec();
    out.extend_from_slice(&event.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Session / task frame (StartSession, FinishSession, TaskRequest): event +
/// session_id + payload.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn build_meta_frame(
    event: i32,
    session_id: &str,
    payload: &Value,
) -> Result<Vec<u8>, ProtocolError> {
    let payload = serialize(payload)?;
    let session = session_id.as_bytes();
    let mut out = build_header(
        MSG_FULL_CLIENT_REQUEST,
        FLAG_WITH_EVENT,
        SER_JSON,
        COMP_NONE,
    )
    .to_vec();
    out.extend_from_slice(&event.to_be_bytes());
    out.extend_from_slice(&(session.len() as u32).to_be_bytes());
    out.extend_from_slice(session);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Frame parsing
// ---------------------------------------------------------------------------

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn parse_frame(data: &[u8]) -> Result<VolcengineFrame, ProtocolError> {
    if data.len() < 8 {
        return Err(stream_err("Volcengine frame too short"));
    }
    let header = parse_header(data)?;
    match header.msg_type {
        MSG_FULL_SERVER_RESPONSE => {
            let (event, session_id, payload_bytes) = parse_event_session_payload(data)?;
            let payload = serde_json::from_slice(&payload_bytes)
                .map_err(|e| stream_err(format!("parse Volcengine meta JSON: {e}")))?;
            Ok(VolcengineFrame::Meta {
                event,
                session_id,
                payload,
            })
        }
        MSG_AUDIO_ONLY_RESPONSE => {
            let (event, session_id, data) = parse_event_session_payload(data)?;
            Ok(VolcengineFrame::Audio {
                event,
                session_id,
                data,
            })
        }
        MSG_ERROR_RESPONSE => parse_error(data),
        other => Err(stream_err(format!(
            "unexpected Volcengine frame type {other}"
        ))),
    }
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_event_session_payload(data: &[u8]) -> Result<(i32, String, Vec<u8>), ProtocolError> {
    if data.len() < 12 {
        return Err(stream_err("Volcengine event frame too short"));
    }
    let event = i32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let session_len = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let session_start = 12;
    let payload_len_start = session_start + session_len;
    if data.len() < payload_len_start + 4 {
        return Err(stream_err("Volcengine frame missing payload length"));
    }
    let session_id = String::from_utf8_lossy(&data[session_start..payload_len_start]).to_string();
    let payload_len = u32::from_be_bytes([
        data[payload_len_start],
        data[payload_len_start + 1],
        data[payload_len_start + 2],
        data[payload_len_start + 3],
    ]) as usize;
    let payload_start = payload_len_start + 4;
    if data.len() < payload_start + payload_len {
        return Err(stream_err("Volcengine frame payload truncated"));
    }
    Ok((
        event,
        session_id,
        data[payload_start..payload_start + payload_len].to_vec(),
    ))
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_error(data: &[u8]) -> Result<VolcengineFrame, ProtocolError> {
    if data.len() < 12 {
        return Err(stream_err("Volcengine error frame too short"));
    }
    let code = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let len = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let message = if data.len() >= 12 + len {
        String::from_utf8_lossy(&data[12..12 + len]).to_string()
    } else {
        String::new()
    };
    Ok(VolcengineFrame::Error { code, message })
}

// ---------------------------------------------------------------------------
// Frame → unified event mapping
// ---------------------------------------------------------------------------

/// Project a decoded TTS frame onto a unified [`StreamEvent`]: synthesized audio
/// (`pcm_s16le` @ 24 kHz) becomes `AudioDelta`; an error frame a fatal `Error`;
/// session/lifecycle `Meta` frames carry no content event.
pub fn map_frame(frame: VolcengineFrame) -> Option<StreamEvent> {
    match frame {
        VolcengineFrame::Audio { data, .. } => Some(StreamEvent::AudioDelta {
            data: Bytes::from(data),
            format: AudioFormat::Pcm16Le,
            sequence: 0,
        }),
        VolcengineFrame::Error { code, message } => Some(StreamEvent::Error {
            error: ProtocolError::new(
                ErrorCode::ProviderTaskFailed,
                format!("Volcengine TTS error {code}: {message}"),
            ),
            fatal: true,
        }),
        VolcengineFrame::Meta { .. } => None,
    }
}

// ---------------------------------------------------------------------------
// Unidirectional synthesis loop (transport-agnostic; live WS is a thin adapter)
// ---------------------------------------------------------------------------

/// Drive one unidirectional TTS synthesis over `transport`: send the prebuilt
/// client `request_frame` (text + voice config), then stream server frames —
/// audio chunks become `AudioDelta`, the terminal `SESSION_FINISHED` emits a
/// final `Done`, and an error frame a fatal `Error`. Generic over [`ByteDuplex`]
/// so the synthesis behavior is testable without a network (the live
/// `tokio-tungstenite` WebSocket is the same thin adapter the ASR path uses).
pub async fn run_tts_synthesis<T: ByteDuplex>(
    mut transport: T,
    request_frame: Vec<u8>,
    events: mpsc::Sender<StreamEvent>,
) {
    if transport.send(request_frame).await.is_err() {
        return;
    }
    while let Some(bytes) = transport.recv().await {
        match parse_frame(&bytes) {
            Ok(frame) => {
                let finished = matches!(
                    &frame,
                    VolcengineFrame::Meta { event, .. } if *event == EVENT_SESSION_FINISHED
                );
                if let Some(event) = map_frame(frame) {
                    if events.send(event).await.is_err() {
                        return;
                    }
                }
                if finished {
                    let _ = events
                        .send(StreamEvent::Done {
                            usage: TokenUsage::default(),
                        })
                        .await;
                    break;
                }
            }
            Err(error) => {
                let _ = events.send(StreamEvent::Error { error, fatal: true }).await;
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::json;

    #[test]
    fn connect_frame_header_and_event() {
        let frame = build_connect_frame(EVENT_START_CONNECTION, &json!({})).unwrap();
        let header = parse_header(&frame).unwrap();
        assert_eq!(header.msg_type, MSG_FULL_CLIENT_REQUEST);
        assert_eq!(header.flags, FLAG_WITH_EVENT);
        let event = i32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]);
        assert_eq!(event, EVENT_START_CONNECTION);
    }

    fn server_meta_frame(event: i32, session_id: &str, payload: &Value) -> Vec<u8> {
        // The server echoes meta as a FULL_SERVER_RESPONSE (what parse_frame
        // decodes); build_meta_frame produces the CLIENT side and is checked
        // separately via parse_header.
        let payload = serde_json::to_vec(payload).unwrap();
        let mut out = build_header(
            MSG_FULL_SERVER_RESPONSE,
            FLAG_WITH_EVENT,
            SER_JSON,
            COMP_NONE,
        )
        .to_vec();
        out.extend_from_slice(&event.to_be_bytes());
        out.extend_from_slice(&(session_id.len() as u32).to_be_bytes());
        out.extend_from_slice(session_id.as_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(&payload);
        out
    }

    #[test]
    fn client_meta_frame_is_well_formed() {
        let frame =
            build_meta_frame(EVENT_START_SESSION, "sess-42", &json!({"speaker": "vv"})).unwrap();
        let header = parse_header(&frame).unwrap();
        assert_eq!(header.msg_type, MSG_FULL_CLIENT_REQUEST);
        assert_eq!(header.flags, FLAG_WITH_EVENT);
        assert_eq!(
            i32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]),
            EVENT_START_SESSION
        );
    }

    #[test]
    fn server_meta_frame_roundtrips_through_parse() {
        let frame = server_meta_frame(EVENT_SESSION_STARTED, "sess-42", &json!({"speaker": "vv"}));
        match parse_frame(&frame).unwrap() {
            VolcengineFrame::Meta {
                event,
                session_id,
                payload,
            } => {
                assert_eq!(event, EVENT_SESSION_STARTED);
                assert_eq!(session_id, "sess-42");
                assert_eq!(payload["speaker"], "vv");
            }
            _ => panic!("expected Meta"),
        }
    }

    fn audio_frame(session_id: &str, audio: &[u8]) -> Vec<u8> {
        // Build a server audio-only response frame the way the wire does.
        let mut out = build_header(
            MSG_AUDIO_ONLY_RESPONSE,
            FLAG_WITH_EVENT,
            SER_JSON,
            COMP_NONE,
        )
        .to_vec();
        out.extend_from_slice(&EVENT_TTS_RESPONSE.to_be_bytes());
        out.extend_from_slice(&(session_id.len() as u32).to_be_bytes());
        out.extend_from_slice(session_id.as_bytes());
        out.extend_from_slice(&(audio.len() as u32).to_be_bytes());
        out.extend_from_slice(audio);
        out
    }

    #[test]
    fn parse_and_map_audio_response() {
        let frame = audio_frame("sess-1", b"pcm-bytes");
        match parse_frame(&frame).unwrap() {
            VolcengineFrame::Audio {
                event,
                session_id,
                data,
            } => {
                assert_eq!(event, EVENT_TTS_RESPONSE);
                assert_eq!(session_id, "sess-1");
                assert_eq!(data, b"pcm-bytes");
            }
            _ => panic!("expected Audio"),
        }
        let mapped = map_frame(parse_frame(&frame).unwrap());
        assert!(matches!(
            mapped,
            Some(StreamEvent::AudioDelta {
                format: AudioFormat::Pcm16Le,
                ..
            })
        ));
    }

    #[test]
    fn parse_and_map_error() {
        let mut frame =
            build_header(MSG_ERROR_RESPONSE, FLAG_WITH_EVENT, SER_JSON, COMP_NONE).to_vec();
        frame.extend_from_slice(&3000u32.to_be_bytes());
        let msg = b"quota exceeded";
        frame.extend_from_slice(&(msg.len() as u32).to_be_bytes());
        frame.extend_from_slice(msg);
        match map_frame(parse_frame(&frame).unwrap()) {
            Some(StreamEvent::Error { fatal: true, error }) => {
                assert!(error.message.contains("quota exceeded"));
            }
            _ => panic!("expected fatal Error"),
        }
    }

    #[test]
    fn meta_frame_maps_to_no_event() {
        let frame = server_meta_frame(EVENT_SESSION_FINISHED, "s", &json!({}));
        assert!(map_frame(parse_frame(&frame).unwrap()).is_none());
    }

    struct ChannelDuplex {
        out: mpsc::Sender<Vec<u8>>,
        inbound: mpsc::Receiver<Vec<u8>>,
    }

    #[async_trait]
    impl ByteDuplex for ChannelDuplex {
        async fn send(&mut self, frame: Vec<u8>) -> Result<(), ProtocolError> {
            self.out.send(frame).await.map_err(|_| stream_err("closed"))
        }
        async fn recv(&mut self) -> Option<Vec<u8>> {
            self.inbound.recv().await
        }
    }

    #[tokio::test]
    async fn run_tts_synthesis_streams_audio_then_done() {
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let (in_tx, in_rx) = mpsc::channel(8);
        let transport = ChannelDuplex {
            out: out_tx,
            inbound: in_rx,
        };
        let (events_tx, mut events_rx) = mpsc::channel(8);

        let handle = tokio::spawn(run_tts_synthesis(transport, vec![0u8; 8], events_tx));

        // The synthesis sends the request frame, which the server observes.
        assert!(out_rx.recv().await.is_some());
        // Server streams one audio chunk, then signals SESSION_FINISHED.
        in_tx.send(audio_frame("s", b"pcm")).await.unwrap();
        in_tx
            .send(server_meta_frame(EVENT_SESSION_FINISHED, "s", &json!({})))
            .await
            .unwrap();

        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::AudioDelta { .. }
        ));
        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::Done { .. }
        ));
        handle.await.unwrap(); // synthesis terminated on SESSION_FINISHED
    }
}
