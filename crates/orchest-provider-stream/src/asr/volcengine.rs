//! Volcengine openspeech **ASR** wire codec, on the shared [`crate::openspeech`]
//! core (Issue 006). Ported from `agent-runtime-asr-providers`'s
//! `providers/volcengine/protocol.rs` — same frame layout, now over the unified
//! header/constants/gzip and [`ProtocolError`] instead of a provider-local copy.
//!
//! ASR frames the (gzipped JSON) full-client-request and raw audio-only requests
//! with a big-endian `u32` payload-length prefix, and uses the sequence flags to
//! mark the final server response. The event-tagged `FLAG_WITH_EVENT` framing is
//! tts/omni-only and lives there.

use async_trait::async_trait;
use orchest_protocol::{
    Asr, Capability, CapabilityDescriptor, ErrorCode, EventStream, Language, LifecycleEvent,
    Modality, ProtocolError, RealtimeHandle, SegmentRef, SessionInput, StreamEvent,
    StreamingTranscribeRequest, TranscribeRequest, TranscribeResult, TranscriptStability,
    TranscriptUpdateKind,
};
use orchest_provider_core::registry::ProviderConfig;
use orchest_provider_core::ws::{connect_async, tungstenite};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::transport::{ByteDuplex, WsDuplex, WsFrame};

use crate::openspeech::{
    build_header, compress_gzip, decompress_gzip, parse_header, COMP_GZIP, COMP_NONE,
    FLAG_LAST_NO_SEQUENCE, FLAG_NO_SEQUENCE, FLAG_SEQUENCE_NEGATIVE, FLAG_SEQUENCE_POSITIVE,
    MSG_AUDIO_ONLY_REQUEST, MSG_ERROR_RESPONSE, MSG_FULL_CLIENT_REQUEST, MSG_FULL_SERVER_RESPONSE,
    SER_JSON, SER_NONE,
};

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn stream_err(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(ErrorCode::ProviderStreamError, message)
}

// ---------------------------------------------------------------------------
// Frame building
// ---------------------------------------------------------------------------

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn build_full_client_request(payload_json: &Value) -> Result<Vec<u8>, ProtocolError> {
    let json_bytes = serde_json::to_vec(payload_json)
        .map_err(|e| ProtocolError::new(ErrorCode::InvalidRequest, format!("serialize: {e}")))?;
    let compressed = compress_gzip(&json_bytes)?;

    let header = build_header(
        MSG_FULL_CLIENT_REQUEST,
        FLAG_NO_SEQUENCE,
        SER_JSON,
        COMP_GZIP,
    );
    let payload_size = (compressed.len() as u32).to_be_bytes();

    let mut frame = Vec::with_capacity(4 + 4 + compressed.len());
    frame.extend_from_slice(&header);
    frame.extend_from_slice(&payload_size);
    frame.extend_from_slice(&compressed);
    Ok(frame)
}

pub fn build_audio_frame(data: &[u8], is_last: bool) -> Vec<u8> {
    let flags = if is_last {
        FLAG_LAST_NO_SEQUENCE
    } else {
        FLAG_NO_SEQUENCE
    };
    let header = build_header(MSG_AUDIO_ONLY_REQUEST, flags, SER_NONE, COMP_NONE);
    let payload_size = (data.len() as u32).to_be_bytes();

    let mut frame = Vec::with_capacity(4 + 4 + data.len());
    frame.extend_from_slice(&header);
    frame.extend_from_slice(&payload_size);
    frame.extend_from_slice(data);
    frame
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum VolcengineFrame {
    ServerResponse {
        sequence: i32,
        payload: VolcenginePayload,
        is_last: bool,
    },
    ErrorResponse {
        code: u32,
        message: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct VolcenginePayload {
    pub result: Option<VolcengineResult>,
    pub audio_info: Option<AudioInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VolcengineResult {
    pub text: String,
    pub utterances: Option<Vec<VolcengineUtterance>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VolcengineUtterance {
    pub text: String,
    pub definite: bool,
    pub start_time: i32,
    pub end_time: i32,
    pub words: Option<Vec<VolcengineWord>>,
    #[serde(default)]
    pub additions: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VolcengineWord {
    pub text: String,
    pub start_time: i32,
    pub end_time: i32,
    #[serde(default)]
    pub blank_duration: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AudioInfo {
    pub duration: Option<u64>,
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn parse_response(data: &[u8]) -> Result<VolcengineFrame, ProtocolError> {
    let header = parse_header(data)?;
    match header.msg_type {
        MSG_ERROR_RESPONSE => parse_error_response(data),
        MSG_FULL_SERVER_RESPONSE => parse_server_response(data, header.flags, header.compression),
        other => Err(stream_err(format!("unexpected message type: 0x{other:X}"))),
    }
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_error_response(data: &[u8]) -> Result<VolcengineFrame, ProtocolError> {
    if data.len() < 12 {
        return Err(stream_err("error response too short"));
    }
    let code = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let msg_size = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let message = if data.len() >= 12 + msg_size {
        String::from_utf8_lossy(&data[12..12 + msg_size]).to_string()
    } else {
        String::new()
    };
    Ok(VolcengineFrame::ErrorResponse { code, message })
}

#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
fn parse_server_response(
    data: &[u8],
    flags: u8,
    compression: u8,
) -> Result<VolcengineFrame, ProtocolError> {
    let has_sequence = flags == FLAG_SEQUENCE_POSITIVE || flags == FLAG_SEQUENCE_NEGATIVE;
    let is_last = flags == FLAG_LAST_NO_SEQUENCE || flags == FLAG_SEQUENCE_NEGATIVE;

    let seq_offset = if has_sequence { 4 } else { 0 };
    let sequence = if has_sequence && data.len() >= 8 {
        i32::from_be_bytes([data[4], data[5], data[6], data[7]])
    } else {
        0
    };

    let size_offset = 4 + seq_offset;
    if data.len() < size_offset + 4 {
        return Err(stream_err("response too short for payload size"));
    }

    let payload_size = u32::from_be_bytes([
        data[size_offset],
        data[size_offset + 1],
        data[size_offset + 2],
        data[size_offset + 3],
    ]) as usize;

    let payload_start = size_offset + 4;
    let payload_end = payload_start + payload_size;
    if data.len() < payload_end {
        return Err(stream_err(format!(
            "response truncated: expected {payload_end} bytes, got {}",
            data.len()
        )));
    }

    let payload_bytes = &data[payload_start..payload_end];
    let json_bytes = if compression == COMP_GZIP {
        decompress_gzip(payload_bytes)?
    } else {
        payload_bytes.to_vec()
    };

    let payload: VolcenginePayload = serde_json::from_slice(&json_bytes)
        .map_err(|e| stream_err(format!("failed to parse response JSON: {e}")))?;

    Ok(VolcengineFrame::ServerResponse {
        sequence,
        payload,
        is_last,
    })
}

// ---------------------------------------------------------------------------
// Frame → unified event mapping (the semantic core of the `Asr` reader loop)
// ---------------------------------------------------------------------------

/// Fixed segment id for the rolling-text fallback branch: a response with no
/// utterance list carries no native segment identity, so all of it maps to one
/// synthetic segment.
const ROLLING_SEGMENT_ID: &str = "rolling";

/// Stateful projector of decoded [`VolcengineFrame`]s onto unified
/// [`StreamEvent`]s — the bridge the streaming `Asr` reader loop emits onto its
/// `EventStream`.
///
/// With `result_type: "single"` the server resends the **full** utterance list
/// every frame, so the mapper diffs each utterance (keyed by its native
/// `start_time`) against what it last emitted and only emits on change: one
/// utterance maps to one segment (`utt{start_time}`, `Snapshot`), and its
/// `Provisional` updates and final `Committed` share that segment id. A
/// response with no utterances but a non-empty rolling `text` maps to the fixed
/// `ROLLING_SEGMENT_ID` segment (committed iff the frame is last). The final
/// frame additionally emits `EndOfSpeech`; an error frame becomes a single
/// fatal `Error`. Both are unaffected by the diff.
#[derive(Debug, Default)]
pub struct VolcengineMapper {
    /// `start_time` → last emitted `(text, definite)`.
    emitted: std::collections::HashMap<i32, (String, bool)>,
    /// Last emitted rolling `(text, committed)`.
    rolling: Option<(String, bool)>,
}

impl VolcengineMapper {
    pub fn new() -> Self {
        Self::default()
    }

    /// Project one frame onto events for what changed since the previous frame.
    pub fn map(&mut self, frame: VolcengineFrame) -> Vec<StreamEvent> {
        match frame {
            VolcengineFrame::ServerResponse {
                payload, is_last, ..
            } => {
                let mut events = Vec::new();
                if let Some(result) = payload.result {
                    match result.utterances {
                        Some(utterances) if !utterances.is_empty() => {
                            for utt in utterances {
                                if self
                                    .emitted
                                    .get(&utt.start_time)
                                    .is_some_and(|(t, d)| t == &utt.text && *d == utt.definite)
                                {
                                    continue;
                                }
                                self.emitted
                                    .insert(utt.start_time, (utt.text.clone(), utt.definite));
                                events.push(StreamEvent::Transcript {
                                    text: utt.text,
                                    stability: if utt.definite {
                                        TranscriptStability::Committed
                                    } else {
                                        TranscriptStability::Provisional
                                    },
                                    segment: Some(SegmentRef {
                                        segment_id: Some(format!("utt{}", utt.start_time)),
                                        update_kind: TranscriptUpdateKind::Snapshot,
                                    }),
                                });
                            }
                        }
                        _ if !result.text.is_empty()
                            && self.rolling.as_ref() != Some(&(result.text.clone(), is_last)) =>
                        {
                            self.rolling = Some((result.text.clone(), is_last));
                            events.push(StreamEvent::Transcript {
                                text: result.text,
                                stability: if is_last {
                                    TranscriptStability::Committed
                                } else {
                                    TranscriptStability::Provisional
                                },
                                segment: Some(SegmentRef {
                                    segment_id: Some(ROLLING_SEGMENT_ID.to_string()),
                                    update_kind: TranscriptUpdateKind::Snapshot,
                                }),
                            });
                        }
                        _ => {}
                    }
                }
                if is_last {
                    events.push(StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech {
                        segment: None,
                    }));
                }
                events
            }
            VolcengineFrame::ErrorResponse { code, message } => vec![StreamEvent::Error {
                error: ProtocolError::new(
                    ErrorCode::ProviderTaskFailed,
                    format!("Volcengine ASR error {code}: {message}"),
                ),
                fatal: true,
            }],
        }
    }
}

// ---------------------------------------------------------------------------
// Streaming loop (transport-agnostic; live WS is a thin ByteDuplex adapter)
// ---------------------------------------------------------------------------

/// Drive one ASR streaming session over `transport`: client [`SessionInput`]
/// audio is framed as openspeech audio-only requests and sent; server frames are
/// parsed and projected onto `events` via [`VolcengineMapper`]. Closing `input`
/// flushes a final audio frame; the loop ends on the last server frame or
/// transport EOF.
pub async fn run_asr_stream<T: ByteDuplex>(
    mut transport: T,
    mut input: mpsc::Receiver<SessionInput>,
    events: mpsc::Sender<StreamEvent>,
) {
    let mut mapper = VolcengineMapper::new();
    let mut input_open = true;
    loop {
        tokio::select! {
            maybe_input = input.recv(), if input_open => match maybe_input {
                Some(SessionInput::Audio(bytes)) => {
                    if transport
                        .send(WsFrame::Binary(build_audio_frame(&bytes, false)))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                // End of audio (input closed or an explicit interrupt): flush the
                // last audio frame and keep reading server frames until the last.
                None | Some(SessionInput::Interrupt) => {
                    input_open = false;
                    let _ = transport.send(WsFrame::Binary(build_audio_frame(&[], true))).await;
                }
                // Text / tool-result are not part of the ASR send side.
                Some(_) => {}
            },
            maybe_frame = transport.recv() => match maybe_frame.as_ref().and_then(WsFrame::as_binary) {
                Some(bytes) => match parse_response(bytes) {
                    Ok(frame) => {
                        let is_last = matches!(
                            frame,
                            VolcengineFrame::ServerResponse { is_last: true, .. }
                        );
                        for event in mapper.map(frame) {
                            if events.send(event).await.is_err() {
                                return;
                            }
                        }
                        if is_last {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = events.send(StreamEvent::Error { error, fatal: true }).await;
                        break;
                    }
                },
                // A text frame (openspeech is binary) or transport EOF.
                None => match maybe_frame {
                    Some(_) => continue, // unexpected text frame — ignore
                    None => break,
                },
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Live WebSocket transport + the spine `Asr` impl
// ---------------------------------------------------------------------------

/// Volcengine streaming-ASR configuration (the openspeech `sauc` endpoints).
#[derive(Debug, Clone)]
pub struct VolcengineAsrConfig {
    pub model: String,
    pub ws_url: String,
    pub api_key: String,
    pub access_key: Option<String>,
    pub resource_id: String,
}

/// The Volcengine streaming ASR provider as the spine [`Asr`]. Construction is
/// cheap and synchronous (the WS handshake is deferred to [`Asr::start_stream`]),
/// so this slots straight into the registry factory.
pub struct VolcengineAsr {
    config: VolcengineAsrConfig,
}

impl VolcengineAsr {
    pub fn new(config: VolcengineAsrConfig) -> Self {
        Self { config }
    }

    fn client_payload(&self, request: &StreamingTranscribeRequest) -> Value {
        let mut req_obj = serde_json::json!({
            "model_name": "bigmodel",
            "show_utterances": true,
            "result_type": "single",
        });
        if let Some(extra) = request.options.as_object() {
            for (key, value) in extra {
                req_obj[key] = value.clone();
            }
        }
        serde_json::json!({
            "user": {"uid": "orchest-sdk"},
            "audio": {"format": "pcm", "rate": 16000, "bits": 16, "channel": 1},
            "request": req_obj,
        })
    }
}

fn extract_host(url: &str) -> &str {
    url.strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .and_then(|s| s.split('/').next())
        .unwrap_or("openspeech.bytedance.com")
}

/// The static descriptor the registry filters on for the Volcengine ASR dialect.
pub fn entry_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor::new("volcengine", "bigmodel", Capability::Asr)
        .streaming(true)
        .duplex(true)
        .with_input_modalities([Modality::Audio])
        .with_output_modalities([Modality::Text])
}

/// Build a [`VolcengineAsr`] from a registry [`ProviderConfig`]: `api_url` is the
/// `wss://` endpoint, `api_key` the secret, and `options.{access_key,resource_id}`
/// carry the dialect-specific knobs.
#[allow(clippy::result_large_err)] // justified: ProtocolError carries diagnostic context (matches the workspace error convention)
pub fn from_provider_config(cfg: &ProviderConfig) -> Result<VolcengineAsr, ProtocolError> {
    let ws_url = cfg.api_url.clone().ok_or_else(|| {
        ProtocolError::new(
            ErrorCode::InvalidRequest,
            "volcengine ASR requires api_url (wss:// endpoint)",
        )
    })?;
    let api_key = cfg.api_key.clone().ok_or_else(|| {
        ProtocolError::new(ErrorCode::MissingApiKey, "volcengine ASR requires api_key")
    })?;
    let model = if cfg.model.is_empty() {
        "bigmodel".to_string()
    } else {
        cfg.model.clone()
    };
    let access_key = cfg
        .options
        .get("access_key")
        .and_then(Value::as_str)
        .map(String::from);
    let resource_id = cfg
        .options
        .get("resource_id")
        .and_then(Value::as_str)
        .unwrap_or("volc.bigasr.sauc.duration")
        .to_string();
    Ok(VolcengineAsr::new(VolcengineAsrConfig {
        model,
        ws_url,
        api_key,
        access_key,
        resource_id,
    }))
}

#[async_trait]
impl Asr for VolcengineAsr {
    fn provider_name(&self) -> &str {
        "volcengine"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::new("volcengine", self.config.model.clone(), Capability::Asr)
            .streaming(true)
            .duplex(true)
            .with_input_modalities([Modality::Audio])
            .with_output_modalities([Modality::Text])
    }

    fn supported_languages(&self) -> &[Language] {
        &[]
    }

    async fn transcribe(
        &self,
        _request: TranscribeRequest,
    ) -> Result<TranscribeResult, ProtocolError> {
        Err(ProtocolError::new(
            ErrorCode::UnsupportedOperation,
            "volcengine ASR is streaming-only; use start_stream",
        ))
    }

    async fn start_stream(
        &self,
        request: StreamingTranscribeRequest,
    ) -> Result<RealtimeHandle, ProtocolError> {
        if !self.config.ws_url.starts_with("wss://") {
            return Err(ProtocolError::new(
                ErrorCode::InvalidRequest,
                "WebSocket URL must use wss:// for secure credential transport",
            ));
        }

        let connect_id = uuid::Uuid::new_v4().to_string();
        let request_id = uuid::Uuid::new_v4().to_string();

        let mut builder = tungstenite::http::Request::builder()
            .uri(&self.config.ws_url)
            .header("X-Api-Key", &self.config.api_key);
        if let Some(access_key) = &self.config.access_key {
            builder = builder.header("X-Api-Access-Key", access_key);
        }
        let ws_request = builder
            .header("X-Api-Resource-Id", &self.config.resource_id)
            .header("X-Api-Connect-Id", &connect_id)
            .header("X-Api-Request-Id", &request_id)
            .header("X-Api-Sequence", "-1")
            .header("Host", extract_host(&self.config.ws_url))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .map_err(|e| {
                ProtocolError::new(ErrorCode::InvalidRequest, format!("build ws request: {e}"))
            })?;

        let (ws_stream, _response) = connect_async(ws_request)
            .await
            .map_err(|e| stream_err(format!("WebSocket connection failed: {e}")))?;

        let mut duplex = WsDuplex::new(ws_stream);
        duplex
            .send(WsFrame::Binary(build_full_client_request(
                &self.client_payload(&request),
            )?))
            .await?;

        let (input_tx, input_rx) = mpsc::channel(32);
        let (events_tx, events) = EventStream::channel(64);

        // Surface route selection up front, like the legacy adapter did.
        let _ = events_tx
            .send(StreamEvent::Lifecycle(LifecycleEvent::RouteSelected {
                provider: "volcengine".into(),
                model: self.config.model.clone(),
                trace_id: None,
            }))
            .await;

        tokio::spawn(run_asr_stream(duplex, input_rx, events_tx));

        Ok(RealtimeHandle {
            input: input_tx,
            events,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn audio_frame_normal_and_last() {
        let frame = build_audio_frame(&[1, 2, 3, 4], false);
        assert_eq!(frame[1] & 0x0F, FLAG_NO_SEQUENCE);
        let size = u32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]);
        assert_eq!(size, 4);
        assert_eq!(&frame[8..], &[1, 2, 3, 4]);
        assert_eq!(
            build_audio_frame(&[1, 2], true)[1] & 0x0F,
            FLAG_LAST_NO_SEQUENCE
        );
    }

    #[test]
    fn full_client_request_roundtrip() {
        let payload = json!({
            "request": {"model_name": "bigmodel", "show_utterances": true}
        });
        let frame = build_full_client_request(&payload).unwrap();
        assert_eq!(frame[0], 0x11);
        assert_eq!((frame[1] >> 4) & 0x0F, MSG_FULL_CLIENT_REQUEST);
        assert_eq!((frame[2] >> 4) & 0x0F, SER_JSON);
        assert_eq!(frame[2] & 0x0F, COMP_GZIP);

        let payload_size = u32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]) as usize;
        let decompressed = decompress_gzip(&frame[8..8 + payload_size]).unwrap();
        let parsed: Value = serde_json::from_slice(&decompressed).unwrap();
        assert_eq!(parsed["request"]["model_name"], "bigmodel");
    }

    fn server_frame(flags: u8, sequence: Option<i32>, body: &Value) -> Vec<u8> {
        let compressed = compress_gzip(&serde_json::to_vec(body).unwrap()).unwrap();
        let mut frame = build_header(MSG_FULL_SERVER_RESPONSE, flags, SER_JSON, COMP_GZIP).to_vec();
        if let Some(seq) = sequence {
            frame.extend_from_slice(&seq.to_be_bytes());
        }
        frame.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
        frame.extend_from_slice(&compressed);
        frame
    }

    #[test]
    fn parse_server_response_with_sequence() {
        let body = json!({
            "result": {
                "text": "测试",
                "utterances": [{
                    "text": "测试", "definite": true, "start_time": 0, "end_time": 1000,
                    "words": [{"text": "测", "start_time": 0, "end_time": 500}]
                }]
            },
            "audio_info": {"duration": 1000}
        });
        match parse_response(&server_frame(FLAG_SEQUENCE_POSITIVE, Some(1), &body)).unwrap() {
            VolcengineFrame::ServerResponse {
                sequence,
                payload,
                is_last,
            } => {
                assert_eq!(sequence, 1);
                assert!(!is_last);
                let result = payload.result.unwrap();
                assert_eq!(result.text, "测试");
                assert!(result.utterances.unwrap()[0].definite);
            }
            _ => panic!("expected ServerResponse"),
        }
    }

    #[test]
    fn parse_last_response_negative_sequence() {
        let body = json!({"result": {"text": "end"}, "audio_info": {"duration": 500}});
        match parse_response(&server_frame(FLAG_SEQUENCE_NEGATIVE, Some(-1), &body)).unwrap() {
            VolcengineFrame::ServerResponse {
                is_last, sequence, ..
            } => {
                assert!(is_last);
                assert_eq!(sequence, -1);
            }
            _ => panic!("expected ServerResponse"),
        }
    }

    #[test]
    fn parse_response_no_sequence() {
        let body = json!({"result": {"text": "ok"}, "audio_info": {}});
        match parse_response(&server_frame(FLAG_NO_SEQUENCE, None, &body)).unwrap() {
            VolcengineFrame::ServerResponse {
                sequence,
                payload,
                is_last,
            } => {
                assert_eq!(sequence, 0);
                assert!(!is_last);
                assert_eq!(payload.result.unwrap().text, "ok");
            }
            _ => panic!("expected ServerResponse"),
        }
    }

    #[test]
    fn parse_error_response_frame() {
        let mut frame =
            build_header(MSG_ERROR_RESPONSE, FLAG_NO_SEQUENCE, SER_JSON, COMP_NONE).to_vec();
        frame.extend_from_slice(&45000001u32.to_be_bytes());
        let msg = b"invalid parameters";
        frame.extend_from_slice(&(msg.len() as u32).to_be_bytes());
        frame.extend_from_slice(msg);

        match parse_response(&frame).unwrap() {
            VolcengineFrame::ErrorResponse { code, message } => {
                assert_eq!(code, 45000001);
                assert_eq!(message, "invalid parameters");
            }
            _ => panic!("expected ErrorResponse"),
        }
    }

    fn server(is_last: bool, payload: VolcenginePayload) -> VolcengineFrame {
        VolcengineFrame::ServerResponse {
            sequence: 0,
            payload,
            is_last,
        }
    }

    fn utterance(text: &str, definite: bool, start_time: i32) -> VolcengineUtterance {
        VolcengineUtterance {
            text: text.into(),
            definite,
            start_time,
            end_time: start_time + 500,
            words: None,
            additions: None,
        }
    }

    fn utterances_payload(utterances: Vec<VolcengineUtterance>) -> VolcenginePayload {
        VolcenginePayload {
            result: Some(VolcengineResult {
                text: "ignored when utterances present".into(),
                utterances: Some(utterances),
            }),
            audio_info: None,
        }
    }

    #[test]
    fn mapper_emits_per_utterance_with_native_segment_ids() {
        let mut mapper = VolcengineMapper::new();
        let events = mapper.map(server(
            false,
            utterances_payload(vec![
                utterance("hello", false, 0),
                utterance("world", true, 500),
            ]),
        ));
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            StreamEvent::Transcript {
                stability: TranscriptStability::Provisional,
                segment: Some(SegmentRef {
                    segment_id,
                    update_kind: TranscriptUpdateKind::Snapshot,
                }),
                ..
            } if segment_id.as_deref() == Some("utt0")
        ));
        assert!(matches!(
            &events[1],
            StreamEvent::Transcript {
                stability: TranscriptStability::Committed,
                segment: Some(SegmentRef { segment_id, .. }),
                ..
            } if segment_id.as_deref() == Some("utt500")
        ));
    }

    #[test]
    fn mapper_suppresses_unchanged_utterances_on_resend() {
        let mut mapper = VolcengineMapper::new();
        // `result_type: "single"` resends the same definite utterance every frame.
        let frame = || {
            server(
                false,
                utterances_payload(vec![utterance("hello", true, 100)]),
            )
        };
        assert_eq!(mapper.map(frame()).len(), 1);
        assert!(mapper.map(frame()).is_empty());
    }

    #[test]
    fn mapper_shares_segment_id_across_provisional_and_committed() {
        let mut mapper = VolcengineMapper::new();
        let first = mapper.map(server(
            false,
            utterances_payload(vec![utterance("he", false, 450)]),
        ));
        let second = mapper.map(server(
            false,
            utterances_payload(vec![utterance("hello", false, 450)]),
        ));
        let third = mapper.map(server(
            false,
            utterances_payload(vec![utterance("hello", true, 450)]),
        ));
        for events in [&first, &second, &third] {
            assert_eq!(events.len(), 1);
            assert!(matches!(
                &events[0],
                StreamEvent::Transcript {
                    segment: Some(SegmentRef {
                        segment_id,
                        update_kind: TranscriptUpdateKind::Snapshot,
                    }),
                    ..
                } if segment_id.as_deref() == Some("utt450")
            ));
        }
        assert!(matches!(
            third[0],
            StreamEvent::Transcript {
                stability: TranscriptStability::Committed,
                ..
            }
        ));
    }

    fn rolling_payload(text: &str) -> VolcenginePayload {
        VolcenginePayload {
            result: Some(VolcengineResult {
                text: text.into(),
                utterances: None,
            }),
            audio_info: None,
        }
    }

    #[test]
    fn mapper_rolling_branch_uses_fixed_segment_and_commits_on_last() {
        let mut mapper = VolcengineMapper::new();
        let first = mapper.map(server(false, rolling_payload("fin")));
        assert_eq!(first.len(), 1);
        assert!(matches!(
            &first[0],
            StreamEvent::Transcript {
                stability: TranscriptStability::Provisional,
                segment: Some(SegmentRef {
                    segment_id,
                    update_kind: TranscriptUpdateKind::Snapshot,
                }),
                ..
            } if segment_id.as_deref() == Some(ROLLING_SEGMENT_ID)
        ));
        // Same rolling text mid-stream: suppressed by the diff.
        assert!(mapper.map(server(false, rolling_payload("fin"))).is_empty());
        // Same text on the last frame still commits (the stability transition).
        let last = mapper.map(server(true, rolling_payload("fin")));
        assert_eq!(last.len(), 2);
        assert!(matches!(
            &last[0],
            StreamEvent::Transcript {
                stability: TranscriptStability::Committed,
                ..
            }
        ));
        assert!(matches!(
            &last[1],
            StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech { segment: None })
        ));
    }

    #[test]
    fn mapper_error_is_fatal() {
        let mut mapper = VolcengineMapper::new();
        let events = mapper.map(VolcengineFrame::ErrorResponse {
            code: 45000001,
            message: "bad".into(),
        });
        assert!(matches!(events[0], StreamEvent::Error { fatal: true, .. }));
    }

    struct ChannelDuplex {
        out: mpsc::Sender<WsFrame>,
        inbound: mpsc::Receiver<WsFrame>,
    }

    #[async_trait]
    impl ByteDuplex for ChannelDuplex {
        async fn send(&mut self, frame: WsFrame) -> Result<(), ProtocolError> {
            self.out.send(frame).await.map_err(|_| stream_err("closed"))
        }
        async fn recv(&mut self) -> Option<WsFrame> {
            self.inbound.recv().await
        }
    }

    #[tokio::test]
    async fn run_asr_stream_maps_audio_in_and_server_frames_out() {
        let (out_tx, mut out_rx) = mpsc::channel(16); // loop -> server observes
        let (in_tx, in_rx) = mpsc::channel(16); // server -> loop
        let transport = ChannelDuplex {
            out: out_tx,
            inbound: in_rx,
        };
        let (input_tx, input_rx) = mpsc::channel(16);
        let (events_tx, mut events_rx) = mpsc::channel(16);

        let handle = tokio::spawn(run_asr_stream(transport, input_rx, events_tx));

        // audio in -> the loop frames it as an openspeech audio-only request
        input_tx
            .send(SessionInput::Audio(bytes::Bytes::from_static(b"mic")))
            .await
            .unwrap();
        let framed = out_rx.recv().await.expect("audio frame emitted");
        assert_eq!(
            parse_header(framed.as_binary().unwrap()).unwrap().msg_type,
            MSG_AUDIO_ONLY_REQUEST
        );

        // server sends its final response -> Transcript(Committed) + EndOfSpeech, then ends
        let body = json!({"result": {"text": "done"}});
        in_tx
            .send(WsFrame::Binary(server_frame(
                FLAG_SEQUENCE_NEGATIVE,
                Some(-1),
                &body,
            )))
            .await
            .unwrap();

        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::Transcript {
                stability: TranscriptStability::Committed,
                ..
            }
        ));
        assert!(matches!(
            events_rx.recv().await.unwrap(),
            StreamEvent::Lifecycle(LifecycleEvent::EndOfSpeech { .. })
        ));

        handle.await.unwrap(); // the loop terminated on the last server frame
    }
}
